//! Prepared finite-FIR interpolated peak estimates, with explicit tail ownership.
use std::fmt;
// Defensive aggregate cap, retained unchanged. For every admitted profile the
// largest prepared footprint (64 channels, half-width 512, 16 phases) is
// history 64*1025*8 = 524,800 bytes + bank coefficient copy 16*1025*8 = 131,200
// + normalized scratch 1025*8 = 8,200 + size_of::<Self>() (about 1.3 KB), i.e.
// roughly 665,000 bytes (about 650 KiB), under 8% of this 8 MiB cap. The
// ResourceLimit paths therefore cannot be reached through the public API today;
// only the private helper unit tests below exercise the arithmetic, and no
// public cap-exhaustion test is claimed.
const MAX_BYTES: usize = 8 * 1024 * 1024;
/// Explicit sampling grid; more phases do not certify a continuous maximum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TruePeakFactor {
    Four,
    Eight,
    Sixteen,
}
impl TruePeakFactor {
    pub const fn factor(self) -> usize {
        match self {
            Self::Four => 4,
            Self::Eight => 8,
            Self::Sixteen => 16,
        }
    }
}
/// Invalid profile, whole-call refusal or failed control allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TruePeakError {
    SampleRate,
    HalfWidth,
    Beta,
    Coefficient,
    ChannelCount,
    MaximumFrames,
    BufferLength,
    TooManyFrames,
    FrameCount,
    Finalized,
    ResourceLimit,
    Allocation,
}
impl fmt::Display for TruePeakError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SampleRate => "true-peak sample rate must be 8000..768000 Hz",
            Self::HalfWidth => "true-peak source half-width must be 32..512 frames",
            Self::Beta => "true-peak Kaiser beta must be finite 0..16",
            Self::Coefficient => "true-peak coefficients must have representable normalization",
            Self::ChannelCount => "true-peak channels must match the prepared 1..64 layout",
            Self::MaximumFrames => "true-peak maximum callback must be 1..65536 frames",
            Self::BufferLength => "true-peak buffers must cover the complete prefix",
            Self::TooManyFrames => "true-peak callback exceeds the prepared maximum",
            Self::FrameCount => "true-peak counters must retain complete final-tail headroom",
            Self::Finalized => "true-peak input requires reset after explicit finish",
            Self::ResourceLimit => "true-peak prepared arrays exceed 8 MiB",
            Self::Allocation => "true-peak control allocation failed",
        })
    }
}
impl std::error::Error for TruePeakError {}
fn bytes(count: usize) -> Result<usize, TruePeakError> {
    count
        .checked_mul(size_of::<f64>())
        .filter(|n| *n <= MAX_BYTES)
        .ok_or(TruePeakError::ResourceLimit)
}
fn reserve(count: usize) -> Result<Vec<f64>, TruePeakError> {
    bytes(count)?;
    let mut result = Vec::new();
    result
        .try_reserve_exact(count)
        .map_err(|_| TruePeakError::Allocation)?;
    bytes(result.capacity())?;
    Ok(result)
}
// Neumaier summation keeps alternating sinc weights from accumulating a
// width-dependent DC floor. Native products and final amplitude remain f64.
fn sum_compensated(values: impl Iterator<Item = f64>) -> f64 {
    let mut sum = 0_f64;
    let mut correction = 0_f64;
    for value in values {
        let next = sum + value;
        correction += if sum.abs() >= value.abs() {
            (sum - next) + value
        } else {
            (value - next) + sum
        };
        sum = next;
    }
    sum + correction
}
fn i0(x: f64) -> Result<f64, TruePeakError> {
    let mut sum = 1.;
    let mut term = 1.;
    for k in 1..=128 {
        term *= (x * x) / (4. * (k * k) as f64);
        let next = sum + term;
        if next == sum {
            return Ok(sum);
        }
        sum = next;
    }
    Err(TruePeakError::Coefficient)
}
/// Own immutable coefficients prepared on CONTROL. Clone allocates on CONTROL.
///
/// Integer rate 8000..768000 Hz, half-width 32..512, beta 0..16, explicit 4/8/16
/// phases. Kaiser-windowed cardinal sinc is normalized to unity DC per phase.
/// Phase zero is the exact delayed source sample. Neumaier accumulation is used
/// for normalization, DC observations and scaled phase sums; rounded actual DC
/// sums remain exposed rather than repaired after division. All source coordinates use
/// integer counts; there is no duration-growing floating phase recurrence.
/// This finite window and sampled grid are estimates, without a standards or
/// continuous-maximum certificate. Inspect measured profile limits before use.
///
/// The sample rate is validated metadata only: it must lie in 8000..768000 Hz
/// and is stored and returned by `sample_rate()`, but coefficients and
/// measurements depend only on half-width, factor and beta, which are in
/// normalized source-sample coordinates. The library cannot verify the real
/// source clock, so a caller declaring a different rate for the same samples
/// obtains bitwise identical rows and readings. The documented
/// 6 rates x 36 profiles = 216 admission combinations are therefore 36 distinct
/// coefficient profiles (3 factors x 3 half-widths x 4 betas sampled); a
/// consumer that must not mix rates has to compare `sample_rate()` itself, as the
/// test Graph adapter does.
#[derive(Clone, Debug)]
pub struct TruePeakDesign {
    rate: u32,
    half: usize,
    factor: TruePeakFactor,
    beta: f64,
    coefficients: Vec<f64>,
}
impl TruePeakDesign {
    pub fn new(
        sample_rate: u32,
        half_width: usize,
        factor: TruePeakFactor,
        beta: f64,
    ) -> Result<Self, TruePeakError> {
        if !(8000..=768000).contains(&sample_rate) {
            return Err(TruePeakError::SampleRate);
        }
        if !(32..=512).contains(&half_width) {
            return Err(TruePeakError::HalfWidth);
        }
        if !beta.is_finite() || !(0. ..=16.).contains(&beta) {
            return Err(TruePeakError::Beta);
        }
        let taps = 2 * half_width + 1;
        let count = taps
            .checked_mul(factor.factor())
            .ok_or(TruePeakError::ResourceLimit)?;
        let mut coefficients = reserve(count)?;
        let denominator = i0(beta)?;
        for phase in 0..factor.factor() {
            let start = coefficients.len();
            for j in 0..taps {
                let value = if phase == 0 {
                    f64::from(j == half_width)
                } else {
                    let distance =
                        j as f64 - half_width as f64 + phase as f64 / factor.factor() as f64;
                    if distance.abs() > half_width as f64 {
                        0.
                    } else {
                        let ratio = distance / half_width as f64;
                        let sinc = (std::f64::consts::PI * distance).sin()
                            / (std::f64::consts::PI * distance);
                        sinc * i0(beta * (1. - ratio * ratio).sqrt())? / denominator
                    }
                };
                coefficients.push(value);
            }
            let row = &mut coefficients[start..];
            let normalization = sum_compensated(row.iter().copied());
            if !normalization.is_finite() || normalization <= 0. {
                return Err(TruePeakError::Coefficient);
            }
            for value in row.iter_mut() {
                *value /= normalization;
            }
            if row.iter().any(|v| !v.is_finite())
                || !row.iter().map(|v| v.abs()).sum::<f64>().is_finite()
            {
                return Err(TruePeakError::Coefficient);
            }
        }
        Ok(Self {
            rate: sample_rate,
            half: half_width,
            factor,
            beta,
            coefficients,
        })
    }
    pub const fn sample_rate(&self) -> u32 {
        self.rate
    }
    pub const fn half_width(&self) -> usize {
        self.half
    }
    pub const fn factor(&self) -> TruePeakFactor {
        self.factor
    }
    pub const fn beta(&self) -> f64 {
        self.beta
    }
    pub fn taps(&self) -> usize {
        2 * self.half + 1
    }
    /// Actual row, in newest-to-oldest source order. Invalid phase returns None.
    pub fn phase_coefficients(&self, phase: usize) -> Option<&[f64]> {
        (phase < self.factor.factor())
            .then(|| &self.coefficients[phase * self.taps()..(phase + 1) * self.taps()])
    }
    pub fn phase_dc_sum(&self, phase: usize) -> Option<f64> {
        self.phase_coefficients(phase)
            .map(|row| sum_compensated(row.iter().copied()))
    }
}
/// Wide measurement of one channel. Audio is never edited by observation.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TruePeakReading64 {
    pub sample_peak: f64,
    pub interpolated_peak: f64,
    pub source_frames: u64,
    pub phase_observations: u64,
    pub tail_frames: u64,
    pub nonfinite_input: bool,
    pub amplitude_overflow: bool,
    pub finalized: bool,
}
impl TruePeakReading64 {
    /// Decibels relative to amplitude one. Silence is -infinity, overflow +infinity.
    pub fn db_tp(self) -> f64 {
        20. * self.interpolated_peak.log10()
    }
}
/// Prepared 1..64-channel f64 measurement bank; f32 observation widens exactly.
///
/// Each causal phase is at t-half_width+phase/factor after source step t.
/// Negative indices are zero. Live readings have half_width frames of pending
/// source suffix (capped at the accepted source length); explicit finish evaluates the full zero-extended finite support.
/// Artificial tail steps never increment original source frames. Empty finish is
/// empty, repeat finish idempotent, and later input (including empty calls) requires reset.
///
/// Entire shape/prefix/frame/count admission precedes any measurement/history
/// mutation. Nonfinite samples are measurement silence with a flag; audio is
/// untouched. Per-window max-absolute scaling and Neumaier summation avoid intermediate
/// overflow and reduce alternating-weight accumulation error;
/// genuine final amplitudes beyond f64::MAX report infinity and an overflow flag.
/// Native underflow/rounding remains observable. No callback heap/locks/I/O or
/// implicit Drop finalization; finish performs bounded 2*half_width steps and
/// belongs after stopped audio ownership. No deadline or conformance guarantee.
///
/// A live `interpolated_peak` is not a peak bound: it can be below
/// `sample_peak` because the newest half_width source frames have not yet had
/// their own phase-zero coordinate evaluated. Coordinates beyond the last source
/// frame (for example the overshoot after a truncated near-Nyquist run) are only
/// measured by `finish()`. Coordinates far beyond the last frame only see small
/// window edge weights, so the peak value is not expected to discriminate a
/// tail much longer than half_width steps (an argument, not a proof); the
/// 2*half_width tail is evidenced by the `tail_frames`, `phase_observations`
/// and `last_phase_numerator()` counters rather than by the peak.
///
/// Two flags have different meanings. Nonfinite input samples are replaced by
/// zero before measurement (also in `sample_peak`) and latch `nonfinite_input`;
/// `amplitude_overflow` is set only when the final arithmetic result of finite
/// samples exceeds f64::MAX. Flags and peaks are per channel; a poisoned channel
/// leaves its neighbours' readings and histories unchanged. Channels share
/// nothing but the step counter and a recomputed scratch array.
#[derive(Debug)]
pub struct TruePeakBank64 {
    design: TruePeakDesign,
    channels: usize,
    maximum: usize,
    history: Vec<f64>,
    normalized: Vec<f64>,
    next: usize,
    sample_peaks: [f64; 64],
    peaks: [f64; 64],
    nonfinite: [bool; 64],
    overflow: [bool; 64],
    source: u64,
    phases: u64,
    tail: u64,
    finalized: bool,
}
impl TruePeakBank64 {
    /// Borrowed design survives any constructor refusal; all allocation is CONTROL.
    pub fn new(
        design: &TruePeakDesign,
        channels: usize,
        maximum_frames: usize,
    ) -> Result<Self, TruePeakError> {
        if !(1..=64).contains(&channels) {
            return Err(TruePeakError::ChannelCount);
        }
        if !(1..=65536).contains(&maximum_frames) {
            return Err(TruePeakError::MaximumFrames);
        }
        let count = channels
            .checked_mul(design.taps())
            .ok_or(TruePeakError::ResourceLimit)?;
        let declared = bytes(count)?
            .checked_add(bytes(design.coefficients.len())?)
            .and_then(|n| n.checked_add(design.taps() * size_of::<f64>()))
            .and_then(|n| n.checked_add(size_of::<Self>()))
            .ok_or(TruePeakError::ResourceLimit)?;
        if declared > MAX_BYTES {
            return Err(TruePeakError::ResourceLimit);
        }
        let mut history = reserve(count)?;
        let mut coefficients = reserve(design.coefficients.len())?;
        let mut normalized = reserve(design.taps())?;
        if bytes(history.capacity())?
            .checked_add(bytes(coefficients.capacity())?)
            .and_then(|n| n.checked_add(normalized.capacity() * size_of::<f64>()))
            .and_then(|n| n.checked_add(size_of::<Self>()))
            .ok_or(TruePeakError::ResourceLimit)?
            > MAX_BYTES
        {
            return Err(TruePeakError::ResourceLimit);
        }
        history.resize(count, 0.);
        normalized.resize(design.taps(), 0.);
        coefficients.extend_from_slice(&design.coefficients);
        Ok(Self {
            design: TruePeakDesign {
                coefficients,
                ..design.clone_metadata()
            },
            channels,
            maximum: maximum_frames,
            history,
            normalized,
            next: 0,
            sample_peaks: [0.; 64],
            peaks: [0.; 64],
            nonfinite: [false; 64],
            overflow: [false; 64],
            source: 0,
            phases: 0,
            tail: 0,
            finalized: false,
        })
    }
    pub fn design(&self) -> &TruePeakDesign {
        &self.design
    }
    pub const fn channels(&self) -> usize {
        self.channels
    }
    pub const fn maximum_frames(&self) -> usize {
        self.maximum
    }
    pub const fn source_frames(&self) -> u64 {
        self.source
    }
    /// Latest fully evaluated phase coordinate numerator, denominator factor.
    pub fn last_phase_numerator(&self) -> Option<i128> {
        (self.phases != 0).then(|| {
            self.phases as i128 - 1 - (self.design.half * self.design.factor.factor()) as i128
        })
    }
    pub fn pending_source_suffix_frames(&self) -> u64 {
        if self.finalized {
            0
        } else {
            self.source.min(self.design.half as u64)
        }
    }
    pub fn reading(&self, channel: usize) -> Result<TruePeakReading64, TruePeakError> {
        if channel >= self.channels {
            return Err(TruePeakError::ChannelCount);
        }
        Ok(TruePeakReading64 {
            sample_peak: self.sample_peaks[channel],
            interpolated_peak: self.peaks[channel],
            source_frames: self.source,
            phase_observations: self.phases,
            tail_frames: self.tail,
            nonfinite_input: self.nonfinite[channel],
            amplitude_overflow: self.overflow[channel],
            finalized: self.finalized,
        })
    }
    fn preflight(&self, channels: usize, frames: usize) -> Result<(), TruePeakError> {
        if channels != self.channels {
            return Err(TruePeakError::ChannelCount);
        }
        if frames > self.maximum {
            return Err(TruePeakError::TooManyFrames);
        }
        if self.finalized {
            return Err(TruePeakError::Finalized);
        }
        let limit = u64::MAX / self.design.factor.factor() as u64 - (2 * self.design.half) as u64;
        if self
            .source
            .checked_add(frames as u64)
            .is_none_or(|n| n > limit)
        {
            return Err(TruePeakError::FrameCount);
        }
        Ok(())
    }
    fn step(&mut self, input: &[f64], original: bool) {
        let taps = self.design.taps();
        for (channel, &sample) in input.iter().enumerate() {
            let sample = if sample.is_finite() {
                sample
            } else {
                self.nonfinite[channel] = true;
                0.
            };
            if original {
                self.sample_peaks[channel] = self.sample_peaks[channel].max(sample.abs());
            }
            let history = &mut self.history[channel * taps..(channel + 1) * taps];
            history[self.next] = sample;
            let scale = history.iter().fold(0_f64, |a, v| a.max(v.abs()));
            if scale != 0. {
                for (j, value) in self.normalized.iter_mut().enumerate() {
                    *value = history[(self.next + taps - j) % taps] / scale;
                }
            }
            for phase in 0..self.design.factor.factor() {
                let value = if phase == 0 {
                    history[(self.next + taps - self.design.half) % taps].abs()
                } else if scale == 0. {
                    0.
                } else {
                    let row = self
                        .design
                        .phase_coefficients(phase)
                        .expect("prepared phase");
                    let sum = sum_compensated(
                        row.iter()
                            .zip(&self.normalized)
                            .map(|(h, sample)| h * sample),
                    );
                    scale * sum.abs()
                };
                if !value.is_finite() {
                    self.overflow[channel] = true;
                }
                self.peaks[channel] = self.peaks[channel].max(value);
            }
        }
        self.next = (self.next + 1) % taps;
        self.phases += self.design.factor.factor() as u64;
        if original {
            self.source += 1;
        } else {
            self.tail += 1;
        }
    }
    pub fn observe_frame(&mut self, input: &[f64]) -> Result<(), TruePeakError> {
        self.preflight(input.len(), 1)?;
        self.step(input, true);
        Ok(())
    }
    pub fn observe_frame_f32(&mut self, input: &[f32]) -> Result<(), TruePeakError> {
        self.preflight(input.len(), 1)?;
        let mut frame = [0.; 64];
        for (a, b) in frame.iter_mut().zip(input) {
            *a = *b as f64;
        }
        self.step(&frame[..self.channels], true);
        Ok(())
    }
    pub fn observe_planar(&mut self, input: &[&[f64]], frames: usize) -> Result<(), TruePeakError> {
        self.observe(input.iter().copied(), input.len(), frames, |v| v)
    }
    pub fn observe_planar_f32(
        &mut self,
        input: &[&[f32]],
        frames: usize,
    ) -> Result<(), TruePeakError> {
        self.observe(input.iter().copied(), input.len(), frames, |v| v as f64)
    }
    pub fn observe_in_place(
        &mut self,
        input: &[&mut [f64]],
        frames: usize,
    ) -> Result<(), TruePeakError> {
        self.observe(input.iter().map(|s| &**s), input.len(), frames, |v| v)
    }
    pub fn observe_in_place_f32(
        &mut self,
        input: &[&mut [f32]],
        frames: usize,
    ) -> Result<(), TruePeakError> {
        self.observe(input.iter().map(|s| &**s), input.len(), frames, |v| {
            v as f64
        })
    }
    fn observe<'a, S: Copy + 'a, I: Iterator<Item = &'a [S]> + Clone>(
        &mut self,
        input: I,
        channels: usize,
        frames: usize,
        convert: impl Fn(S) -> f64,
    ) -> Result<(), TruePeakError> {
        self.preflight(channels, frames)?;
        if input.clone().any(|s| s.len() < frames) {
            return Err(TruePeakError::BufferLength);
        }
        let mut frame = [0.; 64];
        for i in 0..frames {
            for (out, source) in frame.iter_mut().zip(input.clone()) {
                *out = convert(source[i]);
            }
            self.step(&frame[..self.channels], true);
        }
        Ok(())
    }
    /// Explicit stopped finalization; all finite support, no original-frame inflation.
    pub fn finish(&mut self) {
        if self.finalized {
            return;
        }
        if self.source != 0 {
            let zero = [0.; 64];
            for _ in 0..2 * self.design.half {
                self.step(&zero[..self.channels], false);
            }
        }
        self.finalized = true;
    }
    pub fn reset(&mut self) {
        self.history.fill(0.);
        self.normalized.fill(0.);
        self.next = 0;
        self.sample_peaks.fill(0.);
        self.peaks.fill(0.);
        self.nonfinite.fill(false);
        self.overflow.fill(false);
        self.source = 0;
        self.phases = 0;
        self.tail = 0;
        self.finalized = false;
    }
}
impl TruePeakDesign {
    fn clone_metadata(&self) -> Self {
        Self {
            rate: self.rate,
            half: self.half,
            factor: self.factor,
            beta: self.beta,
            coefficients: Vec::new(),
        }
    }
}
/// Single-channel convenience owner. All counters and finalization match the bank.
///
/// ```
/// use ravet_signal::{TruePeakDesign, TruePeakFactor, TruePeakMeter64};
/// let profile = TruePeakDesign::new(48000, 32, TruePeakFactor::Eight, 12.)?;
/// let mut meter = TruePeakMeter64::new(&profile, 67)?;
/// meter.observe_f32(&[0.5, -0.5])?;
/// meter.finish(); // Explicitly after stopped source ownership.
/// let reading = meter.reading();
/// assert_eq!(reading.source_frames, 2);
/// assert_eq!(reading.tail_frames, 64);
/// assert!(reading.interpolated_peak >= reading.sample_peak);
/// # Ok::<(), ravet_signal::TruePeakError>(())
/// ```
#[derive(Debug)]
pub struct TruePeakMeter64 {
    bank: TruePeakBank64,
}
impl TruePeakMeter64 {
    pub fn new(design: &TruePeakDesign, maximum_frames: usize) -> Result<Self, TruePeakError> {
        Ok(Self {
            bank: TruePeakBank64::new(design, 1, maximum_frames)?,
        })
    }
    pub fn design(&self) -> &TruePeakDesign {
        self.bank.design()
    }
    pub fn maximum_frames(&self) -> usize {
        self.bank.maximum_frames()
    }
    pub fn pending_source_suffix_frames(&self) -> u64 {
        self.bank.pending_source_suffix_frames()
    }
    pub fn last_phase_numerator(&self) -> Option<i128> {
        self.bank.last_phase_numerator()
    }
    pub fn observe_sample(&mut self, input: f64) -> Result<(), TruePeakError> {
        self.bank.observe_frame(&[input])
    }
    pub fn observe(&mut self, input: &[f64]) -> Result<(), TruePeakError> {
        self.bank.observe_planar(&[input], input.len())
    }
    pub fn observe_f32(&mut self, input: &[f32]) -> Result<(), TruePeakError> {
        self.bank.observe_planar_f32(&[input], input.len())
    }
    pub fn reading(&self) -> TruePeakReading64 {
        self.bank.reading(0).expect("single channel")
    }
    pub fn finish(&mut self) {
        self.bank.finish();
    }
    pub fn reset(&mut self) {
        self.bank.reset();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_counter_limit_and_limit_plus_one_reserve_finish_without_history_mutation() {
        for factor in [
            TruePeakFactor::Four,
            TruePeakFactor::Eight,
            TruePeakFactor::Sixteen,
        ] {
            let d = TruePeakDesign::new(48000, 32, factor, 12.).unwrap();
            let mut bank = TruePeakBank64::new(&d, 2, 5).unwrap();
            let limit = u64::MAX / factor.factor() as u64 - 64;
            bank.source = limit - 1;
            bank.phases = bank.source * factor.factor() as u64;
            let before = bank.history.clone();
            let readings = [bank.reading(0).unwrap(), bank.reading(1).unwrap()];
            assert_eq!(
                bank.observe_planar(&[&[0.; 2], &[0.; 2]], 2),
                Err(TruePeakError::FrameCount)
            );
            assert_eq!(bank.history, before);
            assert_eq!(
                [bank.reading(0).unwrap(), bank.reading(1).unwrap()],
                readings
            );
            bank.observe_frame(&[0., 0.]).unwrap();
            assert_eq!(bank.source, limit);
            let before = bank.history.clone();
            assert_eq!(
                bank.observe_frame(&[0., 0.]),
                Err(TruePeakError::FrameCount)
            );
            assert_eq!(bank.history, before);
            bank.finish();
            assert_eq!(bank.tail, 64);
            assert_eq!(bank.phases, (limit + 64) * factor.factor() as u64);
            assert!(bank.last_phase_numerator().unwrap() > i64::MAX as i128);
        }
    }
    #[test]
    fn scaled_steady_window_avoids_actual_intermediate_overflow() {
        let d = TruePeakDesign::new(48000, 32, TruePeakFactor::Eight, 12.).unwrap();
        let value = f64::MAX * 0.9;
        let row = d.phase_coefficients(4).unwrap();
        let naive = row.iter().fold(0., |sum, h| sum + h * value);
        assert!(
            !naive.is_finite(),
            "fixture must distinguish intermediate overflow"
        );
        let mut bank = TruePeakBank64::new(&d, 1, 1).unwrap();
        bank.history.fill(value);
        bank.step(&[value], true);
        let reading = bank.reading(0).unwrap();
        assert!(!reading.amplitude_overflow);
        assert!(reading.interpolated_peak.is_finite());
        assert!((reading.interpolated_peak / value - 1.).abs() < 2e-15);
        println!(
            "WIDE-SCALE steady window: naive intermediate={naive}, scaled peak={:.17e}, original={value:.17e}; naive-order DC={:.17e}, compensated DC={:.17e}",
            reading.interpolated_peak,
            row.iter().sum::<f64>(),
            d.phase_dc_sum(4).unwrap()
        );
    }
    // Private-helper arithmetic only. The final `< MAX_BYTES` comparison on the
    // maximum admitted bank (about 665 KB) is a sanity bound, not a cap-exhaustion
    // test: admitted profiles cannot reach the 8 MiB cap.
    #[test]
    fn numeric_payload_budget_precedes_reservation_and_empty_finish_is_empty() {
        assert_eq!(bytes(MAX_BYTES / 8 + 1), Err(TruePeakError::ResourceLimit));
        assert_eq!(
            reserve(MAX_BYTES / 8 + 1).unwrap_err(),
            TruePeakError::ResourceLimit
        );
        assert_eq!(bytes(usize::MAX), Err(TruePeakError::ResourceLimit));
        let d = TruePeakDesign::new(48000, 512, TruePeakFactor::Sixteen, 16.).unwrap();
        let mut bank = TruePeakBank64::new(&d, 64, 65536).unwrap();
        assert!(
            bytes(bank.history.capacity()).unwrap()
                + bytes(bank.design.coefficients.capacity()).unwrap()
                < MAX_BYTES
        );
        bank.finish();
        for channel in 0..64 {
            let reading = bank.reading(channel).unwrap();
            assert_eq!(reading.source_frames, 0);
            assert_eq!(reading.phase_observations, 0);
            assert_eq!(reading.tail_frames, 0);
            assert!(reading.finalized);
        }
        assert_eq!(bank.last_phase_numerator(), None);
    }
}
