//! Explicit bounded output-rate read-ahead; all reader/converter work is off audio.
use crate::{
    AudioConsumer, AudioFileError as Error, AudioProducer, PlaybackConversion, PlaybackReader,
    ReadAheadProgress, ReadAheadPump,
};
use ravet_signal::SincResampler;
const SOURCE_CEILING: usize = 16_777_216;
const WORKING_CEILING: usize = 32 * 1024 * 1024;
/// Explicit worker profile, validated before source reads or factory calls.
/// Copy/floating Sinc duration is capped at 16,777,216 original frames. Explicit
/// IntegerSinc permits caller usize source bounds with checked retained duration
/// at most u64::MAX-65536, leaving largest automatic callback terminal headroom.
/// Reader/container limits remain separate; source/conversion batches are 1..4096.
/// Source/converter/output arrays are capped at 32MiB.
/// Encoded readers and the separately prepared queue retain their own limits.
/// Copy requires equal rates; Sinc retains causal (taps-1)/2 source-sample delay
/// and finite tail, with even 16..256 taps and ratio 1/16..16.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConvertedReadAheadOptions {
    pub output_rate: u32,
    pub conversion: PlaybackConversion,
    pub maximum_source_frames: usize,
    pub maximum_output_frames: u64,
    pub source_batch_frames: usize,
    pub conversion_batch_frames: usize,
}
/// Original file and converted queue coordinates are deliberately distinct.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConvertedReadAheadCounts {
    /// Latest original reader cursor, including progress committed before errors.
    pub input_frames_read: u64,
    /// Original source frames irreversibly consumed by copy/conversion.
    pub input_frames_submitted: u64,
    /// Explicit zero padding, excluded from original source frame counts.
    pub padding_frames_submitted: usize,
    /// Absolute target-rate output generation, including replay-discarded prefix.
    pub output_frames_generated: u64,
    /// Publications in this generation only; replay discard is not publication.
    pub output_frames_published: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConvertedReadAheadProgress {
    pub read_input_frames: usize,
    pub submitted_input_frames: usize,
    pub submitted_padding_frames: usize,
    pub generated_output_frames: usize,
    pub published_output_frames: usize,
    pub pending_input_frames: usize,
    pub pending_output_frames: usize,
    /// Verified source EOF, finite tail generated and retained suffix published.
    /// Previously published queue frames may remain for the callback to consume.
    pub end_of_file: bool,
}
/// Failure-owned preparation: reader, fresh-reader factory and producer survive.
/// Preparation performs no reader read/seek or factory call.
pub struct ConvertedReadAheadFailure<R, F> {
    reader: R,
    factory: F,
    producer: AudioProducer,
    error: Error,
}
impl<R, F> ConvertedReadAheadFailure<R, F> {
    pub fn error(&self) -> &Error {
        &self.error
    }
    pub fn reader(&self) -> &R {
        &self.reader
    }
    pub fn into_parts(self) -> (R, F, AudioProducer, Error) {
        (self.reader, self.factory, self.producer, self.error)
    }
}
impl<R, F> std::fmt::Debug for ConvertedReadAheadFailure<R, F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConvertedReadAheadFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl<R, F> std::fmt::Display for ConvertedReadAheadFailure<R, F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "converted read-ahead preparation: {}", self.error)
    }
}
impl<R, F> std::error::Error for ConvertedReadAheadFailure<R, F> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
fn zeros<T: Default + Clone>(n: usize) -> Result<Vec<T>, Error> {
    let mut v = Vec::new();
    v.try_reserve_exact(n).map_err(|_| Error::Capacity)?;
    v.resize(n, T::default());
    Ok(v)
}
fn total(n: u64, input: u32, options: ConvertedReadAheadOptions) -> Result<u64, Error> {
    if n == 0 {
        return Err(Error::InvalidSamples);
    }
    match options.conversion {
        PlaybackConversion::Copy => Ok(n),
        PlaybackConversion::Sinc { taps } | PlaybackConversion::IntegerSinc { taps } => {
            u64::try_from(
                (u128::from(n) + (taps - 2) as u128) * u128::from(options.output_rate)
                    / u128::from(input)
                    + 1,
            )
            .map_err(|_| Error::Capacity)
        }
    }
}
struct State<R> {
    reader: R,
    options: ConvertedReadAheadOptions,
    channels: usize,
    input_rate: u32,
    declared: Option<u64>,
    maximum_output: u64,
    filters: Vec<SincResampler>,
    source: Vec<f32>,
    input: Vec<Vec<f64>>,
    output: Vec<Vec<f64>>,
    converted: Vec<f32>,
    input_start: usize,
    input_end: usize,
    input_padding: bool,
    output_start: usize,
    output_end: usize,
    source_eof: bool,
    target: Option<u64>,
    counts: ConvertedReadAheadCounts,
}
impl<R: PlaybackReader> State<R> {
    fn new(reader: R, options: ConvertedReadAheadOptions) -> Result<Self, (R, Error)> {
        Self::new_profile(reader, options, false)
    }
    fn new_indexed(reader: R, options: ConvertedReadAheadOptions) -> Result<Self, (R, Error)> {
        Self::new_profile(reader, options, true)
    }
    fn new_profile(
        reader: R,
        options: ConvertedReadAheadOptions,
        allow_empty: bool,
    ) -> Result<Self, (R, Error)> {
        let channels = reader.channels();
        let input_rate = reader.sample_rate();
        let declared = reader.declared_frames();
        let prepared = (|| {
            if !(1..=64).contains(&channels)
                || !(1..=768000).contains(&input_rate)
                || !(1..=768000).contains(&options.output_rate)
                || reader.position_frames() != 0
            {
                return Err(Error::InvalidSamples);
            }
            if !(1..=4096).contains(&options.source_batch_frames)
                || !(1..=4096).contains(&options.conversion_batch_frames)
                || !(1..=65536).contains(&reader.maximum_read_frames())
                || options.maximum_source_frames == 0
                || (!matches!(options.conversion, PlaybackConversion::IntegerSinc { .. })
                    && options.maximum_source_frames > SOURCE_CEILING)
            {
                return Err(Error::Capacity);
            }
            let taps = match options.conversion {
                PlaybackConversion::Copy => {
                    if input_rate != options.output_rate {
                        return Err(Error::Unsupported);
                    }
                    0
                }
                PlaybackConversion::Sinc { taps } | PlaybackConversion::IntegerSinc { taps } => {
                    if !(16..=256).contains(&taps)
                        || !taps.is_multiple_of(2)
                        || !(1. / 16. ..=16.)
                            .contains(&(f64::from(input_rate) / f64::from(options.output_rate)))
                    {
                        return Err(Error::Unsupported);
                    }
                    taps
                }
            };
            let bound = declared.unwrap_or(options.maximum_source_frames as u64);
            if bound > options.maximum_source_frames as u64 {
                return Err(Error::Capacity);
            }
            let maximum = if allow_empty && bound == 0 {
                0
            } else {
                total(bound, input_rate, options)?
            };
            if maximum > options.maximum_output_frames
                || (matches!(options.conversion, PlaybackConversion::IntegerSinc { .. })
                    && maximum > u64::MAX - 65_536)
            {
                return Err(Error::Capacity);
            }
            let batch = options
                .source_batch_frames
                .min(reader.maximum_read_frames());
            let working = channels
                * (batch * 4
                    + options.conversion_batch_frames * 4
                    + if taps == 0 {
                        0
                    } else {
                        taps * 1025 * 8 + taps * 8 + (batch + options.conversion_batch_frames) * 8
                    });
            if working > WORKING_CEILING {
                return Err(Error::Capacity);
            }
            let mut filters = Vec::new();
            let mut input = Vec::new();
            let mut output = Vec::new();
            if taps != 0 {
                filters
                    .try_reserve_exact(channels)
                    .map_err(|_| Error::Capacity)?;
                input
                    .try_reserve_exact(channels)
                    .map_err(|_| Error::Capacity)?;
                output
                    .try_reserve_exact(channels)
                    .map_err(|_| Error::Capacity)?;
                for _ in 0..channels {
                    filters.push(
                        match options.conversion {
                            PlaybackConversion::IntegerSinc { .. } => {
                                SincResampler::new_integer_rates(
                                    input_rate,
                                    options.output_rate,
                                    taps,
                                )
                            }
                            _ => SincResampler::new(
                                f64::from(input_rate),
                                f64::from(options.output_rate),
                                taps,
                            ),
                        }
                        .map_err(|_| Error::InvalidRecord)?,
                    );
                    input.push(zeros(batch)?);
                    output.push(zeros(options.conversion_batch_frames)?);
                }
            }
            Ok((
                maximum,
                filters,
                input,
                output,
                zeros(batch * channels)?,
                zeros(options.conversion_batch_frames * channels)?,
            ))
        })();
        match prepared {
            Ok((maximum_output, filters, input, output, source, converted)) => Ok(Self {
                reader,
                options,
                channels,
                input_rate,
                declared,
                maximum_output,
                filters,
                input,
                output,
                source,
                converted,
                input_start: 0,
                input_end: 0,
                input_padding: false,
                output_start: 0,
                output_end: 0,
                source_eof: false,
                target: None,
                counts: ConvertedReadAheadCounts::default(),
            }),
            Err(error) => Err((reader, error)),
        }
    }
    fn pending_output(&self) -> usize {
        self.output_end - self.output_start
    }
    fn complete(&self) -> bool {
        self.source_eof
            && self.target == Some(self.counts.output_frames_generated)
            && self.pending_output() == 0
    }
    fn generated(&self) -> u64 {
        self.counts.output_frames_generated
    }
    /// One read at most, followed by one bounded conversion. Never overwrites suffix.
    fn generate(&mut self) -> Result<(), Error> {
        if self.pending_output() != 0 || self.complete() {
            return Ok(());
        }
        if self.input_start == self.input_end {
            self.input_start = 0;
            self.input_end = 0;
            if !self.source_eof {
                let before = self.reader.position_frames();
                let read = self.reader.read_frames(&mut self.source);
                self.counts.input_frames_read = self.reader.position_frames();
                let frames = read?;
                if frames > self.source.len() / self.channels
                    || self.counts.input_frames_read
                        != before.checked_add(frames as u64).ok_or(Error::Capacity)?
                {
                    return Err(Error::InvalidRecord);
                }
                if self.counts.input_frames_read > self.options.maximum_source_frames as u64
                    || self
                        .declared
                        .is_some_and(|n| self.counts.input_frames_read > n)
                {
                    return Err(Error::Capacity);
                }
                if frames == 0 {
                    if self
                        .declared
                        .is_some_and(|n| n != self.counts.input_frames_read)
                    {
                        return Err(Error::InvalidRecord);
                    }
                    self.target = Some(
                        if self.counts.input_frames_read == 0 && self.maximum_output == 0 {
                            // Only explicit indexed known-empty preparation has a zero bound.
                            0
                        } else {
                            total(self.counts.input_frames_read, self.input_rate, self.options)?
                        },
                    );
                    self.source_eof = true;
                } else {
                    if self.source[..frames * self.channels]
                        .iter()
                        .any(|v| !v.is_finite())
                    {
                        return Err(Error::InvalidSamples);
                    }
                    self.input_end = frames;
                    self.input_padding = false;
                    for (channel, plane) in self.input.iter_mut().enumerate() {
                        for (frame, value) in plane.iter_mut().take(frames).enumerate() {
                            *value = f64::from(self.source[frame * self.channels + channel]);
                        }
                    }
                }
            }
            if self.source_eof {
                if self.target == Some(self.generated()) {
                    return Ok(());
                }
                let taps = match self.options.conversion {
                    PlaybackConversion::Copy => return Err(Error::InvalidRecord),
                    PlaybackConversion::Sinc { taps }
                    | PlaybackConversion::IntegerSinc { taps } => taps,
                };
                let remaining = taps
                    .checked_sub(self.counts.padding_frames_submitted)
                    .ok_or(Error::InvalidRecord)?;
                self.input_end = remaining.min(self.source.len() / self.channels);
                if self.input_end == 0 {
                    return Err(Error::InvalidRecord);
                }
                self.input_padding = true;
                for plane in &mut self.input {
                    plane[..self.input_end].fill(0.);
                }
            }
        }
        let remaining = self
            .target
            .unwrap_or(self.maximum_output)
            .checked_sub(self.generated())
            .ok_or(Error::Capacity)?;
        let capacity = usize::try_from(remaining.min(self.options.conversion_batch_frames as u64))
            .map_err(|_| Error::Capacity)?;
        if capacity == 0 {
            return Err(Error::InvalidRecord);
        }
        let (consumed, produced) = if self.filters.is_empty() {
            let n = capacity.min(self.input_end - self.input_start);
            self.converted[..n * self.channels].copy_from_slice(
                &self.source
                    [self.input_start * self.channels..(self.input_start + n) * self.channels],
            );
            (n, n)
        } else {
            let mut expected = None;
            for channel in 0..self.channels {
                let p = self.filters[channel].process(
                    &self.input[channel][self.input_start..self.input_end],
                    &mut self.output[channel][..capacity],
                );
                if expected.is_some_and(|previous| previous != p) {
                    return Err(Error::InvalidRecord);
                }
                expected = Some(p);
            }
            let p = expected.expect("nonempty channels");
            if self
                .output
                .iter()
                .any(|plane| plane[..p.produced].iter().any(|v| !(*v as f32).is_finite()))
            {
                self.input_start += p.consumed;
                if self.input_padding {
                    self.counts.padding_frames_submitted += p.consumed
                } else {
                    self.counts.input_frames_submitted += p.consumed as u64
                }
                return Err(Error::InvalidSamples);
            }
            for frame in 0..p.produced {
                for channel in 0..self.channels {
                    self.converted[frame * self.channels + channel] =
                        self.output[channel][frame] as f32;
                }
            }
            (p.consumed, p.produced)
        };
        self.input_start += consumed;
        if self.input_padding {
            self.counts.padding_frames_submitted += consumed
        } else {
            self.counts.input_frames_submitted += consumed as u64
        }
        self.counts.output_frames_generated = self
            .generated()
            .checked_add(produced as u64)
            .ok_or(Error::Capacity)?;
        self.output_start = 0;
        self.output_end = produced;
        if consumed == 0 && produced == 0 {
            return Err(Error::InvalidRecord);
        }
        Ok(())
    }
}
/// Explicit converted output queue plus a caller-selected fresh-reader factory.
/// Original input coordinates are exposed separately. Exact stopped seek replays
/// a fresh candidate in output coordinates and adopts only after success. No
/// callback waits, whole PCM clip, device clock or cross-thread mailbox is added.
/// Source/backend version stability is the factory caller's responsibility; fixed
/// metadata does not authenticate content. Retained Sinc output count is
/// floor((N+taps-2)*output_rate/input_rate)+1, including its fractional delay.
///
/// ```
/// use ravet_audio_file::{audio_ring, ConvertedReadAhead, ConvertedReadAheadOptions,
///     PlaybackConversion, WaveEncoding, WaveFormat, WaveReader, WaveWriter};
/// use std::io::Cursor;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut writer = WaveWriter::new(Cursor::new(Vec::new()),
///     WaveFormat { channels: 1, sample_rate: 48000, encoding: WaveEncoding::Float32 }, 17)?;
/// writer.write_frames(&[0.25; 17])?;
/// let bytes = writer.finish()?.into_inner();
/// let reader = WaveReader::new(Cursor::new(bytes.clone()), 17)?;
/// let (producer, mut consumer) = audio_ring(1, 32).unwrap();
/// let mut source = ConvertedReadAhead::new(reader,
///     move || WaveReader::new(Cursor::new(bytes.clone()), 17), producer, &consumer,
///     ConvertedReadAheadOptions { output_rate: 48000, conversion: PlaybackConversion::Copy,
///         maximum_source_frames: 17, maximum_output_frames: 17,
///         source_batch_frames: 17, conversion_batch_frames: 17 })?;
/// assert_eq!(source.pump()?.published_output_frames, 17);
/// // Exclusive stopped endpoint: failed candidates preserve this queued prefix.
/// source.seek_stopped(&mut consumer, 1)?;
/// assert_eq!(source.output_origin(), 1);
/// # Ok(()) }
/// ```
pub struct ConvertedReadAhead<R, F> {
    state: State<R>,
    factory: F,
    producer: AudioProducer,
    generation: u64,
    output_origin: u64,
    published: u64,
    failed: bool,
    last_progressed: bool,
}
impl<R: PlaybackReader, F: FnMut() -> Result<R, Error>> ConvertedReadAhead<R, F> {
    /// Validate a frame-zero reader and matching empty stopped queue, then allocate
    /// bounded worker state. Every failure returns original owners; no source reads.
    pub fn new(
        reader: R,
        factory: F,
        producer: AudioProducer,
        consumer: &AudioConsumer,
        options: ConvertedReadAheadOptions,
    ) -> Result<Self, ConvertedReadAheadFailure<R, F>> {
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
        match State::new(reader, options) {
            Ok(state) => Ok(Self {
                state,
                factory,
                producer,
                generation: 0,
                output_origin: 0,
                published: 0,
                failed: false,
                last_progressed: false,
            }),
            Err((reader, error)) => Err(ConvertedReadAheadFailure {
                reader,
                factory,
                producer,
                error,
            }),
        }
    }
    pub fn counts(&self) -> ConvertedReadAheadCounts {
        let mut counts = self.state.counts;
        counts.output_frames_published = self.published;
        counts
    }
    pub fn input_reader_position(&self) -> u64 {
        self.state.reader.position_frames()
    }
    pub fn output_generated_position(&self) -> u64 {
        self.state.generated()
    }
    pub fn output_origin(&self) -> u64 {
        self.output_origin
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn pending_output_frames(&self) -> usize {
        self.state.pending_output()
    }
    pub fn pending_input_frames(&self) -> usize {
        if self.state.input_padding {
            0
        } else {
            self.state.input_end - self.state.input_start
        }
    }
    /// Return off-audio owners and counts; the first direct pump error belongs to
    /// its caller, or to ReadAheadWorker's exit. No hidden I/O/drain/seek occurs.
    pub fn into_parts(self) -> (R, F, AudioProducer, ConvertedReadAheadCounts) {
        let counts = self.counts();
        (self.state.reader, self.factory, self.producer, counts)
    }
    /// At most one read, one conversion step/channel and one publication, off audio.
    /// Full queues retain exact input/output state. First operational error belongs
    /// to this Result (or worker exit); later calls return Poisoned without reads.
    pub fn pump(&mut self) -> Result<ConvertedReadAheadProgress, Error> {
        if self.failed {
            return Err(Error::Poisoned);
        }
        let before = self.counts();
        if self.producer.available_frames() != 0
            && let Err(error) = self.state.generate()
        {
            self.failed = true;
            self.last_progressed = false;
            return Err(error);
        }
        let frames = self
            .producer
            .push(
                &self.state.converted[self.state.output_start * self.state.channels
                    ..self.state.output_end * self.state.channels],
            )
            .expect("prepared whole frames");
        self.state.output_start += frames;
        self.published = self
            .published
            .checked_add(frames as u64)
            .ok_or(Error::Capacity)?;
        let after = self.counts();
        let progress = ConvertedReadAheadProgress {
            read_input_frames: usize::try_from(after.input_frames_read - before.input_frames_read)
                .map_err(|_| Error::Capacity)?,
            submitted_input_frames: (after.input_frames_submitted - before.input_frames_submitted)
                as usize,
            submitted_padding_frames: after.padding_frames_submitted
                - before.padding_frames_submitted,
            generated_output_frames: (after.output_frames_generated
                - before.output_frames_generated) as usize,
            published_output_frames: frames,
            pending_input_frames: self.pending_input_frames(),
            pending_output_frames: self.pending_output_frames(),
            end_of_file: self.state.complete(),
        };
        self.last_progressed = progress.read_input_frames != 0
            || progress.submitted_input_frames != 0
            || progress.submitted_padding_frames != 0
            || progress.generated_output_frames != 0
            || frames != 0;
        Ok(progress)
    }
    /// Replay a fresh candidate to an absolute target-rate output frame off audio.
    /// Requires exclusive stopped matching endpoints. Failure preserves old queue,
    /// phase, pending suffixes and generation; success adopts retained candidate
    /// suffix and clears the old queue. EOF candidates must verify source integrity.
    /// Replay is O(target), with bounded memory and no cancellation/deadline promise.
    /// Factory side effects cannot be rolled back; two bounded states coexist.
    pub fn seek_stopped(
        &mut self,
        consumer: &mut AudioConsumer,
        output_frame: u64,
    ) -> Result<(), Error> {
        if !self.producer.owns(consumer) {
            return Err(Error::InvalidSamples);
        }
        let generation = self.generation.checked_add(1).ok_or(Error::Capacity)?;
        let upper = self.state.target.unwrap_or(self.state.maximum_output);
        if output_frame > upper {
            return Err(Error::Capacity);
        }
        let reader = (self.factory)()?;
        let mut candidate = State::new(reader, self.state.options).map_err(|(_, error)| error)?;
        if candidate.channels != self.state.channels
            || candidate.input_rate != self.state.input_rate
            || candidate.declared != self.state.declared
        {
            return Err(Error::InvalidRecord);
        }
        let mut discarded = 0u64;
        loop {
            let known_eof = self.state.target.or(self
                .state
                .declared
                .map(|n| total(n, self.state.input_rate, self.state.options))
                .transpose()?);
            if self.state.source_eof
                && candidate.source_eof
                && candidate.counts.input_frames_read != self.state.counts.input_frames_read
            {
                return Err(Error::InvalidRecord);
            }
            if discarded == output_frame {
                if known_eof == Some(output_frame) && candidate.pending_output() != 0 {
                    return Err(Error::InvalidRecord);
                }
                let boundary = candidate.pending_output() == 0
                    && candidate.input_start == candidate.input_end
                    && !candidate.source_eof;
                if !(known_eof == Some(output_frame) && !candidate.complete()) && !boundary {
                    break;
                }
            }
            if candidate.pending_output() == 0 {
                candidate.generate()?;
            }
            let remaining = output_frame
                .checked_sub(discarded)
                .ok_or(Error::InvalidRecord)?;
            let count = usize::try_from(remaining.min(candidate.pending_output() as u64))
                .map_err(|_| Error::Capacity)?;
            candidate.output_start += count;
            discarded += count as u64;
            if candidate.complete() {
                if self.state.source_eof
                    && candidate.counts.input_frames_read != self.state.counts.input_frames_read
                {
                    return Err(Error::InvalidRecord);
                }
                if discarded != output_frame {
                    return Err(Error::Capacity);
                }
                break;
            }
        }
        self.producer
            .clear(consumer)
            .expect("matching stopped endpoints");
        self.state = candidate;
        self.generation = generation;
        self.output_origin = output_frame;
        self.published = 0;
        self.failed = false;
        self.last_progressed = false;
        Ok(())
    }
}
impl<R: PlaybackReader + Send + 'static, F: FnMut() -> Result<R, Error> + Send + 'static>
    ReadAheadPump for ConvertedReadAhead<R, F>
{
    fn pump(&mut self) -> Result<ReadAheadProgress, Error> {
        let p = ConvertedReadAhead::pump(self)?;
        Ok(ReadAheadProgress {
            read_frames: p.generated_output_frames,
            queued_frames: p.published_output_frames,
            pending_frames: p.pending_output_frames,
            end_of_file: p.end_of_file,
        })
    }
    fn worker_progressed(&self, _: &ReadAheadProgress) -> bool {
        self.last_progressed
    }
    fn owns(&self, consumer: &AudioConsumer) -> bool {
        self.producer.owns(consumer)
    }
    fn pending_frames(&self) -> usize {
        self.pending_output_frames()
    }
    fn reader_position(&self) -> u64 {
        self.output_generated_position()
    }
    fn sample_rate(&self) -> u32 {
        self.state.options.output_rate
    }
    fn seek_stopped(&mut self, consumer: &mut AudioConsumer, frame: u64) -> Result<(), Error> {
        ConvertedReadAhead::seek_stopped(self, consumer, frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_ring;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct Reader;
    impl PlaybackReader for Reader {
        fn channels(&self) -> usize {
            1
        }
        fn sample_rate(&self) -> u32 {
            48000
        }
        fn position_frames(&self) -> u64 {
            0
        }
        fn declared_frames(&self) -> Option<u64> {
            Some(17)
        }
        fn maximum_read_frames(&self) -> usize {
            17
        }
        fn read_frames(&mut self, _: &mut [f32]) -> Result<usize, Error> {
            panic!("preflight must not read")
        }
    }
    #[test]
    fn foreign_endpoint_and_generation_overflow_reject_before_factory_and_preserve_queues() {
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&calls);
        let (producer, mut consumer) = audio_ring(1, 4).unwrap();
        let mut pump = ConvertedReadAhead::new(
            Reader,
            move || {
                observed.fetch_add(1, Ordering::Relaxed);
                Ok(Reader)
            },
            producer,
            &consumer,
            ConvertedReadAheadOptions {
                output_rate: 48000,
                conversion: PlaybackConversion::Copy,
                maximum_source_frames: 17,
                maximum_output_frames: 17,
                source_batch_frames: 17,
                conversion_batch_frames: 17,
            },
        )
        .unwrap();
        let (mut foreign_producer, mut foreign_consumer) = audio_ring(1, 4).unwrap();
        foreign_producer.push(&[0.5]).unwrap();
        pump.producer.push(&[0.25]).unwrap();
        let before = pump.counts();
        assert!(matches!(
            pump.seek_stopped(&mut foreign_consumer, 0),
            Err(Error::InvalidSamples)
        ));
        assert_eq!(pump.generation(), 0);
        pump.generation = u64::MAX;
        assert!(matches!(
            pump.seek_stopped(&mut consumer, 0),
            Err(Error::Capacity)
        ));
        assert_eq!(pump.generation(), u64::MAX);
        assert_eq!(pump.counts(), before);
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        let mut out = [0.];
        consumer.pop(&mut out).unwrap();
        assert_eq!(out, [0.25]);
        foreign_consumer.pop(&mut out).unwrap();
        assert_eq!(out, [0.5]);
    }
}

#[path = "indexed.rs"]
mod indexed;
pub use indexed::{
    IndexedConvertedReadAhead, IndexedReadAheadCounts, IndexedSeekFailure, IndexedSeekReceipt,
};
