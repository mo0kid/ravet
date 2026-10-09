use crate::{DelayLine, FilterError};

/// Fixed one-channel Schroeder-style network preparation. The four comb delays
/// are 31/37/41/43 ms and two canonical allpass delays 5/1.3 ms, multiplied by
/// room_scale and rounded to samples. This is a synthetic effect, not a physical
/// room model. Decay specifies each comb's nominal DC -60 dB feedback decay;
/// damping and mixing change the complete network's measured decay.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReverbConfig {
    /// Finite rate 8000..=192000 Hz.
    pub sample_rate: f64,
    /// Finite delay multiplier 0.5..=2.
    pub room_scale: f64,
    /// Finite nominal comb decay 0.1..=20 seconds.
    pub decay_seconds: f64,
    /// Finite feedback lowpass pole frequency >=20 Hz and strictly below Nyquist.
    pub damping_hz: f64,
    /// Finite linear crossfade 0..=1 (zero dry, one wet).
    pub wet: f64,
}
impl Default for ReverbConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000.,
            room_scale: 1.,
            decay_seconds: 1.5,
            damping_hz: 6000.,
            wet: 0.25,
        }
    }
}
#[derive(Debug)]
struct Comb {
    delay: DelayLine,
    feedback: f64,
    lowpass: f64,
}
#[derive(Debug)]
struct Allpass {
    delay: DelayLine,
}
impl Allpass {
    fn process(&mut self, input: f64) -> f64 {
        let delayed = self.delay.read_integer(0);
        let output = delayed - 0.5 * input;
        self.delay.write(input + 0.5 * output);
        self.delay.advance();
        output
    }
}

/// Prepared mono f64 reverb: four parallel lowpass-feedback combs averaged into
/// two serial canonical unity-magnitude allpasses. Processing/reset allocate
/// nothing; construction owns six bounded delay allocations. Instantiate
/// independently for channels; no stereo decorrelation/crossfeed is supplied.
/// No modulation, freeze, physical-room fidelity, headroom/denormal policy or
/// native callback-deadline guarantee. Nonfinite audio can poison histories;
/// extreme finite audio may overflow. Reset retains prepared configuration.
#[derive(Debug)]
pub struct SchroederReverb {
    combs: [Comb; 4],
    allpasses: [Allpass; 2],
    pole: f64,
    wet: f64,
    lengths: [usize; 6],
}
impl SchroederReverb {
    pub fn new(config: ReverbConfig) -> Result<Self, FilterError> {
        Self::with_delay_offset(config, 0)
    }
    // Only the stereo constructor supplies a separately validated <=1920 offset.
    fn with_delay_offset(config: ReverbConfig, offset: usize) -> Result<Self, FilterError> {
        if !config.sample_rate.is_finite()
            || !(8000. ..=192000.).contains(&config.sample_rate)
            || !config.room_scale.is_finite()
            || !(0.5..=2.).contains(&config.room_scale)
            || !config.decay_seconds.is_finite()
            || !(0.1..=20.).contains(&config.decay_seconds)
            || !config.damping_hz.is_finite()
            || config.damping_hz < 20.
            || config.damping_hz >= config.sample_rate * 0.5
            || !config.wet.is_finite()
            || !(0. ..=1.).contains(&config.wet)
        {
            return Err(FilterError);
        }
        let lengths = [0.031, 0.037, 0.041, 0.043, 0.005, 0.0013].map(|seconds| {
            (seconds * config.room_scale * config.sample_rate).round() as usize + offset
        });
        let combs = std::array::from_fn(|i| Comb {
            delay: DelayLine::new(lengths[i]).expect("validated positive delay"),
            feedback: 10f64
                .powf(-3. * lengths[i] as f64 / (config.sample_rate * config.decay_seconds)),
            lowpass: 0.,
        });
        let allpasses = std::array::from_fn(|i| Allpass {
            delay: DelayLine::new(lengths[i + 4]).expect("validated positive delay"),
        });
        Ok(Self {
            combs,
            allpasses,
            pole: (-std::f64::consts::TAU * config.damping_hz / config.sample_rate).exp(),
            wet: config.wet,
            lengths,
        })
    }
    /// Exact rounded prepared delay lengths: four combs followed by two allpasses.
    pub fn delay_lengths(&self) -> [usize; 6] {
        self.lengths
    }
    pub fn wet(&self) -> f64 {
        self.wet
    }
    /// Validate before mutation; retain all histories without smoothing.
    pub fn set_wet(&mut self, wet: f64) -> Result<(), FilterError> {
        if !wet.is_finite() || !(0. ..=1.).contains(&wet) {
            return Err(FilterError);
        }
        self.wet = wet;
        Ok(())
    }
    pub fn reset(&mut self) {
        for comb in &mut self.combs {
            comb.delay.reset();
            comb.lowpass = 0.;
        }
        for allpass in &mut self.allpasses {
            allpass.delay.reset();
        }
    }
    /// Dry mode still advances the wet network, allowing later mix changes.
    pub fn process_sample(&mut self, input: f64) -> f64 {
        let mut reverberated = 0.;
        for comb in &mut self.combs {
            let delayed = comb.delay.read_integer(0);
            comb.lowpass = (1. - self.pole) * delayed + self.pole * comb.lowpass;
            comb.delay.write(input + comb.feedback * comb.lowpass);
            comb.delay.advance();
            reverberated += delayed * 0.25;
        }
        for allpass in &mut self.allpasses {
            reverberated = allpass.process(reverberated);
        }
        if self.wet == 0. {
            input
        } else if self.wet == 1. {
            reverberated
        } else {
            input * (1. - self.wet) + reverberated * self.wet
        }
    }
    pub fn process_in_place(&mut self, samples: &mut [f64]) {
        for sample in samples {
            *sample = self.process_sample(*sample);
        }
    }
}

