//! Double-precision quadrature rotation for caller-owned oscillator banks.

/// A precomputed rotation through a finite fraction of one cycle.
///
/// Compute a new rotation when the frequency changes, outside the sample loop.
/// [`Self::rotate`] and [`Self::rotate_bank`] use only arithmetic and do not
/// allocate. They preserve the chosen increment without frequency quantization.
/// Floating-point errors accumulate with repeated rotations: callers should
/// periodically reanchor sine/cosine values to their independently tracked phase.
/// This is not a phase accumulator or a bit-exact replacement for `sin_cos`.
#[derive(Clone, Copy, Debug)]
pub struct PhaseRotation {
    sine: f64,
    cosine: f64,
}

impl PhaseRotation {
    /// Prepare a signed increment in cycles. Reject nonfinite increments and
    /// values outside `-1..=1` without creating a rotation.
    pub fn new(cycles: f64) -> Option<Self> {
        if !cycles.is_finite() || !(-1.0..=1.0).contains(&cycles) {
            return None;
        }
        let (sine, cosine) = (cycles * std::f64::consts::TAU).sin_cos();
        Some(Self { sine, cosine })
    }

    /// Rotate a caller-owned `(sine, cosine)` pair by the prepared increment.
    /// Nonfinite oscillator values propagate through the arithmetic. AArch64
    /// and targets with FMA use fused multiply-add for the final sums; other
    /// targets use separate operations, so rounding can differ by target.
    #[inline]
    pub fn rotate(self, sine: f64, cosine: f64) -> (f64, f64) {
        #[cfg(any(target_arch = "aarch64", target_feature = "fma"))]
        return (
            sine.mul_add(self.cosine, cosine * self.sine),
            cosine.mul_add(self.cosine, -(sine * self.sine)),
        );
        #[cfg(not(any(target_arch = "aarch64", target_feature = "fma")))]
        (
            sine * self.cosine + cosine * self.sine,
            cosine * self.cosine - sine * self.sine,
        )
    }

    /// Rotate equally sized fixed banks, retaining independent lane state.
    /// Separate arrays allow a compiler to vectorize without changing the
    /// arithmetic order within each lane. An empty bank is legal.
    #[inline]
    pub fn rotate_bank<const N: usize>(self, sines: &mut [f64; N], cosines: &mut [f64; N]) {
        for (sine, cosine) in sines.iter_mut().zip(cosines) {
            (*sine, *cosine) = self.rotate(*sine, *cosine);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_increments_are_rejected_and_empty_bank_is_legal() {
        for increment in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.01, 1.01] {
            assert!(PhaseRotation::new(increment).is_none());
        }
        PhaseRotation::new(0.0)
            .unwrap()
            .rotate_bank(&mut [], &mut []);
        assert_eq!(
            PhaseRotation::new(0.0).unwrap().rotate(0.0, 1.0),
            (0.0, 1.0)
        );
    }

    #[test]
    fn signed_rotations_track_phase_between_periodic_reanchors() {
        for increment in [-0.49, -0.003, -1.0 / 96_000.0, 0.0, 5.0 / 8_000.0, 0.49] {
            let rotation = PhaseRotation::new(increment).unwrap();
            let mut phases: [f64; 16] = std::array::from_fn(|i| i as f64 / 16.0);
            let mut sines = [0.0; 16];
            let mut cosines = [0.0; 16];
            for sample in 0..8192 {
                if sample % 128 == 0 {
                    for i in 0..16 {
                        (sines[i], cosines[i]) = (phases[i] * std::f64::consts::TAU).sin_cos();
                    }
                }
                for i in 0..16 {
                    let (sine, cosine) = (phases[i] * std::f64::consts::TAU).sin_cos();
                    assert!((sines[i] - sine).abs() < 4e-13);
                    assert!((cosines[i] - cosine).abs() < 4e-13);
                    assert!((sines[i] * sines[i] + cosines[i] * cosines[i] - 1.0).abs() < 4e-14);
                    phases[i] += increment;
                    if phases[i] >= 1.0 {
                        phases[i] -= 1.0;
                    }
                    if phases[i] < 0.0 {
                        phases[i] += 1.0;
                    }
                }
                rotation.rotate_bank(&mut sines, &mut cosines);
            }
        }
    }
}
