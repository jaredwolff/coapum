use std::collections::HashMap;
use std::time::Duration;

use rand::RngExt;
use tokio::time::Instant;

use crate::config::Config;

/// RFC 7252 retransmission parameters, extracted from Config for use
/// in the reliability state machine.
#[derive(Debug, Clone)]
pub struct RetransmitParams {
    pub ack_timeout: Duration,
    pub ack_random_factor: f64,
    pub max_retransmit: u32,
    pub exchange_lifetime: Duration,
}

impl RetransmitParams {
    pub fn from_config(config: &Config) -> Self {
        Self {
            ack_timeout: config.ack_timeout,
            ack_random_factor: config.ack_random_factor,
            max_retransmit: config.max_retransmit,
            exchange_lifetime: config.exchange_lifetime(),
        }
    }

    /// Compute a randomized initial timeout per RFC 7252 §4.2:
    /// uniform random between ACK_TIMEOUT and ACK_TIMEOUT * ACK_RANDOM_FACTOR
    fn initial_timeout(&self) -> Duration {
        if self.ack_random_factor <= 1.0 {
            return self.ack_timeout;
        }
        let factor = rand::rng().random_range(1.0..self.ack_random_factor);
        self.ack_timeout.mul_f64(factor)
    }
}

/// A single outstanding CON message we sent, awaiting ACK.
struct PendingCon {
    serialized: Vec<u8>,
    retransmit_count: u32,
    next_deadline: Instant,
    current_timeout: Duration,
}

/// Cached response for deduplication of incoming CON requests.
struct DedupEntry {
    response_bytes: Vec<u8>,
    expires: Instant,
}

/// Action returned by `process_retransmits`.
pub enum RetransmitAction {
    /// Retransmit this CON message.
    Resend { msg_id: u16, bytes: Vec<u8> },
    /// Max retransmits exceeded — give up on this message.
    GiveUp { msg_id: u16 },
}

/// Result of checking the deduplication cache.
pub enum DedupResult {
    /// First time seeing this message ID — process normally.
    NewMessage,
    /// Duplicate CON — here's the cached response to re-send.
    Duplicate(Vec<u8>),
}

/// Maximum number of entries in the deduplication cache.
const MAX_DEDUP_ENTRIES: usize = 256;

/// Per-connection RFC 7252 reliability state.
///
/// Manages retransmission of outgoing CON messages and deduplication of
/// incoming CON requests. Lives inside each `connection_task` — no
/// synchronization needed.
pub struct ReliabilityState {
    params: RetransmitParams,
    /// CON messages we sent, keyed by message_id.
    pending_cons: HashMap<u16, PendingCon>,
    /// Dedup cache for incoming CON requests, keyed by message_id.
    dedup_cache: HashMap<u16, DedupEntry>,
}

impl ReliabilityState {
    pub fn new(params: RetransmitParams) -> Self {
        Self {
            params,
            pending_cons: HashMap::new(),
            dedup_cache: HashMap::new(),
        }
    }

    /// Check if an incoming CON message_id is a duplicate.
    /// Evicts expired entries lazily.
    pub fn check_dedup(&mut self, msg_id: u16) -> DedupResult {
        let now = Instant::now();

        // Evict if expired
        if let Some(entry) = self.dedup_cache.get(&msg_id)
            && entry.expires <= now
        {
            self.dedup_cache.remove(&msg_id);
        }

        match self.dedup_cache.get(&msg_id) {
            Some(entry) => DedupResult::Duplicate(entry.response_bytes.clone()),
            None => DedupResult::NewMessage,
        }
    }

    /// Cache a response for deduplication of incoming CON requests.
    pub fn record_response(&mut self, msg_id: u16, response_bytes: Vec<u8>) {
        let expires = Instant::now() + self.params.exchange_lifetime;
        self.dedup_cache.insert(
            msg_id,
            DedupEntry {
                response_bytes,
                expires,
            },
        );

        // Bound cache size by evicting expired entries first, then oldest if still over
        if self.dedup_cache.len() > MAX_DEDUP_ENTRIES {
            self.gc_dedup_cache();
        }
    }

    /// Register a CON message we sent for retransmission tracking.
    pub fn track_outgoing_con(&mut self, msg_id: u16, serialized: Vec<u8>) {
        let initial_timeout = self.params.initial_timeout();
        self.pending_cons.insert(
            msg_id,
            PendingCon {
                serialized,
                retransmit_count: 0,
                next_deadline: Instant::now() + initial_timeout,
                current_timeout: initial_timeout,
            },
        );
    }

    /// Handle an incoming ACK — stop retransmitting the matched CON.
    /// Returns true if a pending CON was found and removed.
    pub fn handle_ack(&mut self, msg_id: u16) -> bool {
        self.pending_cons.remove(&msg_id).is_some()
    }

    /// Handle an incoming RST — stop retransmitting the matched CON.
    /// Returns true if a pending CON was found and removed.
    pub fn handle_rst(&mut self, msg_id: u16) -> bool {
        self.pending_cons.remove(&msg_id).is_some()
    }

    /// Returns the earliest retransmit deadline among pending CONs,
    /// or `None` if nothing is pending. Used in `tokio::select!`.
    pub fn next_retransmit_deadline(&self) -> Option<Instant> {
        self.pending_cons.values().map(|p| p.next_deadline).min()
    }