/// Prepared synthetic stereo network configuration. The original mono network
/// receives (left + right)/2 in both branches; the right branch adds a fixed
/// sample-rounded spread to each delay. Dry audio preserves its original channel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StereoReverbConfig {
    pub reverb: ReverbConfig,
    /// Finite seconds 0..=0.010 added to all six right-channel delays.
    /// Zero spread intentionally gives identical wet branches.
    pub spread_seconds: f64,
    /// Finite wet width 0..=1: zero averages branches, one separates them.
    pub width: f64,
}
impl Default for StereoReverbConfig {
    fn default() -> Self {
        Self {
            reverb: ReverbConfig::default(),
            spread_seconds: 0.0005,
            width: 1.,
        }
    }
}

/// Two independently damped prepared f64 Schroeder networks with common mono
/// excitation and bounded stereo spread. Owns twelve delay allocations totaling
/// at most 133094 f64 samples. Processing/reset/mix changes allocate nothing.
/// No physical room, guaranteed decorrelation, freeze/modulation, input sanitation,
/// smoothing, denormal/headroom or native callback deadline policy is inferred.
/// Nonfinite/extreme finite audio may poison/overflow histories until reset.
#[derive(Debug)]
pub struct StereoSchroederReverb {
    branches: [SchroederReverb; 2],
    wet: f64,
    width: f64,
}
impl StereoSchroederReverb {
    pub fn new(config: StereoReverbConfig) -> Result<Self, FilterError> {
        if !config.spread_seconds.is_finite()
            || !(0. ..=0.010).contains(&config.spread_seconds)
            || !valid_mix(config.width)
            || !valid_mix(config.reverb.wet)
        {
            return Err(FilterError);
        }
        let wet = config.reverb.wet;
        let mono = ReverbConfig {
            wet: 1.,
            ..config.reverb
        };
        let left = SchroederReverb::new(mono)?;
        let spread = (config.spread_seconds * config.reverb.sample_rate).round() as usize;
        let right = SchroederReverb::with_delay_offset(mono, spread)?;
        Ok(Self {
            branches: [left, right],
            wet,
            width: config.width,
        })
    }
    /// Left/right rounded delays, each four combs then two allpasses.
    pub fn delay_lengths(&self) -> [[usize; 6]; 2] {
        self.branches.each_ref().map(SchroederReverb::delay_lengths)
    }
    pub fn wet(&self) -> f64 {
        self.wet
    }
    pub fn width(&self) -> f64 {
        self.width
    }
    /// Validate the complete mix before mutation; histories continue, no smoothing.
    pub fn set_mix(&mut self, wet: f64, width: f64) -> Result<(), FilterError> {
        if !valid_mix(wet) || !valid_mix(width) {
            return Err(FilterError);
        }
        self.wet = wet;
        self.width = width;
        Ok(())
    }
    pub fn reset(&mut self) {
        for branch in &mut self.branches {
            branch.reset();
        }
    }
    /// Advance both wet branches even while dry. Width affects only wet audio;
    /// opposite-polarity input cancels wet excitation, preserving dry stereo.
    pub fn process_sample(&mut self, input: [f64; 2]) -> [f64; 2] {
        let excitation = input[0] * 0.5 + input[1] * 0.5;
        let wet = self
            .branches
            .each_mut()
            .map(|branch| branch.process_sample(excitation));
        let middle = wet[0] * 0.5 + wet[1] * 0.5;
        std::array::from_fn(|channel| {
            let reverberated = if self.width == 1. {
                wet[channel]
            } else if self.width == 0. {
                middle
            } else {
                middle * (1. - self.width) + wet[channel] * self.width
            };
            if self.wet == 0. {
                input[channel]
            } else if self.wet == 1. {
                reverberated
            } else {
                input[channel] * (1. - self.wet) + reverberated * self.wet
            }
        })
    }
    /// Whole stereo frames; no partial frame can advance a channel independently.
    pub fn process_in_place(&mut self, frames: &mut [[f64; 2]]) {
        for frame in frames {
            *frame = self.process_sample(*frame);
        }
    }
}
fn valid_mix(value: f64) -> bool {
    value.is_finite() && (0. ..=1.).contains(&value)
}
