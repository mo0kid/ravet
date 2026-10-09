/// Invalid smoothing input. Failed updates preserve the previous state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmoothingError {
    NonFiniteValue,
    /// Multiplicative smoothing requires strictly positive endpoints.
    NonPositiveValue,
    InvalidTime,
    InvalidSampleRate,
    DurationTooLong,
}

impl std::fmt::Display for SmoothingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NonFiniteValue => "smoothing values must be finite",
            Self::NonPositiveValue => "multiplicative smoothing values must be positive",
            Self::InvalidTime => "smoothing time must be finite and nonnegative",
            Self::InvalidSampleRate => "sample rate must be finite and positive",
            Self::DurationTooLong => "smoothing duration exceeds u32 sample capacity",
        })
    }
}
impl std::error::Error for SmoothingError {}

/// Convert milliseconds to the nearest sample, with ties rounded upward.
/// Zero duration is immediate. Reject overflow instead of silently saturating.
pub fn smoothing_samples(milliseconds: f64, sample_rate: f64) -> Result<u32, SmoothingError> {
    if !milliseconds.is_finite() || milliseconds < 0.0 {
        return Err(SmoothingError::InvalidTime);
    }
    if !sample_rate.is_finite() || sample_rate <= 0.0 {
        return Err(SmoothingError::InvalidSampleRate);
    }
    let samples = (milliseconds * (sample_rate / 1000.0)).round();
    if !samples.is_finite() || samples > u32::MAX as f64 {
        return Err(SmoothingError::DurationTooLong);
    }
    Ok(samples as u32)
}

/// A linear transition lasting exactly the configured number of samples.
///
/// After retargeting, the first [`Self::next_value`] advances by 1/N of the
/// distance and the Nth reaches the exact target. Zero samples means immediate.
/// A changed target starts from the current effective value. An identical
/// target does not restart an existing ramp. Sample progress makes output
/// independent of block boundaries. Interpolation uses f64 to keep finite f32
/// endpoints from overflowing intermediate arithmetic. This type allocates
/// nothing and performs bounded work per call.
#[derive(Clone, Copy, Debug)]
pub struct LinearSmoother {
    current: f32,
    start: f64,
    target: f32,
    ramp_samples: u32,
    elapsed: u32,
}

impl LinearSmoother {
    pub fn new(initial: f32, ramp_samples: u32) -> Result<Self, SmoothingError> {
        if !initial.is_finite() {
            return Err(SmoothingError::NonFiniteValue);
        }
        Ok(Self {
            current: initial,
            start: initial as f64,
            target: initial,
            ramp_samples,
            elapsed: ramp_samples,
        })
    }
    pub fn current(&self) -> f32 {
        self.current
    }
    pub fn target(&self) -> f32 {
        self.target
    }
    pub fn is_smoothing(&self) -> bool {
        self.elapsed < self.ramp_samples
    }

    pub fn set_target(&mut self, target: f32) -> Result<(), SmoothingError> {
        if !target.is_finite() {
            return Err(SmoothingError::NonFiniteValue);
        }
        if self.target == target {
            return Ok(());
        }
        self.start = self.current as f64;
        self.target = target;
        self.elapsed = 0;
        if self.ramp_samples == 0 {
            self.current = target;
        }
        Ok(())
    }

    pub fn next_value(&mut self) -> f32 {
        if self.elapsed < self.ramp_samples {
            self.elapsed += 1;
            if self.elapsed == self.ramp_samples {
                self.current = self.target;
            } else {
                let fraction = self.elapsed as f64 / self.ramp_samples as f64;
                self.current = (self.start + (self.target as f64 - self.start) * fraction) as f32;
            }
        }
        self.current
    }

