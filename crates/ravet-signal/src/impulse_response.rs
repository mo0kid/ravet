//! Explicit source-rate impulse-response preparation for the native convolution
//! engines ([`Convolver32`]/[`Convolver64`], [`ConvolutionBank32`]/[`ConvolutionBank64`]).
//!
//! [`prepare_impulse_response`] takes planar f64 channels at the response's source
//! rate and an [`ImpulseResponseRequest`] that names every policy: target engine
//! rate and [`ResponseRateConversion`], [`ResponseChannelMap`], [`ResponseTrim`]
//! thresholds and [`ResponseNormalization`]. Nothing is implicit: there is no
//! automatic resampling, channel guess, silence trimming or level change.
//!
//! The stages run in a fixed order: channel mapping, rate conversion (with its
//! explicit gain and delay policy), leading/trailing trim, then normalization.
//! Every offset and gain is recorded in the [`ImpulseResponseReport`].
//!
//! This is CONTROL/worker work. It allocates (fallibly for the result and staging
//! storage; the bounded existing [`SincResampler`] table and history use its own
//! infallible allocation, counted in the declared budget) and must not run on an audio
//! thread. Bounds are checked by [`ImpulseResponseRequest::plan`] before any
//! allocation and a refused call returns no partial response.
//!
//! ```
//! use ravet_signal::{
//!     ConvolutionPartitioning, ImpulseResponseRequest, ResponseChannelMap,
//!     ResponseNormalization, ResponseRateConversion, ResponseTrim, TrimThreshold,
//!     prepare_impulse_response,
//! };
//! let left = [0.0, 0.0, 0.5, -0.25, 0.0];
//! let right = [0.0, 0.0, 0.0, 1.0, 0.0];
//! let request = ImpulseResponseRequest {
//!     target_rate: 48_000,
//!     conversion: ResponseRateConversion::RequireEqualRates,
//!     channels: ResponseChannelMap::Identity,
//!     trim: ResponseTrim {
//!         leading: Some(TrimThreshold::Absolute(0.0)),
//!         trailing: Some(TrimThreshold::Absolute(0.0)),
//!     },
//!     normalization: ResponseNormalization::Peak { target: 0.5 },
//!     maximum_taps: 16,
//!     maximum_working_bytes: 1 << 20,
//! };
//! let prepared = prepare_impulse_response(48_000, &[&left, &right], &request)?;
//! assert_eq!(prepared.report().leading_trimmed_frames, 2);
//! assert_eq!(prepared.report().trailing_trimmed_frames, 1);
//! assert_eq!(prepared.report().normalization_gain, 0.5);
//! assert_eq!(prepared.channel(0), Some(&[0.25, -0.125][..]));
//! assert_eq!(prepared.channel(1), Some(&[0.0, 0.5][..]));
//! let bank = prepared.bank64(ConvolutionPartitioning::ZeroLatency { head: 16 }, 64, 1 << 20)?;
//! assert_eq!(bank.channels(), 2);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
use crate::{
    ConvolutionBank32, ConvolutionBank64, ConvolutionPartitioning, ConvolutionPreparationError,
    Convolver32, Convolver64, SincResampler,
};

/// Maximum source and prepared channels.
pub const IMPULSE_RESPONSE_MAX_CHANNELS: usize = 64;
/// Maximum source frames per channel (sixteen times the engine tap limit).
pub const IMPULSE_RESPONSE_MAX_SOURCE_FRAMES: usize = 4_194_304;
/// Maximum prepared taps per channel; equals the convolution engine limit.
pub const IMPULSE_RESPONSE_MAX_TAPS: usize = 262_144;
/// Upper bound for [`ImpulseResponseRequest::maximum_working_bytes`].
pub const IMPULSE_RESPONSE_MAX_WORKING_BYTES: usize = 256 * 1024 * 1024;
const MAX_RATE: u32 = 768_000;
const SINC_PHASES: usize = 1025;

