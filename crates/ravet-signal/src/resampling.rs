//! Prepared causal windowed-sinc conversion with bounded caller-owned I/O.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResamplingError;
/// Exact prefix progress. Retry with unconsumed input and fresh output capacity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResamplingProgress {
    pub consumed: usize,
    pub produced: usize,
}
/// One-channel f64 arbitrary-ratio streaming converter, rate ratio 1/16..16.
/// Preparation allocates a 1025-phase Blackman-windowed sinc table and history.
/// Processing interpolates table phases and allocates nothing. Filter delay is
/// (taps-1)/2 input samples; initial history is zero. This is a fixed quality
/// profile, not a spectral rejection or callback deadline guarantee. Fixed
/// rates use [`Self::new`]; [`Self::new_integer_rates`] selects an exact integer
/// schedule; [`Self::new_variable`] prepares bounded live ppm
/// adjustments with a conservative fixed cutoff and retained stream phase.
#[derive(Clone, Debug)]
pub struct SincResampler {
    step: f64,
    nominal_step: f64,
    maximum_adjustment_ppm: f64,
    distance: f64,
    integer_phase: Option<IntegerPhase>,
    has_input: bool,
    history: Vec<f64>,
    coefficients: Vec<f64>,
    write: usize,
}
/// Immutable next-output coordinates for an explicitly integer-scheduled stream.
/// This is a mathematical schedule, not evidence that a reader decoded its prefix.
/// Padded input coordinates can exceed u64; absolute products use u128.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntegerResamplingPosition {
    output_frame: u64,
    input_frames_consumed: u128,
    distance: i64,
}
impl IntegerResamplingPosition {
    pub fn output_frame(&self) -> u64 {
        self.output_frame
    }
    pub fn input_frames_consumed(&self) -> u128 {
        self.input_frames_consumed
    }
}
/// A relative phase with no duration-dependent counter or rounding recurrence.
#[derive(Clone, Copy, Debug)]
struct IntegerPhase {
    distance: i64,
    input_rate: u32,
    output_rate: u32,
}
impl IntegerPhase {
    fn new(input_rate: u32, output_rate: u32) -> Result<Self, ResamplingError> {
        if input_rate == 0
            || output_rate == 0
            || u64::from(input_rate) > 16 * u64::from(output_rate)
            || u64::from(output_rate) > 16 * u64::from(input_rate)
        {
            return Err(ResamplingError);
        }
        Ok(Self {
            distance: i64::from(output_rate),
            input_rate,
            output_rate,
        })
    }
    fn consume(&mut self) {
        // Called only for initial input or distance>0. State stays >-output_rate.
        self.distance -= i64::from(self.output_rate);
    }
    fn produce(&mut self) {
        // Called only for distance<=0. State stays <=input_rate.
        self.distance += i64::from(self.input_rate);
    }
    fn fraction(&self) -> f64 {
        -(self.distance as f64) / f64::from(self.output_rate)
    }
}
impl SincResampler {
    /// Finite positive rates and even tap count 16..256. Control-thread work.
    /// Rates are expressed in the same units; no hardware/format conversion.
    pub fn new(input_rate: f64, output_rate: f64, taps: usize) -> Result<Self, ResamplingError> {
        Self::prepare(input_rate, output_rate, taps, 0.)
    }
    /// Prepare a caller-selected exact integer-rate schedule. Positive u32 rates,
    /// ratio 1/16..16 and even 16..256 taps; coefficient/history preparation matches
    /// the fixed profile. Scheduling uses a bounded signed relative numerator,
    /// without duration-dependent floating recurrence. Only phase interpolation
    /// uses f64; numerical PCM can differ from `new`. This does not improve the
    /// spectral profile or establish device/callback deadlines. Nonzero live ppm
    /// adjustments reject; finite streams still require caller-supplied padding.
    pub fn new_integer_rates(
        input_rate: u32,
        output_rate: u32,
        taps: usize,
    ) -> Result<Self, ResamplingError> {
        let phase = IntegerPhase::new(input_rate, output_rate)?;
        let mut prepared = Self::prepare(f64::from(input_rate), f64::from(output_rate), taps, 0.)?;
        prepared.integer_phase = Some(phase);
        Ok(prepared)
    }
    /// Compute the exact next-output input coordinate without changing state.
    /// Integer profiles only; u128 input count includes explicit caller padding.
    /// No allocation, I/O, source validation or finite-file tail policy is implied.
    pub fn integer_position(
        &self,
        output_frame: u64,
    ) -> Result<IntegerResamplingPosition, ResamplingError> {
        let phase = self.integer_phase.ok_or(ResamplingError)?;
        let product = u128::from(output_frame) * u128::from(phase.input_rate);
        let divisor = u128::from(phase.output_rate);
        let remainder = product % divisor;
        let input_frames_consumed = 1 + product / divisor + u128::from(remainder != 0);
        let distance = if remainder == 0 {
            0
        } else {
            remainder as i64 - i64::from(phase.output_rate)
        };
        Ok(IntegerResamplingPosition {
            output_frame,
            input_frames_consumed,
            distance,
        })
    }
    /// Prime an integer stream so its next sample is the specified output frame.
    /// Off-audio source-window preparation supplies exactly `taps` finite samples
    /// oldest-to-newest, ending at input coordinate C-1 from integer_position.
    /// Leading/trailing zeros and finite-file bounds belong to the caller. Invalid
    /// mode/history preserves all state. This operation allocates nothing and does
    /// not read, seek, authenticate source history or change coefficients/rates.
    pub fn prime_integer_at(
        &mut self,
        output_frame: u64,
        chronological_history: &[f64],
    ) -> Result<IntegerResamplingPosition, ResamplingError> {
        let position = self.integer_position(output_frame)?;
        if chronological_history.len() != self.history.len()
            || chronological_history.iter().any(|v| !v.is_finite())
        {
            return Err(ResamplingError);
        }
        self.history.copy_from_slice(chronological_history);
        self.write = 0;
        self.has_input = true;
        self.integer_phase
            .as_mut()
            .expect("validated integer profile")
            .distance = position.distance;
        Ok(position)
    }
    /// Prepare a live-adjustable clock-drift profile. Maximum adjustment must
    /// be finite in (0,10000] ppm (one percent); the entire permitted input/
    /// output step range must stay within 1/16..16. Filter cutoff uses the
    /// largest permitted step, avoiding live coefficient allocation. This is
    /// not an automatic drift estimator or a spectral/deadline guarantee.
    pub fn new_variable(
        input_rate: f64,
        output_rate: f64,
        taps: usize,
        maximum_adjustment_ppm: f64,
    ) -> Result<Self, ResamplingError> {
        Self::validate_variable(input_rate, output_rate, taps, maximum_adjustment_ppm)?;
        Self::prepare(input_rate, output_rate, taps, maximum_adjustment_ppm)
    }
    /// Check live-adjustable preparation bounds without allocating coefficient
    /// storage or mutating a stream. This does not prepare a converter or prove
    /// native device support. Rates are finite positive values in matching units.
    pub fn validate_variable(
        input_rate: f64,
        output_rate: f64,
        taps: usize,
        maximum_adjustment_ppm: f64,
    ) -> Result<(), ResamplingError> {
        if !maximum_adjustment_ppm.is_finite()
            || maximum_adjustment_ppm <= 0.
            || maximum_adjustment_ppm > 10000.
        {
            return Err(ResamplingError);
        }
        Self::validate_rates(input_rate, output_rate, taps, maximum_adjustment_ppm)
    }
    fn validate_rates(
        input_rate: f64,
        output_rate: f64,
        taps: usize,
        ppm: f64,
    ) -> Result<(), ResamplingError> {
        if !input_rate.is_finite()
            || input_rate <= 0.
            || !output_rate.is_finite()
            || output_rate <= 0.
            || !(16..=256).contains(&taps)
            || !taps.is_multiple_of(2)
        {
            return Err(ResamplingError);
        }
        let step = input_rate / output_rate;
        if !(1. / 16. ..=16.).contains(&(step * (1. - ppm * 1e-6)))
            || !(1. / 16. ..=16.).contains(&(step * (1. + ppm * 1e-6)))
        {
            return Err(ResamplingError);
        }
        Ok(())
    }
    fn prepare(
        input_rate: f64,
        output_rate: f64,
        taps: usize,
        maximum_adjustment_ppm: f64,
    ) -> Result<Self, ResamplingError> {
        Self::validate_rates(input_rate, output_rate, taps, maximum_adjustment_ppm)?;
        let step = input_rate / output_rate;
        let maximum_step = step * (1. + maximum_adjustment_ppm * 1e-6);
        let cutoff = if maximum_adjustment_ppm == 0. {
            0.95 * (output_rate / input_rate).min(1.)
        } else {
            0.95 * (1. / maximum_step).min(1.)
        };
        let delay = (taps - 1) as f64 * 0.5;
        let mut coefficients = vec![0.; 1025 * taps];
        for phase in 0..=1024 {
            let fraction = phase as f64 / 1024.;
            let row = &mut coefficients[phase * taps..(phase + 1) * taps];
            let mut sum = 0.;
            for (tap, coefficient) in row.iter_mut().enumerate() {
                let x = (tap as f64 - delay - fraction) * cutoff;
                let sinc = if x.abs() < 1e-12 {
                    1.
                } else {
                    (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
                };
                let angle = std::f64::consts::TAU * tap as f64 / (taps - 1) as f64;
                let window = 0.42 - 0.5 * angle.cos() + 0.08 * (2. * angle).cos();
                *coefficient = cutoff * sinc * window;
                sum += *coefficient;
            }
            if !sum.is_finite() || sum.abs() < 1e-12 {
                return Err(ResamplingError);
            }
            for coefficient in row {
                *coefficient /= sum;
            }
        }
        Ok(Self {
            step,
            nominal_step: step,
            maximum_adjustment_ppm,
            distance: 1.,
            integer_phase: None,
            has_input: false,
            history: vec![0.; taps],
            coefficients,
            write: 0,
        })
    }
    /// Change future output spacing without resetting history or pending phase.
    /// Positive ppm consumes input faster per output sample. The next output's
    /// already scheduled position is retained; new spacing applies afterward.
    /// Reject before mutation. Fixed constructor accepts only zero adjustment.
    /// No smoothing/estimation is implicit; this operation allocates nothing.
    pub fn set_rate_adjustment_ppm(&mut self, ppm: f64) -> Result<(), ResamplingError> {
        if !ppm.is_finite() || ppm.abs() > self.maximum_adjustment_ppm {
            return Err(ResamplingError);
        }
        self.step = self.nominal_step * (1. + ppm * 1e-6);
        Ok(())
    }
    /// Current input samples per output sample, including live adjustment.
    pub fn rate_step(&self) -> f64 {
        self.step
    }
    pub fn maximum_adjustment_ppm(&self) -> f64 {
        self.maximum_adjustment_ppm
    }
    pub fn latency_input_samples(&self) -> f64 {
        (self.history.len() - 1) as f64 * 0.5
    }
    /// Clear stream state and zero history without reallocating coefficients.
    /// Retain the current rate adjustment; set zero explicitly to return nominal.
    pub fn reset(&mut self) {
        self.distance = 1.;
        if let Some(phase) = self.integer_phase.as_mut() {
            phase.distance = i64::from(phase.output_rate);
        }
        self.has_input = false;
        self.write = 0;
        self.history.fill(0.);
    }
    /// Consume/produce bounded prefixes, retaining pending output internally.
    /// Zero output capacity consumes nothing. Zero input may drain pending output.
    /// Output beyond `produced` is untouched. To flush a finite stream, supply
    /// explicit trailing zeros; this method does not invent input or final padding.
    /// Nonfinite audio propagates. Fixed construction retains its rate; the
    /// variable profile accepts only its prepared bounded ppm adjustments.
    pub fn process(&mut self, input: &[f64], output: &mut [f64]) -> ResamplingProgress {
        let mut progress = ResamplingProgress::default();
        let taps = self.history.len();
        while progress.produced < output.len() {
            if !self.has_input
                || self
                    .integer_phase
                    .map_or(self.distance > 0., |phase| phase.distance > 0)
            {
                if progress.consumed == input.len() {
                    break;
                }
                self.history[self.write] = input[progress.consumed];
                self.write = (self.write + 1) % taps;
                if let Some(phase) = self.integer_phase.as_mut() {
                    phase.consume();
                } else {
                    self.distance -= 1.;
                }
                self.has_input = true;
                progress.consumed += 1;
                continue;
            }
            let position = self
                .integer_phase
                .map_or(-self.distance, |phase| phase.fraction())
                .clamp(0., 1.)
                * 1024.;
            let phase = (position.floor() as usize).min(1023);
            let fraction = position - phase as f64;
            let mut sample = 0.;
            for tap in 0..taps {
                let a = self.coefficients[phase * taps + tap];
                let b = self.coefficients[(phase + 1) * taps + tap];
                let value = self.history[(self.write + taps - 1 - tap) % taps];
                sample += value * (a + fraction * (b - a));
            }
            output[progress.produced] = sample;
            progress.produced += 1;
            if let Some(phase) = self.integer_phase.as_mut() {
                phase.produce();
            } else {
                self.distance += self.step;
            }
        }
        progress
    }
}

#[cfg(test)]
mod integer_phase_tests {
    use super::IntegerPhase;
    #[test]
    fn recurrence_passes_old_source_ceiling_without_accumulated_phase_error() {
        let n = 16_777_216u64 + 4097;
        let input_rate = 44100;
        let output_rate = 48000;
        let mut phase = IntegerPhase::new(input_rate, output_rate).unwrap();
        let mut produced = 0u64;
        for consumed in 1..=n {
            assert!(phase.distance > 0);
            phase.consume();
            while phase.distance <= 0 {
                assert!(phase.distance > -i64::from(output_rate));
                phase.produce();
                produced += 1;
            }
            assert!(phase.distance <= i64::from(input_rate));
            if consumed % 65536 == 0 || consumed == n {
                let expected = (u128::from(consumed - 1) * u128::from(output_rate))
                    / u128::from(input_rate)
                    + 1;
                assert_eq!(u128::from(produced), expected);
                let absolute_numerator = u128::from(output_rate)
                    + u128::from(produced) * u128::from(input_rate)
                    - u128::from(consumed) * u128::from(output_rate);
                assert_eq!(phase.distance as u128, absolute_numerator);
            }
        }
    }
}
