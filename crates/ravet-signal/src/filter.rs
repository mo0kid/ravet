//! Validated double-precision biquads with coefficients prepared off the audio path.

/// Second-order filter designs; bandpass has unity gain at its center.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FilterKind {
    LowPass,
    HighPass,
    BandPass,
    Notch,
    AllPass,
    Peak { gain_db: f64 },
}

/// Shelf endpoint selected by [`BiquadCoefficients::design_shelf`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShelfKind {
    Low,
    High,
}

/// Invalid/nonfinite design, unrepresentable or unstable coefficients.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FilterError;

/// Normalized coefficients for H(z)=(b0+b1*z^-1+b2*z^-2)/(1+a1*z^-1+a2*z^-2).
/// Private fields keep finite/stable constructor invariants intact.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BiquadCoefficients {
    b: [f64; 3],
    a: [f64; 2],
}
impl BiquadCoefficients {
    /// Validate finite normalized coefficients and strict second-order Jury
    /// stability. Extremely small stability margins may be unrepresentable.
    pub fn new(b: [f64; 3], a: [f64; 2]) -> Result<Self, FilterError> {
        if b.iter().chain(a.iter()).any(|x| !x.is_finite())
            || a[1].abs() >= 1.0
            || 1.0 + a[0] + a[1] <= 0.0
            || 1.0 - a[0] + a[1] <= 0.0
        {
            return Err(FilterError);
        }
        Ok(Self { b, a })
    }
    /// Cookbook designs: finite sample rate >0, frequency strictly between 0
    /// and Nyquist, Q in (0, 1,000,000], peak gain in [-120,120] dB.
    /// Coefficient design belongs on control/preparation threads; no smoothing
    /// or click-free parameter transition is implied by replacement.
    pub fn design(
        kind: FilterKind,
        sample_rate: f64,
        frequency: f64,
        q: f64,
    ) -> Result<Self, FilterError> {
        if !sample_rate.is_finite()
            || sample_rate <= 0.
            || !frequency.is_finite()
            || frequency <= 0.
            || frequency >= sample_rate * 0.5
            || !q.is_finite()
            || q <= 0.
            || q > 1e6
        {
            return Err(FilterError);
        }
        let omega = std::f64::consts::TAU * (frequency / sample_rate);
        let cos = omega.cos();
        let alpha = omega.sin() / (2. * q);
        let mut denominator = [1. + alpha, -2. * cos, 1. - alpha];
        let numerator = match kind {
            FilterKind::LowPass => [(1. - cos) * 0.5, 1. - cos, (1. - cos) * 0.5],
            FilterKind::HighPass => [(1. + cos) * 0.5, -(1. + cos), (1. + cos) * 0.5],
            FilterKind::BandPass => [alpha, 0., -alpha],
            FilterKind::Notch => [1., -2. * cos, 1.],
            FilterKind::AllPass => [1. - alpha, -2. * cos, 1. + alpha],
            FilterKind::Peak { gain_db } => {
                if !gain_db.is_finite() || !(-120.0..=120.0).contains(&gain_db) {
                    return Err(FilterError);
                }
                let amplitude = 10f64.powf(gain_db / 40.);
                denominator = [1. + alpha / amplitude, -2. * cos, 1. - alpha / amplitude];
                [1. + alpha * amplitude, -2. * cos, 1. - alpha * amplitude]
            }
        };
        Self::new(
            numerator.map(|x| x / denominator[0]),
            [
                denominator[1] / denominator[0],
                denominator[2] / denominator[0],
            ],
        )
    }
    /// Control-thread low/high shelf design following the Audio EQ Cookbook.
    /// Rate is finite/positive; midpoint frequency is strictly between DC and
    /// Nyquist, gain -120..120 dB and shelf slope in (0,1]. Slope one is the
    /// steepest monotonic profile; it is distinct from Q in [`Self::design`].
    /// Unrepresentable coefficients/stability margins reject. Existing processing
    /// and coefficient replacement retain their unsmoothed, one-channel policy.
    pub fn design_shelf(
        kind: ShelfKind,
        sample_rate: f64,
        frequency: f64,
        gain_db: f64,
        slope: f64,
    ) -> Result<Self, FilterError> {
        if !sample_rate.is_finite()
            || sample_rate <= 0.
            || !frequency.is_finite()
            || frequency <= 0.
            || frequency >= sample_rate * 0.5
            || !gain_db.is_finite()
            || !(-120.0..=120.0).contains(&gain_db)
            || !slope.is_finite()
            || slope <= 0.
            || slope > 1.
        {
            return Err(FilterError);
        }
        let omega = std::f64::consts::TAU * (frequency / sample_rate);
        let amplitude = 10f64.powf(gain_db / 40.);
        let plus = amplitude + 1.;
        let minus = amplitude - 1.;
        let cosine = omega.cos();
        let root = omega.sin()
            * ((amplitude * amplitude + 1.) * (1. / slope - 1.) + 2. * amplitude).sqrt();
        let (b, a) = match kind {
            ShelfKind::Low => (
                [
                    amplitude * (plus - minus * cosine + root),
                    2. * amplitude * (minus - plus * cosine),
                    amplitude * (plus - minus * cosine - root),
                ],
                [
                    plus + minus * cosine + root,
                    -2. * (minus + plus * cosine),
                    plus + minus * cosine - root,
                ],
            ),
            ShelfKind::High => (
                [
                    amplitude * (plus + minus * cosine + root),
                    -2. * amplitude * (minus + plus * cosine),
                    amplitude * (plus + minus * cosine - root),
                ],
                [
                    plus - minus * cosine + root,
                    2. * (minus - plus * cosine),
                    plus - minus * cosine - root,
                ],
            ),
        };
        Self::new(b.map(|x| x / a[0]), [a[1] / a[0], a[2] / a[0]])
    }
    /// Finite normalized frequency from DC=0 to Nyquist=0.5, for control-thread
    /// response inspection. This returns linear amplitude, not decibels.
    pub fn magnitude_at(self, cycles_per_sample: f64) -> Result<f64, FilterError> {
        if !cycles_per_sample.is_finite() || !(0.0..=0.5).contains(&cycles_per_sample) {
            return Err(FilterError);
        }
        let omega = std::f64::consts::TAU * cycles_per_sample;
        let numerator = (self.b[0] + self.b[1] * omega.cos() + self.b[2] * (2. * omega).cos())
            .hypot(self.b[1] * omega.sin() + self.b[2] * (2. * omega).sin());
        let denominator = (1. + self.a[0] * omega.cos() + self.a[1] * (2. * omega).cos())
            .hypot(self.a[0] * omega.sin() + self.a[1] * (2. * omega).sin());
        Ok(numerator / denominator)
    }
}

