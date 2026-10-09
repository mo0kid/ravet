//! One lock-free word makes requested gain, bypass and restore generation coherent.
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) struct SharedGain(AtomicU64);
impl SharedGain {
    pub fn new() -> Self {
        Self(AtomicU64::new(1.0_f32.to_bits() as u64))
    }
    pub fn snapshot(&self) -> u64 {
        self.0.load(Ordering::Acquire)
    }
    pub fn gain(snapshot: u64) -> f32 {
        f32::from_bits(snapshot as u32)
    }
    pub fn generation(snapshot: u64) -> u32 {
        (snapshot >> 33) as u32
    }
    /// Control thread may retry. A new generation is published even for the
    /// same value: restoring that value must still reset smoothing.
    pub fn restore(&self, gain: f32, bypassed: bool) {
        let _ = self
            .0
            .try_update(Ordering::AcqRel, Ordering::Acquire, |old| {
                Some(Self::pack(
                    Self::generation(old).wrapping_add(1),
                    gain,
                    bypassed,
                ))
            });
    }
    pub fn bypassed(snapshot: u64) -> bool {
        snapshot & (1 << 32) != 0
    }
    fn pack(generation: u32, gain: f32, bypassed: bool) -> u64 {
        (((generation & 0x7fff_ffff) as u64) << 33)
            | ((bypassed as u64) << 32)
            | gain.to_bits() as u64
    }
    /// Audio thread makes one attempt, never a retry loop. A concurrent restore
    /// wins over automation from a callback that started before that restore.
    pub fn reflect(&self, snapshot: u64, gain: f32, bypassed: bool) -> u64 {
        let next = Self::pack(Self::generation(snapshot), gain, bypassed);
        let _ = self
            .0
            .compare_exchange(snapshot, next, Ordering::AcqRel, Ordering::Acquire);
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restore_wins_over_stale_callback_even_when_values_repeat() {
        let shared = SharedGain::new();
        let started = shared.snapshot();
        shared.restore(0.5, true);
        shared.restore(1.0, true);
        shared.reflect(started, 3.0, false);
        assert_eq!(SharedGain::gain(shared.snapshot()), 1.0);
        assert_eq!(SharedGain::generation(shared.snapshot()), 2);
        assert!(SharedGain::bypassed(shared.snapshot()));
    }
    #[test]
    fn automation_does_not_change_restore_generation() {
        let shared = SharedGain::new();
        shared.restore(0.25, false);
        let started = shared.snapshot();
        shared.reflect(started, 2.0, true);
        assert_eq!(SharedGain::gain(shared.snapshot()), 2.0);
        assert_eq!(SharedGain::generation(shared.snapshot()), 1);
    }
}
