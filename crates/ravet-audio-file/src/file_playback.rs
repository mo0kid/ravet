//! Explicit bounded file preparation; file I/O and conversion stay off audio.
#[cfg(feature = "compressed")]
use crate::CompressedReader;
use crate::{AiffReader, AudioFileError as Error, MemoryPlayback, WaveReader};
use ravet_signal::SincResampler;
use std::io::{Read, Seek};
const MAX_SAMPLES: usize = 16_777_216;
const MAX_WORKING_BYTES: usize = 32 * 1024 * 1024;
const OUTPUT_BATCH: usize = 4096;
/// A stable metadata/committed-frame interface for control/worker file preparation.
/// Metadata getters must not read/seek. Successful reads advance position by
/// exactly the returned complete frames; errors expose their actual committed
/// frame position, which can have advanced. No rollback is implied. A zero read
/// means validated EOF; compressed implementations must retain gapless trimming.
/// Implementations own their original I/O/codec errors and source bounds.
pub trait PlaybackReader {
    fn channels(&self) -> usize;
    fn sample_rate(&self) -> u32;
    fn position_frames(&self) -> u64;
    fn declared_frames(&self) -> Option<u64>;
    fn maximum_read_frames(&self) -> usize;
    fn read_frames(&mut self, output: &mut [f32]) -> Result<usize, Error>;
}
/// Explicit indexed original-frame positioning for known-duration PCM readers.
/// Implementations report stable known duration and exact original coordinates.
/// Seek/read work belongs off audio. Successful seek sets exactly the requested
/// logical frame; failure exposes its actual committed position and original error.
/// This does not promise a filesystem deadline or authenticate skipped regions.
/// WAV/AIFF defer physical seeking to their next read. No compressed impl exists.
pub trait IndexedPlaybackReader: PlaybackReader {
    fn seek_original_frame(&mut self, frame: u64) -> Result<(), Error>;
}
impl<R: Read + Seek> IndexedPlaybackReader for WaveReader<R> {
    fn seek_original_frame(&mut self, frame: u64) -> Result<(), Error> {
        self.seek_frame(frame)
    }
}
impl<R: Read + Seek> IndexedPlaybackReader for AiffReader<R> {
    fn seek_original_frame(&mut self, frame: u64) -> Result<(), Error> {
        self.seek_frame(frame)
    }
}
impl<R: Read + Seek> PlaybackReader for WaveReader<R> {
    fn channels(&self) -> usize {
        usize::from(self.format().channels)
    }
    fn sample_rate(&self) -> u32 {
        self.format().sample_rate
    }
    fn position_frames(&self) -> u64 {
        self.position()
    }
    fn declared_frames(&self) -> Option<u64> {
        Some(self.frames())
    }
    fn maximum_read_frames(&self) -> usize {
        self.maximum_frames()
    }
    fn read_frames(&mut self, output: &mut [f32]) -> Result<usize, Error> {
        WaveReader::read_frames(self, output)
    }
}
impl<R: Read + Seek> PlaybackReader for AiffReader<R> {
    fn channels(&self) -> usize {
        usize::from(self.format().channels)
    }
    fn sample_rate(&self) -> u32 {
        self.format().sample_rate
    }
    fn position_frames(&self) -> u64 {
        self.position()
    }
    fn declared_frames(&self) -> Option<u64> {
        Some(self.frames())
    }
    fn maximum_read_frames(&self) -> usize {
        self.maximum_frames()
    }
    fn read_frames(&mut self, output: &mut [f32]) -> Result<usize, Error> {
        AiffReader::read_frames(self, output)
    }
}
#[cfg(feature = "compressed")]
impl PlaybackReader for CompressedReader {
    fn channels(&self) -> usize {
        usize::from(CompressedReader::channels(self))
    }
    fn sample_rate(&self) -> u32 {
        CompressedReader::sample_rate(self)
    }
    fn position_frames(&self) -> u64 {
        CompressedReader::position_frames(self) as u64
    }
    fn declared_frames(&self) -> Option<u64> {
        CompressedReader::declared_frames(self)
    }
    fn maximum_read_frames(&self) -> usize {
        65536
    }
    fn read_frames(&mut self, output: &mut [f32]) -> Result<usize, Error> {
        CompressedReader::read_frames(self, output)
    }
}
/// Explicit same-rate copy or causal sinc filtering; no implicit bypass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackConversion {
    /// Exact sample copy; unequal source/target rates reject before reads.
    Copy,
    /// Filter even when rates match; retain fractional delay and finite tail.
    Sinc { taps: usize },
    /// Exact integer-rate schedule; filters at equal rates and retains the same
    /// fractional delay/tail profile. PCM can differ from the floating Sinc mode.
    /// This additive variant requires downstream exhaustive matches to handle it.
    IntegerSinc { taps: usize },
}
/// Caller-selected budgets and target. No defaults select hidden conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FilePlaybackOptions {
    pub output_rate: u32,
    pub conversion: PlaybackConversion,
    /// Complete source bounds; at most 16,777,216 interleaved samples.
    pub maximum_source_frames: usize,
    /// Prepared clip bound including retained filter tail, at most 64MiB.
    pub maximum_output_frames: usize,
    pub source_batch_frames: usize,
    pub maximum_render_frames: usize,
}
/// Counts committed before success/failure. Reads can advance before an error;
/// submissions and prepared output can lag those committed reader frames.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FilePlaybackProgress {
    /// Latest committed logical source cursor observed after an attempted read.
    /// Preflight rejection reports zero because this preparation performed no read;
    /// the recovered reader can itself already have a nonzero cursor.
    pub source_frames_read: u64,
    pub source_frames_submitted: u64,
    pub padding_frames_submitted: usize,
    pub prepared_frames: usize,
}
/// Immutable complete preparation metadata. Player cursors/loops use output
/// frames. Nominal duration and retained filter delay/tail are distinct.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FilePlaybackInfo {
    pub source_rate: u32,
    pub output_rate: u32,
    pub channels: usize,
    pub source_frames: u64,
    pub nominal_output_frames: usize,
    pub prepared_frames: usize,
    pub tail_output_frames: usize,
    pub conversion: PlaybackConversion,
}
impl FilePlaybackInfo {
    pub fn filter_delay_input_samples(&self) -> f64 {
        match self.conversion {
            PlaybackConversion::Copy => 0.,
            PlaybackConversion::Sinc { taps } | PlaybackConversion::IntegerSinc { taps } => {
                (taps - 1) as f64 / 2.
            }
        }
    }
    pub fn filter_delay_output_samples(&self) -> f64 {
        self.filter_delay_input_samples() * f64::from(self.output_rate)
            / f64::from(self.source_rate)
    }
}
/// Owned complete output-rate clip plus explicit source/delay/tail metadata.
/// No reader/converter remains in the callback player. Existing pause/seek/loop
/// operates in prepared output frames; boundaries can click. No live buffered
/// conversion, drift correction or physical-device validation is implied.
pub struct PreparedFilePlayback {
    playback: MemoryPlayback,
    info: FilePlaybackInfo,
    progress: FilePlaybackProgress,
}
impl PreparedFilePlayback {
    pub fn playback(&self) -> &MemoryPlayback {
        &self.playback
    }
    pub fn playback_mut(&mut self) -> &mut MemoryPlayback {
        &mut self.playback
    }
    pub fn info(&self) -> FilePlaybackInfo {
        self.info
    }
    pub fn progress(&self) -> FilePlaybackProgress {
        self.progress
    }
    pub fn into_playback(self) -> MemoryPlayback {
        self.playback
    }
}
/// Original reader/error with exact committed progress and incomplete output.
/// Reader logical/underlying byte position may have advanced; no seek rollback
/// or complete playable clip is promised by this failure-owned prefix.
pub struct FilePlaybackFailure<R> {
    reader: R,
    error: Error,
    progress: FilePlaybackProgress,
    partial_samples: Vec<f32>,
}
impl<R> FilePlaybackFailure<R> {
    pub fn error(&self) -> &Error {
        &self.error
    }
    pub fn reader(&self) -> &R {
        &self.reader
    }
    pub fn progress(&self) -> FilePlaybackProgress {
        self.progress
    }
    pub fn partial_samples(&self) -> &[f32] {
        &self.partial_samples
    }
    pub fn into_parts(self) -> (R, Error, FilePlaybackProgress, Vec<f32>) {
        (self.reader, self.error, self.progress, self.partial_samples)
    }
}
impl<R> std::fmt::Debug for FilePlaybackFailure<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilePlaybackFailure")
            .field("error", &self.error)
            .field("progress", &self.progress)
            .finish_non_exhaustive()
    }
}
impl<R> std::fmt::Display for FilePlaybackFailure<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "file playback preparation failed: {}", self.error)
    }
}
impl<R> std::error::Error for FilePlaybackFailure<R> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
fn nominal_frames(frames: u64, input: u32, output: u32) -> Result<usize, Error> {
    let n = u128::from(frames) * u128::from(output);
    usize::try_from(n.div_ceil(u128::from(input))).map_err(|_| Error::Capacity)
}
fn prepared_frames(
    frames: u64,
    input: u32,
    output: u32,
    conversion: PlaybackConversion,
) -> Result<usize, Error> {
    if frames == 0 {
        return Err(Error::InvalidSamples);
    }
    match conversion {
        PlaybackConversion::Copy => usize::try_from(frames).map_err(|_| Error::Capacity),
        PlaybackConversion::Sinc { taps } | PlaybackConversion::IntegerSinc { taps } => {
            let support = u128::from(frames) + (taps - 2) as u128;
            usize::try_from(support * u128::from(output) / u128::from(input) + 1)
                .map_err(|_| Error::Capacity)
        }
    }
}
fn zeroed<T: Default + Clone>(count: usize) -> Result<Vec<T>, Error> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(count)
        .map_err(|_| Error::Capacity)?;
    result.resize(count, T::default());
    Ok(result)
}
struct Converter {
    filters: Vec<SincResampler>,
    input: Vec<Vec<f64>>,
    output: Vec<Vec<f64>>,
}
impl Converter {
    fn new(
        channels: usize,
        input_rate: u32,
        options: FilePlaybackOptions,
        batch: usize,
    ) -> Result<Self, Error> {
        let mut filters = Vec::new();
        let mut input = Vec::new();
        let mut output = Vec::new();
        if let PlaybackConversion::Sinc { taps } | PlaybackConversion::IntegerSinc { taps } =
            options.conversion
        {
            for _ in 0..channels {
                filters.push(
                    match options.conversion {
                        PlaybackConversion::IntegerSinc { .. } => {
                            SincResampler::new_integer_rates(input_rate, options.output_rate, taps)
                        }
                        _ => SincResampler::new(
                            f64::from(input_rate),
                            f64::from(options.output_rate),
                            taps,
                        ),
                    }
                    .map_err(|_| Error::InvalidRecord)?,
                );
                input.push(zeroed(batch)?);
                output.push(zeroed(OUTPUT_BATCH.min(options.maximum_output_frames))?);
            }
        }
        Ok(Self {
            filters,
            input,
            output,
        })
    }
    fn submit(
        &mut self,
        samples: &[f32],
        channels: usize,
        padding: bool,
        limit: usize,
        destination: &mut Vec<f32>,
        progress: &mut FilePlaybackProgress,
    ) -> Result<(), Error> {
        if samples.iter().any(|sample| !sample.is_finite()) {
            return Err(Error::InvalidSamples);
        }
        let frames = samples.len() / channels;
        if self.filters.is_empty() {
            if destination.len() / channels + frames > limit {
                return Err(Error::Capacity);
            }
            destination.extend_from_slice(samples);
            progress.source_frames_submitted += frames as u64;
            progress.prepared_frames = destination.len() / channels;
            return Ok(());
        }
        for (channel, input) in self.input.iter_mut().enumerate() {
            for frame in 0..frames {
                input[frame] = f64::from(samples[frame * channels + channel]);
            }
        }
        let mut offset = 0;
        loop {
            let available = limit - progress.prepared_frames;
            if available == 0 {
                return if offset == frames || padding {
                    Ok(())
                } else {
                    Err(Error::Capacity)
                };
            }
            let count = available.min(self.output[0].len());
            let mut expected = None;
            for channel in 0..channels {
                let p = self.filters[channel].process(
                    &self.input[channel][offset..frames],
                    &mut self.output[channel][..count],
                );
                if expected.is_some_and(|previous| previous != p) {
                    return Err(Error::InvalidRecord);
                }
                expected = Some(p);
            }
            let p = expected.unwrap();
            offset += p.consumed;
            if padding {
                progress.padding_frames_submitted += p.consumed
            } else {
                progress.source_frames_submitted += p.consumed as u64
            }
            if self.output.iter().any(|output| {
                output[..p.produced]
                    .iter()
                    .any(|sample| !(*sample as f32).is_finite())
            }) {
                return Err(Error::InvalidSamples);
            }
            for frame in 0..p.produced {
                for output in &self.output {
                    destination.push(output[frame] as f32)
                }
            }
            progress.prepared_frames = destination.len() / channels;
            if p.consumed == 0 && p.produced == 0 {
                return if offset == frames {
                    Ok(())
                } else {
                    Err(Error::InvalidRecord)
                };
            }
        }
    }
}
/// Prepare from frame zero using explicit source/target budgets, off audio.
/// Bounds/configuration/known duration reject before any source reads; unknown
/// duration uses the caller's maximum source bound for output-capacity preflight.
/// Sinc retains exactly (taps-1)/2 source-sample delay. For N source frames the
/// T-tap causal history has support through source coordinate N+T-2, so retained
/// output length is floor((N+T-2)*output/input)+1. Nominal duration remains
/// ceil(N*output/input). At most T explicit zero frames flush that support,
/// including one numeric boundary guard; actual padding progress is reported.
/// Fixed ratios1/16..16 and the existing 1025-phase Blackman profile apply.
/// No output rate, bypass, gain, delay compensation or decoder trimming is implicit.
///
/// ```
/// use ravet_audio_file::{FilePlaybackOptions, PlaybackConversion, WaveEncoding,
///     WaveFormat, WaveReader, WaveWriter, prepare_file_playback};
/// use std::io::Cursor;
/// let mut writer = WaveWriter::new(Cursor::new(Vec::new()),
///     WaveFormat { channels: 1, sample_rate: 32000, encoding: WaveEncoding::Float32 }, 17)?;
/// writer.write_frames(&[0.25; 17])?;
/// let source = WaveReader::new(Cursor::new(writer.finish()?.into_inner()), 17)?;
/// let mut prepared = prepare_file_playback(source, FilePlaybackOptions {
///     output_rate: 48000, conversion: PlaybackConversion::Sinc { taps: 16 },
///     maximum_source_frames: 17, maximum_output_frames: 64,
///     source_batch_frames: 17, maximum_render_frames: 64,
/// })?;
/// assert_eq!(prepared.info().nominal_output_frames, 26);
/// assert_eq!(prepared.info().prepared_frames, 47); // explicit causal tail
/// assert_eq!(prepared.info().filter_delay_input_samples(), 7.5);
/// prepared.playback_mut().play();
/// let mut output = [0.; 64];
/// assert_eq!(prepared.playback_mut().render(&mut output)?.source_frames, 47);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn prepare_file_playback<R: PlaybackReader>(
    mut reader: R,
    options: FilePlaybackOptions,
) -> Result<PreparedFilePlayback, FilePlaybackFailure<R>> {
    let mut samples = Vec::new();
    let mut progress = FilePlaybackProgress::default();
    let result = (|| {
        let channels = reader.channels();
        let input_rate = reader.sample_rate();
        let declared = reader.declared_frames();
        if !(1..=64).contains(&channels)
            || !(1..=768000).contains(&input_rate)
            || !(1..=768000).contains(&options.output_rate)
            || reader.position_frames() != 0
        {
            return Err(Error::InvalidSamples);
        }
        if !(1..=4096).contains(&options.source_batch_frames)
            || !(1..=65536).contains(&options.maximum_render_frames)
            || !(1..=65536).contains(&reader.maximum_read_frames())
            || options.maximum_source_frames == 0
            || options.maximum_source_frames > MAX_SAMPLES / channels
            || options.maximum_output_frames == 0
            || options.maximum_output_frames > MAX_SAMPLES / channels
        {
            return Err(Error::Capacity);
        }
        let batch = options
            .source_batch_frames
            .min(reader.maximum_read_frames());
        let taps = match options.conversion {
            PlaybackConversion::Copy => {
                if input_rate != options.output_rate {
                    return Err(Error::Unsupported);
                }
                0
            }
            PlaybackConversion::Sinc { taps } | PlaybackConversion::IntegerSinc { taps } => {
                let ratio = f64::from(input_rate) / f64::from(options.output_rate);
                if !(16..=256).contains(&taps)
                    || !taps.is_multiple_of(2)
                    || !(1. / 16. ..=16.).contains(&ratio)
                {
                    return Err(Error::Unsupported);
                }
                taps
            }
        };
        let bound = declared.unwrap_or(options.maximum_source_frames as u64);
        if bound == 0 {
            return Err(Error::InvalidSamples);
        }
        if bound > options.maximum_source_frames as u64 {
            return Err(Error::Capacity);
        }
        let limit = prepared_frames(bound, input_rate, options.output_rate, options.conversion)?;
        if limit > options.maximum_output_frames {
            return Err(Error::Capacity);
        }
        let converter_bytes = if taps == 0 {
            0
        } else {
            taps * 1025 * 8
                + taps * 8
                + (batch + OUTPUT_BATCH.min(options.maximum_output_frames)) * 8
        };
        let working = channels * (converter_bytes + batch * 4);
        if working > MAX_WORKING_BYTES {
            return Err(Error::Capacity);
        }
        samples
            .try_reserve_exact(limit * channels)
            .map_err(|_| Error::Capacity)?;
        let mut scratch: Vec<f32> = zeroed(batch * channels)?;
        let mut converter = Converter::new(channels, input_rate, options, batch)?;
        loop {
            let before = reader.position_frames();
            let read = reader.read_frames(&mut scratch);
            progress.source_frames_read = reader.position_frames();
            let frames = read?;
            if frames > batch
                || progress.source_frames_read
                    != before.checked_add(frames as u64).ok_or(Error::Capacity)?
            {
                return Err(Error::InvalidRecord);
            }
            if progress.source_frames_read > options.maximum_source_frames as u64
                || declared.is_some_and(|n| progress.source_frames_read > n)
            {
                return Err(Error::Capacity);
            }
            if frames == 0 {
                break;
            }
            converter.submit(
                &scratch[..frames * channels],
                channels,
                false,
                limit,
                &mut samples,
                &mut progress,
            )?;
        }
        let source_frames = progress.source_frames_read;
        if declared.is_some_and(|frames| frames != source_frames) {
            return Err(Error::InvalidRecord);
        }
        let total = prepared_frames(
            source_frames,
            input_rate,
            options.output_rate,
            options.conversion,
        )?;
        if !converter.filters.is_empty() {
            let mut padding = 0;
            scratch.fill(0.);
            while progress.prepared_frames < total {
                let count = batch.min(taps - padding);
                if count == 0 {
                    return Err(Error::InvalidRecord);
                }
                let before = progress.padding_frames_submitted;
                converter.submit(
                    &scratch[..count * channels],
                    channels,
                    true,
                    total,
                    &mut samples,
                    &mut progress,
                )?;
                padding += progress.padding_frames_submitted - before;
                if progress.padding_frames_submitted == before && progress.prepared_frames < total {
                    return Err(Error::InvalidRecord);
                }
            }
        }
        if progress.prepared_frames != total {
            return Err(Error::InvalidRecord);
        }
        let nominal = nominal_frames(source_frames, input_rate, options.output_rate)?;
        Ok(FilePlaybackInfo {
            source_rate: input_rate,
            output_rate: options.output_rate,
            channels,
            source_frames,
            nominal_output_frames: nominal,
            prepared_frames: total,
            tail_output_frames: total - nominal,
            conversion: options.conversion,
        })
    })();
    match result {
        Ok(info) => Ok(PreparedFilePlayback {
            playback: MemoryPlayback::from_owned_validated(
                samples,
                info.channels,
                info.output_rate,
                options.maximum_render_frames,
            ),
            info,
            progress,
        }),
        Err(error) => Err(FilePlaybackFailure {
            reader,
            error,
            progress,
            partial_samples: samples,
        }),
    }
}