/// Explicit sample-rate conversion policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponseRateConversion {
    /// Exact copy. Refused unless the source and target rates are equal.
    RequireEqualRates,
    /// Convert with the existing [`SincResampler::new_integer_rates`] profile
    /// (1025-phase Blackman windowed sinc, exact integer schedule), also when the
    /// rates are equal. Even `taps` in 16..=256; rate ratio 1/16..=16.
    Sinc {
        /// Even sinc length, 16..=256.
        taps: usize,
        /// Explicit amplitude policy for the changed sample density.
        gain: ResponseRateGain,
        /// Explicit treatment of the resampler's causal filter delay.
        delay: ResponseDelay,
    },
}

/// What a rate change preserves. Resampling an impulse response changes the
/// number of samples that represent the same continuous response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponseRateGain {
    /// Multiply every converted sample by `source_rate / target_rate`, so the
    /// convolution frequency response (including DC gain) is approximately
    /// preserved at the target rate.
    PreserveResponseGain,
    /// Keep interpolated sample amplitudes (gain 1). DC gain then scales by
    /// `target_rate / source_rate`.
    PreserveSampleAmplitude,
}

/// Treatment of the sinc converter's `(taps - 1) / 2` source-sample delay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponseDelay {
    /// Keep the complete causal delay and tail; the response onset is delayed by
    /// `(taps - 1) * target_rate / (2 * source_rate)` target frames.
    Retain,
    /// Discard `floor((taps - 1) * target_rate / (2 * source_rate))` leading
    /// converted frames. The fractional remainder (in `[0, 1)` target frames) stays
    /// in the response and is reported, never interpolated away.
    RemoveWholeTargetFrames,
}

/// Explicit output-channel mapping from source channels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponseChannelMap<'a> {
    /// Output channel `i` is source channel `i`; same count.
    Identity,
    /// Output channel `i` is source channel `map[i]`. 1..=64 entries, each a valid
    /// source index; duplicates (for example mono to stereo `[0, 0]`) and reorders
    /// are allowed. There is no mixing or crossfeed.
    Select(&'a [usize]),
}

/// One explicit trim threshold. A frame is kept when any channel's absolute
/// sample is strictly greater than the resolved threshold.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrimThreshold {
    /// Finite absolute amplitude `>= 0`.
    Absolute(f64),
    /// Finite fraction in `[0, 1)` of the linked peak after rate conversion.
    RelativeToPeak(f64),
}

/// Explicit leading/trailing trim. `None` keeps that end unchanged. Trimming is
/// linked across channels so inter-channel timing is preserved, and the removed
/// frame counts are reported.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ResponseTrim {
    /// Remove frames before the first frame above this threshold.
    pub leading: Option<TrimThreshold>,
    /// Remove frames after the last frame above this threshold.
    pub trailing: Option<TrimThreshold>,
}

/// Explicit linked normalization; one gain is applied to every channel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ResponseNormalization {
    /// No gain change (gain reported as exactly 1).
    None,
    /// Gain `target / peak` where peak is the largest absolute sample of all channels.
    Peak {
        /// Finite positive target peak.
        target: f64,
    },
    /// Gain `sqrt(target / energy)` where energy is the largest per-channel sum of
    /// squares (summed in frame order). The sum is formed after scaling by an
    /// exact power of two near `1 / peak`, so huge finite samples are not zeroed
    /// and tiny non-silent responses are not refused as silent; an overflowing
    /// gain is [`ImpulseResponseError::NonFinite`] and an underflowing one is
    /// [`ImpulseResponseError::UnrepresentableGain`].
    Energy {
        /// Finite positive target sum of squares for the loudest channel.
        target: f64,
    },
}

/// Complete explicit preparation request. There are no defaults.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImpulseResponseRequest<'a> {
    /// Engine sample rate in Hz, 1..=768,000.
    pub target_rate: u32,
    /// Rate conversion policy.
    pub conversion: ResponseRateConversion,
    /// Channel mapping.
    pub channels: ResponseChannelMap<'a>,
    /// Trim policy.
    pub trim: ResponseTrim,
    /// Normalization policy.
    pub normalization: ResponseNormalization,
    /// Largest accepted prepared length per channel, 1..=262,144.
    pub maximum_taps: usize,
    /// Budget for result, staging and sinc table bytes, 1..=256 MiB. Caller-owned
    /// source data and allocator overhead are excluded.
    pub maximum_working_bytes: usize,
}

