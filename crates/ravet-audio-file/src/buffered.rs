//! Explicit worker pumps; no hidden threads, device capture, or audio I/O.
use crate::{AudioConsumer, AudioFileError, AudioProducer, WaveFormat, WaveReader, WaveWriter};
use std::io::{Read, Seek, Write};

/// Progress from one bounded read-ahead pump. EOF means the reader is exhausted;
/// pending and queued audio may still need playback after EOF becomes true.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
/// One bounded pump in queue-source coordinates at ReadAheadPump::sample_rate.
/// Native readers count original frames; explicit converted sources count
/// generated output frames and expose original input progress separately.
pub struct ReadAheadProgress {
    pub read_frames: usize,
    pub queued_frames: usize,
    pub pending_frames: usize,
    pub end_of_file: bool,
}
/// Worker-owned WAV read-ahead with retained unqueued samples under backpressure.
/// Prepare 1..=65,536 scratch frames no greater than the reader's own capacity.
/// Calls perform seek/read I/O and must never run on an audio callback.
pub struct WaveReadAhead<R> {
    inner: crate::pcm_buffered::PcmReadAhead<WaveReader<R>>,
}
impl<R: Read + Seek> WaveReadAhead<R> {
    pub fn new(
        reader: WaveReader<R>,
        producer: AudioProducer,
        maximum_frames: usize,
    ) -> Result<Self, AudioFileError> {
        Ok(Self {
            inner: crate::pcm_buffered::PcmReadAhead::new(reader, producer, maximum_frames)?,
        })
    }
    pub fn format(&self) -> WaveFormat {
        self.inner.reader().format()
    }
    /// Reader position includes pending/queued frames, not the audible position.
    pub fn reader_position(&self) -> u64 {
        self.inner.position()
    }
    /// At most one bounded file read. A full ring retains the entire remaining
    /// scratch suffix and prevents another read until that suffix is published.
    /// Failed reads preserve reader position and may be retried explicitly.
    pub fn pump(&mut self) -> Result<ReadAheadProgress, AudioFileError> {
        self.inner.pump()
    }
    /// Seek after stopping and reclaiming both endpoints. Reject a foreign ring
    /// or out-of-range frame before changing reader/pending/queued state. Success
    /// discards old queued and unqueued data; no old generation leaks into playback.
    pub fn seek_stopped(
        &mut self,
        consumer: &mut AudioConsumer,
        frame: u64,
    ) -> Result<(), AudioFileError> {
        self.inner.seek_stopped(consumer, frame)
    }
}

/// Worker-owned recording sink for an input ring, with explicit finalization.
/// Device callbacks only publish to the producer; pump performs file I/O here.
/// No thread, physical device, file path or recording permission is chosen.
pub struct WaveRecorder<W> {
    inner: crate::pcm_buffered::PcmRecorder<WaveWriter<W>>,
}
impl<W: Write + Seek> WaveRecorder<W> {
    /// Validate matching channels before writing a header to an empty destination.
    pub fn new(
        destination: W,
        format: WaveFormat,
        maximum_frames: usize,
        consumer: AudioConsumer,
    ) -> Result<Self, AudioFileError> {
        if usize::from(format.channels) != consumer.channels() {
            return Err(AudioFileError::InvalidSamples);
        }
        let writer = WaveWriter::new(destination, format, maximum_frames)?;
        Ok(Self {
            inner: crate::pcm_buffered::PcmRecorder::new(writer, consumer, maximum_frames),
        })
    }
    pub fn frames(&self) -> u64 {
        self.inner.frames()
    }
    /// Drain at most prepared scratch frames. Empty returns zero without file I/O.
    /// Any recording error poisons this sink because dequeued frames cannot be
    /// rolled back; later calls reject. Input ring drops remain producer diagnostics.
    pub fn pump(&mut self) -> Result<usize, AudioFileError> {
        self.inner.pump()
    }
    /// Stop/join the producer first. Drain at most one ring capacity, patch WAV
    /// lengths and flush. Continuing publication can reject finalization or lose
    /// late frames: stopped-producer ownership is a caller lifecycle obligation.
    /// Drop performs no implicit drain/finalization/file I/O.
    pub fn finish(self) -> Result<W, AudioFileError> {
        self.inner.finish()
    }
}
impl<R: Read + Seek + Send + 'static> crate::ReadAheadPump for WaveReadAhead<R> {
    fn pump(&mut self) -> Result<ReadAheadProgress, AudioFileError> {
        WaveReadAhead::pump(self)
    }
    fn owns(&self, consumer: &AudioConsumer) -> bool {
        self.inner.owns(consumer)
    }
    fn pending_frames(&self) -> usize {
        self.inner.pending_frames()
    }
    fn reader_position(&self) -> u64 {
        WaveReadAhead::reader_position(self)
    }
    fn sample_rate(&self) -> u32 {
        self.format().sample_rate
    }
    fn seek_stopped(
        &mut self,
        consumer: &mut AudioConsumer,
        frame: u64,
    ) -> Result<(), AudioFileError> {
        WaveReadAhead::seek_stopped(self, consumer, frame)
    }
}

impl<W: Write + Seek + Send + 'static> crate::RecordingPump for WaveRecorder<W> {
    type Destination = W;
    fn consumer(&self) -> &AudioConsumer {
        self.inner.consumer()
    }
    fn owns(&self, producer: &AudioProducer) -> bool {
        self.inner.owns(producer)
    }
    fn pump(&mut self) -> Result<usize, AudioFileError> {
        WaveRecorder::pump(self)
    }
    fn frames(&self) -> u64 {
        WaveRecorder::frames(self)
    }
    fn finish(self) -> Result<W, AudioFileError> {
        WaveRecorder::finish(self)
    }
}
