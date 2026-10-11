//! Stack-backed low-cost streaming reconstruction; no anti-alias filtering.
use crate::ResamplingProgress;

/// Immutable reconstruction kernel and explicitly different input-sample delay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterpolationKind {
    /// Previous source sample at floor(output index * input rate / output rate).
    ZeroOrderHold,
    /// Two-point linear interpolation, zero nominal delay.
    Linear,
    /// Four-point cardinal cubic with half-difference slopes, one-sample delay.
    CatmullRom,
    /// Four-point third-degree Lagrange polynomial, one-sample delay.
    Lagrange3rd,
    /// Five-point fourth-degree Lagrange polynomial, two-sample delay.
    Lagrange4th,
}

/// Preparation or additive-gain validation failure; no stream/output edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterpolationError {
    InvalidRates,
    InvalidGain,
}
impl core::fmt::Display for InterpolationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::InvalidRates => "interpolation rates must be positive with ratio 1/16..=16",
            Self::InvalidGain => "interpolation additive gain must be finite",
        })
    }
}
impl std::error::Error for InterpolationError {}

#[derive(Clone, Copy, Debug)]
enum Phase {
    Floating {
        distance: f64,
        step: f64,
    },
    Integer {
        distance: i64,
        input: u32,
        output: u32,
    },
}
impl Phase {
    fn needs_input(self) -> bool {
        match self {
            Self::Floating { distance, .. } => distance > 0.,
            Self::Integer { distance, .. } => distance > 0,
        }
    }
    fn consume(&mut self) {
        match self {
            Self::Floating { distance, .. } => *distance -= 1.,
            Self::Integer {
                distance, output, ..
            } => *distance -= i64::from(*output),
        }
    }
    fn produce(&mut self) {
        match self {
            Self::Floating { distance, step } => *distance += *step,
            Self::Integer {
                distance, input, ..
            } => *distance += i64::from(*input),
        }
    }
    fn fraction(self) -> f64 {
        match self {
            Self::Floating { distance, .. } => -distance,
            Self::Integer {
                distance, output, ..
            } => -(distance as f64) / f64::from(output),
        }
        .clamp(0., 1.)
    }
    fn reset(&mut self) {
        match self {
            Self::Floating { distance, .. } => *distance = 1.,
            Self::Integer {
                distance, output, ..
            } => *distance = i64::from(*output),
        }
    }
}

/// One-channel f64 reconstruction with five stack-backed history samples.
///
/// Construction, processing, additive processing, clone, reset and destruction
/// allocate/deallocate nothing. Every channel needs its own owner. Kind/rates are
/// immutable; construction of another owner explicitly changes the profile.
/// This is a low-cost reconstruction family, **without anti-alias filtering**.
/// Downsampling aliases and cubic/quartic kernels can overshoot. Use the separate
/// [`crate::SincResampler`] profile when filtering is required. No headroom,
/// denormal, finite-input sanitation or callback deadline policy is implied.
///
/// Integer output index J requires newest source index k=ceil(J*I/O), with
/// backward fraction f=k-J*I/O and initially zero history. Hold returns `x[floor(J*I/O)]`; linear interpolates `x[k]`
/// and `x[k-1]`. Catmull-Rom and cubic Lagrange
/// interpolate at J*I/O-1; quartic Lagrange at J*I/O-2. All profiles use common
/// bracketing consumption k+1, even when hold needs only the earlier sample.
/// Fractional bracketing can read up to one input sample ahead of the interpolation
/// coordinate; reported nominal delays do not include adapter buffering.
///
/// No source wrapping or EOS/tail policy: with N>0 supplied input samples and
/// integer rates, all pending outputs total 1+floor((N-1)*O/I). Empty source
/// produces nothing. Caller-supplied padding is ordinary consumed input; no
/// trailing zeros or delay trimming are invented. Equal rates retain kernel
/// delay/history, without a hidden copy bypass.
#[derive(Clone, Debug)]
pub struct StreamInterpolator {
    kind: InterpolationKind,
    phase: Phase,
    history: [f64; 5],
    has_input: bool,
}
impl StreamInterpolator {
    /// Prepare finite positive floating rates in matching units, ratio 1/16..=16.
    /// Floating recurrence has no long-duration exact-count guarantee. Prefer
    /// [`Self::new_integer_rates`] for exact positive-u32 rate scheduling.
    pub fn new(
        kind: InterpolationKind,
        input_rate: f64,
        output_rate: f64,
    ) -> Result<Self, InterpolationError> {
        let step = input_rate / output_rate;
        if !input_rate.is_finite()
            || !output_rate.is_finite()
            || input_rate <= 0.
            || output_rate <= 0.
            || !(1. / 16. ..=16.).contains(&step)
        {
            return Err(InterpolationError::InvalidRates);
        }
        Ok(Self::prepare(kind, Phase::Floating { distance: 1., step }))
    }