    /// Snap to the latest target without changing the configured duration.
    pub fn reset(&mut self) {
        self.current = self.target;
        self.start = self.target as f64;
        self.elapsed = self.ramp_samples;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_ramp_and_mid_ramp_retarget() {
        let mut smoother = LinearSmoother::new(0.0, 4).unwrap();
        smoother.set_target(1.0).unwrap();
        assert_eq!(smoother.next_value(), 0.25);
        assert_eq!(smoother.next_value(), 0.5);
        smoother.set_target(0.0).unwrap();
        assert_eq!(
            std::array::from_fn::<_, 4, _>(|_| smoother.next_value()),
            [0.375, 0.25, 0.125, 0.0]
        );
        assert!(!smoother.is_smoothing());
        assert_eq!(smoother.next_value(), 0.0);
    }

    #[test]
    fn repeated_target_does_not_restart_and_reset_snaps() {
        let mut smoother = LinearSmoother::new(1.0, 4).unwrap();
        smoother.set_target(0.0).unwrap();
        assert_eq!(smoother.next_value(), 0.75);
        smoother.set_target(0.0).unwrap();
        assert_eq!(smoother.next_value(), 0.5);
        smoother.reset();
        assert_eq!(smoother.next_value(), 0.0);
        assert!(!smoother.is_smoothing());
    }

    #[test]
    fn immediate_and_one_sample_ramps() {
        for duration in [0, 1] {
            let mut smoother = LinearSmoother::new(1.0, duration).unwrap();
            smoother.set_target(2.0).unwrap();
            assert_eq!(smoother.next_value(), 2.0);
            assert!(!smoother.is_smoothing());
        }
    }

    #[test]
    fn finite_extremes_and_invalid_targets() {
        let mut smoother = LinearSmoother::new(-f32::MAX, 2).unwrap();
        smoother.set_target(f32::MAX).unwrap();
        assert_eq!(smoother.next_value(), 0.0);
        assert_eq!(
            smoother.set_target(f32::NAN),
            Err(SmoothingError::NonFiniteValue)
        );
        assert_eq!(smoother.next_value(), f32::MAX);
        assert!(LinearSmoother::new(f32::INFINITY, 2).is_err());
    }

    #[test]
    fn time_conversion_checks_rates_rounding_and_overflow() {
        assert_eq!(smoothing_samples(5.0, 48_000.0), Ok(240));
        assert_eq!(smoothing_samples(5.0, 96_000.0), Ok(480));
        assert_eq!(smoothing_samples(0.5, 1000.0), Ok(1));
        assert_eq!(smoothing_samples(0.0, 48_000.0), Ok(0));
        for rate in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                smoothing_samples(5.0, rate),
                Err(SmoothingError::InvalidSampleRate)
            );
        }
        for time in [-1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                smoothing_samples(time, 48_000.0),
                Err(SmoothingError::InvalidTime)
            );
        }
        assert_eq!(
            smoothing_samples(f64::MAX, 48_000.0),
            Err(SmoothingError::DurationTooLong)
        );
    }
}

/// Positive f32 endpoints transition geometrically over an exact sample count.
/// First output advances one of N multiplicative steps; the Nth snaps exactly to
/// target. Zero duration is immediate. Identical targets retain an active ramp;
/// changed targets restart from the current effective f32 value. Zero/negative
/// endpoints reject without mutation (use a linear smoother for silence/crossing).
/// Ratio design uses f64 logs/exp on retarget; sample processing uses one f64
/// multiply and bounded endpoint clipping. Rounding can repeat output values,
/// especially for very long ramps; no hard real-time deadline claim is inferred.
/// Construction, retarget, sample processing and reset allocate nothing.
#[derive(Clone, Copy, Debug)]
pub struct MultiplicativeSmoother {
    current: f32,
    effective: f64,
    target: f32,
    ratio: f64,
    ramp_samples: u32,
    elapsed: u32,
}
impl MultiplicativeSmoother {
    pub fn new(initial: f32, ramp_samples: u32) -> Result<Self, SmoothingError> {
        positive(initial)?;
        Ok(Self {
            current: initial,
            effective: f64::from(initial),
            target: initial,
            ratio: 1.,
            ramp_samples,
            elapsed: ramp_samples,
        })
    }
    pub fn current(&self) -> f32 {
        self.current
    }
    pub fn target(&self) -> f32 {
        self.target
    }
    pub fn is_smoothing(&self) -> bool {
        self.elapsed < self.ramp_samples
    }
    /// Validate before mutation; changed target preserves the current output.
    pub fn set_target(&mut self, target: f32) -> Result<(), SmoothingError> {
        positive(target)?;
        if target == self.target {
            return Ok(());
        }
        self.effective = f64::from(self.current);
        self.target = target;
        self.elapsed = 0;
        if self.ramp_samples == 0 {
            self.current = target;
            self.effective = f64::from(target);
            self.ratio = 1.;
        } else {
            // Log difference avoids overflow in target/current division in f32.
            self.ratio = ((f64::from(target).ln() - self.effective.ln())
                / f64::from(self.ramp_samples))
            .exp();
        }
        Ok(())
    }
    pub fn next_value(&mut self) -> f32 {
        if self.elapsed < self.ramp_samples {
            self.elapsed += 1;
            if self.elapsed == self.ramp_samples {
                self.effective = f64::from(self.target);
                self.current = self.target;
            } else {
                self.effective *= self.ratio;
                let target = f64::from(self.target);
                // Prevent accumulated rounding from overshooting the endpoint.
                self.effective = if self.target >= self.current {
                    self.effective.min(target)
                } else {
                    self.effective.max(target)
                };
                self.current = self.effective as f32;
            }
        }
        self.current
    }
    /// Snap to the latest target while retaining configured duration.
    pub fn reset(&mut self) {
        self.current = self.target;
        self.effective = f64::from(self.target);
        self.ratio = 1.;
        self.elapsed = self.ramp_samples;
    }
}
fn positive(value: f32) -> Result<(), SmoothingError> {
    if !value.is_finite() {
        Err(SmoothingError::NonFiniteValue)
    } else if value <= 0. {
        Err(SmoothingError::NonPositiveValue)
    } else {
        Ok(())
    }
}
