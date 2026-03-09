//! Async DTLS client wrapper around dimpl's sans-IO API.
//!
//! Provides a simple `connect → send → recv` interface for CoAP clients,
//! driving the dimpl DTLS state machine over a tokio UDP socket.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dimpl::{Dtls, Output};
use tokio::net::UdpSocket;

/// An async DTLS client that wraps dimpl's sans-IO state machine.
pub struct DtlsClient {
    socket: UdpSocket,
    dtls: Dtls,
    remote: SocketAddr,
    out_buf: Vec<u8>,
}

impl DtlsClient {
    /// Connect to a DTLS server and complete the handshake.
    pub async fn connect(
        remote_addr: &str,
        config: Arc<dimpl::Config>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let remote: SocketAddr = remote_addr.parse()?;
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.connect(remote).await?;

        let mut dtls = Dtls::new_12_psk(config, Instant::now());
        dtls.set_active(true); // client role

        let mut out_buf = vec![0u8; 2048];
        let mut recv_buf = vec![0u8; 2048];
        let handshake_timeout = Duration::from_secs(10);
        let start = tokio::time::Instant::now();

        // Drive the handshake to completion
        loop {
            if start.elapsed() > handshake_timeout {
                return Err("DTLS handshake timed out".into());
            }

            // Handle retransmit timers
            dtls.handle_timeout(Instant::now())?;

            // Drain outputs: send packets, check for Connected
            let mut is_connected = false;
            #[allow(unused_assignments)]
            let mut wait_duration = Duration::from_millis(100);
            loop {
                match dtls.poll_output(&mut out_buf) {
                    Output::Packet(p) => {
                        socket.send(p).await?;
                    }
                    Output::Connected => {
                        is_connected = true;
                    }
                    Output::Timeout(t) => {
                        wait_duration = t.saturating_duration_since(Instant::now());
                        break;
                    }
                    _ => {}
                }
            }

            if is_connected {
                return Ok(Self {
                    socket,
                    dtls,
                    remote,
                    out_buf,
                });
            }

            match tokio::time::timeout(wait_duration, socket.recv(&mut recv_buf)).await {
                Ok(Ok(n)) => {
                    dtls.handle_packet(&recv_buf[..n])?;
                }
                Ok(Err(e)) => return Err(e.into()),
                Err(_) => {} // Timeout — loop back to handle_timeout
            }
        }
    }

    /// Send application data over the DTLS connection.
    pub async fn send(&mut self, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        self.dtls.send_application_data(data)?;
        loop {
            match self.dtls.poll_output(&mut self.out_buf) {
                Output::Packet(p) => {
                    self.socket.send(p).await?;
                }
                Output::Timeout(_) => break,
                _ => {}
            }
        }
        Ok(())
    }

    /// Receive application data from the DTLS connection.
    ///
    /// Blocks until data is available or the timeout is reached.
    pub async fn recv(&mut self, timeout: Duration) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut recv_buf = vec![0u8; 2048];
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err("recv timed out".into());
            }

            let remaining = deadline - tokio::time::Instant::now();
            match tokio::time::timeout(remaining, self.socket.recv(&mut recv_buf)).await {
                Ok(Ok(n)) => {
                    self.dtls.handle_packet(&recv_buf[..n])?;

                    // Check for application data
                    loop {
                        match self.dtls.poll_output(&mut self.out_buf) {
                            Output::ApplicationData(data) => {
                                return Ok(data.to_vec());
                            }
                            Output::Packet(p) => {
                                self.socket.send(p).await?;
                            }
                            Output::Timeout(_) => break,
                            _ => {}
                        }
                    }
                }
                Ok(Err(e)) => return Err(e.into()),
                Err(_) => return Err("recv timed out".into()),
            }
        }
    }

    /// Get the remote address.
    pub fn remote_addr(&self) -> SocketAddr {
        self.remote
    }
}
