//! Prepared block-based stereo oversampling with the fixed half-band profile.

use crate::OversamplingFactor;
use crate::oversampling::{DOWN, UP};

/// One half-band stage for both channels. Lanes are
/// `[left direct, left delayed, right direct, right delayed]`: the two polyphase
/// branches of both channels advance together in one four-lane vector. Each
/// lane performs exactly the scalar operations of [`crate::HalfBand2x`].
#[derive(Clone, Copy, Debug, Default)]
struct StereoStage {
    up: [[f32; 4]; 4],
    down: [[f32; 4]; 3],
    delayed: [f32; 2],
}

const UP_LANES: [[f32; 4]; 4] = {
    let mut lanes = [[0.; 4]; 4];
    let mut section = 0;
    while section < 4 {
        // The delayed branch has three sections; its fourth lane is unused.
        let delayed = if section < 3 { UP[4 + section] } else { 0. };
        lanes[section] = [UP[section], delayed, UP[section], delayed];
        section += 1;
    }
    lanes
};
const DOWN_LANES: [[f32; 4]; 3] = {
    let mut lanes = [[0.; 4]; 3];
    let mut section = 0;
    while section < 3 {
        lanes[section] = [
            DOWN[section],
            DOWN[3 + section],
            DOWN[section],
            DOWN[3 + section],
        ];
        section += 1;
    }
    lanes
};

#[inline(always)]
fn section(sample: [f32; 4], coefficient: [f32; 4], state: &mut [f32; 4]) -> [f32; 4] {
    let mut filtered = [0.; 4];
    for lane in 0..4 {
        filtered[lane] = coefficient[lane] * sample[lane] + state[lane];
        state[lane] = sample[lane] - coefficient[lane] * filtered[lane];
    }
    filtered
}

impl StereoStage {
    /// One input frame to two output frames per channel.
    #[inline(always)]
    fn upsample(&mut self, left: f32, right: f32) -> [[f32; 2]; 2] {
        let mut x = [left, left, right, right];
        for (coefficient, state) in UP_LANES[..3].iter().zip(&mut self.up) {
            x = section(x, *coefficient, state);
        }
        let last = section(x, UP_LANES[3], &mut self.up[3]);
        [[last[0], x[1]], [last[2], x[3]]]
    }

    /// Two input frames per channel to one output frame.
    #[inline(always)]
    fn downsample(&mut self, left: [f32; 2], right: [f32; 2]) -> [f32; 2] {
        let mut x = [left[0], left[1], right[0], right[1]];
        for (coefficient, state) in DOWN_LANES.iter().zip(&mut self.down) {
            x = section(x, *coefficient, state);
        }
        let output = [
            (self.delayed[0] + x[0]) * 0.5,
            (self.delayed[1] + x[2]) * 0.5,
        ];
        self.delayed = [x[1], x[3]];
        output
    }
}

/// Maximum prepared host-rate block length of [`StereoOversampler`].
pub const MAXIMUM_OVERSAMPLER_FRAMES: usize = 65_536;

/// Invalid [`StereoOversampler`] preparation or block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OversamplerError {
    /// `maximum_frames` is zero or above 65,536.
    InvalidMaximumFrames,
    /// Channel lengths differ, a block exceeds the prepared maximum, or the
    /// downsampling outputs do not match the last upsampled block.
    InvalidBlock,
}
impl core::fmt::Display for OversamplerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "oversampler error: {self:?}")
    }
}
impl std::error::Error for OversamplerError {}

/// Prepared stereo 1×/2×/4×/8× oversampling over whole blocks.
///
/// Uses the same fixed polyphase half-band IIR stages as [`crate::HalfBand2x`] and
/// [`crate::Oversampler`], and produces bit-identical samples to one
/// [`crate::Oversampler`] per channel with the same per-sample processing. It
/// works stage by stage over a block, with both channels in the same loop, which
/// removes per-sample closure and staging overhead.
///
/// Preparation allocates high-rate buffers for `maximum_frames` host frames;
/// [`StereoOversampler::upsample`], [`StereoOversampler::downsample`],
/// [`StereoOversampler::process`] and reset never allocate. The IIR stages add
/// frequency-dependent delay; no constant integer latency is reported and the
/// dry path is not aligned. Process high-rate DSP at the host rate times
/// [`StereoOversampler::factor`].
#[derive(Clone, Debug)]
pub struct StereoOversampler {
    factor: OversamplingFactor,
    maximum_frames: usize,
    stages: [StereoStage; 3],
    /// Two ping-pong buffer pairs of `maximum_frames × factor` samples.
    buffers: [[Vec<f32>; 2]; 2],
    /// Host frames of the last upsampled block.
    frames: usize,
}

