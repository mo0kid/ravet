//! A streaming two-times polyphase half-band filter for prepared DSP paths.

// Fixed f32 half-band coefficients for this filter profile:
// up: transition 0.05, stopband -90 dB; down: 0.06, -75 dB. The recurrence
// below implements the first-order allpass transfer (a + z^-1)/(1 + a*z^-1).
// No external coefficient designer or DSP library is needed at runtime.
const UP: [f32; 7] = [
    0.045_728_147,
    0.332_501_1,
    0.663_202_05,
    0.933_855_83,
    0.168_087_54,
    0.504_485_7,
    0.803_780_85,
];
const DOWN: [f32; 6] = [
    0.054_217_525,
    0.383_087_34,
    0.748_720_94,
    0.196_797_97,
    0.573_136_4,
    0.914_293_7,
];

/// One channel of 2× interpolation and decimation with fixed half-band IIR
/// coefficients. State persists across samples and blocks; reset allocates
/// nothing. Upsample once, process both samples at twice the host sample rate,
/// and downsample once. This introduces frequency-dependent wet-path delay;
/// it does not align the dry path or provide host latency compensation.
///
/// Input and filter state use f32. No block-boundary tiny-value snapping is
/// applied, making the stream independent of block partitioning. Native
/// floating-point denormal handling and callback deadlines are not guaranteed.
#[derive(Clone, Debug, Default)]
pub struct HalfBand2x {
    up: [f32; 7],
    down: [f32; 6],
    delayed: f32,
}

#[inline]
fn cascade(mut sample: f32, coefficients: &[f32], states: &mut [f32]) -> f32 {
    for (&coefficient, state) in coefficients.iter().zip(states) {
        let filtered = coefficient * sample + *state;
        *state = sample - coefficient * filtered;
        sample = filtered;
    }
    sample
}

impl HalfBand2x {
    #[inline]
    pub fn upsample(&mut self, sample: f32) -> [f32; 2] {
        [
            cascade(sample, &UP[..4], &mut self.up[..4]),
            cascade(sample, &UP[4..], &mut self.up[4..]),
        ]
    }

    #[inline]
    pub fn downsample(&mut self, samples: [f32; 2]) -> f32 {
        let direct = cascade(samples[0], &DOWN[..3], &mut self.down[..3]);
        let delayed = cascade(samples[1], &DOWN[3..], &mut self.down[3..]);
        let output = (self.delayed + direct) * 0.5;
        self.delayed = delayed;
        output
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_preserves_dc_and_reset_clears_both_paths() {
        let mut filter = HalfBand2x::default();
        let mut output = 0.0;
        for _ in 0..512 {
            let up = filter.upsample(0.25);
            output = filter.downsample(up);
        }
        assert!((output - 0.25).abs() < 1e-6);
        filter.reset();
        let up = filter.upsample(0.0);
        assert_eq!(filter.downsample(up), 0.0);
    }

    #[test]
    fn downsampling_rejects_high_rate_nyquist() {
        let mut filter = HalfBand2x::default();
        let mut output = 1.0;
        for _ in 0..512 {
            output = filter.downsample([1.0, -1.0]);
        }
        assert!(output.abs() < 1e-6);
    }
}

/// Supported powers of two; higher factors cascade the fixed 2x filter profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OversamplingFactor {
    One,
    Two,
    Four,
    Eight,
}
impl OversamplingFactor {
    pub const fn factor(self) -> usize {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Four => 4,
            Self::Eight => 8,
        }
    }
    const fn stages(self) -> usize {
        match self {
            Self::One => 0,
            Self::Two => 1,
            Self::Four => 2,
            Self::Eight => 3,
        }
    }
}

/// One-channel configurable 1x/2x/4x/8x streaming f32 oversampling.
/// Three fixed half-band stages and stack scratch require no preparation heap.
/// Changing factor requires a new instance/reset; no live crossfade, dry-path
/// alignment or constant integer latency is supplied. Closure DSP must be
/// prepared at host sample rate times factor and obey callback obligations.
#[derive(Clone, Debug)]
pub struct Oversampler {
    factor: OversamplingFactor,
    stages: [HalfBand2x; 3],
}
impl Oversampler {
    pub fn new(factor: OversamplingFactor) -> Self {
        Self {
            factor,
            stages: std::array::from_fn(|_| HalfBand2x::default()),
        }
    }
    pub fn factor(&self) -> OversamplingFactor {
        self.factor
    }
    pub fn reset(&mut self) {
        for stage in &mut self.stages {
            stage.reset();
        }
    }
    /// Upsample in time order, invoke DSP once per high-rate sample, then
    /// decimate in time order. The closure is called exactly factor times.
    pub fn process_sample(&mut self, input: f32, mut process: impl FnMut(f32) -> f32) -> f32 {
        let mut samples = [0.; 8];
        samples[0] = input;
        let mut count = 1;
        for stage in &mut self.stages[..self.factor.stages()] {
            let mut next = [0.; 8];
            for (index, &sample) in samples[..count].iter().enumerate() {
                next[index * 2..index * 2 + 2].copy_from_slice(&stage.upsample(sample));
            }
            samples = next;
            count *= 2;
        }
        for sample in &mut samples[..count] {
            *sample = process(*sample);
        }
        for stage in self.stages[..self.factor.stages()].iter_mut().rev() {
            let mut next = [0.; 8];
            for (index, pair) in samples[..count].as_chunks::<2>().0.iter().enumerate() {
                next[index] = stage.downsample([pair[0], pair[1]]);
            }
            samples = next;
            count /= 2;
        }
        samples[0]
    }
}

#[cfg(test)]
mod cascade_tests {
    use super::*;
    #[test]
    fn factors_preserve_dc_call_counts_and_reset_state() {
        for factor in [
            OversamplingFactor::One,
            OversamplingFactor::Two,
            OversamplingFactor::Four,
            OversamplingFactor::Eight,
        ] {
            let mut oversampler = Oversampler::new(factor);
            let mut calls = 0;
            let mut last = 0.;
            for _ in 0..2048 {
                last = oversampler.process_sample(0.25, |sample| {
                    calls += 1;
                    sample
                });
            }
            assert_eq!(calls, 2048 * factor.factor());
            assert!((last - 0.25).abs() < 2e-6, "{factor:?}: {last}");
            oversampler.reset();
            assert_eq!(oversampler.process_sample(0., |x| x), 0.);
        }
    }
    #[test]
    fn unity_factor_is_exact_and_stream_does_not_depend_on_block_partition() {
        let input: Vec<f32> = (0..1024).map(|i| (i as f32 * 0.17).sin()).collect();
        let mut unity = Oversampler::new(OversamplingFactor::One);
        for &sample in &input {
            assert_eq!(unity.process_sample(sample, |x| x * x), sample * sample);
        }
        let mut whole = Oversampler::new(OversamplingFactor::Eight);
        let expected: Vec<_> = input
            .iter()
            .map(|&x| whole.process_sample(x, f32::tanh))
            .collect();
        let mut partitioned = Oversampler::new(OversamplingFactor::Eight);
        let actual: Vec<_> = input
            .chunks(13)
            .flat_map(|chunk| {
                chunk
                    .iter()
                    .map(|&x| partitioned.process_sample(x, f32::tanh))
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(actual, expected);
    }
}