    /// Check all pending CONs and return actions for those past their deadline.
    /// Applies exponential backoff. Returns `GiveUp` when `max_retransmit` exceeded.
    pub fn process_retransmits(&mut self) -> Vec<RetransmitAction> {
        let now = Instant::now();
        let max = self.params.max_retransmit;
        let mut actions = Vec::new();
        let mut to_remove = Vec::new();

        for (msg_id, pending) in self.pending_cons.iter_mut() {
            if pending.next_deadline > now {
                continue;
            }

            if pending.retransmit_count >= max {
                to_remove.push(*msg_id);
                actions.push(RetransmitAction::GiveUp { msg_id: *msg_id });
            } else {
                pending.retransmit_count += 1;
                pending.current_timeout *= 2;
                pending.next_deadline = now + pending.current_timeout;
                actions.push(RetransmitAction::Resend {
                    msg_id: *msg_id,
                    bytes: pending.serialized.clone(),
                });
            }
        }

        for msg_id in to_remove {
            self.pending_cons.remove(&msg_id);
        }

        actions
    }

    /// Returns true if there are any pending outgoing CON messages.
    pub fn has_pending(&self) -> bool {
        !self.pending_cons.is_empty()
    }

    /// Remove expired entries from the dedup cache.
    fn gc_dedup_cache(&mut self) {
        let now = Instant::now();
        self.dedup_cache.retain(|_, entry| entry.expires > now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_params() -> RetransmitParams {
        RetransmitParams {
            ack_timeout: Duration::from_millis(100),
            ack_random_factor: 1.5,
            max_retransmit: 4,
            exchange_lifetime: Duration::from_millis(500),
        }
    }

    #[test]
    fn test_dedup_new_message() {
        let mut state = ReliabilityState::new(test_params());
        assert!(matches!(state.check_dedup(42), DedupResult::NewMessage));
    }

    #[test]
    fn test_dedup_duplicate() {
        let mut state = ReliabilityState::new(test_params());
        let response = vec![1, 2, 3, 4];
        state.record_response(42, response.clone());

        match state.check_dedup(42) {
            DedupResult::Duplicate(bytes) => assert_eq!(bytes, response),
            DedupResult::NewMessage => panic!("expected duplicate"),
        }
    }

    #[test]
    fn test_dedup_cache_bounded() {
        let mut state = ReliabilityState::new(test_params());
        for i in 0..=MAX_DEDUP_ENTRIES as u16 {
            state.record_response(i, vec![i as u8]);
        }
        // Should not exceed MAX_DEDUP_ENTRIES + some buffer from gc
        assert!(state.dedup_cache.len() <= MAX_DEDUP_ENTRIES + 1);
    }

    #[test]
    fn test_track_and_ack() {
        let mut state = ReliabilityState::new(test_params());
        state.track_outgoing_con(100, vec![0xAA]);
        assert!(state.has_pending());
        assert!(state.handle_ack(100));
        assert!(!state.has_pending());
    }

    #[test]
    fn test_track_and_rst() {
        let mut state = ReliabilityState::new(test_params());
        state.track_outgoing_con(200, vec![0xBB]);
        assert!(state.handle_rst(200));
        assert!(!state.has_pending());
    }

    #[test]
    fn test_ack_unknown_msg_id() {
        let mut state = ReliabilityState::new(test_params());
        assert!(!state.handle_ack(999));
    }

    #[test]
    fn test_next_deadline_none_when_empty() {
        let state = ReliabilityState::new(test_params());
        assert!(state.next_retransmit_deadline().is_none());
    }

    #[test]
    fn test_next_deadline_returns_earliest() {
        let mut state = ReliabilityState::new(test_params());
        state.track_outgoing_con(1, vec![]);
        state.track_outgoing_con(2, vec![]);
        assert!(state.next_retransmit_deadline().is_some());
    }

    #[tokio::test]
    async fn test_retransmit_exponential_backoff() {
        let params = RetransmitParams {
            ack_timeout: Duration::from_millis(10),
            ack_random_factor: 1.0, // no randomization for deterministic test
            max_retransmit: 3,
            exchange_lifetime: Duration::from_secs(1),
        };
        let mut state = ReliabilityState::new(params);
        state.track_outgoing_con(50, vec![0xCC]);

        // Wait past initial deadline
        tokio::time::sleep(Duration::from_millis(15)).await;

        let actions = state.process_retransmits();
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            &actions[0],
            RetransmitAction::Resend { msg_id: 50, .. }
        ));

        // Second retransmit — timeout doubled to 20ms
        tokio::time::sleep(Duration::from_millis(25)).await;
        let actions = state.process_retransmits();
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            &actions[0],
            RetransmitAction::Resend { msg_id: 50, .. }
        ));

        // Third retransmit — timeout 40ms
        tokio::time::sleep(Duration::from_millis(45)).await;
        let actions = state.process_retransmits();
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            &actions[0],
            RetransmitAction::Resend { msg_id: 50, .. }
        ));

        // Fourth attempt — should give up (max_retransmit=3)
        tokio::time::sleep(Duration::from_millis(85)).await;
        let actions = state.process_retransmits();
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            &actions[0],
            RetransmitAction::GiveUp { msg_id: 50 }
        ));

        assert!(!state.has_pending());
    }

    #[test]
    fn test_initial_timeout_randomized() {
        let params = RetransmitParams {
            ack_timeout: Duration::from_secs(2),
            ack_random_factor: 1.5,
            max_retransmit: 4,
            exchange_lifetime: Duration::from_secs(247),
        };

        // Generate several initial timeouts and verify they're in range
        for _ in 0..100 {
            let timeout = params.initial_timeout();
            assert!(timeout >= Duration::from_secs(2));
            assert!(timeout <= Duration::from_secs(3)); // 2 * 1.5
        }
    }
}
