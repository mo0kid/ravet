//! Bounded coherent word snapshots for control/audio communication.
//!
//! Callers encode their own finite values, IDs, and metadata into words. Every
//! read or publication makes one attempt; contention returns immediately. An
//! audio reader retains its last accepted snapshot when a read is unavailable.
//! The mailbox does not queue events or guarantee that every publication is seen.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::SeqCst};

/// A coherent snapshot and its publication generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WordSnapshot<const N: usize> {
    pub words: [u32; N],
    /// Even sequence number; wraps after 2^63 publications.
    pub generation: u64,
}

/// A publication failed without changing any word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublishError {
    /// Another publisher currently owns the mailbox, or won the same attempt.
    Busy,
    /// The expected generation no longer represents the current snapshot.
    Stale,
}

/// Fixed-size, allocation-free coherent snapshots with bounded attempts.
///
/// Available on targets with 64-bit atomics. The generation must not wrap during
/// a single read or between reading a snapshot and publishing conditionally.
/// Atomic words and sequential consistency avoid unsynchronized shared storage.
/// Successful writers hold a sequence reservation for N stores, without waiting
/// on readers. A preempted writer makes reads unavailable until it completes;
/// callers must never spin in an audio callback waiting for a snapshot.
///
/// ```
/// use ravet_foundation::{SnapshotMailbox, PublishError};
/// let mailbox = SnapshotMailbox::new([1.0_f32.to_bits(), 0]);
/// let before = mailbox.try_read().unwrap();
/// mailbox.try_publish([0.5_f32.to_bits(), 1]).unwrap();
/// assert_eq!(mailbox.try_publish_if(before.generation, before.words), Err(PublishError::Stale));
/// assert_eq!(f32::from_bits(mailbox.try_read().unwrap().words[0]), 0.5);
/// ```
pub struct SnapshotMailbox<const N: usize> {
    sequence: AtomicU64,
    words: [AtomicU32; N],
}

impl<const N: usize> SnapshotMailbox<N> {
    pub fn new(words: [u32; N]) -> Self {
        Self {
            sequence: AtomicU64::new(0),
            words: words.map(AtomicU32::new),
        }
    }

    /// Return one coherent snapshot or None when a publication overlaps it.
    pub fn try_read(&self) -> Option<WordSnapshot<N>> {
        let before = self.sequence.load(SeqCst);
        if before & 1 != 0 {
            return None;
        }
        let words = std::array::from_fn(|i| self.words[i].load(SeqCst));
        (before == self.sequence.load(SeqCst)).then_some(WordSnapshot {
            words,
            generation: before,
        })
    }

    /// Publish a complete snapshot, failing immediately if a writer overlaps.
    pub fn try_publish(&self, words: [u32; N]) -> Result<u64, PublishError> {
        let expected = self.sequence.load(SeqCst);
        self.try_publish_if(expected, words)
    }

    /// Publish only if no newer snapshot has superseded the caller's generation.
    /// Audio-side reflection uses this to avoid overwriting a concurrent restore.
    pub fn try_publish_if(&self, expected: u64, words: [u32; N]) -> Result<u64, PublishError> {
        let current = self.sequence.load(SeqCst);
        if current & 1 != 0 {
            return Err(PublishError::Busy);
        }
        if expected != current {
            return Err(PublishError::Stale);
        }
        self.sequence
            .compare_exchange(expected, expected.wrapping_add(1), SeqCst, SeqCst)
            .map_err(|_| PublishError::Busy)?;
        for (target, word) in self.words.iter().zip(words) {
            target.store(word, SeqCst);
        }
        let next = expected.wrapping_add(2);
        self.sequence.store(next, SeqCst);
        Ok(next)
    }
}