    /// Positive u32 rates, ratio 1/16..=16. A bounded signed relative numerator
    /// schedules source/output without duration-dependent rounding recurrence.
    /// Only kernel evaluation uses f64; no rate edits/clock-drift mode.
    pub fn new_integer_rates(
        kind: InterpolationKind,
        input_rate: u32,
        output_rate: u32,
    ) -> Result<Self, InterpolationError> {
        if input_rate == 0
            || output_rate == 0
            || u64::from(input_rate) > 16 * u64::from(output_rate)
            || u64::from(output_rate) > 16 * u64::from(input_rate)
        {
            return Err(InterpolationError::InvalidRates);
        }
        Ok(Self::prepare(
            kind,
            Phase::Integer {
                distance: i64::from(output_rate),
                input: input_rate,
                output: output_rate,
            },
        ))
    }
    fn prepare(kind: InterpolationKind, phase: Phase) -> Self {
        Self {
            kind,
            phase,
            history: [0.; 5],
            has_input: false,
        }
    }
    pub fn kind(&self) -> InterpolationKind {
        self.kind
    }
    /// Nominal reconstruction delay in source samples: hold/linear 0,
    /// Catmull-Rom/cubic Lagrange 1, quartic Lagrange 2. Fractional source
    /// bracketing, caller finite-stream tail and adapter buffering are separate.
    pub fn latency_input_samples(&self) -> f64 {
        match self.kind {
            InterpolationKind::ZeroOrderHold | InterpolationKind::Linear => 0.,
            InterpolationKind::CatmullRom | InterpolationKind::Lagrange3rd => 1.,
            InterpolationKind::Lagrange4th => 2.,
        }
    }
    /// Clear history/pending phase, preserving kind/rates without allocation.
    pub fn reset(&mut self) {
        self.history.fill(0.);
        self.has_input = false;
        self.phase.reset();
    }
    /// Replace only the produced output prefix. Retry with unconsumed input and
    /// fresh output capacity. Zero output capacity consumes nothing; empty input
    /// can drain pending upsample output. Suffix remains untouched. Nonfinite
    /// input can propagate through history until reset; consumption is irreversible.
    ///
    /// ```
    /// use ravet_signal::{InterpolationKind, StreamInterpolator};
    /// let mut stream = StreamInterpolator::new_integer_rates(
    ///     InterpolationKind::Linear, 1, 2).unwrap();
    /// let mut first = [99.];
    /// let progress = stream.process(&[2., 4.], &mut first);
    /// assert_eq!((progress.consumed, progress.produced), (1, 1));
    /// assert_eq!(first, [2.]);
    /// let mut rest = [99.; 3];
    /// let progress = stream.process(&[4.], &mut rest);
    /// assert_eq!((progress.consumed, progress.produced), (1, 2));
    /// assert_eq!(rest, [3., 4., 99.]);
    /// ```
    pub fn process(&mut self, input: &[f64], output: &mut [f64]) -> ResamplingProgress {
        self.run(input, output, None)
    }
    /// Add gain*reconstructed sample over the produced prefix only. Finite gain
    /// validation precedes all consumption/history/output edits. Zero gain still
    /// advances stream state but leaves output bitwise unchanged, including with
    /// nonfinite input. Finite gain/output arithmetic can overflow; no clamp.
    pub fn process_add(
        &mut self,
        input: &[f64],
        output: &mut [f64],
        gain: f64,
    ) -> Result<ResamplingProgress, InterpolationError> {
        if !gain.is_finite() {
            return Err(InterpolationError::InvalidGain);
        }
        Ok(self.run(input, output, Some(gain)))
    }
    fn run(&mut self, input: &[f64], output: &mut [f64], gain: Option<f64>) -> ResamplingProgress {
        let mut progress = ResamplingProgress::default();
        while progress.produced < output.len() {
            if !self.has_input || self.phase.needs_input() {
                if progress.consumed == input.len() {
                    break;
                }
                self.history.copy_within(1.., 0);
                self.history[4] = input[progress.consumed];
                self.phase.consume();
                self.has_input = true;
                progress.consumed += 1;
                continue;
            }
            match gain {
                Some(0.) => {}
                Some(gain) => output[progress.produced] += gain * self.sample(),
                None => output[progress.produced] = self.sample(),
            }
            self.phase.produce();
            progress.produced += 1;
        }
        progress
    }
    fn sample(&self) -> f64 {
        let f = self.phase.fraction();
        let h = self.history;
        // Exact phase nodes preserve direct sample identity, avoiding 0*NaN
        // from history outside that kernel's mathematical support at the node.
        if f == 0. {
            return match self.kind {
                InterpolationKind::ZeroOrderHold | InterpolationKind::Linear => h[4],
                InterpolationKind::CatmullRom | InterpolationKind::Lagrange3rd => h[3],
                InterpolationKind::Lagrange4th => h[2],
            };
        }
        match self.kind {
            InterpolationKind::ZeroOrderHold => h[3],
            InterpolationKind::Linear => h[4] * (1. - f) + h[3] * f,
            InterpolationKind::CatmullRom => {
                let t = 1. - f;
                let [p0, p1, p2, p3] = [h[1], h[2], h[3], h[4]];
                0.5 * (2. * p1
                    + t * ((p2 - p0)
                        + t * (2. * p0 - 5. * p1 + 4. * p2 - p3
                            + t * (-p0 + 3. * p1 - 3. * p2 + p3))))
            }
            InterpolationKind::Lagrange3rd => {
                let t = 1. - f;
                -h[1] * t * (t - 1.) * (t - 2.) / 6. + h[2] * (t + 1.) * (t - 1.) * (t - 2.) / 2.
                    - h[3] * (t + 1.) * t * (t - 2.) / 2.
                    + h[4] * (t + 1.) * t * (t - 1.) / 6.
            }
            InterpolationKind::Lagrange4th => {
                let x = 2. - f;
                h[0] * (x - 1.) * (x - 2.) * (x - 3.) * (x - 4.) / 24.
                    - h[1] * x * (x - 2.) * (x - 3.) * (x - 4.) / 6.
                    + h[2] * x * (x - 1.) * (x - 3.) * (x - 4.) / 4.
                    - h[3] * x * (x - 1.) * (x - 2.) * (x - 4.) / 6.
                    + h[4] * x * (x - 1.) * (x - 2.) * (x - 3.) / 24.
            }
        }
    }
}