/// Exact pre-allocation coordinates computed by [`ImpulseResponseRequest::plan`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImpulseResponsePlan {
    /// Prepared channel count.
    pub channels: usize,
    /// Frames produced by the converter before delay removal (equals source frames
    /// for an exact copy). Sinc: `floor((N + taps - 2) * target / source) + 1`.
    pub staged_frames: usize,
    /// Leading converted frames removed by [`ResponseDelay::RemoveWholeTargetFrames`].
    pub removed_delay_frames: usize,
    /// Frames per channel before trimming.
    pub converted_frames: usize,
    /// Result, staging and sinc table bytes this request will allocate.
    pub declared_working_bytes: usize,
}

/// Recorded offsets, gains and levels of one successful preparation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImpulseResponseReport {
    /// Source rate in Hz.
    pub source_rate: u32,
    /// Target engine rate in Hz.
    pub target_rate: u32,
    /// Source channel count.
    pub source_channels: usize,
    /// Prepared channel count.
    pub channels: usize,
    /// Source frames per channel.
    pub source_frames: usize,
    /// Converter output frames before delay removal.
    pub staged_frames: usize,
    /// Frames per channel after conversion and delay removal, before trimming.
    pub converted_frames: usize,
    /// Prepared frames per channel.
    pub frames: usize,
    /// Conversion policy applied.
    pub conversion: ResponseRateConversion,
    /// Gain applied by the rate policy (exactly 1 for a copy or
    /// [`ResponseRateGain::PreserveSampleAmplitude`]).
    pub rate_gain: f64,
    /// Leading frames removed by the delay policy.
    pub removed_delay_frames: usize,
    /// Converter delay still present at the start of the converted response, in
    /// target frames (0 for an exact copy).
    pub retained_delay_target_frames: f64,
    /// Linked leading frames removed by trimming.
    pub leading_trimmed_frames: usize,
    /// Linked trailing frames removed by trimming.
    pub trailing_trimmed_frames: usize,
    /// Resolved absolute leading threshold, when trimming was requested.
    pub leading_threshold: Option<f64>,
    /// Resolved absolute trailing threshold, when trimming was requested.
    pub trailing_threshold: Option<f64>,
    /// Normalization policy applied.
    pub normalization: ResponseNormalization,
    /// Normalization gain applied (exactly 1 for none).
    pub normalization_gain: f64,
    /// Linked peak after conversion and trimming, before normalization.
    pub peak_before_normalization: f64,
    /// Linked peak of the prepared response.
    pub peak: f64,
    /// Largest per-channel sum of squares of the prepared response (computed
    /// with power-of-two scaling; the reported value itself can round to 0 or
    /// infinity for responses near the f64 range limits).
    pub maximum_channel_energy: f64,
    /// Bytes declared and budgeted before allocation.
    pub declared_working_bytes: usize,
}

/// Whole-call refusal. No partial response is returned; borrowed input is unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImpulseResponseError {
    /// A source or target rate is outside 1..=768,000 Hz.
    InvalidRate,
    /// Source has 0 or more than 64 channels, unequal lengths, 0 or more than
    /// 4,194,304 frames, or a nonfinite sample.
    InvalidSource,
    /// Channel map is empty, longer than 64 or names a missing source channel.
    InvalidChannelMap,
    /// Unequal rates under `RequireEqualRates`, invalid taps or ratio outside 1/16..=16.
    InvalidConversion,
    /// A trim threshold is nonfinite, negative or (relative) not below 1.
    InvalidTrim,
    /// A normalization target is nonfinite or not positive.
    InvalidNormalization,
    /// `maximum_taps` or `maximum_working_bytes` is outside its range, or the
    /// declared bytes exceed the budget.
    Capacity,
    /// A fallible reservation was refused.
    Allocation,
    /// No frame exceeds a trim threshold.
    BelowThreshold,
    /// The prepared length exceeds `maximum_taps`.
    TooLong,
    /// Normalization was requested for an all-zero response.
    Silent,
    /// A gain overflowed or produced a nonfinite sample.
    NonFinite,
    /// A normalization gain underflows below the smallest normal f64.
    UnrepresentableGain,
    /// The converter produced an unexpected frame count (internal consistency).
    Conversion,
}

