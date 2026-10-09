//! Explicit opt-in worker analytics. No endpoint, identity, clock, background
//! thread, storage or network activity is chosen by this module.
use std::collections::{BTreeMap, VecDeque};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalyticsError {
    InvalidEvent,
    InvalidLimits,
    Capacity,
}
impl std::fmt::Display for AnalyticsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "analytics error: {self:?}")
    }
}
impl std::error::Error for AnalyticsError {}
fn name_valid(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
/// Validated numeric event, at most 64 unique named metrics and 4,096 encoded
/// bytes. Caller supplies event name and Unix milliseconds; no identity or
/// automatic timestamp is attached. Names are 1..64 ASCII alphanumeric/_/-.
/// Metric values must be finite. Construction/encoding allocate outside audio.
pub struct AnalyticsEvent {
    encoded: Vec<u8>,
}
impl AnalyticsEvent {
    pub fn new(
        name: &str,
        unix_milliseconds: u64,
        metrics: &[(&str, f64)],
    ) -> Result<Self, AnalyticsError> {
        if !name_valid(name) || metrics.len() > 64 {
            return Err(AnalyticsError::InvalidEvent);
        }
        let mut values = BTreeMap::new();
        for &(key, value) in metrics {
            if !name_valid(key) || !value.is_finite() || values.insert(key, value).is_some() {
                return Err(AnalyticsError::InvalidEvent);
            }
        }
        let record =
            serde_json::json!({"name": name, "time_ms": unix_milliseconds, "metrics": values});
        let encoded = serde_json::to_vec(&record).map_err(|_| AnalyticsError::InvalidEvent)?;
        if encoded.len() > 4096 {
            return Err(AnalyticsError::InvalidEvent);
        }
        Ok(Self { encoded })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordOutcome {
    Disabled,
    Queued,
}
/// In-memory opt-in queue, disabled initially. Limits: 1..1,024 events and
/// 13..1,048,576 bytes for the complete JSON batch. Overflow rejects the newest
/// event without changing accepted events. Disabling discards all queued data.
/// Enabling must be an explicit consumer action after its own consent decision;
/// RAVET supplies no user-consent UI, collection policy or product endpoint.
///
/// Only explicit `flush` calls the consumer's sender, synchronously on its
/// worker thread. Failed sends retain the complete batch for an explicit retry;
/// successful sends remove it. Delivery is at-least-once under response loss,
/// not exactly-once. This module never retries, persists or schedules itself.
/// All methods/drop allocate/deallocate or may call blocking consumer code and
/// must stay outside audio callbacks. Exclusive ownership serializes revocation
/// against flush; it cannot retract data already accepted by a sender.
pub struct AnalyticsQueue {
    enabled: bool,
    events: VecDeque<AnalyticsEvent>,
    maximum_events: usize,
    maximum_bytes: usize,
    event_bytes: usize,
}
impl AnalyticsQueue {
    pub fn new(maximum_events: usize, maximum_bytes: usize) -> Result<Self, AnalyticsError> {
        if !(1..=1024).contains(&maximum_events) || !(13..=1_048_576).contains(&maximum_bytes) {
            return Err(AnalyticsError::InvalidLimits);
        }
        Ok(Self {
            enabled: false,
            events: VecDeque::new(),
            maximum_events,
            maximum_bytes,
            event_bytes: 0,
        })
    }
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
    pub fn pending(&self) -> usize {
        self.events.len()
    }
    /// Explicit consumer opt-in; false also erases pending events immediately.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.events.clear();
            self.event_bytes = 0;
        }
    }
    pub fn record(&mut self, event: AnalyticsEvent) -> Result<RecordOutcome, AnalyticsError> {
        if !self.enabled {
            return Ok(RecordOutcome::Disabled);
        }
        let next_bytes = self.event_bytes + event.encoded.len();
        // Complete envelope is 13 bytes plus one comma between each event.
        if self.events.len() == self.maximum_events
            || next_bytes + 13 + self.events.len() > self.maximum_bytes
        {
            return Err(AnalyticsError::Capacity);
        }
        self.event_bytes = next_bytes;
        self.events.push_back(event);
        Ok(RecordOutcome::Queued)
    }
    /// No sender invocation when disabled or empty. Sender chooses destination,
    /// credentials, timeout and what counts as acceptance (e.g. HTTPS 2xx only).
    /// A sender error leaves all pending events unchanged; a panic also retains
    /// them during unwinding. The batch contains only validated numeric events.
    pub fn flush<E>(&mut self, sender: impl FnOnce(&[u8]) -> Result<(), E>) -> Result<usize, E> {
        if !self.enabled || self.events.is_empty() {
            return Ok(0);
        }
        let count = self.events.len();
        let mut body = Vec::with_capacity(self.event_bytes + 13 + count - 1);
        body.extend_from_slice(b"{\"events\":[");
        for (index, event) in self.events.iter().enumerate() {
            if index != 0 {
                body.push(b',');
            }
            body.extend_from_slice(&event.encoded);
        }
        body.extend_from_slice(b"]}");
        sender(&body)?;
        self.events.clear();
        self.event_bytes = 0;
        Ok(count)
    }
}
