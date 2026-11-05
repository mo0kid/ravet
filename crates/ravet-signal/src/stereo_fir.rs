//! Prepared fixed-size stereo FIR filtering for block processing.
//!
//! [`StereoFir`] keeps each channel's history in one contiguous buffer, so
//! every output sample is a sum over adjacent memory. The kernel keeps a block
//! of running sums in registers (64 f32 or 32 f64 on arm64) and broadcasts one
//! tap at a time across them. These are plain element-wise loops that the
//! compiler vectorises on targets with SIMD (NEON on arm64, SSE2 on x86_64)
//! without `unsafe`, target-specific intrinsics or fast-math reassociation.

use crate::ConvolutionError;
use core::ops::{Add, Mul};

/// Sample types accepted by [`StereoFir`]: `f32` and `f64`.
pub trait FirSample:
    Copy + Default + PartialEq + Add<Output = Self> + Mul<Output = Self> + sealed::Sealed
{
    /// Whether the value is finite.
    fn finite(self) -> bool;
    /// `self + a * b`, fused on targets where fused multiply-add is a baseline
    /// instruction (arm64), and a separate multiply and add elsewhere.
    fn accumulate(self, a: Self, b: Self) -> Self;
    /// Run the block kernel with this type's register-blocking width.
    #[doc(hidden)]
    fn convolve(reversed: &[Self], input: &[Self], output: &mut [Self]);
}
macro_rules! fir_sample {
    ($type:ty, $outputs:literal) => {
        impl FirSample for $type {
            #[inline]
            fn convolve(reversed: &[Self], input: &[Self], output: &mut [Self]) {
                convolve::<Self, $outputs>(reversed, input, output)
            }
            fn finite(self) -> bool {
                self.is_finite()
            }
            #[inline(always)]
            fn accumulate(self, a: Self, b: Self) -> Self {
                #[cfg(target_arch = "aarch64")]
                {
                    a.mul_add(b, self)
                }
                #[cfg(not(target_arch = "aarch64"))]
                {
                    self + a * b
                }
            }
        }
    };
}
// Outputs per wide kernel step. On arm64, 64 f32 or 32 f64 running sums fill
// sixteen of the 32 NEON registers. Elsewhere (x86_64 SSE2 has sixteen
// registers) the width is halved; it has not been tuned on x86_64.
#[cfg(target_arch = "aarch64")]
fir_sample!(f32, 64);
#[cfg(target_arch = "aarch64")]
fir_sample!(f64, 32);
#[cfg(not(target_arch = "aarch64"))]
fir_sample!(f32, 32);
#[cfg(not(target_arch = "aarch64"))]
fir_sample!(f64, 16);
mod sealed {
    pub trait Sealed {}
    impl Sealed for f32 {}
    impl Sealed for f64 {}
}

/// Maximum taps accepted by [`StereoFir`]. Direct convolution costs O(taps)
/// per sample; use [`crate::PartitionedConvolver`] for longer responses.
pub const MAXIMUM_STEREO_FIR_TAPS: usize = 16_384;
/// Maximum prepared block length of [`StereoFir`].
pub const MAXIMUM_STEREO_FIR_FRAMES: usize = 65_536;

/// Two channel buffers of different lengths were passed to
/// [`StereoFir::process`]. Nothing was processed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelLengthMismatch;
impl core::fmt::Display for ChannelLengthMismatch {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("stereo channels have different lengths")
    }
}
impl std::error::Error for ChannelLengthMismatch {}

/// A prepared stereo direct-form FIR with a fixed tap count.
///
/// Each channel has its own impulse response; both have the same length.
/// Preparation copies 1..=16,384 finite taps per channel and allocates history
/// for blocks of up to `maximum_frames` (1..=65,536). Processing and reset never
/// allocate, lock or perform I/O. Longer blocks are processed in prepared-size
/// pieces, so results do not depend on block partitioning.
///
/// The filter is causal with zero added latency: output `n` is
/// `sum(h[k] * x[n - k])`. [`StereoFir::tail_samples`] is `taps - 1`. Nonfinite
/// input propagates. Each output is summed sequentially from the oldest tap to
/// the newest, so results do not depend on block partitioning. On arm64 each
/// step is a fused multiply-add, so results can differ in the last bits between
/// arm64 and other targets. Replacing a response requires a new instance on a
/// control thread.
#[derive(Clone, Debug)]
pub struct StereoFir<S: FirSample = f32> {
    /// Responses in reverse order, oldest-sample tap first.
    reversed: [Vec<S>; 2],
    /// `taps - 1` history samples followed by up to `maximum_frames` inputs.
    buffers: [Vec<S>; 2],
    taps: usize,
    maximum_frames: usize,
}