/// One channel of transposed direct-form II f64 processing. State persists
/// across blocks; process/reset allocate nothing. Nonfinite audio propagates.
/// No denormal policy, coefficient smoothing or callback deadline is imposed.
#[derive(Clone, Debug)]
pub struct Biquad {
    coefficients: BiquadCoefficients,
    state: [f64; 2],
}
impl Biquad {
    pub fn new(coefficients: BiquadCoefficients) -> Self {
        Self {
            coefficients,
            state: [0.; 2],
        }
    }
    /// Preserve delay state while replacing already validated coefficients.
    pub fn set_coefficients(&mut self, coefficients: BiquadCoefficients) {
        self.coefficients = coefficients;
    }
    pub fn reset(&mut self) {
        self.state = [0.; 2];
    }
    /// Copy of both actual transposed delay memories, without changing history.
    /// These are recurrence memories, not an independently sanitised audio signal.
    pub fn delay_state(&self) -> [f64; 2] {
        self.state
    }
    pub fn process_sample(&mut self, sample: f64) -> f64 {
        let coefficients = self.coefficients;
        let output = coefficients.b[0] * sample + self.state[0];
        self.state[0] = coefficients.b[1] * sample - coefficients.a[0] * output + self.state[1];
        self.state[1] = coefficients.b[2] * sample - coefficients.a[1] * output;
        output
    }
    pub fn process_in_place(&mut self, samples: &mut [f64]) {
        for sample in samples {
            *sample = self.process_sample(*sample);
        }
    }
}
impl std::fmt::Display for FilterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid filter design or unstable/unrepresentable coefficients")
    }
}
impl std::error::Error for FilterError {}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn response_matches_design_landmarks() {
        let rate = 48000.;
        let frequency = 1000.;
        let q = std::f64::consts::FRAC_1_SQRT_2;
        let low = BiquadCoefficients::design(FilterKind::LowPass, rate, frequency, q).unwrap();
        let high = BiquadCoefficients::design(FilterKind::HighPass, rate, frequency, q).unwrap();
        assert!((low.magnitude_at(0.).unwrap() - 1.).abs() < 1e-12);
        assert!(low.magnitude_at(0.5).unwrap() < 1e-12);
        assert!(high.magnitude_at(0.).unwrap() < 1e-12);
        assert!((high.magnitude_at(0.5).unwrap() - 1.).abs() < 1e-12);
        assert!((low.magnitude_at(frequency / rate).unwrap() - q).abs() < 1e-12);
        assert!((high.magnitude_at(frequency / rate).unwrap() - q).abs() < 1e-12);
        for kind in [
            FilterKind::BandPass,
            FilterKind::Notch,
            FilterKind::AllPass,
            FilterKind::Peak { gain_db: 6. },
        ] {
            let filter = BiquadCoefficients::design(kind, rate, frequency, 1.).unwrap();
            let expected = match kind {
                FilterKind::Notch => 0.,
                FilterKind::Peak { .. } => 10f64.powf(6. / 20.),
                _ => 1.,
            };
            assert!((filter.magnitude_at(frequency / rate).unwrap() - expected).abs() < 1e-12);
        }
    }
    #[test]
    fn impulse_recurrence_partitioning_and_reset_match() {
        let coefficients =
            BiquadCoefficients::design(FilterKind::LowPass, 48000., 2000., 0.7).unwrap();
        let mut block_filter = Biquad::new(coefficients);
        let mut input = [0.; 100];
        input[0] = 1.;
        let mut expected = [0.; 100];
        for n in 0..100 {
            expected[n] = coefficients.b[0] * input[n];
            if n >= 1 {
                expected[n] +=
                    coefficients.b[1] * input[n - 1] - coefficients.a[0] * expected[n - 1];
            }
            if n >= 2 {
                expected[n] +=
                    coefficients.b[2] * input[n - 2] - coefficients.a[1] * expected[n - 2];
            }
        }
        for chunk in input.chunks_mut(7) {
            block_filter.process_in_place(chunk);
        }
        for (actual, expected) in input.iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-15);
        }
        block_filter.reset();
        assert_eq!(block_filter.process_sample(0.), 0.);
    }
    #[test]
    fn malformed_and_unstable_coefficients_reject() {
        for (rate, frequency, q) in [
            (0., 1., 1.),
            (48000., 0., 1.),
            (48000., 24000., 1.),
            (48000., 1000., 0.),
            (f64::NAN, 1., 1.),
            (48000., 1000., f64::INFINITY),
        ] {
            assert!(BiquadCoefficients::design(FilterKind::LowPass, rate, frequency, q).is_err());
        }
        assert!(
            BiquadCoefficients::design(FilterKind::Peak { gain_db: 121. }, 48000., 1000., 1.)
                .is_err()
        );
        for a in [[0., 1.], [-2., 0.], [2., 0.], [0., f64::NAN]] {
            assert!(BiquadCoefficients::new([1., 0., 0.], a).is_err());
        }
    }
}
