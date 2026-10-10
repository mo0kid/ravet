//! Explicit sample-wise transfer functions without implicit oversampling.

/// A caller-owned transfer function, stored inline without boxing or conversion.
///
/// Functions and closures accepting/returning `f32` or `f64` preserve that sample
/// precision. Each sample invokes the function once, in buffer order. The wrapper
/// has no history, sample-rate configuration, latency or tail of its own, so it
/// needs no preparation or reset. Captured resources and any interior state
/// belong to the caller; cloning/destruction follows the function's ownership.
///
/// The wrapper allocates nothing. A custom function must itself avoid allocation,
/// blocking and I/O when used on an audio thread. Nonfinite samples are passed
/// directly to it. Consumers own drive, output gain, smoothing, headroom and
/// denormal policy. Nonlinear curves can alias; use an explicit oversampling path
/// when needed. No alias-free or callback-deadline guarantee is supplied.
///
/// ```
/// use ravet_signal::{Oversampler, OversamplingFactor, Waveshaper};
///
/// let shape = Waveshaper::new(|x: f32| (2.0 * x).tanh());
/// let mut samples = [0.0_f32, 0.25, -0.25];
/// shape.process_in_place(&mut samples);
/// assert_eq!(samples[0], 0.0);
/// assert_eq!(samples[1], -samples[2]);
///
/// let mut oversampler = Oversampler::new(OversamplingFactor::Four);
/// let wet = oversampler.process_sample(0.25, |x| shape.process_sample(x));
/// assert!(wet.is_finite());
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Waveshaper<F> {
    function: F,
}

impl<F> Waveshaper<F> {
    /// Store a function or closure inline. Its input and output types must match
    /// when processing; no sample sanitization or transfer-function validation.
    pub fn new(function: F) -> Self {
        Self { function }
    }

    /// Apply the transfer function exactly once, preserving its sample type.
    #[inline]
    pub fn process_sample<T>(&self, sample: T) -> T
    where
        F: Fn(T) -> T,
    {
        (self.function)(sample)
    }

    /// Transform every sample in order. Empty slices do not invoke the function.
    /// No wrapper allocation; custom function behavior remains caller-owned.
    pub fn process_in_place<T: Copy>(&self, samples: &mut [T])
    where
        F: Fn(T) -> T,
    {
        for sample in samples {
            *sample = self.process_sample(*sample);
        }
    }

    /// Transform equal-length input/output buffers. A mismatch preserves output
    /// and never invokes the function. Empty buffers are a valid no-op.
    pub fn process_block<T: Copy>(
        &self,
        input: &[T],
        output: &mut [T],
    ) -> Result<(), WaveshaperLengthError>
    where
        F: Fn(T) -> T,
    {
        if input.len() != output.len() {
            return Err(WaveshaperLengthError);
        }
        for (&sample, output) in input.iter().zip(output) {
            *output = self.process_sample(sample);
        }
        Ok(())
    }
}

impl Waveshaper<fn(f64) -> f64> {
    /// Symmetric f64 hard clipping: clamp to [-1, 1]. Infinities clip to the
    /// endpoints and NaN propagates. The interior has unity gain.
    pub fn hard_clip() -> Self {
        Self::new(|x| x.clamp(-1.0, 1.0))
    }

    /// Symmetric f64 hyperbolic tangent, with unity slope at zero and asymptotes
    /// at +/-1. Uses a transcendental per sample; NaN propagates, infinities map
    /// to +/-1. Drive and output gain are explicit caller operations.
    pub fn tanh() -> Self {
        Self::new(f64::tanh)
    }

    /// Symmetric f64 cubic soft clipping: x - x^3/3 for |x| < 1, otherwise
    /// +/-2/3. Unity slope at zero and zero slope at the clipping knees; this is
    /// not peak-normalized to unity. Clamping before arithmetic bounds even
    /// extreme finite/infinite inputs; NaN propagates.
    pub fn cubic_soft_clip() -> Self {
        Self::new(|x| {
            let x = x.clamp(-1.0, 1.0);
            x * (1.0 - x * x / 3.0)
        })
    }
}

/// Input and output buffers must have exactly equal lengths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaveshaperLengthError;

impl std::fmt::Display for WaveshaperLengthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("waveshaper input and output lengths differ")
    }
}

impl std::error::Error for WaveshaperLengthError {}
