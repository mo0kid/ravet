//! Optional bounded local MPEG1 Layer III encoding on control/worker threads.
use crate::AudioFileError as Error;
use mp3lame_encoder::{
    Bitrate, Builder, Encoder, FlushGap, InterleavedPcm, Mode, MonoPcm, Quality, VbrMode,
};
use std::io::{Cursor, Seek, SeekFrom, Write};
use std::num::NonZeroU32;
const BATCH: usize = 4096;
const SCRATCH: usize = 12320; // ceil(1.25 * BATCH) + 7200, primary LAME bound.
const TAG: usize = 4096;
const MAX_FRAMES: u64 = 16_777_216;
const MAX_BYTES: u64 = 67_108_864;
/// Explicit supported constant bit rate. The pinned backend 192 kbps preset
/// applies a 0.97 input scale; 256/320 presets use 1.0. RAVET adds no compensation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mp3Bitrate {
    Kbps192,
    Kbps256,
    Kbps320,
}
impl Mp3Bitrate {
    pub fn kbps(self) -> u16 {
        match self {
            Self::Kbps192 => 192,
            Self::Kbps256 => 256,
            Self::Kbps320 => 320,
        }
    }
    fn backend(self) -> Bitrate {
        match self {
            Self::Kbps192 => Bitrate::Kbps192,
            Self::Kbps256 => Bitrate::Kbps256,
            Self::Kbps320 => Bitrate::Kbps320,
        }
    }
}
/// Caller-selected supported CBR, highest quality, MPEG1 file profile.
/// Mono/stereo at exactly 32 kHz, 44.1 kHz or 48 kHz; no hidden resampling/channel remap/ID3/VBR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mp3Format {
    pub channels: u16,
    pub sample_rate: u32,
    pub bit_rate: Mp3Bitrate,
}
impl Mp3Format {
    fn validate(self) -> Result<(), Error> {
        if !matches!(self.channels, 1 | 2) || !matches!(self.sample_rate, 32000 | 44100 | 48000) {
            return Err(Error::Unsupported);
        }
        Ok(())
    }
}
/// Call preflight rejects without codec mutation; operational failure poisons and
/// borrows the retained original error, recoverable with the sink at finish.
#[derive(Debug)]
pub enum Mp3WriteError<'a> {
    Rejected(Error),
    Failed(&'a Error),
}
impl std::fmt::Display for Mp3WriteError<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rejected(error) => write!(formatter, "MP3 call rejected: {error}"),
            Self::Failed(error) => write!(formatter, "MP3 writer failed: {error}"),
        }
    }
}
impl std::error::Error for Mp3WriteError<'_> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Rejected(error) => Some(error),
            Self::Failed(error) => Some(*error),
        }
    }
}
/// A failed constructor/finalization still owns its partial sink and first error.
/// Counts distinguish accepted source frames, final file-region bytes and actual sink writes,
/// including the rewritten first frame. No automatic restore/retry/finalization.
#[derive(Debug)]
pub struct Mp3Failure<W> {
    destination: W,
    error: Error,
    accepted_frames: u64,
    file_bytes: u64,
    written_bytes: u64,
}
impl<W> std::fmt::Display for Mp3Failure<W> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "MP3 destination recovered after failure: {}",
            self.error
        )
    }
}
impl<W: std::fmt::Debug> std::error::Error for Mp3Failure<W> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
impl<W> Mp3Failure<W> {
    pub fn error(&self) -> &Error {
        &self.error
    }
    pub fn accepted_frames(&self) -> u64 {
        self.accepted_frames
    }
    pub fn file_bytes(&self) -> u64 {
        self.file_bytes
    }
    pub fn written_bytes(&self) -> u64 {
        self.written_bytes
    }
    pub fn into_parts(self) -> (W, Error) {
        (self.destination, self.error)
    }
}
/// Worker-only seekable MP3 writer. New validates a fresh region at current EOF;
/// callers own truncation/concurrent access. All sample-call preflight precedes
/// codec mutation. Explicit finish flushes codec, validates Info/LAME gapless
/// counts, replaces reserved first frame, restores end and flushes destination.
/// Completed files have exact source-frame metadata; gapless-aware file decoders
/// honor it, whereas arbitrary stream players may retain padding. Drop/abort do
/// not write/seek/flush. Sink destructor is caller policy. Backend working memory
/// remains backend-owned, no total-memory or real-time deadline claim.
///
/// ```
/// use ravet_audio_file::{Mp3Bitrate, Mp3Format, Mp3Writer};
/// use std::io::Cursor;
/// let format = Mp3Format {channels: 2, sample_rate: 48000,
///     bit_rate: Mp3Bitrate::Kbps256};
/// let mut writer = Mp3Writer::new(Cursor::new(Vec::new()), format, 17, 65536).unwrap();
/// writer.write_samples(&[0.25_f32; 34]).unwrap();
/// let complete = writer.finish().unwrap().into_inner();
/// assert!(!complete.is_empty());
/// ```
pub struct Mp3Writer<W: Write + Seek> {
    destination: Option<W>,
    encoder: Option<Encoder>,
    format: Mp3Format,
    output: Vec<u8>,
    tag: Vec<u8>,
    origin: u64,
    accepted: u64,
    file_bytes: u64,
    written: u64,
    maximum_frames: u64,
    maximum_writes: u64,
    first_header: [u8; 4],
    first_header_len: usize,
    failure: Option<Error>,
}
impl<W: Write + Seek> Mp3Writer<W> {
    pub(crate) fn validate_configuration(
        format: Mp3Format,
        frames: u64,
        writes: u64,
    ) -> Result<(), Error> {
        format.validate()?;
        if !(1..=MAX_FRAMES).contains(&frames) || !(1..=MAX_BYTES).contains(&writes) {
            return Err(Error::Capacity);
        }
        Ok(())
    }
    pub fn new(
        destination: W,
        format: Mp3Format,
        maximum_frames: u64,
        maximum_written_bytes: u64,
    ) -> Result<Self, Mp3Failure<W>> {
        let result = (|| {
            Self::validate_configuration(format, maximum_frames, maximum_written_bytes)?;
            let mut output = Vec::new();
            output
                .try_reserve_exact(SCRATCH)
                .map_err(|_| Error::Capacity)?;
            let mut tag = Vec::new();
            tag.try_reserve_exact(TAG).map_err(|_| Error::Capacity)?;
            let mut builder = Builder::new().ok_or(Error::Capacity)?;
            builder
                .set_num_channels(format.channels as u8)
                .map_err(|_| Error::Unsupported)?;
            builder
                .set_sample_rate(format.sample_rate)
                .map_err(|_| Error::Unsupported)?;
            builder
                .set_output_sample_rate(NonZeroU32::new(format.sample_rate))
                .map_err(|_| Error::Unsupported)?;
            builder
                .set_brate(format.bit_rate.backend())
                .map_err(|_| Error::Unsupported)?;
            builder
                .set_mode(if format.channels == 1 {
                    Mode::Mono
                } else {
                    Mode::Stereo
                })
                .map_err(|_| Error::Unsupported)?;
            builder
                .set_quality(Quality::Best)
                .map_err(|_| Error::Unsupported)?;
            builder
                .set_vbr_mode(VbrMode::Off)
                .map_err(|_| Error::Unsupported)?;
            builder
                .set_to_write_vbr_tag(true)
                .map_err(|_| Error::Unsupported)?;
            let encoder = builder.build().map_err(|_| Error::InvalidRecord)?;
            if !encoder.is_lame_tag_written()
                || encoder.id3v2_tag_size() != 0
                || encoder.num_channels() != format.channels as u8
                || encoder.sample_rate() != format.sample_rate
            {
                return Err(Error::InvalidRecord);
            }
            Ok((output, tag, encoder))
        })();
        let (output, tag, encoder) = match result {
            Ok(v) => v,
            Err(error) => {
                return Err(Mp3Failure {
                    destination,
                    error,
                    accepted_frames: 0,
                    file_bytes: 0,
                    written_bytes: 0,
                });
            }
        };
        let mut destination = destination;
        // A failed seek is returned immediately; it may have changed cursor. Do
        // not perform implicit restoration after an error/poison.
        let origin = match (|| {
            let origin = destination.stream_position()?;
            let end = destination.seek(SeekFrom::End(0))?;
            if origin != end {
                destination.seek(SeekFrom::Start(origin))?;
                return Err(Error::InvalidRecord);
            }
            Ok(origin)
        })() {
            Ok(v) => v,
            Err(error) => {
                return Err(Mp3Failure {
                    destination,
                    error,
                    accepted_frames: 0,
                    file_bytes: 0,
                    written_bytes: 0,
                });
            }
        };
        Ok(Self {
            destination: Some(destination),
            encoder: Some(encoder),
            format,
            output,
            tag,
            origin,
            accepted: 0,
            file_bytes: 0,
            written: 0,
            maximum_frames,
            maximum_writes: maximum_written_bytes,
            first_header: [0; 4],
            first_header_len: 0,
            failure: None,
        })
    }
    pub fn format(&self) -> Mp3Format {
        self.format
    }
    pub fn accepted_frames(&self) -> u64 {
        self.accepted
    }
    pub fn file_bytes(&self) -> u64 {
        self.file_bytes
    }
    pub fn written_bytes(&self) -> u64 {
        self.written
    }
    pub fn failure(&self) -> Option<&Error> {
        self.failure.as_ref()
    }
    pub fn write_samples(&mut self, samples: &[f32]) -> Result<usize, Mp3WriteError<'_>> {
        // Borrow the original failure only after all potential mutations, so
        // the returned reference and codec/sink mutation remain exclusive.
        if self.failure.is_none() {
            let channels = usize::from(self.format.channels);
            if samples.is_empty() || !samples.len().is_multiple_of(channels) {
                return Err(Mp3WriteError::Rejected(Error::InvalidSamples));
            }
            let frames = samples.len() / channels;
            if frames > 65536
                || self
                    .accepted
                    .checked_add(frames as u64)
                    .is_none_or(|v| v > self.maximum_frames)
            {
                return Err(Mp3WriteError::Rejected(Error::Capacity));
            }
            if samples
                .iter()
                .any(|x| !x.is_finite() || !(-1.0..=1.0).contains(x))
            {
                return Err(Mp3WriteError::Rejected(Error::InvalidSamples));
            }
            let result = (|| {
                for chunk in samples.chunks(BATCH * channels) {
                    self.output.clear();
                    // Vec methods use spare capacity and only extend after positive
                    // safe upstream return; clear each bounded call and never grow.
                    if self.output.capacity() < SCRATCH {
                        return Err(Error::Capacity);
                    }
                    let encoder = self.encoder.as_mut().unwrap();
                    if channels == 1 {
                        encoder.encode_to_vec(MonoPcm(chunk), &mut self.output)
                    } else {
                        encoder.encode_to_vec(InterleavedPcm(chunk), &mut self.output)
                    }
                    .map_err(|_| Error::InvalidRecord)?;
                    if self.output.len() > SCRATCH {
                        return Err(Error::InvalidRecord);
                    }
                    self.accepted += (chunk.len() / channels) as u64;
                    self.emit_output()?;
                }
                Ok(frames)
            })();
            match result {
                Ok(v) => return Ok(v),
                Err(error) => self.failure = Some(error),
            }
        }
        Err(Mp3WriteError::Failed(
            self.failure
                .as_ref()
                .expect("operational failure was retained"),
        ))
    }
    fn emit_output(&mut self) -> Result<(), Error> {
        let size = self.output.len();
        if self
            .written
            .checked_add(size as u64)
            .is_none_or(|v| v > self.maximum_writes)
            || self
                .origin
                .checked_add(self.file_bytes)
                .and_then(|v| v.checked_add(size as u64))
                .is_none()
        {
            return Err(Error::Capacity);
        }
        let mut offset = 0;
        while offset < size {
            let n = self
                .destination
                .as_mut()
                .unwrap()
                .write(&self.output[offset..])?;
            if n == 0 {
                return Err(Error::Io(std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "MP3 sink accepted no bytes",
                )));
            }
            if n > size - offset {
                return Err(Error::InvalidRecord);
            }
            let header = (4 - self.first_header_len).min(n);
            self.first_header[self.first_header_len..self.first_header_len + header]
                .copy_from_slice(&self.output[offset..offset + header]);
            self.first_header_len += header;
            self.file_bytes += n as u64;
            self.written += n as u64;
            offset += n;
        }
        Ok(())
    }
    fn finalize(&mut self) -> Result<(), Error> {
        if self.accepted == 0 {
            return Err(Error::InvalidSamples);
        }
        self.output.clear();
        self.encoder
            .as_mut()
            .unwrap()
            .flush_to_vec::<FlushGap>(&mut self.output)
            .map_err(|_| Error::InvalidRecord)?;
        if self.output.len() > SCRATCH {
            return Err(Error::InvalidRecord);
        }
        self.emit_output()?;
        let encoder = self.encoder.as_ref().unwrap();
        let size = encoder.lame_tag_size();
        if size == 0 || size > TAG || size > self.tag.capacity() {
            return Err(Error::Capacity);
        }
        self.tag.clear();
        let returned = encoder
            .lame_tag_encode_to_vec(&mut self.tag)
            .ok_or(Error::InvalidRecord)?
            .get();
        if returned != size || self.tag.len() != size {
            return Err(Error::InvalidRecord);
        }
        let info = parse_tag(&self.tag, self.format)?;
        if self.first_header_len != 4
            || frame_size(&self.first_header, self.format)? != size
            || self.first_header != self.tag[..4]
            || info.source_frames != self.accepted
            || u64::from(info.bytes) != self.file_bytes
        {
            return Err(Error::InvalidRecord);
        }
        if self
            .written
            .checked_add(size as u64)
            .is_none_or(|v| v > self.maximum_writes)
        {
            return Err(Error::Capacity);
        }
        let end = self
            .origin
            .checked_add(self.file_bytes)
            .ok_or(Error::Capacity)?;
        let destination = self.destination.as_mut().unwrap();
        if destination.seek(SeekFrom::Start(self.origin))? != self.origin {
            return Err(Error::InvalidRecord);
        }
        let mut offset = 0;
        while offset < size {
            let n = destination.write(&self.tag[offset..])?;
            if n == 0 {
                return Err(Error::Io(std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "MP3 tag accepted no bytes",
                )));
            }
            if n > size - offset {
                return Err(Error::InvalidRecord);
            }
            self.written += n as u64;
            offset += n;
        }
        if destination.seek(SeekFrom::Start(end))? != end {
            return Err(Error::InvalidRecord);
        }
        destination.flush()?;
        Ok(())
    }
    /// Explicitly finalize or return partial sink with retained original error.
    pub fn finish(self) -> Result<W, Mp3Failure<W>> {
        let value = self.finish_reclaim();
        match value.completion {
            crate::codec_recording::CodecRecordingCompletion::Finalized => Ok(value.sink),
            crate::codec_recording::CodecRecordingCompletion::Failed(error) => Err(Mp3Failure {
                destination: value.sink,
                error,
                accepted_frames: value.accepted_frames,
                file_bytes: value.file_bytes,
                written_bytes: value.written_bytes,
            }),
            crate::codec_recording::CodecRecordingCompletion::Aborted => {
                unreachable!("explicit finish returns finalized or failed")
            }
        }
    }
    /// Recover incomplete destination without codec finalization or sink I/O.
    /// This convenience abort discards stored error/counts; inspect getters first
    /// or finish a poisoned writer to recover original diagnostics without I/O.
    pub fn abort(self) -> W {
        self.abort_reclaim(None).sink
    }
    pub(crate) fn abort_reclaim(
        mut self,
        error: Option<Error>,
    ) -> crate::codec_recording::CodecRecovery<W> {
        // A private session marker must never displace the earlier original
        // writer fault. Pumping stops on the first fault, so precedence is exact.
        let error = self.failure.take().or(error);
        self.encoder.take();
        crate::codec_recording::CodecRecovery {
            sink: self.destination.take().unwrap(),
            completion: error
                .map(crate::codec_recording::CodecRecordingCompletion::Failed)
                .unwrap_or(crate::codec_recording::CodecRecordingCompletion::Aborted),
            accepted_frames: self.accepted,
            file_bytes: self.file_bytes,
            written_bytes: self.written,
        }
    }
    pub(crate) fn finish_reclaim(mut self) -> crate::codec_recording::CodecRecovery<W> {
        let error = match self.failure.take() {
            Some(error) => Some(error),
            None => self.finalize().err(),
        };
        self.encoder.take();
        crate::codec_recording::CodecRecovery {
            sink: self.destination.take().unwrap(),
            completion: error
                .map(crate::codec_recording::CodecRecordingCompletion::Failed)
                .unwrap_or(crate::codec_recording::CodecRecordingCompletion::Finalized),
            accepted_frames: self.accepted,
            file_bytes: self.file_bytes,
            written_bytes: self.written,
        }
    }
}
#[derive(Debug)]
struct TagInfo {
    source_frames: u64,
    bytes: u32,
}
fn frame_size(header: &[u8], format: Mp3Format) -> Result<usize, Error> {
    if header.len() < 4 || header[0] != 0xff || header[1] & 0xfe != 0xfa || header[1] & 1 != 1 {
        return Err(Error::InvalidRecord);
    } // MPEG1 LayerIII, noCRC
    let rates = [44100, 48000, 32000];
    let rate = *rates
        .get(usize::from((header[2] >> 2) & 3))
        .ok_or(Error::InvalidRecord)?;
    if rate != format.sample_rate || ((header[3] >> 6) == 3) != (format.channels == 1) {
        return Err(Error::InvalidRecord);
    }
    let rates_kbps = [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0,
    ];
    let bitrate = rates_kbps[usize::from(header[2] >> 4)];
    if bitrate == 0 {
        return Err(Error::InvalidRecord);
    }
    Ok((144000 * bitrate / rate + u32::from((header[2] >> 1) & 1)) as usize)
}
fn parse_tag(tag: &[u8], format: Mp3Format) -> Result<TagInfo, Error> {
    if frame_size(tag, format)? != tag.len() {
        return Err(Error::InvalidRecord);
    }
    let mut offset = 4 + if format.channels == 1 { 17 } else { 32 };
    let magic = tag.get(offset..offset + 4).ok_or(Error::InvalidRecord)?;
    if magic != b"Info" && magic != b"Xing" {
        return Err(Error::InvalidRecord);
    }
    offset += 4;
    let get_u32 = |at: usize| -> Result<u32, Error> {
        Ok(u32::from_be_bytes(
            tag.get(at..at + 4)
                .ok_or(Error::InvalidRecord)?
                .try_into()
                .unwrap(),
        ))
    };
    let flags = get_u32(offset)?;
    offset += 4;
    if flags & !15 != 0 || flags & 3 != 3 {
        return Err(Error::InvalidRecord);
    }
    let frames = get_u32(offset)?;
    offset += 4;
    let bytes = get_u32(offset)?;
    offset += 4;
    if flags & 4 != 0 {
        offset += 100
    }
    if flags & 8 != 0 {
        offset += 4
    }
    if tag.get(offset..offset + 9) != Some(b"LAME3.100") {
        return Err(Error::InvalidRecord);
    }
    let crc_end = offset.checked_add(34).ok_or(Error::InvalidRecord)?;
    let stored_crc = u16::from_be_bytes(
        tag.get(crc_end..crc_end + 2)
            .ok_or(Error::InvalidRecord)?
            .try_into()
            .unwrap(),
    );
    if crc16(tag.get(..crc_end).ok_or(Error::InvalidRecord)?) != stored_crc {
        return Err(Error::InvalidRecord);
    }
    let packed = tag
        .get(offset + 21..offset + 24)
        .ok_or(Error::InvalidRecord)?;
    let delay = u64::from(packed[0]) * 16 + u64::from(packed[1] >> 4);
    let padding = u64::from(packed[1] & 15) * 256 + u64::from(packed[2]);
    let source_frames = u64::from(frames)
        .checked_mul(1152)
        .and_then(|v| v.checked_sub(delay + padding))
        .ok_or(Error::InvalidRecord)?;
    if source_frames == 0 || delay != 576 || padding > 3000 {
        return Err(Error::InvalidRecord);
    }
    Ok(TagInfo {
        source_frames,
        bytes,
    })
}
fn crc16(bytes: &[u8]) -> u16 {
    // LAME tag CRC covers physical first frame through music CRC, before its
    // own two-byte field; reflected x^16+x^15+x^2+1, initial zero.
    let mut crc = 0_u16;
    for byte in bytes {
        crc ^= u16::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0xa001 } else { 0 };
        }
    }
    crc
}
/// Encode a complete supported finite clip; same worker-only/gapless-file limits.
pub fn encode_mp3(format: Mp3Format, samples: &[f32]) -> Result<Vec<u8>, Error> {
    format.validate()?;
    let channels = usize::from(format.channels);
    if samples.is_empty() || !samples.len().is_multiple_of(channels) {
        return Err(Error::InvalidSamples);
    }
    if samples.len() > 4_194_304 {
        return Err(Error::Capacity);
    }
    if samples
        .iter()
        .any(|x| !x.is_finite() || !(-1.0..=1.0).contains(x))
    {
        return Err(Error::InvalidSamples);
    }
    let mut writer = Mp3Writer::new(
        Cursor::new(Vec::new()),
        format,
        (samples.len() / channels) as u64,
        MAX_BYTES,
    )
    .map_err(|failure| failure.into_parts().1)?;
    for chunk in samples.chunks(65536 * channels) {
        if writer.write_samples(chunk).is_err() {
            return Err(writer.finish().err().unwrap().into_parts().1);
        }
    }
    writer
        .finish()
        .map(|w| w.into_inner())
        .map_err(|failure| failure.into_parts().1)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn format() -> Mp3Format {
        Mp3Format {
            channels: 2,
            sample_rate: 48000,
            bit_rate: Mp3Bitrate::Kbps192,
        }
    }
    #[test]
    fn bounded_scratch_and_independently_parsed_physical_frames_agree_with_gapless_tag() {
        for bit_rate in [
            Mp3Bitrate::Kbps192,
            Mp3Bitrate::Kbps256,
            Mp3Bitrate::Kbps320,
        ] {
            for channels in [1, 2] {
                for sample_rate in [32000, 44100, 48000] {
                    let profile = Mp3Format {
                        bit_rate,
                        channels,
                        sample_rate,
                    };
                    for frames in [1, 17, 4096, 8193] {
                        let mut writer =
                            Mp3Writer::new(Cursor::new(Vec::new()), profile, frames, 65536)
                                .ok()
                                .unwrap();
                        let output_capacity = writer.output.capacity();
                        let tag_capacity = writer.tag.capacity();
                        for _ in 0..frames {
                            writer
                                .write_samples(&[0.25, -0.25][..usize::from(channels)])
                                .unwrap();
                            assert_eq!(writer.output.capacity(), output_capacity);
                            assert!(writer.output.len() <= SCRATCH);
                            assert_eq!(writer.tag.capacity(), tag_capacity);
                        }
                        let bytes = writer.finish().unwrap().into_inner();
                        let first_size = frame_size(&bytes, profile).unwrap();
                        let info = parse_tag(&bytes[..first_size], profile).unwrap();
                        assert_eq!(info.source_frames, frames);
                        assert_eq!(u64::from(info.bytes), bytes.len() as u64);
                        let mut offset = 0;
                        let mut physical_frames = 0;
                        while offset < bytes.len() {
                            let size = frame_size(&bytes[offset..], profile).unwrap();
                            assert!(offset + size <= bytes.len());
                            if physical_frames > 0 {
                                assert_eq!(
                                    bytes[offset + 2] >> 4,
                                    match bit_rate {
                                        Mp3Bitrate::Kbps192 => 11,
                                        Mp3Bitrate::Kbps256 => 13,
                                        Mp3Bitrate::Kbps320 => 14,
                                    },
                                    "actual caller audio bit rate, metadata frame can differ"
                                );
                            }
                            offset += size;
                            physical_frames += 1;
                        }
                        assert_eq!(offset, bytes.len());
                        let at = (if channels == 1 { 21 } else { 36 }) + 8;
                        let declared = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
                        assert_eq!(physical_frames, u64::from(declared) + 1);
                    }
                }
            }
        }
    }
    #[test]
    fn tag_magic_version_flags_crc_length_and_profile_reject_malformed_metadata() {
        let bytes = encode_mp3(format(), &[0.25, -0.25]).unwrap();
        let size = frame_size(&bytes, format()).unwrap();
        let original = bytes[..size].to_vec();
        assert_eq!(parse_tag(&original, format()).unwrap().source_frames, 1);
        for offset in [0, 36, 43, 100, 156] {
            let mut tag = original.clone();
            tag[offset] ^= 0x80;
            assert!(parse_tag(&tag, format()).is_err(), "offset{offset}");
        }
        assert!(parse_tag(&original[..original.len() - 1], format()).is_err());
        assert!(
            parse_tag(
                &original,
                Mp3Format {
                    channels: 1,
                    sample_rate: 48000,
                    bit_rate: Mp3Bitrate::Kbps192
                }
            )
            .is_err()
        );
        assert!(
            parse_tag(
                &original,
                Mp3Format {
                    channels: 2,
                    sample_rate: 44100,
                    bit_rate: Mp3Bitrate::Kbps192
                }
            )
            .is_err()
        );
    }
}