impl StereoOversampler {
    /// Prepare for host blocks of up to `maximum_frames` (1..=65,536).
    pub fn new(
        factor: OversamplingFactor,
        maximum_frames: usize,
    ) -> Result<Self, OversamplerError> {
        if maximum_frames == 0 || maximum_frames > MAXIMUM_OVERSAMPLER_FRAMES {
            return Err(OversamplerError::InvalidMaximumFrames);
        }
        let length = maximum_frames * factor.factor();
        let pair = || [vec![0.; length], vec![0.; length]];
        Ok(Self {
            factor,
            maximum_frames,
            stages: Default::default(),
            buffers: [pair(), pair()],
            frames: 0,
        })
    }

    /// The oversampling factor.
    pub fn factor(&self) -> OversamplingFactor {
        self.factor
    }

    /// Prepared maximum host block length.
    pub fn maximum_frames(&self) -> usize {
        self.maximum_frames
    }

    /// Clear filter state and forget the last upsampled block.
    pub fn reset(&mut self) {
        self.stages = Default::default();
        self.frames = 0;
    }

    fn stage_count(&self) -> usize {
        self.factor.factor().trailing_zeros() as usize
    }

    /// Upsample one host block and return the high-rate left and right
    /// buffers, `factor` times longer, for in-place processing.
    pub fn upsample(
        &mut self,
        left: &[f32],
        right: &[f32],
    ) -> Result<[&mut [f32]; 2], OversamplerError> {
        let frames = left.len();
        if right.len() != frames || frames > self.maximum_frames {
            return Err(OversamplerError::InvalidBlock);
        }
        self.frames = frames;
        let stages = self.stage_count();
        let [first, second] = &mut self.buffers;
        first[0][..frames].copy_from_slice(left);
        first[1][..frames].copy_from_slice(right);
        let mut count = frames;
        let mut source_is_first = true;
        for stage in &mut self.stages[..stages] {
            let (source, destination) = if source_is_first {
                (&*first, &mut *second)
            } else {
                (&*second, &mut *first)
            };
            let [out_left, out_right] = destination;
            let pairs = out_left[..2 * count]
                .as_chunks_mut::<2>()
                .0
                .iter_mut()
                .zip(out_right[..2 * count].as_chunks_mut::<2>().0);
            for ((&l, &r), (out_l, out_r)) in source[0][..count]
                .iter()
                .zip(&source[1][..count])
                .zip(pairs)
            {
                [*out_l, *out_r] = stage.upsample(l, r);
            }
            count *= 2;
            source_is_first = !source_is_first;
        }
        let [l, r] = if source_is_first { first } else { second };
        Ok([&mut l[..count], &mut r[..count]])
    }

    /// Decimate the high-rate buffers of the last [`StereoOversampler::upsample`]
    /// call into host-rate outputs whose length equals that block.
    pub fn downsample(
        &mut self,
        left: &mut [f32],
        right: &mut [f32],
    ) -> Result<(), OversamplerError> {
        let frames = self.frames;
        if left.len() != frames || right.len() != frames {
            return Err(OversamplerError::InvalidBlock);
        }
        let stages = self.stage_count();
        let [first, second] = &mut self.buffers;
        // After upsampling, the high-rate data is in `first` for an even stage count.
        let mut source_is_first = stages.is_multiple_of(2);
        let mut count = frames << stages;
        for stage in self.stages[..stages].iter_mut().rev() {
            let (source, destination) = if source_is_first {
                (&*first, &mut *second)
            } else {
                (&*second, &mut *first)
            };
            count /= 2;
            let [out_left, out_right] = destination;
            for ((l, r), (out_l, out_r)) in source[0][..2 * count]
                .as_chunks::<2>()
                .0
                .iter()
                .zip(source[1][..2 * count].as_chunks::<2>().0)
                .zip(out_left[..count].iter_mut().zip(&mut out_right[..count]))
            {
                [*out_l, *out_r] = stage.downsample(*l, *r);
            }
            source_is_first = !source_is_first;
        }
        let [l, r] = if source_is_first { first } else { second };
        left.copy_from_slice(&l[..frames]);
        right.copy_from_slice(&r[..frames]);
        Ok(())
    }

    /// Upsample, call `process` once per prepared-size piece with the high-rate
    /// left and right buffers, and decimate back in place. Blocks of any length
    /// are accepted; channels of different lengths reject before any change.
    pub fn process(
        &mut self,
        left: &mut [f32],
        right: &mut [f32],
        mut process: impl FnMut(&mut [f32], &mut [f32]),
    ) -> Result<(), OversamplerError> {
        if left.len() != right.len() {
            return Err(OversamplerError::InvalidBlock);
        }
        let maximum = self.maximum_frames;
        for (left, right) in left.chunks_mut(maximum).zip(right.chunks_mut(maximum)) {
            let [high_left, high_right] = self.upsample(left, right)?;
            process(high_left, high_right);
            self.downsample(left, right)?;
        }
        Ok(())
    }
}
