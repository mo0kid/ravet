//! Explicit worker-only compressed read-ahead into prepared audio handoff.
use crate::{AudioConsumer, AudioFileError, AudioProducer, CompressedReader, ReadAheadProgress};
/// Owned compressed worker pump. Prepare 1..65536 scratch frames and matching
/// channel ring. No thread/clock/device is created. Decode/seek and destruction
/// may allocate and must stay outside audio; only consumer pop is callback-safe.
/// Pending decoded scratch retains unqueued suffixes under backpressure.
pub struct CompressedReadAhead {
    reader: CompressedReader,
    producer: AudioProducer,
    scratch: Vec<f32>,
    start: usize,
    end: usize,
    failed: bool,
}
impl CompressedReadAhead {
    pub fn new(
        reader: CompressedReader,
        producer: AudioProducer,
        maximum_frames: usize,
    ) -> Result<Self, AudioFileError> {
        if usize::from(reader.channels()) != producer.channels() {
            return Err(AudioFileError::InvalidSamples);
        }
        if !(1..=65536).contains(&maximum_frames) {
            return Err(AudioFileError::Capacity);
        }
        let scratch = vec![0.; maximum_frames * producer.channels()];
        Ok(Self {
            reader,
            producer,
            scratch,
            start: 0,
            end: 0,
            failed: false,
        })
    }
    pub fn sample_rate(&self) -> u32 {
        self.reader.sample_rate()
    }
    pub fn channels(&self) -> u16 {
        self.reader.channels()
    }
    /// Includes pending/queued frames, rather than audible/sample-clock position.
    pub fn reader_position(&self) -> usize {
        self.reader.position_frames()
    }
    /// At most one bounded stream read. Full rings retain remaining scratch and
    /// prevent another read until it is queued. EOF means verified stream end;
    /// pending/queued audio can still require playback. Decode faults poison the
    /// pump; queued earlier audio remains and cannot be rolled back.
    pub fn pump(&mut self) -> Result<ReadAheadProgress, AudioFileError> {
        if self.failed {
            return Err(AudioFileError::Poisoned);
        }
        let channels = self.producer.channels();
        let mut progress = ReadAheadProgress::default();
        if self.start == self.end && !self.reader.end_of_file() {
            let count = match self.reader.read_frames(&mut self.scratch) {
                Ok(count) => count,
                Err(error) => {
                    self.failed = true;
                    return Err(error);
                }
            };
            self.start = 0;
            self.end = count * channels;
            progress.read_frames = count;
        }
        let queued = self
            .producer
            .push(&self.scratch[self.start..self.end])
            .expect("whole-frame scratch");
        self.start += queued * channels;
        progress.queued_frames = queued;
        progress.pending_frames = (self.end - self.start) / channels;
        progress.end_of_file = self.reader.end_of_file();
        Ok(progress)
    }
    /// Reclaim both endpoints after stopping/joining users. Reject foreign queues
    /// or failed replay before changing pending/queued state. Success clears old
    /// generation audio and poison. Default readers replay O(target); indexed FLAC
    /// readers decode one indexed packet. Neither mode provides a live seek.
    pub fn seek_stopped(
        &mut self,
        consumer: &mut AudioConsumer,
        frame: usize,
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
        self.failed = false;
        Ok(())
    }
}

impl crate::ReadAheadPump for CompressedReadAhead {
    fn pump(&mut self) -> Result<ReadAheadProgress, AudioFileError> {
        CompressedReadAhead::pump(self)
    }
    fn owns(&self, consumer: &AudioConsumer) -> bool {
        self.producer.owns(consumer)
    }
    fn pending_frames(&self) -> usize {
        (self.end - self.start) / self.producer.channels()
    }
    fn reader_position(&self) -> u64 {
        CompressedReadAhead::reader_position(self) as u64
    }
    fn sample_rate(&self) -> u32 {
        CompressedReadAhead::sample_rate(self)
    }
    fn seek_stopped(
        &mut self,
        consumer: &mut AudioConsumer,
        frame: u64,
    ) -> Result<(), AudioFileError> {
        let frame = usize::try_from(frame).map_err(|_| AudioFileError::Capacity)?;
        CompressedReadAhead::seek_stopped(self, consumer, frame)
    }
}
