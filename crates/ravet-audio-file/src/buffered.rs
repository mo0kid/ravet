//! Explicit worker pumps; no hidden threads, device capture, or audio I/O.
use crate::{AudioConsumer, AudioFileError, AudioProducer, WaveFormat, WaveReader, WaveWriter};
use std::io::{Read, Seek, Write};

/// Progress from one bounded read-ahead pump. EOF means the reader is exhausted;
/// pending and queued audio may still need playback after EOF becomes true.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
    reader: WaveReader<R>,
    producer: AudioProducer,
    scratch: Vec<f32>,
    start: usize,
    end: usize,
}
impl<R: Read + Seek> WaveReadAhead<R> {
    pub fn new(
        reader: WaveReader<R>,
        producer: AudioProducer,
        maximum_frames: usize,
    ) -> Result<Self, AudioFileError> {
        if usize::from(reader.format().channels) != producer.channels() {
            return Err(AudioFileError::InvalidSamples);
        }
        if maximum_frames == 0 || maximum_frames > reader.maximum_frames() {
            return Err(AudioFileError::Capacity);
        }
        let scratch = vec![0.; maximum_frames * producer.channels()];
        Ok(Self {
            reader,
            producer,
            scratch,
            start: 0,
            end: 0,
        })
    }
    pub fn format(&self) -> WaveFormat {
        self.reader.format()
    }
    /// Reader position includes pending/queued frames, not the audible position.
    pub fn reader_position(&self) -> u64 {
        self.reader.position()
    }
    /// At most one bounded file read. A full ring retains the entire remaining
    /// scratch suffix and prevents another read until that suffix is published.
    /// Failed reads preserve reader position and may be retried explicitly.
    pub fn pump(&mut self) -> Result<ReadAheadProgress, AudioFileError> {
        let mut progress = ReadAheadProgress::default();
        let channels = self.producer.channels();
        if self.start == self.end && self.reader.position() < self.reader.frames() {
            let read = self.reader.read_frames(&mut self.scratch)?;
            progress.read_frames = read;
            self.start = 0;
            self.end = read * channels;
        }
        let queued = self
            .producer
            .push(&self.scratch[self.start..self.end])
            .expect("whole-frame prepared scratch");
        self.start += queued * channels;
        progress.queued_frames = queued;
        progress.pending_frames = (self.end - self.start) / channels;
        progress.end_of_file = self.reader.position() == self.reader.frames();
        Ok(progress)
    }
    /// Seek after stopping and reclaiming both endpoints. Reject a foreign ring
    /// or out-of-range frame before changing reader/pending/queued state. Success
    /// discards old queued and unqueued data; no old generation leaks into playback.
    pub fn seek_stopped(
        &mut self,
        consumer: &mut AudioConsumer,
        frame: u64,
    ) -> Result<(), AudioFileError> {
        if !self.producer.owns(consumer) {
            return Err(AudioFileError::InvalidSamples);
        }
        self.reader.seek_frame(frame)?;
        self.producer
            .clear(consumer)
            .expect("matching exclusive endpoints");
        self.start = 0;
        self.end = 0;
        Ok(())
    }
}

/// Worker-owned recording sink for an input ring, with explicit finalization.
/// Device callbacks only publish to the producer; pump performs file I/O here.
/// No thread, physical device, file path or recording permission is chosen.
pub struct WaveRecorder<W> {
    writer: WaveWriter<W>,
    consumer: AudioConsumer,
    scratch: Vec<f32>,
    failed: bool,
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
        let scratch = vec![0.; maximum_frames * consumer.channels()];
        Ok(Self {
            writer,
            consumer,
            scratch,
            failed: false,
        })
    }
    pub fn frames(&self) -> u64 {
        self.writer.frames()
    }
    /// Drain at most prepared scratch frames. Empty returns zero without file I/O.
    /// Any recording error poisons this sink because dequeued frames cannot be
    /// rolled back; later calls reject. Input ring drops remain producer diagnostics.
    pub fn pump(&mut self) -> Result<usize, AudioFileError> {
        self.pump_frames(self.scratch.len() / self.consumer.channels())
    }
    fn pump_frames(&mut self, maximum: usize) -> Result<usize, AudioFileError> {
        if self.failed {
            return Err(AudioFileError::Poisoned);
        }
        let frames = self
            .consumer
            .pop(&mut self.scratch[..maximum * self.consumer.channels()])
            .expect("whole-frame scratch");
        if frames != 0
            && let Err(error) = self
                .writer
                .write_frames(&self.scratch[..frames * self.consumer.channels()])
        {
            self.failed = true;
            return Err(error);
        }
        Ok(frames)
    }
    /// Stop/join the producer first. Drain at most one ring capacity, patch WAV
    /// lengths and flush. Continuing publication can reject finalization or lose
    /// late frames: stopped-producer ownership is a caller lifecycle obligation.
    /// Drop performs no implicit drain/finalization/file I/O.
    pub fn finish(mut self) -> Result<W, AudioFileError> {
        let limit = self.consumer.capacity_frames();
        let mut drained = 0;
        while drained < limit {
            let frames = self.pump_frames(
                (limit - drained).min(self.scratch.len() / self.consumer.channels()),
            )?;
            if frames == 0 {
                break;
            }
            drained += frames;
        }
        if self.consumer.queued_frames() != 0 {
            return Err(AudioFileError::Poisoned);
        }
        self.writer.finish()
    }
}
