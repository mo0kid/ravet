//! Worker-only compressed streaming with atomic replay-based exact seeks.
use crate::{AudioFileError, compressed::error};
use std::{io::Cursor, sync::Arc};
use symphonia::core::{
    audio::{SampleBuffer, SignalSpec},
    codecs::{CODEC_TYPE_FLAC, CODEC_TYPE_MP3, CODEC_TYPE_VORBIS, Decoder, DecoderOptions},
    errors::Error,
    formats::{FormatOptions, FormatReader},
    io::MediaSourceStream,
    meta::{Limit, MetadataOptions},
    probe::Hint,
};
const MAX_SAMPLES: usize = 16_777_216;
struct ReplayState {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track: u32,
    declared: Option<u64>,
    spec: Option<SignalSpec>,
    pending: Vec<f32>,
    offset: usize,
    packets: usize,
    decoded: usize,
    position: usize,
    maximum: usize,
    eof: bool,
}
impl ReplayState {
    fn new(encoded: Arc<[u8]>, maximum: usize) -> Result<Self, AudioFileError> {
        let source = MediaSourceStream::new(Box::new(Cursor::new(encoded)), Default::default());
        let format = symphonia::default::get_probe()
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
        if ![CODEC_TYPE_FLAC, CODEC_TYPE_MP3, CODEC_TYPE_VORBIS].contains(&track.codec_params.codec)
        {
            return Err(AudioFileError::Unsupported);
        }
        let id = track.id;
        let declared = track.codec_params.n_frames;
        if declared.is_some_and(|frames| frames > maximum as u64) {
            return Err(AudioFileError::Capacity);
        }
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions { verify: true })
            .map_err(error)?;
        let mut state = Self {
            format,
            decoder,
            track: id,
            declared,
            spec: None,
            pending: Vec::new(),
            offset: 0,
            packets: 0,
            decoded: 0,
            position: 0,
            maximum,
            eof: false,
        };
        if !state.next()? {
            return Err(AudioFileError::InvalidRecord);
        }
        Ok(state)
    }
    fn channels(&self) -> usize {
        self.spec.expect("initial decoded packet").channels.count()
    }
    fn next(&mut self) -> Result<bool, AudioFileError> {
        self.pending.clear();
        self.offset = 0;
        if self.eof {
            return Ok(false);
        }
        loop {
            let packet = match self.format.next_packet() {
                Ok(packet) => packet,
                Err(Error::IoError(io)) if io.kind() == std::io::ErrorKind::UnexpectedEof => {
                    if self
                        .declared
                        .is_some_and(|frames| frames != self.decoded as u64)
                        || self.decoder.finalize().verify_ok == Some(false)
                    {
                        return Err(AudioFileError::InvalidRecord);
                    }
                    self.eof = true;
                    return Ok(false);
                }
                Err(failure) => return Err(error(failure)),
            };
            self.packets += 1;
            if self.packets > 1_048_576 {
                return Err(AudioFileError::Capacity);
            }
            while !self.format.metadata().is_latest() {
                self.format.metadata().pop();
            }
            if packet.track_id() != self.track {
                continue;
            }
            let decoded = self.decoder.decode(&packet).map_err(error)?;
            let spec = *decoded.spec();
            if decoded.capacity() > 65536 {
                return Err(AudioFileError::Capacity);
            }
            if self.spec.is_some_and(|previous| previous != spec) {
                return Err(AudioFileError::Unsupported);
            }
            let channels = spec.channels.count();
            if !(1..=64).contains(&channels) || !(1..=768000).contains(&spec.rate) {
                return Err(AudioFileError::Unsupported);
            }
            let total = self
                .decoded
                .checked_add(decoded.frames())
                .ok_or(AudioFileError::Capacity)?;
            if total > self.maximum
                || total
                    .checked_mul(channels)
                    .is_none_or(|samples| samples > MAX_SAMPLES)
            {
                return Err(AudioFileError::Capacity);
            }
            let mut converted = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
            converted.copy_interleaved_ref(decoded);
            if converted.samples().len() != (total - self.decoded) * channels
                || converted.samples().iter().any(|value| !value.is_finite())
            {
                return Err(AudioFileError::InvalidSamples);
            }
            self.spec = Some(spec);
            self.decoded = total;
            self.pending.extend_from_slice(converted.samples());
            if !self.pending.is_empty() {
                return Ok(true);
            }
        }
    }
    fn read(&mut self, output: &mut [f32]) -> Result<usize, AudioFileError> {
        let channels = self.channels();
        let mut used = 0;
        while used < output.len() {
            if self.offset == self.pending.len() && !self.next()? {
                break;
            }
            let count = (output.len() - used).min(self.pending.len() - self.offset);
            output[used..used + count]
                .copy_from_slice(&self.pending[self.offset..self.offset + count]);
            self.offset += count;
            used += count;
            self.position += count / channels;
        }
        Ok(used / channels)
    }
    fn skip(&mut self, target: usize) -> Result<(), AudioFileError> {
        let channels = self.channels();
        while self.position < target {
            if self.offset == self.pending.len() && !self.next()? {
                return Err(AudioFileError::InvalidRecord);
            }
            let frames =
                (target - self.position).min((self.pending.len() - self.offset) / channels);
            self.offset += frames * channels;
            self.position += frames;
        }
        Ok(())
    }
}
enum DecoderState {
    Replay(ReplayState),
    Indexed(crate::compressed_indexed::IndexedState),
}
impl DecoderState {
    fn channels(&self) -> usize {
        match self {
            Self::Replay(s) => s.channels(),
            Self::Indexed(s) => s.channels(),
        }
    }
    fn sample_rate(&self) -> u32 {
        match self {
            Self::Replay(s) => s.spec.expect("initial packet").rate,
            Self::Indexed(s) => s.sample_rate(),
        }
    }
    fn position(&self) -> usize {
        match self {
            Self::Replay(s) => s.position,
            Self::Indexed(s) => s.position(),
        }
    }
    fn declared(&self) -> Option<u64> {
        match self {
            Self::Replay(s) => s.declared,
            Self::Indexed(s) => s.declared(),
        }
    }
    fn eof(&self) -> bool {
        match self {
            Self::Replay(s) => s.eof,
            Self::Indexed(s) => s.eof(),
        }
    }
    fn read(&mut self, output: &mut [f32]) -> Result<usize, AudioFileError> {
        match self {
            Self::Replay(s) => s.read(output),
            Self::Indexed(s) => s.read(output),
        }
    }
}
/// Lazy decoded single-default-track FLAC/MP3/Vorbis reader. Requires `compressed`.
/// Encoded input is copied once (1..64 MiB); decoded frames are capped by caller
/// 1..16,777,216 and total interleaved samples <=16,777,216. Decoded packet capacity
/// <=65536 frames, channels 1..64/rate 1..768000. Backend allocations remain its own
/// contract. No file/network I/O, resampling, speaker remapping or callback use.
/// Gapless trims apply; EOF checks declared duration/decoder integrity. Changing
/// configurations/chained resets reject. The default constructor uses replay seeks.
/// `new_indexed_flac` instead verifies the whole native FLAC stream once and retains
/// a bounded compressed-packet index; both modes swap seek candidates only on success.
pub struct CompressedReader {
    encoded: Option<Arc<[u8]>>,
    state: DecoderState,
    failed: bool,
}
impl CompressedReader {
    pub fn new(encoded: &[u8], maximum_frames: usize) -> Result<Self, AudioFileError> {
        if encoded.is_empty()
            || encoded.len() > 67_108_864
            || !(1..=MAX_SAMPLES).contains(&maximum_frames)
        {
            return Err(AudioFileError::Capacity);
        }
        let encoded: Arc<[u8]> = Arc::from(encoded);
        let state = DecoderState::Replay(ReplayState::new(Arc::clone(&encoded), maximum_frames)?);
        Ok(Self {
            encoded: Some(encoded),
            state,
            failed: false,
        })
    }
    /// Build an exact native-FLAC compressed-packet index on the calling worker.
    /// Copies 1..64 MiB input temporarily, decodes/verifies every packet once and
    /// retains at most 65,536 compressed packets / 64 MiB payload plus index metadata.
    /// No complete PCM clip or original container copy is retained. Caller frame
    /// and 16,777,216-sample limits still apply. A nonzero declared FLAC frame
    /// count is required. MP3, Vorbis and Ogg-FLAC reject.
    /// Subsequent seeks binary-search packet timestamps and decode only the
    /// containing packet (at most 65,536 frames); EOF seeks decode none. All methods
    /// still allocate on a worker and supply no file/device or deadline guarantee.
    pub fn new_indexed_flac(encoded: &[u8], maximum_frames: usize) -> Result<Self, AudioFileError> {
        if encoded.is_empty()
            || encoded.len() > 67_108_864
            || !(1..=MAX_SAMPLES).contains(&maximum_frames)
        {
            return Err(AudioFileError::Capacity);
        }
        let state =
            crate::compressed_indexed::IndexedState::new(Arc::from(encoded), maximum_frames)?;
        Ok(Self {
            encoded: None,
            state: DecoderState::Indexed(state),
            failed: false,
        })
    }
    /// Number of verified retained FLAC packets, or `None` in default replay mode.
    pub fn indexed_packets(&self) -> Option<usize> {
        match &self.state {
            DecoderState::Replay(_) => None,
            DecoderState::Indexed(s) => Some(s.packet_count()),
        }
    }
    pub fn sample_rate(&self) -> u32 {
        self.state.sample_rate()
    }
    pub fn channels(&self) -> u16 {
        self.state.channels() as u16
    }
    pub fn position_frames(&self) -> usize {
        self.state.position()
    }
    pub fn declared_frames(&self) -> Option<u64> {
        self.state.declared()
    }
    /// True only after EOF duration/integrity checks complete. Queued/read-ahead
    /// audio may still need playback; cursor equality alone is not this signal.
    pub fn end_of_file(&self) -> bool {
        self.state.eof()
    }
    /// Read up to 65536 whole interleaved frames. Success leaves the unused output
    /// tail unchanged; zero-sized output makes no progress. Invalid buffer lengths
    /// reject before mutation. Decode faults clear the whole supplied output and
    /// poison reads (cursor may have advanced); successful seek can recover.
    pub fn read_frames(&mut self, output: &mut [f32]) -> Result<usize, AudioFileError> {
        if !output.len().is_multiple_of(self.state.channels())
            || output.len() / self.state.channels() > 65536
        {
            return Err(AudioFileError::Capacity);
        }
        if self.failed {
            output.fill(0.);
            return Err(AudioFileError::Poisoned);
        }
        match self.state.read(output) {
            Ok(count) => Ok(count),
            Err(failure) => {
                self.failed = true;
                output.fill(0.);
                Err(failure)
            }
        }
    }
    /// Exact decoded-frame seek including EOF; failure preserves old decoder,
    /// cursor and poison state. Default mode replays O(target) worker frames;
    /// indexed FLAC mode searches the immutable index and decodes one packet.
    /// Both modes allocate candidate backend/pending storage outside audio.
    pub fn seek_frame(&mut self, frame: usize) -> Result<(), AudioFileError> {
        let candidate = match &self.state {
            DecoderState::Replay(state) => {
                if frame > state.maximum
                    || state
                        .declared
                        .is_some_and(|declared| frame as u64 > declared)
                {
                    return Err(AudioFileError::Capacity);
                }
                let mut candidate = ReplayState::new(
                    Arc::clone(self.encoded.as_ref().expect("replay input")),
                    state.maximum,
                )?;
                candidate.skip(frame)?;
                DecoderState::Replay(candidate)
            }
            DecoderState::Indexed(state) => DecoderState::Indexed(state.seek(frame)?),
        };
        self.state = candidate;
        self.failed = false;
        Ok(())
    }
}
