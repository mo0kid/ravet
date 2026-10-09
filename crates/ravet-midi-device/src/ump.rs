//! Prepared UMP packet ownership handoff, independent of native transport clocks.
use crate::MidiInputStatistics;
use ravet_foundation::UmpPacket;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
/// Explicit transport protocol label; framing alone does not prove semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UmpProtocol {
    Midi1,
    Midi2,
}
/// One complete packet and an opaque timestamp in the producer's declared clock.
/// This does not interpret timestamps as microseconds or audio sample positions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimedUmp {
    pub timestamp: u64,
    pub protocol: UmpProtocol,
    pub packet: UmpPacket,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UmpInputError {
    Capacity,
    Malformed,
    Full,
    ReceiverGone,
}
#[derive(Default)]
struct Statistics {
    accepted: AtomicU64,
    dropped: AtomicU64,
    invalid: AtomicU64,
}
impl Statistics {
    fn snapshot(&self) -> MidiInputStatistics {
        MidiInputStatistics {
            accepted: self.accepted.load(Ordering::Relaxed),
            dropped_full: self.dropped.load(Ordering::Relaxed),
            invalid: self.invalid.load(Ordering::Relaxed),
        }
    }
}
/// Generated batch delivery prefix. A refused suffix is discarded, never replayed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UmpDelivery {
    pub accepted: usize,
    pub discarded: usize,
    pub reason: Option<UmpInputError>,
}
/// Unique prepared producer. Construction/drop belong outside callbacks.
/// Capacity is 1..4096 packets. No allocation/locks/I/O on push; its native
/// producer callback must obey single-producer ownership. No deadline guarantee.
pub struct PreparedUmpInput {
    producer: rtrb::Producer<TimedUmp>,
    statistics: Arc<Statistics>,
}
/// Unique consumer; pop is nonblocking and allocates nothing. Its endpoint and
/// queued storage must be destroyed outside callbacks, as with the producer.
pub struct UmpReceiver {
    consumer: rtrb::Consumer<TimedUmp>,
    statistics: Arc<Statistics>,
}
impl PreparedUmpInput {
    pub fn new(capacity: usize) -> Result<(Self, UmpReceiver), UmpInputError> {
        if !(1..=4096).contains(&capacity) {
            return Err(UmpInputError::Capacity);
        }
        let (producer, consumer) = rtrb::RingBuffer::new(capacity);
        let statistics = Arc::new(Statistics::default());
        Ok((
            Self {
                producer,
                statistics: Arc::clone(&statistics),
            },
            UmpReceiver {
                consumer,
                statistics,
            },
        ))
    }
    pub fn statistics(&self) -> MidiInputStatistics {
        self.statistics.snapshot()
    }
    /// Copies one framed packet. Accepted counts publication, not consumption;
    /// receiver abandonment can race the initial check. Rejected records remain
    /// caller-owned because TimedUmp is Copy. Counters wrap/coalesce relaxedly.
    pub fn try_push(&mut self, record: TimedUmp) -> Result<(), UmpInputError> {
        if self.producer.is_abandoned() {
            return Err(UmpInputError::ReceiverGone);
        }
        match self.producer.push(record) {
            Ok(()) => {
                self.statistics.accepted.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(_) => {
                self.statistics.dropped.fetch_add(1, Ordering::Relaxed);
                Err(UmpInputError::Full)
            }
        }
    }
    /// Preflight <=4096 numeric words at a known packet boundary. A malformed
    /// or oversized whole batch publishes nothing. All packets retain the supplied
    /// timestamp/protocol label. Framing accepts opaque/reserved message types;
    /// semantic/protocol conformance belongs to the native producer/consumer.
    /// Full/abandoned queues retain the accepted prefix and discard the suffix;
    /// dropped_full counts packets refused because full, invalid counts batches.
    pub fn push_words(
        &mut self,
        timestamp: u64,
        protocol: UmpProtocol,
        words: &[u32],
    ) -> Result<UmpDelivery, UmpInputError> {
        let validation = (|| {
            if words.len() > 4096 {
                return Err(UmpInputError::Capacity);
            }
            let mut remaining = words;
            let mut count = 0;
            while let Some(&first) = remaining.first() {
                let size = UmpPacket::word_count(first);
                if remaining.len() < size {
                    return Err(UmpInputError::Malformed);
                }
                remaining = &remaining[size..];
                count += 1;
            }
            Ok(count)
        })();
        let count = match validation {
            Ok(count) => count,
            Err(error) => {
                self.statistics.invalid.fetch_add(1, Ordering::Relaxed);
                return Err(error);
            }
        };
        let mut remaining = words;
        let mut accepted = 0;
        while let Some(&first) = remaining.first() {
            let size = UmpPacket::word_count(first);
            let packet = UmpPacket::from_words(&remaining[..size]).expect("preflighted framing");
            match self.try_push(TimedUmp {
                timestamp,
                protocol,
                packet,
            }) {
                Ok(()) => {
                    accepted += 1;
                    remaining = &remaining[size..];
                }
                Err(reason) => {
                    if reason == UmpInputError::Full {
                        self.statistics
                            .dropped
                            .fetch_add((count - accepted - 1) as u64, Ordering::Relaxed);
                    }
                    return Ok(UmpDelivery {
                        accepted,
                        discarded: count - accepted,
                        reason: Some(reason),
                    });
                }
            }
        }
        Ok(UmpDelivery {
            accepted,
            discarded: 0,
            reason: None,
        })
    }
}
impl UmpReceiver {
    pub fn pop(&mut self) -> Option<TimedUmp> {
        self.consumer.pop().ok()
    }
    pub fn statistics(&self) -> MidiInputStatistics {
        self.statistics.snapshot()
    }
    /// True once the producer is gone. Already-published packets remain drainable.
    pub fn producer_gone(&self) -> bool {
        self.consumer.is_abandoned()
    }
}
