//! Explicit AIFF PCM worker adapters over shared prepared handoff mechanics.
use crate::{
    AiffFormat, AiffReader, AiffWriter, AudioConsumer, AudioFileError, AudioProducer,
    ReadAheadProgress,
};
use std::io::{Read, Seek, Write};
/// Worker-owned AIFF read-ahead with retained suffixes under ring backpressure.
/// Requires matching channels and 1..65,536 scratch frames within reader capacity.
/// Pump/seek perform file I/O outside callbacks; only prepared consumer pop or
/// BufferedPlayback rendering belongs in audio. No rate conversion is implicit.
/// Wrap in ReadAheadWorker for explicit automatic worker/transport ownership.
pub struct AiffReadAhead<R> {
    inner: crate::pcm_buffered::PcmReadAhead<AiffReader<R>>,
}
impl<R: Read + Seek> AiffReadAhead<R> {
    pub fn new(
        reader: AiffReader<R>,
        producer: AudioProducer,
        maximum_frames: usize,
    ) -> Result<Self, AudioFileError> {
        Ok(Self {
            inner: crate::pcm_buffered::PcmReadAhead::new(reader, producer, maximum_frames)?,
        })
    }
    pub fn format(&self) -> AiffFormat {
        self.inner.reader().format()
    }
    /// Includes queued/pending frames; this is not an audible sample clock.
    pub fn reader_position(&self) -> u64 {
        self.inner.position()
    }
    /// At most one bounded read; retain all unqueued suffixes. Read I/O failure
    /// preserves logical position and publication and may be retried explicitly.
    /// EOF alone does not mean pending/queued audio has been consumed.
    pub fn pump(&mut self) -> Result<ReadAheadProgress, AudioFileError> {
        self.inner.pump()
    }
    /// Reclaim matching stopped endpoints. Failed seeks preserve queue/pending/
    /// cursor; successful seeks clear all old-generation audio including scratch.
    pub fn seek_stopped(
        &mut self,
        consumer: &mut AudioConsumer,
        frame: u64,
    ) -> Result<(), AudioFileError> {
        self.inner.seek_stopped(consumer, frame)
    }
}
/// AIFF PCM16/24/32 recorder draining a prepared input ring on a worker.
/// Preparation writes a header to a caller-selected empty seekable sink. Capture
/// callbacks only push frames. A dequeued write failure poisons the recorder;
/// accepted/output prefixes cannot be rolled back. Drop never finalizes or drains.
/// No physical device, path, worker thread, rate conversion or clock is selected.
pub struct AiffRecorder<W> {
    inner: crate::pcm_buffered::PcmRecorder<AiffWriter<W>>,
}
impl<W: Write + Seek> AiffRecorder<W> {
    /// Validate matching channels before header I/O; scratch capacity 1..65,536.
    /// Writer format/count/FORM-size and finite PCM conversion rules still apply.
    pub fn new(
        destination: W,
        format: AiffFormat,
        maximum_frames: usize,
        consumer: AudioConsumer,
    ) -> Result<Self, AudioFileError> {
        if usize::from(format.channels) != consumer.channels() {
            return Err(AudioFileError::InvalidSamples);
        }
        let writer = AiffWriter::new(destination, format, maximum_frames)?;
        Ok(Self {
            inner: crate::pcm_buffered::PcmRecorder::new(writer, consumer, maximum_frames),
        })
    }
    /// Successfully written frames; excludes a rejected/partially written call.
    pub fn frames(&self) -> u64 {
        self.inner.frames()
    }
    /// Drain at most scratch capacity. Empty calls perform no file I/O. Any write
    /// or PCM validation failure after dequeue poisons future pump/finalization.
    pub fn pump(&mut self) -> Result<usize, AudioFileError> {
        self.inner.pump()
    }
    /// Stop/join the producer first. Drain at most one ring capacity, finalize
    /// FORM/COMM/SSND lengths and flush. Continuing publication can reject finish
    /// or lose late frames; no implicit capture stop or unlimited drain occurs.
    pub fn finish(self) -> Result<W, AudioFileError> {
        self.inner.finish()
    }
}
impl<R: Read + Seek + Send + 'static> crate::ReadAheadPump for AiffReadAhead<R> {
    fn pump(&mut self) -> Result<ReadAheadProgress, AudioFileError> {
        AiffReadAhead::pump(self)
    }
    fn owns(&self, consumer: &AudioConsumer) -> bool {
        self.inner.owns(consumer)
    }
    fn pending_frames(&self) -> usize {
        self.inner.pending_frames()
    }
    fn reader_position(&self) -> u64 {
        AiffReadAhead::reader_position(self)
    }
    fn sample_rate(&self) -> u32 {
        self.format().sample_rate
    }
    fn seek_stopped(
        &mut self,
        consumer: &mut AudioConsumer,
        frame: u64,
    ) -> Result<(), AudioFileError> {
        AiffReadAhead::seek_stopped(self, consumer, frame)
    }
}

impl<W: Write + Seek + Send + 'static> crate::RecordingPump for AiffRecorder<W> {
    type Destination = W;
    fn consumer(&self) -> &AudioConsumer {
        self.inner.consumer()
    }
    fn owns(&self, producer: &AudioProducer) -> bool {
        self.inner.owns(producer)
    }
    fn pump(&mut self) -> Result<usize, AudioFileError> {
        AiffRecorder::pump(self)
    }
    fn frames(&self) -> u64 {
        AiffRecorder::frames(self)
    }
    fn finish(self) -> Result<W, AudioFileError> {
        AiffRecorder::finish(self)
    }
}
