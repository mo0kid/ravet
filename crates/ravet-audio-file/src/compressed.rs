//! Explicit worker-only FLAC, MP3 and Ogg/Vorbis decoding.
use crate::AudioFileError;
use std::io::Cursor;
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{CODEC_TYPE_FLAC, CODEC_TYPE_MP3, CODEC_TYPE_VORBIS, DecoderOptions},
    errors::Error,
    formats::FormatOptions,
    io::MediaSourceStream,
    meta::{Limit, MetadataOptions},
    probe::Hint,
};

/// Complete finite interleaved f32 clip at its encoded rate/channel order.
/// No implicit resampling, channel mapping or amplitude normalization is applied.
#[derive(Debug)]
pub struct DecodedClip {
    sample_rate: u32,
    channels: u16,
    samples: Vec<f32>,
}
impl DecodedClip {
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    pub fn channels(&self) -> u16 {
        self.channels
    }
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }
    /// Consume without copying; destruction and decoding belong outside audio.
    pub fn into_samples(self) -> Vec<f32> {
        self.samples
    }
}
pub(super) fn error(error: Error) -> AudioFileError {
    match error {
        Error::IoError(error) => AudioFileError::Io(error),
        Error::Unsupported(_) => AudioFileError::Unsupported,
        _ => AudioFileError::InvalidRecord,
    }
}
/// Decode the single default audio track from explicitly supplied encoded bytes.
/// Requires the optional `compressed` feature (pinned Symphonia). Input is copied
/// and limited to 64 MiB; output is limited by caller frames and 16,777,216 samples
/// (64 MiB), channels 1..64 and integer rates 1..768000. At most 1,048,576 packets
/// are processed; decoded packet capacity is limited to 65536 frames. Backend metadata/visual limits are 64 KiB/0; decoder internal
/// allocations remain backend-owned, not a total-memory or deadline guarantee.
/// Gapless trims are enabled. Decode errors, nonfinite samples, changing audio
/// configuration and chained stream resets reject the entire clip. No partial
/// output is returned, no encoder/network/file I/O occurs and no audio callback
/// use is permitted. Metadata and channel speaker positions are not exposed.
pub fn decode_compressed(
    encoded: &[u8],
    maximum_frames: usize,
) -> Result<DecodedClip, AudioFileError> {
    const MAX_SAMPLES: usize = 16_777_216;
    if encoded.is_empty()
        || encoded.len() > 67_108_864
        || maximum_frames == 0
        || maximum_frames > MAX_SAMPLES
    {
        return Err(AudioFileError::Capacity);
    }
    let source =
        MediaSourceStream::new(Box::new(Cursor::new(encoded.to_vec())), Default::default());
    let mut format = symphonia::default::get_probe()
        .format(
            &Hint::new(),
            source,
            &FormatOptions {
                enable_gapless: true,
                ..Default::default()
            },
            &MetadataOptions {
                limit_metadata_bytes: Limit::Maximum(65536),
                limit_visual_bytes: Limit::Maximum(0),
            },
        )
        .map_err(error)?
        .format;
    let track = format.default_track().ok_or(AudioFileError::Unsupported)?;
    if ![CODEC_TYPE_FLAC, CODEC_TYPE_MP3, CODEC_TYPE_VORBIS].contains(&track.codec_params.codec) {
        return Err(AudioFileError::Unsupported);
    }
    let track_id = track.id;
    let declared = track.codec_params.n_frames;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions { verify: true })
        .map_err(error)?;
    let mut clip: Option<DecodedClip> = None;
    let mut specification = None;
    let mut packets = 0usize;
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(failure) => return Err(error(failure)),
        };
        packets += 1;
        if packets > 1_048_576 {
            return Err(AudioFileError::Capacity);
        }
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = decoder.decode(&packet).map_err(error)?;
        if decoded.capacity() > 65536 {
            return Err(AudioFileError::Capacity);
        }
        if specification.is_some_and(|spec| spec != *decoded.spec()) {
            return Err(AudioFileError::Unsupported);
        }
        specification = Some(*decoded.spec());
        let rate = decoded.spec().rate;
        let channels = decoded.spec().channels.count();
        if !(1..=64).contains(&channels) || !(1..=768000).contains(&rate) {
            return Err(AudioFileError::Unsupported);
        }
        let target = clip.get_or_insert_with(|| DecodedClip {
            sample_rate: rate,
            channels: channels as u16,
            samples: Vec::new(),
        });
        if target.sample_rate != rate || usize::from(target.channels) != channels {
            return Err(AudioFileError::Unsupported);
        }
        let count = decoded
            .frames()
            .checked_mul(channels)
            .ok_or(AudioFileError::Capacity)?;
        let total = target
            .samples
            .len()
            .checked_add(count)
            .ok_or(AudioFileError::Capacity)?;
        if total > MAX_SAMPLES || total / channels > maximum_frames {
            return Err(AudioFileError::Capacity);
        }
        let mut converted = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
        converted.copy_interleaved_ref(decoded);
        if converted.samples().len() != count || converted.samples().iter().any(|v| !v.is_finite())
        {
            return Err(AudioFileError::InvalidSamples);
        }
        target.samples.extend_from_slice(converted.samples());
        // Drop consumed revisions so metadata changes cannot accumulate in RAVET.
        while !format.metadata().is_latest() {
            format.metadata().pop();
        }
    }
    let clip = clip.ok_or(AudioFileError::InvalidRecord)?;
    if declared.is_some_and(|frames| frames != clip.frames() as u64)
        || decoder.finalize().verify_ok == Some(false)
    {
        return Err(AudioFileError::InvalidRecord);
    }
    Ok(clip)
}