impl<S: FirSample> StereoFir<S> {
    /// Prepare the same response on both channels.
    pub fn new(impulse: &[S], maximum_frames: usize) -> Result<Self, ConvolutionError> {
        Self::with_responses(impulse, impulse, maximum_frames)
    }

    /// Prepare separate left and right responses of equal length.
    pub fn with_responses(
        left: &[S],
        right: &[S],
        maximum_frames: usize,
    ) -> Result<Self, ConvolutionError> {
        let taps = left.len();
        if taps == 0
            || taps > MAXIMUM_STEREO_FIR_TAPS
            || right.len() != taps
            || maximum_frames == 0
            || maximum_frames > MAXIMUM_STEREO_FIR_FRAMES
            || left.iter().chain(right).any(|tap| !tap.finite())
        {
            return Err(ConvolutionError);
        }
        let reverse = |impulse: &[S]| impulse.iter().rev().copied().collect::<Vec<_>>();
        let length = taps - 1 + maximum_frames;
        Ok(Self {
            reversed: [reverse(left), reverse(right)],
            buffers: [vec![S::default(); length], vec![S::default(); length]],
            taps,
            maximum_frames,
        })
    }

    /// Taps per channel.
    pub fn taps(&self) -> usize {
        self.taps
    }

    /// Prepared maximum block length.
    pub fn maximum_frames(&self) -> usize {
        self.maximum_frames
    }

    /// Algorithmic latency: always zero.
    pub fn latency_samples(&self) -> usize {
        0
    }

    /// Samples of output that follow the last nonzero input.
    pub fn tail_samples(&self) -> usize {
        self.taps - 1
    }

    /// Clear both histories without allocating.
    pub fn reset(&mut self) {
        for buffer in &mut self.buffers {
            buffer.fill(S::default());
        }
    }

    /// Filter both channels in place. Blocks of any length are accepted.
    ///
    /// Channels of different lengths reject with [`ChannelLengthMismatch`]
    /// before any sample or history changes.
    pub fn process(
        &mut self,
        left: &mut [S],
        right: &mut [S],
    ) -> Result<(), ChannelLengthMismatch> {
        if left.len() != right.len() {
            return Err(ChannelLengthMismatch);
        }
        let history = self.taps - 1;
        for (left, right) in left
            .chunks_mut(self.maximum_frames)
            .zip(right.chunks_mut(self.maximum_frames))
        {
            for ((samples, buffer), reversed) in [left, right]
                .into_iter()
                .zip(&mut self.buffers)
                .zip(&self.reversed)
            {
                let frames = samples.len();
                buffer[history..history + frames].copy_from_slice(samples);
                S::convolve(reversed, &buffer[..history + frames], samples);
                buffer.copy_within(frames..history + frames, 0);
            }
        }
        Ok(())
    }
}

/// `output[n] = sum(reversed[j] * input[n + j])`, summed in increasing `j`.
/// `input.len() == reversed.len() - 1 + output.len()`.
///
/// Wide register blocks cover most of the block, then eight-output and single
/// output steps cover the rest, so short host blocks still vectorise. Every
/// output uses the same sequential order, so results do not depend on where a
/// block starts or how it is split.
#[inline]
fn convolve<S: FirSample, const WIDE: usize>(reversed: &[S], input: &[S], output: &mut [S]) {
    let wide = steps::<S, WIDE>(reversed, input, output);
    let narrow = wide + steps::<S, 8>(reversed, &input[wide..], &mut output[wide..]);
    steps::<S, 1>(reversed, &input[narrow..], &mut output[narrow..]);
}

/// Compute whole groups of `OUTPUTS` outputs and return how many were written.
///
/// Each step keeps `OUTPUTS` running sums in registers and broadcasts one tap
/// across them, so the inner loop is a lane-wise multiply-add over adjacent
/// inputs with no horizontal reduction.
#[inline(always)]
fn steps<S: FirSample, const OUTPUTS: usize>(
    reversed: &[S],
    input: &[S],
    output: &mut [S],
) -> usize {
    let taps = reversed.len();
    let (blocks, _) = output.as_chunks_mut::<OUTPUTS>();
    for (index, block) in blocks.iter_mut().enumerate() {
        let start = index * OUTPUTS;
        let window = &input[start..start + taps - 1 + OUTPUTS];
        let mut sums = [S::default(); OUTPUTS];
        for (&tap, samples) in reversed.iter().zip(window.windows(OUTPUTS)) {
            let samples: &[S; OUTPUTS] = samples.try_into().unwrap();
            for (sum, &sample) in sums.iter_mut().zip(samples) {
                *sum = sum.accumulate(tap, sample);
            }
        }
        *block = sums;
    }
    blocks.len() * OUTPUTS
}