#[cfg(test)]
mod phase_tests {
    use super::Phase;
    #[test]
    fn integer_recurrence_matches_u128_coordinates_beyond_old_float_ceiling() {
        for (input, output, count) in [
            (16, 1, 1_048_579_u64),
            (44100, 48000, 130_000),
            (u32::MAX, u32::MAX - 1, 130_000),
            (1, 16, 130_000),
        ] {
            let mut phase = Phase::Integer {
                distance: i64::from(output),
                input,
                output,
            };
            let mut consumed = 0_u128;
            for j in 0..count {
                while consumed == 0 || phase.needs_input() {
                    phase.consume();
                    consumed += 1;
                }
                let product = u128::from(j) * u128::from(input);
                let divisor = u128::from(output);
                assert_eq!(
                    consumed,
                    1 + product / divisor + u128::from(product % divisor != 0)
                );
                let remainder = product % divisor;
                let expected = if remainder == 0 {
                    0
                } else {
                    remainder as i64 - i64::from(output)
                };
                let Phase::Integer { distance, .. } = phase else {
                    unreachable!()
                };
                assert_eq!(distance, expected);
                assert!(distance > -i64::from(output) && distance <= 0);
                phase.produce();
            }
            if input == 16 {
                assert!(consumed > 16_777_216);
            }
        }
    }
}
