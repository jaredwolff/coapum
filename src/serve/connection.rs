use std::{
    collections::HashMap,
    fmt::Debug,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use dimpl::{Dtls, Output};
use tokio::{
    net::UdpSocket,
    sync::{
        Mutex,
        mpsc::{self, Sender, channel},
        watch,
    },
};

use coap_lite::{BlockHandler, BlockHandlerConfig, Packet};

use super::handlers::{handle_notification, handle_request};
use super::helpers::{drain_packets, extract_identity};
use super::{ConnectionInfo, ObserveState};
use crate::{
    config::Config,
    credential::{CredentialStore, resolver::CapturingResolver},
    observer::{Observer, ObserverValue},
    reliability::{ReliabilityState, RetransmitAction, RetransmitParams},
    router::CoapRouter,
};

/// Validate connection and implement rate limiting for reconnections.
///
/// Returns `true` if the connection is allowed, `false` if rate-limited or blocked.
pub(super) async fn manage_connection(
    identity: &str,
    socket_addr: SocketAddr,
    tx: Sender<()>,
    connections: &Mutex<HashMap<String, ConnectionInfo>>,
    min_reconnect_interval: Duration,
    max_reconnect_attempts: usize,
) -> bool {
    let mut guard = connections.lock().await;

    if let Some(old_conn) = guard.get(identity) {
        if old_conn.established_at.elapsed() < min_reconnect_interval {
            tracing::warn!(
                identity = %identity,
                addr = %socket_addr,
                interval_ms = old_conn.established_at.elapsed().as_millis() as u64,
                "connection.rejected.rate_limit"
            );
            return false;
        }

        if old_conn.reconnect_count as usize > max_reconnect_attempts {
            tracing::error!(
                identity = %identity,
                addr = %socket_addr,
                count = old_conn.reconnect_count,
                "connection.rejected.max_attempts"
            );
            return false;
        }

        let _ = old_conn.sender.send(()).await;
    }

    let conn_info = ConnectionInfo {
        sender: tx,
        established_at: Instant::now(),
        source_addr: socket_addr,
        reconnect_count: guard
            .get(identity)
            .map(|c| c.reconnect_count + 1)
            .unwrap_or(0),
    };

    guard.insert(identity.to_string(), conn_info);
    tracing::info!(
        identity = %identity,
        addr = %socket_addr,
        "connection.established"
    );
    true
}

/// Process DTLS outputs after handle_packet(), handling Connected and ApplicationData events.
///
/// Returns `false` if the connection should be terminated.
#[allow(clippy::too_many_arguments)]
pub(super) async fn process_outputs<O, S>(
    dtls: &mut Dtls,
    out_buf: &mut [u8],
    socket: &UdpSocket,
    remote: SocketAddr,
    resolver: &CapturingResolver<impl CredentialStore>,
    connected: &mut bool,
    identity: &mut Option<String>,
    router: &mut CoapRouter<O, S>,
    obs_tx: &Arc<Sender<ObserverValue>>,
    obs: &mut ObserveState,
    block_handler: &mut BlockHandler<SocketAddr>,
    max_observers_per_device: usize,
    connections: &Mutex<HashMap<String, ConnectionInfo>>,
    disconnect_tx: Sender<()>,
    config: &Config,
    reliability: &mut ReliabilityState,
) -> bool
where
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + 'static,
{
    loop {
        match dtls.poll_output(out_buf) {
            Output::Packet(p) => {
                if let Err(e) = socket.send_to(p, remote).await {
                    tracing::error!(addr = %remote, error = %e, "udp.send_failed");
                }
            }
            Output::Connected => {
                tracing::debug!(addr = %remote, "dtls.connected");

                let raw_identity = match resolver.take_last_identity() {
                    Some(id) => id,
                    None => {
                        tracing::error!(addr = %remote, "dtls.no_identity");
                        return false;
                    }
                };

                let validated = match extract_identity(raw_identity.as_bytes()) {
                    Some(id) => id,
                    None => return false,
                };

                if !manage_connection(
                    &validated,
                    remote,
                    disconnect_tx.clone(),
                    connections,
                    config.min_reconnect_interval,
                    config.max_reconnect_attempts,
                )
                .await
                {
                    return false;
                }

                tracing::info!(identity = %validated, addr = %remote, "connection.accepted");
                *identity = Some(validated);
                *connected = true;
            }
            Output::ApplicationData(data) => {
                if let Some(id) = identity.as_ref() {
                    let packet = match Packet::from_bytes(data) {
                        Ok(p) => p,
                        Err(e) => {
                            tracing::error!("Failed to parse packet: {}", e);
                            continue;
                        }
                    };
                    handle_request(
                        packet,
                        remote,
                        id,
                        router,
                        dtls,
                        out_buf,
                        socket,
                        obs_tx,
                        obs,
                        block_handler,
                        config.max_message_size,
                        max_observers_per_device,
                        reliability,
                    )
                    .await;
                }
            }
            Output::ConnectionId(cid) => {
                tracing::debug!(
                    addr = %remote,
                    cid_len = cid.len(),
                    "dtls.connection_id_negotiated"
                );
            }
            Output::Timeout(_) => break,
            _ => {} // PeerCert, KeyingMaterial — not used for PSK
        }
    }
    true
}

/// Per-connection task. Each spawned task owns its own Dtls instance and
/// its own `CapturingResolver`, so identity capture is race-free.
#[allow(clippy::too_many_arguments)]
pub(super) async fn connection_task<O, S, C>(
    remote: SocketAddr,
    mut packet_rx: mpsc::Receiver<Vec<u8>>,
    socket: Arc<UdpSocket>,
    credential_store: C,
    psk_identity_hint: Option<Vec<u8>>,
    cid: Option<Vec<u8>>,
    mut addr_rx: watch::Receiver<SocketAddr>,
    mut router: CoapRouter<O, S>,
    config: Config,
    connections: Arc<Mutex<HashMap<String, ConnectionInfo>>>,
    conn_count: Arc<AtomicUsize>,
    cleanup_tx: mpsc::Sender<(SocketAddr, Option<Vec<u8>>)>,
) where
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + 'static,
    C: CredentialStore,
{
    // Build per-connection resolver + dimpl config so identity capture is race-free
    let resolver = Arc::new(CapturingResolver::new(credential_store));
    let mut builder = dimpl::Config::builder().with_psk_server(
        psk_identity_hint.clone(),
        resolver.clone() as Arc<dyn dimpl::PskResolver>,
    );
    if let Some(ref cid) = cid {
        builder = builder.with_connection_id(cid.clone());
    }
    let dimpl_config = Arc::new(builder.build().expect("valid DTLS config"));

    let mut dtls = Dtls::new_12_psk(dimpl_config, Instant::now());
    let mut out_buf = vec![0u8; 2048];
    let mut connected = false;
    let mut identity: Option<String> = None;
    let mut current_remote = remote;

    let (obs_tx, mut obs_rx) = channel::<ObserverValue>(10);
    let obs_tx = Arc::new(obs_tx);
    let mut obs = ObserveState::new();
    let mut reliability = ReliabilityState::new(RetransmitParams::from_config(&config));
    let mut block_handler = BlockHandler::new(BlockHandlerConfig {
        max_total_message_size: config.max_message_size,
        cache_expiry_duration: config.block_cache_expiry,
    });

    let (disconnect_tx, mut disconnect_rx) = channel::<()>(1);
    let timeout_duration = Duration::from_secs(config.timeout);

    // One-shot session lifetime timer (DTLS 1.2 key wear-out mitigation).
    // Created once before the loop so it is NOT reset on activity.
    let session_deadline = config.max_session_lifetime.map(tokio::time::sleep);
    tokio::pin!(session_deadline);

    loop {
        // Compute next DTLS retransmit deadline
        let dtls_timeout = tokio::time::sleep(timeout_duration);
        tokio::pin!(dtls_timeout);

        tokio::select! {
            biased;

            // Address migration via CID (RFC 9146).
            // Biased first so current_remote is up-to-date before any
            // send_to calls in the arms below.
            Ok(()) = addr_rx.changed() => {
                let new_addr = *addr_rx.borrow();
                tracing::info!(
                    old_addr = %current_remote,
                    new_addr = %new_addr,
                    identity = ?identity,
                    "connection.address_migrated"
                );
                current_remote = new_addr;
            }

            // Incoming DTLS packet from dispatch
            packet = packet_rx.recv() => {
                let Some(raw) = packet else {
                    // Channel closed — dispatch removed us
                    tracing::debug!(addr = %current_remote, "connection.channel_closed");
                    break;
                };

                if let Err(e) = dtls.handle_packet(&raw) {
                    tracing::error!(addr = %current_remote, error = %e, "dtls.packet_error");
                    break;
                }

                if !process_outputs(
                    &mut dtls, &mut out_buf, &socket, current_remote,
                    &resolver, &mut connected, &mut identity,
                    &mut router, &obs_tx, &mut obs, &mut block_handler,
                    config.max_observers_per_device,
                    &connections, disconnect_tx.clone(), &config,
                    &mut reliability,
                ).await {
                    break;
                }
            }

            // Observer notification
            Some(value) = obs_rx.recv(), if connected => {
                handle_notification(
                    value, &mut router, &mut dtls, &mut out_buf,
                    &socket, current_remote, &mut obs, &mut block_handler,
                    &mut reliability,
                ).await;
            }

            // Disconnect signal
            _ = disconnect_rx.recv() => {
                tracing::info!(addr = %current_remote, identity = ?identity, "connection.terminating");
                break;
            }

            // Idle timeout
            () = &mut dtls_timeout => {
                tracing::info!(addr = %current_remote, "connection.timeout");
                break;
            }

            // Session lifetime limit (DTLS 1.2 key wear-out mitigation)
            Some(()) = async {
                match session_deadline.as_mut().as_pin_mut() {
                    Some(f) => { f.await; Some(()) }
                    None => std::future::pending().await,
                }
            } => {
                tracing::info!(
                    addr = %current_remote,
                    identity = ?identity,
                    "connection.session_lifetime_exceeded"
                );
                break;
            }

            // RFC 7252 §4.2: CON retransmission timer
            () = async {
                match reliability.next_retransmit_deadline() {
                    Some(d) => tokio::time::sleep_until(d).await,
                    None => std::future::pending::<()>().await,
                }
            } => {
                for action in reliability.process_retransmits() {
                    match action {
                        RetransmitAction::Resend { msg_id, ref bytes } => {
                            tracing::debug!(msg_id, "reliability.retransmit");
                            if let Err(e) = dtls.send_application_data(bytes) {
                                tracing::error!(error = %e, "reliability.retransmit.send_failed");
                                continue;
                            }
                            drain_packets(&mut dtls, &mut out_buf, &socket, current_remote).await;
                        }
                        RetransmitAction::GiveUp { msg_id } => {
                            tracing::warn!(msg_id, "reliability.give_up");
                            if let Some(path) = obs.notification_msg_ids.remove(&msg_id)
                                && let Some(ref id) = identity
                            {
                                obs.observer_tokens.remove(&path);
                                let _ = router.unregister_observer(id, &path).await;
                                tracing::info!(identity = %id, path = %path, "reliability.observer_deregistered");
                            }
                        }
                    }
                }
            }
        }

        // Drive DTLS retransmit timers after every event
        if let Err(e) = dtls.handle_timeout(Instant::now()) {
            tracing::error!(addr = %current_remote, error = %e, "dtls.timeout_error");
            break;
        }
        drain_packets(&mut dtls, &mut out_buf, &socket, current_remote).await;
    }

    // Cleanup: `current_remote` is always the latest address (updated by
    // addr_rx on migration). The main loop's migration path already removed
    // the old addr_dispatch entry and inserted the new one, so sending
    // `current_remote` here correctly removes the active entry.
    conn_count.fetch_sub(1, Ordering::Relaxed);
    if let Some(ref id) = identity {
        connections.lock().await.remove(id);
        let _ = router.unregister_device(id).await;
        tracing::info!(identity = %id, addr = %current_remote, "connection.terminated");
    }
    let _ = cleanup_tx.send((current_remote, cid)).await;
}
