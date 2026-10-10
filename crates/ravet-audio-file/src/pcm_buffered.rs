//! Shared prepared PCM handoff mechanics; format I/O remains in each stream.
use crate::{
    AiffReader, AiffWriter, AudioConsumer, AudioFileError, AudioProducer, ReadAheadProgress,
    WaveReader, WaveWriter,
};
use std::io::{Read, Seek, Write};
pub(crate) trait PcmReader {
    fn channels(&self) -> usize;
    fn maximum_frames(&self) -> usize;
    fn frames(&self) -> u64;
    fn position(&self) -> u64;
    fn read(&mut self, output: &mut [f32]) -> Result<usize, AudioFileError>;
    fn seek(&mut self, frame: u64) -> Result<(), AudioFileError>;
}
macro_rules! reader {
    ($type:ident) => {
        impl<R: Read + Seek> PcmReader for $type<R> {
            fn channels(&self) -> usize {
                usize::from(self.format().channels)
            }
            fn maximum_frames(&self) -> usize {
                $type::maximum_frames(self)
            }
            fn frames(&self) -> u64 {
                $type::frames(self)
            }
            fn position(&self) -> u64 {
                $type::position(self)
            }
            fn read(&mut self, output: &mut [f32]) -> Result<usize, AudioFileError> {
                self.read_frames(output)
            }
            fn seek(&mut self, frame: u64) -> Result<(), AudioFileError> {
                self.seek_frame(frame)
            }
        }
    };
}
reader!(WaveReader);
reader!(AiffReader);
pub(crate) struct PcmReadAhead<R> {
    reader: R,
    producer: AudioProducer,
    scratch: Vec<f32>,
    start: usize,
    end: usize,
}
impl<R: PcmReader> PcmReadAhead<R> {
    pub(crate) fn new(
        reader: R,
        producer: AudioProducer,
        maximum: usize,
    ) -> Result<Self, AudioFileError> {
        if reader.channels() != producer.channels() {
            return Err(AudioFileError::InvalidSamples);
        }
        if maximum == 0 || maximum > reader.maximum_frames() {
            return Err(AudioFileError::Capacity);
        }
        let scratch = vec![0.; maximum * producer.channels()];
        Ok(Self {
            reader,
            producer,
            scratch,
            start: 0,
            end: 0,
        })
    }
    pub(crate) fn reader(&self) -> &R {
        &self.reader
    }
    pub(crate) fn owns(&self, consumer: &AudioConsumer) -> bool {
        self.producer.owns(consumer)
    }
    pub(crate) fn pending_frames(&self) -> usize {
        (self.end - self.start) / self.producer.channels()
    }
    pub(crate) fn position(&self) -> u64 {
        self.reader.position()
    }
    pub(crate) fn pump(&mut self) -> Result<ReadAheadProgress, AudioFileError> {
        let mut progress = ReadAheadProgress::default();
        let channels = self.producer.channels();
        if self.start == self.end && self.reader.position() < self.reader.frames() {
            let count = self.reader.read(&mut self.scratch)?;
            progress.read_frames = count;
            self.start = 0;
            self.end = count * channels;
        }
        let queued = self
            .producer
            .push(&self.scratch[self.start..self.end])
            .expect("whole-frame prepared scratch");
        self.start += queued * channels;
        progress.queued_frames = queued;
        progress.pending_frames = self.pending_frames();
        progress.end_of_file = self.reader.position() == self.reader.frames();
        Ok(progress)
    }
    pub(crate) fn seek_stopped(
        &mut self,
        consumer: &mut AudioConsumer,
        frame: u64,
    ) -> Result<(), AudioFileError> {
        if !self.owns(consumer) {
            return Err(AudioFileError::InvalidSamples);
        }
        self.reader.seek(frame)?;
        self.producer
            .clear(consumer)
            .expect("matching exclusive endpoints");
        self.start = 0;
        self.end = 0;
        Ok(())
    }
}
pub(crate) trait PcmWriter {
    type Destination;
    fn frames(&self) -> u64;
    fn write(&mut self, samples: &[f32]) -> Result<(), AudioFileError>;
    fn finish(self) -> Result<Self::Destination, AudioFileError>;
}
macro_rules! writer {
    ($type:ident) => {
        impl<W: Write + Seek> PcmWriter for $type<W> {
            type Destination = W;
            fn frames(&self) -> u64 {
                $type::frames(self)
            }
            fn write(&mut self, samples: &[f32]) -> Result<(), AudioFileError> {
                self.write_frames(samples)
            }
            fn finish(self) -> Result<W, AudioFileError> {
                $type::finish(self)
            }
        }
    };
}
writer!(WaveWriter);
writer!(AiffWriter);
pub(crate) struct PcmRecorder<W> {
    writer: W,
    consumer: AudioConsumer,
    scratch: Vec<f32>,
    failed: bool,
}
impl<W: PcmWriter> PcmRecorder<W> {
    pub(crate) fn new(writer: W, consumer: AudioConsumer, maximum: usize) -> Self {
        let scratch = vec![0.; maximum * consumer.channels()];
        Self {
            writer,
            consumer,
            scratch,
            failed: false,
        }
    }
    pub(crate) fn frames(&self) -> u64 {
        self.writer.frames()
    }
    pub(crate) fn consumer(&self) -> &AudioConsumer {
        &self.consumer
    }
    pub(crate) fn owns(&self, producer: &AudioProducer) -> bool {
        producer.owns(&self.consumer)
    }
    pub(crate) fn pump(&mut self) -> Result<usize, AudioFileError> {
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
                .write(&self.scratch[..frames * self.consumer.channels()])
        {
            self.failed = true;
            return Err(error);
        }
        Ok(frames)
    }
    pub(crate) fn finish(mut self) -> Result<W::Destination, AudioFileError> {
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