impl core::fmt::Display for ImpulseResponseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::InvalidRate => "impulse-response rates must be 1..768000 Hz",
            Self::InvalidSource => {
                "impulse-response source is empty, unequal, too long or nonfinite"
            }
            Self::InvalidChannelMap => "impulse-response channel map is invalid",
            Self::InvalidConversion => "impulse-response rate conversion policy is invalid",
            Self::InvalidTrim => "impulse-response trim threshold is invalid",
            Self::InvalidNormalization => "impulse-response normalization target is invalid",
            Self::Capacity => "impulse-response preparation exceeds its declared bounds",
            Self::Allocation => "impulse-response preparation allocation failed",
            Self::BelowThreshold => "no impulse-response frame exceeds the trim threshold",
            Self::TooLong => "prepared impulse response exceeds maximum taps",
            Self::Silent => "cannot normalize an all-zero impulse response",
            Self::NonFinite => "impulse-response gain produced a nonfinite sample",
            Self::UnrepresentableGain => "impulse-response normalization gain is not representable",
            Self::Conversion => "impulse-response conversion produced an unexpected length",
        })
    }
}
impl core::error::Error for ImpulseResponseError {}

impl ImpulseResponseRequest<'_> {
    fn output_channels(&self, source_channels: usize) -> Result<usize, ImpulseResponseError> {
        match self.channels {
            ResponseChannelMap::Identity => Ok(source_channels),
            ResponseChannelMap::Select(map) => {
                if map.is_empty()
                    || map.len() > IMPULSE_RESPONSE_MAX_CHANNELS
                    || map.iter().any(|&c| c >= source_channels)
                {
                    Err(ImpulseResponseError::InvalidChannelMap)
                } else {
                    Ok(map.len())
                }
            }
        }
    }
    fn source_index(&self, channel: usize) -> usize {
        match self.channels {
            ResponseChannelMap::Identity => channel,
            ResponseChannelMap::Select(map) => map[channel],
        }
    }
    /// Validate every policy and bound for a source shape and compute exact frame
    /// counts and declared bytes, without allocating or reading samples. Data-
    /// dependent refusals (nonfinite input, threshold, length after trim, silence)
    /// are decided by [`prepare_impulse_response`].
    pub fn plan(
        &self,
        source_rate: u32,
        source_channels: usize,
        source_frames: usize,
    ) -> Result<ImpulseResponsePlan, ImpulseResponseError> {
        if !(1..=MAX_RATE).contains(&source_rate) || !(1..=MAX_RATE).contains(&self.target_rate) {
            return Err(ImpulseResponseError::InvalidRate);
        }
        if !(1..=IMPULSE_RESPONSE_MAX_CHANNELS).contains(&source_channels)
            || !(1..=IMPULSE_RESPONSE_MAX_SOURCE_FRAMES).contains(&source_frames)
        {
            return Err(ImpulseResponseError::InvalidSource);
        }
        let channels = self.output_channels(source_channels)?;
        let (input, output) = (u128::from(source_rate), u128::from(self.target_rate));
        let (staged, removed, table) = match self.conversion {
            ResponseRateConversion::RequireEqualRates => {
                if source_rate != self.target_rate {
                    return Err(ImpulseResponseError::InvalidConversion);
                }
                (source_frames as u128, 0u128, 0u128)
            }
            ResponseRateConversion::Sinc { taps, delay, .. } => {
                if !(16..=256).contains(&taps)
                    || !taps.is_multiple_of(2)
                    || input > 16 * output
                    || output > 16 * input
                {
                    return Err(ImpulseResponseError::InvalidConversion);
                }
                let t = taps as u128;
                let staged = (source_frames as u128 + t - 2) * output / input + 1;
                let removed = match delay {
                    ResponseDelay::Retain => 0,
                    ResponseDelay::RemoveWholeTargetFrames => (t - 1) * output / (2 * input),
                };
                (staged, removed, (SINC_PHASES as u128 + 1) * t * 8)
            }
        };
        for threshold in [self.trim.leading, self.trim.trailing]
            .into_iter()
            .flatten()
        {
            let valid = match threshold {
                TrimThreshold::Absolute(a) => a.is_finite() && a >= 0.,
                TrimThreshold::RelativeToPeak(r) => r.is_finite() && (0. ..1.).contains(&r),
            };
            if !valid {
                return Err(ImpulseResponseError::InvalidTrim);
            }
        }
        match self.normalization {
            ResponseNormalization::None => {}
            ResponseNormalization::Peak { target } | ResponseNormalization::Energy { target } => {
                if !target.is_finite() || target <= 0. {
                    return Err(ImpulseResponseError::InvalidNormalization);
                }
            }
        }
        if !(1..=IMPULSE_RESPONSE_MAX_TAPS).contains(&self.maximum_taps)
            || !(1..=IMPULSE_RESPONSE_MAX_WORKING_BYTES).contains(&self.maximum_working_bytes)
        {
            return Err(ImpulseResponseError::Capacity);
        }
        let converted = staged - removed;
        if self.trim.leading.is_none()
            && self.trim.trailing.is_none()
            && converted > self.maximum_taps as u128
        {
            return Err(ImpulseResponseError::TooLong);
        }
        let staging = if table == 0 { 0 } else { staged * 8 };
        let bytes = channels as u128 * converted * 8 + staging + table;
        if bytes > self.maximum_working_bytes as u128 {
            return Err(ImpulseResponseError::Capacity);
        }
        // Bounded by the 256 MiB check above, so these narrow without loss.
        Ok(ImpulseResponsePlan {
            channels,
            staged_frames: staged as usize,
            removed_delay_frames: removed as usize,
            converted_frames: converted as usize,
            declared_working_bytes: bytes as usize,
        })
    }
}

