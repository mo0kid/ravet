//! Bounded paired dry/wet ownership with explicit causal wet-latency compensation.
//!
//! ```
//! use ravet_signal::{DryWetMixer, DryWetMixerConfig, DryWetGains,
//!     DelayInterpolation, PanningLaw, PanningNormalization};
//! let config = DryWetMixerConfig { channels: 1, maximum_frames: 4,
//!     fifo_capacity_frames: 4, maximum_latency_samples: 8,
//!     interpolation: DelayInterpolation::Integer, ramp_frames: 0 };
//! let gains = DryWetGains::new(0.5, PanningLaw::Linear,
//!     PanningNormalization::UnityEndpoints).unwrap();
//! let mut mixer = DryWetMixer::new(config, gains).unwrap();
//! mixer.set_wet_latency_samples(1.).unwrap();
//! mixer.push_dry(&[&[1., 2., 3.]], 3).unwrap();
//! let mut output = [99.; 3];
//! mixer.mix_wet(&[&[0., 1., 2.]], &mut [&mut output], 3).unwrap();
//! assert_eq!(output, [0., 1., 2.]);
//! assert_eq!(mixer.queued_frames(), 0);
//! ```
use crate::{
    DelayInterpolation, PanningGains, PanningGains32, PanningLaw, PanningNormalization,
    StereoPanner, StereoPanner32,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DryWetError {
    InvalidMix,
    Configuration,
    Allocation,
    InvalidLatency,
    PendingDry,
    ChannelCount,
    BufferTooShort,
    TooManyFrames,
    QueueFull,
    InsufficientDry,
    CountOverflow,
}
impl std::fmt::Display for DryWetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidMix => "wet fraction must be finite within 0..1",
            Self::Configuration => {
                "dry/wet channels, block, queue or aggregate32MiB capacity invalid"
            }
            Self::Allocation => "dry/wet storage reservation failed",
            Self::InvalidLatency => "latency or native fractional reconstruction is unavailable",
            Self::PendingDry => "changed wet latency requires an empty dry queue",
            Self::ChannelCount => "dry/wet channel count differs from prepared layout",
            Self::BufferTooShort => "dry/wet channel is shorter than active prefix",
            Self::TooManyFrames => "dry/wet prefix exceeds prepared maximum",
            Self::QueueFull => "dry prefix exceeds available queue frames",
            Self::InsufficientDry => "wet prefix exceeds queued dry frames",
            Self::CountOverflow => "dry/wet epoch frame counter would overflow",
        })
    }
}
impl std::error::Error for DryWetError {}
/// All preparation bounds are explicit; no default rate, time or interpolation.
/// Queue frames1..65536, block1..65536, channels1..64, maximum latency0..4194300.
/// Combined native queue/history and fixed recursive values must fit32MiB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DryWetMixerConfig {
    pub channels: usize,
    pub maximum_frames: usize,
    pub fifo_capacity_frames: usize,
    pub maximum_latency_samples: usize,
    pub interpolation: DelayInterpolation,
    pub ramp_frames: u32,
}
/// Prepared direct wet coordinate0..1. The full seven curves and explicit
/// normalization are shared with panning; a tiny wet value is not lost through
/// an affine conversion to pan. Center normalization can boost weights up to2.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DryWetGains {
    wet: f64,
    profile: PanningGains,
}
impl DryWetGains {
    pub fn new(
        wet_fraction: f64,
        law: PanningLaw,
        normalization: PanningNormalization,
    ) -> Result<Self, DryWetError> {
        if !wet_fraction.is_finite() || !(0.0..=1.).contains(&wet_fraction) {
            return Err(DryWetError::InvalidMix);
        }
        Ok(Self {
            wet: wet_fraction,
            profile: PanningGains::from_unit(wet_fraction, law, normalization),
        })
    }
    pub fn wet_fraction(&self) -> f64 {
        self.wet
    }
    pub fn law(&self) -> PanningLaw {
        self.profile.law()
    }
    pub fn normalization(&self) -> PanningNormalization {
        self.profile.normalization()
    }
    pub fn gains(&self) -> [f64; 2] {
        self.profile.gains()
    }
    pub fn quantize_f32(&self) -> DryWetGains32 {
        DryWetGains32 {
            prototype: *self,
            profile: self.profile.quantize_f32(),
        }
    }
}
/// Actual rounded f32 mix weights; requested metadata remains explicit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DryWetGains32 {
    prototype: DryWetGains,
    profile: PanningGains32,
}
impl DryWetGains32 {
    pub fn wet_fraction(&self) -> f64 {
        self.prototype.wet_fraction()
    }
    pub fn law(&self) -> PanningLaw {
        self.prototype.law()
    }
    pub fn normalization(&self) -> PanningNormalization {
        self.prototype.normalization()
    }
    pub fn gains(&self) -> [f32; 2] {
        self.profile.gains()
    }
}
const BUDGET: usize = 32 * 1024 * 1024;
fn sizes(c: DryWetMixerConfig, sample_bytes: usize) -> Result<(usize, usize), DryWetError> {
    if !(1..=64).contains(&c.channels)
        || !(1..=65536).contains(&c.maximum_frames)
        || !(1..=65536).contains(&c.fifo_capacity_frames)
        || c.maximum_latency_samples > 4_194_300
    {
        return Err(DryWetError::Configuration);
    }
    let ring = c
        .maximum_latency_samples
        .checked_add(4)
        .ok_or(DryWetError::Configuration)?;
    let queue = c
        .channels
        .checked_mul(c.fifo_capacity_frames)
        .ok_or(DryWetError::Configuration)?;
    let history = c
        .channels
        .checked_mul(ring)
        .ok_or(DryWetError::Configuration)?;
    bytes(queue, history, sample_bytes)?;
    Ok((queue, history))
}
fn bytes(queue: usize, history: usize, sample_bytes: usize) -> Result<(), DryWetError> {
    queue
        .checked_add(history)
        .and_then(|n| n.checked_add(128))
        .and_then(|n| n.checked_mul(sample_bytes))
        .filter(|&n| n <= BUDGET)
        .ok_or(DryWetError::Configuration)
        .map(|_| ())
}
macro_rules! mixer {
    ($name:ident,$sample:ty,$profile:ty,$panner:ty) => {
        /// Single-owner native dry/wet consumer. CONTROL construction/reservation
        /// and disposal own storage; push/mix/validated edits/reset use no heap,
        /// locks or I/O. Reset visits full allocated history/queue, not O(1).
        ///
        /// Push queues ALREADY delayed dry and advances source history once per
        /// accepted source frame. Mix consumes in order and advances one shared
        /// gain pair once per frame, independent of channel count. Caller wet
        /// frames must correspond to that same source sequence. No authentication,
        /// clock repair, latency inference or missing-push recovery is performed.
        /// Every prefix admission is atomic; a failure changes no stream state,
        /// counters or output. Empty calls advance nothing; suffixes are untouched.
        ///
        /// For d=n+f, Integer truncates, Linear combines taps n/n+1, cubic Lagrange
        /// uses four causal nodes n..n+3 and Thiran has a=(1-f)/(1+f),
        /// y=a*v+previous_v-a*previous_y. An integer-valued Thiran delay
        /// bypasses recursion and updates both prior values. Fractional FIR is not broadband ideal delay and can overshoot;
        /// fixed Thiran is ideal allpass with frequency-varying group delay and
        /// infinite tail. Native32 rejects nonzero f rounding to0/1; both precisions
        /// reject fractional Thiran when actual a rounds to1. Requested latency
        /// and native parameters remain distinct.
        ///
        /// Changed latency requires an empty queue; identical requested latency
        /// is an idempotent no-op. Immediate retapping retains history without
        /// crossfade/modulation guarantees. Nonfinite/overflow input can propagate
        /// and poison delay until reset. Caller owns headroom/tail/EOF/host latency
        /// denormal handling and deadline policy. Gain-pair interpolation is f64;
        /// delay reconstruction (weights/Thiran recursion) and audio products/sums
        /// use the named native precision, without hidden FMA or normalization repair.
        #[derive(Debug)]
        pub struct $name {
            config: DryWetMixerConfig,
            queue: Vec<$sample>,
            history: Vec<$sample>,
            previous_input: [$sample; 64],
            previous_output: [$sample; 64],
            cursor: usize,
            read: usize,
            write: usize,
            queued: usize,
            requested_delay: f64,
            whole: usize,
            fraction: $sample,
            coefficient: $sample,
            target: $profile,
            panner: $panner,
            accepted: u64,
            consumed: u64,
        }
        impl $name {
            pub fn new(config: DryWetMixerConfig, target: $profile) -> Result<Self, DryWetError> {
                let (q, h) = sizes(config, std::mem::size_of::<$sample>())?;
                let mut queue = Vec::new();
                let mut history = Vec::new();
                queue
                    .try_reserve_exact(q)
                    .map_err(|_| DryWetError::Allocation)?;
                history
                    .try_reserve_exact(h)
                    .map_err(|_| DryWetError::Allocation)?;
                bytes(
                    queue.capacity(),
                    history.capacity(),
                    std::mem::size_of::<$sample>(),
                )?;
                queue.resize(q, 0.);
                history.resize(h, 0.);
                Ok(Self {
                    config,
                    queue,
                    history,
                    previous_input: [0.; 64],
                    previous_output: [0.; 64],
                    cursor: 0,
                    read: 0,
                    write: 0,
                    queued: 0,
                    requested_delay: 0.,
                    whole: 0,
                    fraction: 0.,
                    coefficient: 0.,
                    target,
                    panner: <$panner>::new(
                        target.profile,
                        config.ramp_frames,
                        config.maximum_frames,
                    )
                    .map_err(|_| DryWetError::Configuration)?,
                    accepted: 0,
                    consumed: 0,
                })
            }
            pub fn config(&self) -> DryWetMixerConfig {
                self.config
            }
            pub fn target(&self) -> $profile {
                self.target
            }
            pub fn current_gains(&self) -> [$sample; 2] {
                self.panner.current_gains()
            }
            pub fn remaining_ramp_frames(&self) -> u32 {
                self.panner.remaining_frames()
            }
            pub fn queued_frames(&self) -> usize {
                self.queued
            }
            pub fn free_frames(&self) -> usize {
                self.config.fifo_capacity_frames - self.queued
            }
            pub fn accepted_dry_frames(&self) -> u64 {
                self.accepted
            }
            pub fn consumed_mix_frames(&self) -> u64 {
                self.consumed
            }
            pub fn wet_latency_samples(&self) -> f64 {
                self.requested_delay
            }
            /// Rounded requested fraction, unused by Integer (which floors).
            /// It may equal1 in native32 Integer mode and is not actual host latency.
            pub fn native_fraction(&self) -> $sample {
                self.fraction
            }
            /// Rounded a, used only by fractional Thiran. Other policies and the
            /// integer-valued Thiran bypass do not use this observation.
            pub fn native_thiran_coefficient(&self) -> $sample {
                self.coefficient
            }
            pub fn set_target(&mut self, target: $profile) {
                self.target = target;
                self.panner.set_target(target.profile);
            }
            pub fn set_wet_latency_samples(&mut self, delay: f64) -> Result<(), DryWetError> {
                if !delay.is_finite()
                    || delay < 0.
                    || delay > self.config.maximum_latency_samples as f64
                {
                    return Err(DryWetError::InvalidLatency);
                }
                if delay == self.requested_delay {
                    return Ok(());
                }
                if self.queued != 0 {
                    return Err(DryWetError::PendingDry);
                }
                let whole = delay.floor() as usize;
                let f = delay - whole as f64;
                let fraction = f as $sample;
                let coefficient = (1. as $sample - fraction) / (1. as $sample + fraction);
                if self.config.interpolation != DelayInterpolation::Integer
                    && f > 0.
                    && !(0. < fraction && fraction < 1.)
                {
                    return Err(DryWetError::InvalidLatency);
                }
                if self.config.interpolation == DelayInterpolation::Thiran1st
                    && f > 0.
                    && coefficient >= 1.
                {
                    return Err(DryWetError::InvalidLatency);
                }
                self.requested_delay = delay;
                self.whole = whole;
                self.fraction = fraction;
                self.coefficient = coefficient;
                Ok(())
            }
            fn preflight(
                &self,
                lengths: impl Iterator<Item = usize>,
                frames: usize,
            ) -> Result<(), DryWetError> {
                if frames > self.config.maximum_frames {
                    return Err(DryWetError::TooManyFrames);
                }
                if lengths.into_iter().any(|n| n < frames) {
                    return Err(DryWetError::BufferTooShort);
                }
                Ok(())
            }
            fn tap(&self, ch: usize, distance: usize) -> $sample {
                let length = self.config.maximum_latency_samples + 4;
                let at = if self.cursor >= distance {
                    self.cursor - distance
                } else {
                    length - (distance - self.cursor)
                };
                self.history[ch * length + at]
            }
            fn delay(&mut self, ch: usize, input: $sample) -> $sample {
                let length = self.config.maximum_latency_samples + 4;
                self.history[ch * length + self.cursor] = input;
                let v = self.tap(ch, self.whole);
                let f = self.fraction;
                let output = if f == 0. || self.config.interpolation == DelayInterpolation::Integer
                {
                    v
                } else {
                    match self.config.interpolation {
                        DelayInterpolation::Integer => unreachable!(),
                        DelayInterpolation::Linear => {
                            (1. - f) * v + f * self.tap(ch, self.whole + 1)
                        }
                        DelayInterpolation::Lagrange3rd => {
                            -v * (f - 1.) * (f - 2.) * (f - 3.) / 6.
                                + self.tap(ch, self.whole + 1) * f * (f - 2.) * (f - 3.) / 2.
                                - self.tap(ch, self.whole + 2) * f * (f - 1.) * (f - 3.) / 2.
                                + self.tap(ch, self.whole + 3) * f * (f - 1.) * (f - 2.) / 6.
                        }
                        DelayInterpolation::Thiran1st => {
                            self.coefficient * v + self.previous_input[ch]
                                - self.coefficient * self.previous_output[ch]
                        }
                    }
                };
                if self.config.interpolation == DelayInterpolation::Thiran1st {
                    self.previous_input[ch] = v;
                    self.previous_output[ch] = output;
                }
                output
            }
            pub fn push_dry(
                &mut self,
                input: &[&[$sample]],
                frames: usize,
            ) -> Result<(), DryWetError> {
                if input.len() != self.config.channels {
                    return Err(DryWetError::ChannelCount);
                }
                self.preflight(input.iter().map(|c| c.len()), frames)?;
                let accepted = self
                    .accepted
                    .checked_add(frames as u64)
                    .ok_or(DryWetError::CountOverflow)?;
                if frames > self.free_frames() {
                    return Err(DryWetError::QueueFull);
                }
                for i in 0..frames {
                    for (ch, source) in input.iter().enumerate() {
                        let y = self.delay(ch, source[i]);
                        self.queue[self.write * self.config.channels + ch] = y;
                    }
                    self.cursor += 1;
                    if self.cursor == self.config.maximum_latency_samples + 4 {
                        self.cursor = 0;
                    }
                    self.write += 1;
                    if self.write == self.config.fifo_capacity_frames {
                        self.write = 0;
                    }
                }
                self.queued += frames;
                self.accepted = accepted;
                Ok(())
            }
            fn mix_preflight(&self, frames: usize) -> Result<u64, DryWetError> {
                let consumed = self
                    .consumed
                    .checked_add(frames as u64)
                    .ok_or(DryWetError::CountOverflow)?;
                if frames > self.queued {
                    return Err(DryWetError::InsufficientDry);
                }
                Ok(consumed)
            }
            fn pop_frame(&mut self) {
                self.read += 1;
                if self.read == self.config.fifo_capacity_frames {
                    self.read = 0;
                }
                self.queued -= 1;
            }
            pub fn mix_wet(
                &mut self,
                wet: &[&[$sample]],
                output: &mut [&mut [$sample]],
                frames: usize,
            ) -> Result<(), DryWetError> {
                if wet.len() != self.config.channels || output.len() != self.config.channels {
                    return Err(DryWetError::ChannelCount);
                }
                self.preflight(
                    wet.iter()
                        .map(|c| c.len())
                        .chain(output.iter().map(|c| c.len())),
                    frames,
                )?;
                let consumed = self.mix_preflight(frames)?;
                for i in 0..frames {
                    let gains = self.panner.process_mono_sample(1.);
                    for ch in 0..self.config.channels {
                        output[ch][i] = self.queue[self.read * self.config.channels + ch]
                            * gains[0]
                            + wet[ch][i] * gains[1];
                    }
                    self.pop_frame();
                }
                self.consumed = consumed;
                Ok(())
            }
            pub fn mix_wet_in_place(
                &mut self,
                wet: &mut [&mut [$sample]],
                frames: usize,
            ) -> Result<(), DryWetError> {
                if wet.len() != self.config.channels {
                    return Err(DryWetError::ChannelCount);
                }
                self.preflight(wet.iter().map(|c| c.len()), frames)?;
                let consumed = self.mix_preflight(frames)?;
                for i in 0..frames {
                    let gains = self.panner.process_mono_sample(1.);
                    for (ch, channel) in wet.iter_mut().enumerate() {
                        channel[i] = self.queue[self.read * self.config.channels + ch] * gains[0]
                            + channel[i] * gains[1];
                    }
                    self.pop_frame();
                }
                self.consumed = consumed;
                Ok(())
            }
            /// Clear pending dry/history/counter epoch and snap gains to target,
            /// retaining config, latency, controls and allocations. Capacity work.
            pub fn reset(&mut self) {
                self.queue.fill(0.);
                self.history.fill(0.);
                self.previous_input.fill(0.);
                self.previous_output.fill(0.);
                self.cursor = 0;
                self.read = 0;
                self.write = 0;
                self.queued = 0;
                self.accepted = 0;
                self.consumed = 0;
                self.panner.reset();
            }
        }
    };
}
mixer!(DryWetMixer, f64, DryWetGains, StereoPanner);
mixer!(DryWetMixer32, f32, DryWetGains32, StereoPanner32);
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checked_counter_overflow_preserves_queue_gains_output_and_valid_continuation() {
        let config = DryWetMixerConfig {
            channels: 1,
            maximum_frames: 4,
            fifo_capacity_frames: 4,
            maximum_latency_samples: 3,
            interpolation: DelayInterpolation::Linear,
            ramp_frames: 7,
        };
        let g = DryWetGains::new(
            0.3,
            PanningLaw::Linear,
            PanningNormalization::UnityEndpoints,
        )
        .unwrap();
        let mut m = DryWetMixer::new(config, g).unwrap();
        m.accepted = u64::MAX;
        assert_eq!(m.push_dry(&[&[1.]], 1), Err(DryWetError::CountOverflow));
        assert_eq!(m.queued, 0);
        assert_eq!(m.cursor, 0);
        m.accepted = u64::MAX - 1;
        m.push_dry(&[&[1.]], 1).unwrap();
        m.consumed = u64::MAX;
        let mut y = [99.];
        assert_eq!(
            m.mix_wet(&[&[2.]], &mut [&mut y], 1),
            Err(DryWetError::CountOverflow)
        );
        assert_eq!(y, [99.]);
        assert_eq!(m.queued, 1);
        assert_eq!(m.current_gains(), g.gains());
        m.consumed = u64::MAX - 1;
        m.mix_wet(&[&[2.]], &mut [&mut y], 1).unwrap();
        assert_eq!(y, [g.gains()[0] + 2. * g.gains()[1]]);
        assert_eq!(m.accepted, u64::MAX);
        assert_eq!(m.consumed, u64::MAX);
        m.push_dry(&[&[]], 0).unwrap();
        m.mix_wet(&[&[]], &mut [&mut []], 0).unwrap();
        m.reset();
        assert_eq!(m.accepted, 0);
        assert_eq!(m.consumed, 0);
    }
    #[test]
    fn native32_checked_counter_overflow_preserves_exact_owner_and_continuation() {
        let config = DryWetMixerConfig {
            channels: 1,
            maximum_frames: 4,
            fifo_capacity_frames: 4,
            maximum_latency_samples: 3,
            interpolation: DelayInterpolation::Linear,
            ramp_frames: 7,
        };
        let g = DryWetGains::new(
            0.3,
            PanningLaw::Linear,
            PanningNormalization::UnityEndpoints,
        )
        .unwrap();
        let g = g.quantize_f32();
        let mut m = DryWetMixer32::new(config, g).unwrap();
        m.accepted = u64::MAX;
        assert_eq!(m.push_dry(&[&[1.]], 1), Err(DryWetError::CountOverflow));
        assert_eq!(m.queued, 0);
        assert_eq!(m.cursor, 0);
        m.accepted = u64::MAX - 1;
        m.push_dry(&[&[1.]], 1).unwrap();
        m.consumed = u64::MAX;
        let mut y = [99.];
        assert_eq!(
            m.mix_wet(&[&[2.]], &mut [&mut y], 1),
            Err(DryWetError::CountOverflow)
        );
        assert_eq!(y, [99.]);
        assert_eq!(m.queued, 1);
        assert_eq!(m.current_gains(), g.gains());
        m.consumed = u64::MAX - 1;
        m.mix_wet(&[&[2.]], &mut [&mut y], 1).unwrap();
        assert_eq!(y, [g.gains()[0] + 2. * g.gains()[1]]);
        assert_eq!(m.accepted, u64::MAX);
        assert_eq!(m.consumed, u64::MAX);
        m.push_dry(&[&[]], 0).unwrap();
        m.mix_wet(&[&[]], &mut [&mut []], 0).unwrap();
        m.reset();
        assert_eq!(m.accepted, 0);
        assert_eq!(m.consumed, 0);
    }
}
