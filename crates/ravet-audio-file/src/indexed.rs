//! Finite-history integer PCM seeking; legacy replay and counters stay separate.
use super::{ConvertedReadAhead, ConvertedReadAheadFailure, State, total, zeros};
use crate::{
    AudioConsumer, AudioFileError as Error, AudioProducer, ConvertedReadAheadOptions,
    ConvertedReadAheadProgress, IndexedPlaybackReader, PlaybackConversion, ReadAheadProgress,
    ReadAheadPump,
};

/// Window work and mathematical priming are distinct from decoded file prefixes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IndexedSeekReceipt {
    pub generation: u64,
    pub output_frame: u64,
    pub window_original_start: u64,
    pub window_original_frames: usize,
    /// None at a verified finite endpoint: no next output is primed there.
    pub filter_input_coordinate: Option<u128>,
    pub leading_zero_history_frames: usize,
    pub trailing_zero_history_frames: usize,
    pub reader_position: u64,
    pub verified_end_point: bool,
}
/// Per-generation output work, original cursor movement and priming window work.
/// Skipped prefixes are never labelled decoded or submitted source frames.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IndexedReadAheadCounts {
    pub prime: IndexedSeekReceipt,
    pub original_reader_position: u64,
    /// Includes commits before errors, not evidence of validated decoded PCM.
    /// None if a contract-violating reader moved backwards from the prime cursor.
    pub original_cursor_advance_after_prime: Option<u64>,
    pub original_frames_submitted_after_prime: u64,
    pub padding_frames_submitted_after_prime: usize,
    pub absolute_output_generated: u64,
    pub output_frames_generated_after_prime: u64,
    pub output_frames_published: u64,
}
/// Original seek/decode error plus the failed fresh reader, when one was created.
/// Its cursor can have advanced; no restoration or hidden retry occurs.
pub struct IndexedSeekFailure<R> {
    candidate: Option<R>,
    error: Error,
}
impl<R> IndexedSeekFailure<R> {
    pub fn error(&self) -> &Error {
        &self.error
    }
    pub fn candidate(&self) -> Option<&R> {
        self.candidate.as_ref()
    }
    pub fn into_parts(self) -> (Option<R>, Error) {
        (self.candidate, self.error)
    }
}
impl<R> std::fmt::Debug for IndexedSeekFailure<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndexedSeekFailure")
            .field("error", &self.error)
            .field("has_candidate", &self.candidate.is_some())
            .finish()
    }
}
impl<R> std::fmt::Display for IndexedSeekFailure<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "indexed converted seek: {}", self.error)
    }
}
impl<R> std::error::Error for IndexedSeekFailure<R> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
#[derive(Clone, Copy, Default)]
struct Prime {
    receipt: IndexedSeekReceipt,
    source_submitted: u64,
    padding_submitted: usize,
}
/// Explicit known-duration indexed PCM transport using IntegerSinc only.
/// Each stopped candidate reads at most the taps-sized source window, plus an
/// EOF probe when needed. The factory owns version stability and metadata parsing;
/// skipped content is not authenticated. No compressed reader/deadline is implied.
/// Temporary window scratch is at most 66KiB in addition to existing bounded state.
///
/// ```
/// use ravet_audio_file::{audio_ring, IndexedConvertedReadAhead, ConvertedReadAheadOptions,
///     PlaybackConversion, WaveEncoding, WaveFormat, WaveReader, WaveWriter};
/// use std::io::Cursor;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut writer = WaveWriter::new(Cursor::new(Vec::new()),
///     WaveFormat { channels: 1, sample_rate: 44100, encoding: WaveEncoding::Float32 }, 17)?;
/// writer.write_frames(&[0.25; 17])?;
/// let bytes = writer.finish()?.into_inner();
/// let reader = WaveReader::new(Cursor::new(bytes.clone()), 17)?;
/// let (producer, mut consumer) = audio_ring(1, 32).unwrap();
/// let mut pump = IndexedConvertedReadAhead::new(reader,
///     move || WaveReader::new(Cursor::new(bytes.clone()), 17), producer, &consumer,
///     ConvertedReadAheadOptions { output_rate: 48000,
///         conversion: PlaybackConversion::IntegerSinc { taps: 16 },
///         maximum_source_frames: 17, maximum_output_frames: 64,
///         source_batch_frames: 17, conversion_batch_frames: 17 })?;
/// let receipt = pump.seek_indexed_stopped(&mut consumer, 1)?;
/// assert_eq!(receipt.window_original_frames, 2);
/// assert_eq!(pump.counts().original_frames_submitted_after_prime, 0);
/// assert_eq!(pump.output_origin(), 1);
/// # Ok(()) }
/// ```
pub struct IndexedConvertedReadAhead<R, F> {
    inner: ConvertedReadAhead<R, F>,
    prime: Prime,
}
impl<R: IndexedPlaybackReader, F: FnMut() -> Result<R, Error>> IndexedConvertedReadAhead<R, F> {
    /// Frame-zero, known-duration IntegerSinc preparation, with matching empty
    /// stopped queue. No source reads, seeks or factory calls; failure owns inputs.
    pub fn new(
        reader: R,
        factory: F,
        producer: AudioProducer,
        consumer: &AudioConsumer,
        options: ConvertedReadAheadOptions,
    ) -> Result<Self, ConvertedReadAheadFailure<R, F>> {
        if reader.declared_frames().is_none()
            || !matches!(options.conversion, PlaybackConversion::IntegerSinc { .. })
        {
            return Err(ConvertedReadAheadFailure {
                reader,
                factory,
                producer,
                error: Error::Unsupported,
            });
        }
        if !producer.owns(consumer)
            || consumer.queued_frames() != 0
            || reader.channels() != producer.channels()
        {
            return Err(ConvertedReadAheadFailure {
                reader,
                factory,
                producer,
                error: Error::InvalidSamples,
            });
        }
        match State::new_indexed(reader, options) {
            Ok(state) => Ok(Self {
                inner: ConvertedReadAhead {
                    state,
                    factory,
                    producer,
                    generation: 0,
                    output_origin: 0,
                    published: 0,
                    failed: false,
                    last_progressed: false,
                },
                prime: Prime::default(),
            }),
            Err((reader, error)) => Err(ConvertedReadAheadFailure {
                reader,
                factory,
                producer,
                error,
            }),
        }
    }
    pub fn counts(&self) -> IndexedReadAheadCounts {
        let counts = self.inner.counts();
        let cursor = self.inner.input_reader_position();
        IndexedReadAheadCounts {
            prime: self.prime.receipt,
            original_reader_position: cursor,
            original_cursor_advance_after_prime: cursor
                .checked_sub(self.prime.receipt.reader_position),
            original_frames_submitted_after_prime: counts.input_frames_submitted
                - self.prime.source_submitted,
            padding_frames_submitted_after_prime: counts.padding_frames_submitted
                - self.prime.padding_submitted,
            absolute_output_generated: counts.output_frames_generated,
            output_frames_generated_after_prime: counts.output_frames_generated
                - self.prime.receipt.output_frame,
            output_frames_published: counts.output_frames_published,
        }
    }
    pub fn generation(&self) -> u64 {
        self.inner.generation()
    }
    pub fn output_origin(&self) -> u64 {
        self.inner.output_origin()
    }
    pub fn output_generated_position(&self) -> u64 {
        self.inner.output_generated_position()
    }
    pub fn input_reader_position(&self) -> u64 {
        self.inner.input_reader_position()
    }
    pub fn pending_input_frames(&self) -> usize {
        self.inner.pending_input_frames()
    }
    pub fn pending_output_frames(&self) -> usize {
        self.inner.pending_output_frames()
    }
    pub fn pump(&mut self) -> Result<ConvertedReadAheadProgress, Error> {
        self.inner.pump()
    }
    /// Recover stopped off-audio owners/counters; no seek/drain/read is hidden.
    pub fn into_parts(self) -> (R, F, AudioProducer, IndexedReadAheadCounts) {
        let counts = self.counts();
        let (reader, factory, producer, _) = self.inner.into_parts();
        (reader, factory, producer, counts)
    }
    /// Build a fresh indexed candidate off audio. Old queue/phase/suffix/cursor
    /// survive every failure; success commits one generation and clears the old
    /// matching stopped queue. No live endpoint mailbox or callback wait exists.
    /// Short positive reads are allowed: <=taps successful window read calls,
    /// plus at most one EOF probe. WAV/AIFF full reads use ceil(window/batch).
    pub fn seek_indexed_stopped(
        &mut self,
        consumer: &mut AudioConsumer,
        output_frame: u64,
    ) -> Result<IndexedSeekReceipt, IndexedSeekFailure<R>> {
        let preflight = (|| {
            if !self.inner.producer.owns(consumer) {
                return Err(Error::InvalidSamples);
            }
            let generation = self
                .inner
                .generation
                .checked_add(1)
                .ok_or(Error::Capacity)?;
            let n = self.inner.state.declared.ok_or(Error::Unsupported)?;
            let total = if n == 0 {
                0
            } else {
                total(n, self.inner.state.input_rate, self.inner.state.options)?
            };
            if output_frame > total {
                return Err(Error::Capacity);
            }
            let plan = if output_frame == total {
                None
            } else {
                Some(
                    self.inner.state.filters[0]
                        .integer_position(output_frame)
                        .map_err(|_| Error::InvalidRecord)?,
                )
            };
            Ok((generation, n, total, plan))
        })()
        .map_err(|error| IndexedSeekFailure {
            candidate: None,
            error,
        })?;
        let (generation, n, target, plan) = preflight;
        let reader = (self.inner.factory)().map_err(|error| IndexedSeekFailure {
            candidate: None,
            error,
        })?;
        if reader.channels() != self.inner.state.channels
            || reader.sample_rate() != self.inner.state.input_rate
            || reader.declared_frames() != self.inner.state.declared
        {
            return Err(IndexedSeekFailure {
                candidate: Some(reader),
                error: Error::InvalidRecord,
            });
        }
        let mut candidate =
            State::new_indexed(reader, self.inner.state.options).map_err(|(reader, error)| {
                IndexedSeekFailure {
                    candidate: Some(reader),
                    error,
                }
            })?;
        let result = (|| {
            if candidate.channels != self.inner.state.channels
                || candidate.input_rate != self.inner.state.input_rate
                || candidate.declared != self.inner.state.declared
            {
                return Err(Error::InvalidRecord);
            }
            let taps = match candidate.options.conversion {
                PlaybackConversion::IntegerSinc { taps } => taps,
                _ => return Err(Error::Unsupported),
            };
            let c = plan.map(|p| p.input_frames_consumed());
            let start = match c {
                Some(c) => u64::try_from(c.saturating_sub(taps as u128).min(u128::from(n)))
                    .map_err(|_| Error::Capacity)?,
                None => n.saturating_sub(taps as u64),
            };
            let end = c.map_or(n, |c| c.min(u128::from(n)) as u64);
            let mut window: Vec<f32> = zeros(taps * candidate.channels)?;
            let mut history: Vec<f64> = zeros(taps)?;
            candidate.reader.seek_original_frame(start)?;
            if candidate.reader.position_frames() != start {
                return Err(Error::InvalidRecord);
            }
            let mut cursor = start;
            while cursor < end {
                let requested = usize::try_from(
                    (end - cursor).min((candidate.source.len() / candidate.channels) as u64),
                )
                .map_err(|_| Error::Capacity)?;
                let buffer = &mut candidate.source[..requested * candidate.channels];
                let frames = candidate.reader.read_frames(buffer)?;
                if frames == 0
                    || frames > requested
                    || candidate.reader.position_frames()
                        != cursor.checked_add(frames as u64).ok_or(Error::Capacity)?
                {
                    return Err(Error::InvalidRecord);
                }
                if buffer[..frames * candidate.channels]
                    .iter()
                    .any(|v| !v.is_finite())
                {
                    return Err(Error::InvalidSamples);
                }
                if let Some(c) = c {
                    let offset = usize::try_from(u128::from(cursor) + taps as u128 - c)
                        .map_err(|_| Error::InvalidRecord)?;
                    window[offset * candidate.channels..(offset + frames) * candidate.channels]
                        .copy_from_slice(&buffer[..frames * candidate.channels]);
                }
                cursor += frames as u64;
            }
            let source_eof = c.is_none_or(|c| c >= u128::from(n));
            if source_eof {
                let frames = candidate
                    .reader
                    .read_frames(&mut candidate.source[..candidate.channels])?;
                if frames != 0 || candidate.reader.position_frames() != n {
                    return Err(Error::InvalidRecord);
                }
            }
            let trailing = c.map_or(0, |c| c.saturating_sub(u128::from(n)) as usize);
            let leading = c.map_or(0, |c| taps.saturating_sub(c.min(taps as u128) as usize));
            if c.is_some() {
                for channel in 0..candidate.channels {
                    for frame in 0..taps {
                        history[frame] = f64::from(window[frame * candidate.channels + channel]);
                    }
                    candidate.filters[channel]
                        .prime_integer_at(output_frame, &history)
                        .map_err(|_| Error::InvalidRecord)?;
                }
            }
            let receipt = IndexedSeekReceipt {
                generation,
                output_frame,
                window_original_start: start,
                window_original_frames: (end - start) as usize,
                filter_input_coordinate: c,
                leading_zero_history_frames: leading,
                trailing_zero_history_frames: trailing,
                reader_position: candidate.reader.position_frames(),
                verified_end_point: c.is_none(),
            };
            candidate.source_eof = source_eof;
            candidate.target = source_eof.then_some(target);
            candidate.counts.input_frames_read = end;
            candidate.counts.input_frames_submitted = end;
            candidate.counts.padding_frames_submitted = trailing;
            candidate.counts.output_frames_generated = output_frame;
            Ok(Prime {
                receipt,
                source_submitted: end,
                padding_submitted: trailing,
            })
        })();
        let prime = match result {
            Ok(prime) => prime,
            Err(error) => {
                return Err(IndexedSeekFailure {
                    candidate: Some(candidate.reader),
                    error,
                });
            }
        };
        self.inner
            .producer
            .clear(consumer)
            .expect("matching stopped endpoints");
        self.inner.state = candidate;
        self.inner.generation = generation;
        self.inner.output_origin = output_frame;
        self.inner.published = 0;
        self.inner.failed = false;
        self.inner.last_progressed = false;
        self.prime = prime;
        Ok(prime.receipt)
    }
}
impl<R: IndexedPlaybackReader + Send + 'static, F: FnMut() -> Result<R, Error> + Send + 'static>
    ReadAheadPump for IndexedConvertedReadAhead<R, F>
{
    fn pump(&mut self) -> Result<ReadAheadProgress, Error> {
        let p = IndexedConvertedReadAhead::pump(self)?;
        Ok(ReadAheadProgress {
            read_frames: p.generated_output_frames,
            queued_frames: p.published_output_frames,
            pending_frames: p.pending_output_frames,
            end_of_file: p.end_of_file,
        })
    }
    fn worker_progressed(&self, _: &ReadAheadProgress) -> bool {
        self.inner.last_progressed
    }
    fn owns(&self, consumer: &AudioConsumer) -> bool {
        self.inner.producer.owns(consumer)
    }
    fn pending_frames(&self) -> usize {
        self.pending_output_frames()
    }
    fn reader_position(&self) -> u64 {
        self.output_generated_position()
    }
    fn sample_rate(&self) -> u32 {
        self.inner.state.options.output_rate
    }
    fn seek_stopped(&mut self, consumer: &mut AudioConsumer, frame: u64) -> Result<(), Error> {
        self.seek_indexed_stopped(consumer, frame)
            .map(|_| ())
            .map_err(|failure| failure.into_parts().1)
    }
}