fn zeroed(count: usize) -> Result<Vec<f64>, ImpulseResponseError> {
    let mut v = Vec::new();
    v.try_reserve_exact(count)
        .map_err(|_| ImpulseResponseError::Allocation)?;
    v.resize(count, 0.);
    Ok(v)
}

fn linked_peak(samples: &[f64]) -> f64 {
    samples.iter().fold(0., |m: f64, x| m.max(x.abs()))
}

/// Power of two `s` near `1 / peak` with `peak * s` in `[1, 2)` where
/// representable (exponent clamped so `s` itself is a normal finite value).
fn power_of_two_scale(peak: f64) -> f64 {
    let exponent = peak.log2().floor().clamp(-1023., 1022.) as i64;
    f64::from_bits(((1023 - exponent) as u64) << 52)
}

/// Largest per-channel sum of squares, overflow/underflow safe: samples are
/// multiplied by an exact power of two `s` near `1 / peak` before squaring and
/// `(sum, s)` is returned, so the true energy is `sum / s^2`. When no scaled
/// value over/underflows this equals the direct sum scaled exactly by `s^2`,
/// so `sqrt(target / sum) * s` is bitwise `sqrt(target / energy)`.
fn scaled_energy(samples: &[f64], frames: usize, peak: f64) -> (f64, f64) {
    let scale = power_of_two_scale(peak);
    let sum = samples
        .chunks_exact(frames)
        .map(|c| {
            c.iter().fold(0., |e, x| {
                let y = x * scale;
                e + y * y
            })
        })
        .fold(0., f64::max);
    (sum, scale)
}

