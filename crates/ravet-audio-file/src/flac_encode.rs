//! Optional explicit worker-only integer PCM FLAC encoding.
use crate::AudioFileError;
use crate::FlacWriter;
use std::io::Cursor;
/// Lossless signed integer precision; no implicit float quantization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlacPcmEncoding {
    Pcm16,
    Pcm24,
}
impl FlacPcmEncoding {
    pub fn bits(self) -> usize {
        match self {
            Self::Pcm16 => 16,
            Self::Pcm24 => 24,
        }
    }
}
/// Explicit FLAC channel order/encoded rate. This wrapper supports channels 1..8
/// and integer sample rates 1..96000. It adds no speaker-position metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlacFormat {
    pub channels: u16,
    pub sample_rate: u32,
    pub encoding: FlacPcmEncoding,
}
/// Encode a complete nonempty interleaved signed-PCM clip to owned FLAC bytes.
/// Requires optional `flac-encode` (pinned Apache-2.0 flacenc, default features
/// disabled). 1..4,194,304 input i32 samples (16 MiB), valid whole frames and
/// precision ranges are checked before backend block copying/encoding. Encoded output
/// is limited to 64 MiB through the incremental writer's per-frame preflight.
/// Fixed 4096-frame single-thread encoding with constant/verbatim fallback for
/// excessive prediction sizes; backend allocations remain its own
/// contract. No float conversion/clipping, implicit threads, file/network I/O or
/// callback use. This is a complete-clip encoder, not a streaming recorder.
pub fn encode_flac(samples: &[i32], format: FlacFormat) -> Result<Vec<u8>, AudioFileError> {
    if !(1..=8).contains(&format.channels) || !(1..=96000).contains(&format.sample_rate) {
        return Err(AudioFileError::Unsupported);
    }
    if samples.is_empty() || samples.len() > 4_194_304 {
        return Err(AudioFileError::Capacity);
    }
    let channels = usize::from(format.channels);
    let bits = format.encoding.bits();
    let limit = 1_i32 << (bits - 1);
    if !samples.len().is_multiple_of(channels)
        || samples
            .iter()
            .any(|sample| *sample < -limit || *sample >= limit)
    {
        return Err(AudioFileError::InvalidSamples);
    }
    let mut writer = FlacWriter::new(
        Cursor::new(Vec::new()),
        format,
        (samples.len() / channels) as u64,
        67_108_864,
    )?;
    for chunk in samples.chunks(65536 * channels) {
        writer.write_frames(chunk)?;
    }
    Ok(writer.finish()?.into_inner())
}