fn resolve(threshold: TrimThreshold, peak: f64) -> f64 {
    match threshold {
        TrimThreshold::Absolute(a) => a,
        TrimThreshold::RelativeToPeak(r) => r * peak,
    }
}

/// Prepare a response from planar f64 source channels at `source_rate` for the
/// request's target engine rate. Every policy, bound and the declared byte budget
/// is checked (see [`ImpulseResponseRequest::plan`]) and the whole source is
/// checked for equal lengths and finiteness before any allocation. Stages run in
/// the fixed order map, convert, trim, normalize; a refusal at any stage returns
/// no partial result.
///
/// Sinc conversion feeds the source and exactly enough explicit zeros (at most
/// `taps`) through a freshly reset [`SincResampler`] per output channel and keeps
/// `ImpulseResponsePlan::staged_frames` frames. Converted samples are
/// `staged * rate_gain`; normalized samples are `trimmed * normalization_gain`,
/// each one f64 rounding. An exact copy performs no arithmetic.
///
/// This allocates and is CONTROL/worker work only. No deadline is implied.
pub fn prepare_impulse_response(
    source_rate: u32,
    source: &[&[f64]],
    request: &ImpulseResponseRequest<'_>,
) -> Result<PreparedImpulseResponse, ImpulseResponseError> {
    let frames = source.first().map_or(0, |c| c.len());
    let plan = request.plan(source_rate, source.len(), frames)?;
    if source.iter().any(|c| c.len() != frames)
        || source.iter().any(|c| c.iter().any(|x| !x.is_finite()))
    {
        return Err(ImpulseResponseError::InvalidSource);
    }
    let channels = plan.channels;
    let converted = plan.converted_frames;
    let mut samples = zeroed(channels * converted)?;
    let mut rate_gain = 1.;
    let mut retained_delay = 0.;
    match request.conversion {
        ResponseRateConversion::RequireEqualRates => {
            for (channel, out) in samples.chunks_exact_mut(converted).enumerate() {
                out.copy_from_slice(source[request.source_index(channel)]);
            }
        }
        ResponseRateConversion::Sinc { taps, gain, delay } => {
            let mut staging = zeroed(plan.staged_frames)?;
            let mut resampler =
                SincResampler::new_integer_rates(source_rate, request.target_rate, taps)
                    .map_err(|_| ImpulseResponseError::InvalidConversion)?;
            if gain == ResponseRateGain::PreserveResponseGain {
                rate_gain = f64::from(source_rate) / f64::from(request.target_rate);
            }
            let numerator = (taps as u64 - 1) * u64::from(request.target_rate);
            let denominator = 2 * u64::from(source_rate);
            retained_delay = match delay {
                ResponseDelay::Retain => numerator as f64 / denominator as f64,
                ResponseDelay::RemoveWholeTargetFrames => {
                    (numerator % denominator) as f64 / denominator as f64
                }
            };
            let zeros = [0.; 256];
            for (channel, out) in samples.chunks_exact_mut(converted).enumerate() {
                resampler.reset();
                let input = source[request.source_index(channel)];
                let (mut consumed, mut padding, mut produced) = (0, 0, 0);
                while produced < staging.len() {
                    let (feed, is_padding) = if consumed < input.len() {
                        (&input[consumed..], false)
                    } else {
                        (&zeros[..taps - padding], true)
                    };
                    let p = resampler.process(feed, &mut staging[produced..]);
                    if p.consumed == 0 && p.produced == 0 {
                        return Err(ImpulseResponseError::Conversion);
                    }
                    produced += p.produced;
                    if is_padding {
                        padding += p.consumed;
                    } else {
                        consumed += p.consumed;
                    }
                }
                let kept = &staging[plan.removed_delay_frames..];
                if rate_gain == 1. {
                    out.copy_from_slice(kept);
                } else {
                    for (y, x) in out.iter_mut().zip(kept) {
                        *y = x * rate_gain;
                    }
                }
            }
            drop(staging);
            if samples.iter().any(|x| !x.is_finite()) {
                return Err(ImpulseResponseError::NonFinite);
            }
        }
    }
    // Linked trim on converted samples.
    let converted_peak = linked_peak(&samples);
    let frame_above = |frame: usize, threshold: f64| {
        (0..channels).any(|c| samples[c * converted + frame].abs() > threshold)
    };
    let leading_threshold = request.trim.leading.map(|t| resolve(t, converted_peak));
    let trailing_threshold = request.trim.trailing.map(|t| resolve(t, converted_peak));
    let start = match leading_threshold {
        None => 0,
        Some(t) => (0..converted)
            .find(|&f| frame_above(f, t))
            .ok_or(ImpulseResponseError::BelowThreshold)?,
    };
    let end = match trailing_threshold {
        None => converted,
        Some(t) => {
            (0..converted)
                .rev()
                .find(|&f| frame_above(f, t))
                .ok_or(ImpulseResponseError::BelowThreshold)?
                + 1
        }
    };
    if end <= start {
        return Err(ImpulseResponseError::BelowThreshold);
    }
    let kept = end - start;
    if kept > request.maximum_taps {
        return Err(ImpulseResponseError::TooLong);
    }
    if kept != converted {
        for channel in 0..channels {
            let from = channel * converted + start;
            samples.copy_within(from..from + kept, channel * kept);
        }
        samples.truncate(channels * kept);
    }
    let peak_before = linked_peak(&samples);
    let normalization_gain = match request.normalization {
        ResponseNormalization::None => 1.,
        ResponseNormalization::Peak { target } => {
            if peak_before == 0. {
                return Err(ImpulseResponseError::Silent);
            }
            target / peak_before
        }
        ResponseNormalization::Energy { target } => {
            if peak_before == 0. {
                return Err(ImpulseResponseError::Silent);
            }
            let (scaled, scale) = scaled_energy(&samples, kept, peak_before);
            (target / scaled).sqrt() * scale
        }
    };
    if !normalization_gain.is_finite()
        || samples
            .iter()
            .any(|x| !(x * normalization_gain).is_finite())
    {
        return Err(ImpulseResponseError::NonFinite);
    }
    if normalization_gain < f64::MIN_POSITIVE {
        return Err(ImpulseResponseError::UnrepresentableGain);
    }
    if request.normalization != ResponseNormalization::None {
        for x in &mut samples {
            *x *= normalization_gain;
        }
    }
    let report = ImpulseResponseReport {
        source_rate,
        target_rate: request.target_rate,
        source_channels: source.len(),
        channels,
        source_frames: frames,
        staged_frames: plan.staged_frames,
        converted_frames: converted,
        frames: kept,
        conversion: request.conversion,
        rate_gain,
        removed_delay_frames: plan.removed_delay_frames,
        retained_delay_target_frames: retained_delay,
        leading_trimmed_frames: start,
        trailing_trimmed_frames: converted - end,
        leading_threshold,
        trailing_threshold,
        normalization: request.normalization,
        normalization_gain,
        peak_before_normalization: peak_before,
        peak: linked_peak(&samples),
        maximum_channel_energy: {
            let peak = linked_peak(&samples);
            if peak == 0. {
                0.
            } else {
                let (scaled, scale) = scaled_energy(&samples, kept, peak);
                scaled / (scale * scale)
            }
        },
        declared_working_bytes: plan.declared_working_bytes,
    };
    Ok(PreparedImpulseResponse {
        samples,
        channels,
        frames: kept,
        report,
    })
}

/// Explicit round-to-nearest f64-to-f32 copy with a fallible reservation.
fn narrow(samples: &[f64]) -> Result<Vec<f32>, ConvolutionPreparationError> {
    let mut v = Vec::new();
    v.try_reserve_exact(samples.len())
        .map_err(|_| ConvolutionPreparationError::Allocation)?;
    v.extend(samples.iter().map(|&x| x as f32));
    Ok(v)
}

/// Owned prepared planar f64 response at the target engine rate plus its report.
/// Building engines or banks from it copies the samples into the engine's own
/// storage on CONTROL; this value can then be dropped. Builder scratch (slice
/// tables and the f32 narrow copy) is reserved fallibly and reported as
/// [`ConvolutionPreparationError::Allocation`]; it is not counted in the engine
/// numeric budget.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedImpulseResponse {
    samples: Vec<f64>,
    channels: usize,
    frames: usize,
    report: ImpulseResponseReport,
}

impl PreparedImpulseResponse {
    /// Prepared channel count.
    pub fn channels(&self) -> usize {
        self.channels
    }
    /// Prepared frames (taps) per channel.
    pub fn frames(&self) -> usize {
        self.frames
    }
    /// One prepared channel's taps.
    pub fn channel(&self, channel: usize) -> Option<&[f64]> {
        (channel < self.channels)
            .then(|| &self.samples[channel * self.frames..(channel + 1) * self.frames])
    }
    /// Recorded offsets, gains and levels.
    pub fn report(&self) -> &ImpulseResponseReport {
        &self.report
    }
    /// Build a [`ConvolutionBank64`] from all channels (CONTROL, allocates).
    pub fn bank64(
        &self,
        partitioning: ConvolutionPartitioning,
        maximum_frames: usize,
        maximum_numeric_bytes: usize,
    ) -> Result<ConvolutionBank64, ConvolutionPreparationError> {
        let mut refs: Vec<&[f64]> = Vec::new();
        refs.try_reserve_exact(self.channels)
            .map_err(|_| ConvolutionPreparationError::Allocation)?;
        refs.extend(self.samples.chunks_exact(self.frames));
        ConvolutionBank64::new(&refs, partitioning, maximum_frames, maximum_numeric_bytes)
    }
    /// Build a [`ConvolutionBank32`] after explicit round-to-nearest f64-to-f32
    /// conversion of every tap. Values outside the f32 range become infinite and
    /// the bank refuses them as an invalid response.
    pub fn bank32(
        &self,
        partitioning: ConvolutionPartitioning,
        maximum_frames: usize,
        maximum_numeric_bytes: usize,
    ) -> Result<ConvolutionBank32, ConvolutionPreparationError> {
        let narrow = narrow(&self.samples)?;
        let mut refs: Vec<&[f32]> = Vec::new();
        refs.try_reserve_exact(self.channels)
            .map_err(|_| ConvolutionPreparationError::Allocation)?;
        refs.extend(narrow.chunks_exact(self.frames));
        ConvolutionBank32::new(&refs, partitioning, maximum_frames, maximum_numeric_bytes)
    }
    /// Build one [`Convolver64`] for `channel`. A missing channel is refused as
    /// [`ConvolutionPreparationError::InvalidChannels`].
    pub fn engine64(
        &self,
        channel: usize,
        partitioning: ConvolutionPartitioning,
        maximum_frames: usize,
        maximum_numeric_bytes: usize,
    ) -> Result<Convolver64, ConvolutionPreparationError> {
        let taps = self
            .channel(channel)
            .ok_or(ConvolutionPreparationError::InvalidChannels)?;
        Convolver64::new(taps, partitioning, maximum_frames, maximum_numeric_bytes)
    }
    /// Build one [`Convolver32`] for `channel` with explicit f32 rounding.
    pub fn engine32(
        &self,
        channel: usize,
        partitioning: ConvolutionPartitioning,
        maximum_frames: usize,
        maximum_numeric_bytes: usize,
    ) -> Result<Convolver32, ConvolutionPreparationError> {
        let taps = self
            .channel(channel)
            .ok_or(ConvolutionPreparationError::InvalidChannels)?;
        let narrow = narrow(taps)?;
        Convolver32::new(&narrow, partitioning, maximum_frames, maximum_numeric_bytes)
    }
    /// Consume into `(planar samples, channels, frames, report)`.
    pub fn into_parts(self) -> (Vec<f64>, usize, usize, ImpulseResponseReport) {
        (self.samples, self.channels, self.frames, self.report)
    }
}
