//! Explicit worker capture-ring draining and PCM quantization.
use crate::{AudioConsumer, AudioFileError as Error, FlacFormat, FlacWriter};
use std::io::{Seek, Write};
/// Explicit normalized f32 capture policy before signed PCM16/24 quantization.
/// Finite samples round to nearest integer, ties away from zero, without dither.
/// -1 maps to the signed minimum and +1 to the signed maximum. Nonfinite input
/// always rejects. This policy applies to a complete dequeued worker batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlacCapturePolicy {
    /// Reject finite samples outside -1..1 instead of clipping overload.
    RejectOverload,
    /// Explicitly clip finite overload to -1..1; no automatic level scaling.
    ClipOverload,
}
/// Worker-owned incremental FLAC capture sink fed by a prepared f32 audio ring.
/// Audio callbacks only publish; pump quantizes/encodes/writes on its caller.
/// Any failure after dequeue poisons because accepted ring frames cannot roll
/// back. Producer full-ring drops remain producer policy. Drop does not finalize.
/// No hidden worker, path/device/clock choice or physical recording claim.
pub struct FlacRecorder<W> {
    writer: FlacWriter<W>,
    consumer: AudioConsumer,
    samples: Vec<f32>,
    pcm: Vec<i32>,
    policy: FlacCapturePolicy,
    failed: bool,
}
impl<W: Write + Seek> FlacRecorder<W> {
    /// Validate ring channels and 1..65536 pump frames before creating the writer
    /// header. Writer format, total frame/byte budgets and empty sink rules apply.
    pub fn new(
        destination: W,
        format: FlacFormat,
        maximum_pump_frames: usize,
        maximum_total_frames: u64,
        maximum_bytes: u64,
        policy: FlacCapturePolicy,
        consumer: AudioConsumer,
    ) -> Result<Self, Error> {
        if consumer.channels() != usize::from(format.channels) {
            return Err(Error::InvalidSamples);
        }
        if !(1..=65536).contains(&maximum_pump_frames) {
            return Err(Error::Capacity);
        }
        let samples = vec![0.; maximum_pump_frames * consumer.channels()];
        let pcm = vec![0; samples.len()];
        let writer = FlacWriter::new(destination, format, maximum_total_frames, maximum_bytes)?;
        Ok(Self {
            writer,
            consumer,
            samples,
            pcm,
            policy,
            failed: false,
        })
    }
    /// Frames accepted by the encoder, including its retained pending block.
    /// A rejected dequeued batch is excluded; successful capture is not yet final.
    pub fn frames(&self) -> u64 {
        self.writer.accepted_frames()
    }
    pub fn is_poisoned(&self) -> bool {
        self.failed || self.writer.is_poisoned()
    }
    /// Drain at most one prepared scratch batch; an empty ring performs no I/O.
    /// Malformed/nonfinite/overload input, writer capacity or I/O failure poisons
    /// after dequeue. Failed batches may have an irreversible accepted prefix.
    pub fn pump(&mut self) -> Result<usize, Error> {
        self.pump_frames(self.samples.len() / self.consumer.channels())
    }
    fn pump_frames(&mut self, maximum: usize) -> Result<usize, Error> {
        if self.is_poisoned() {
            return Err(Error::Poisoned);
        }
        let channels = self.consumer.channels();
        let frames = self
            .consumer
            .pop(&mut self.samples[..maximum * channels])
            .expect("prepared whole-frame scratch");
        if frames == 0 {
            return Ok(0);
        }
        let samples = &self.samples[..frames * channels];
        if samples.iter().any(|value| {
            !value.is_finite()
                || (self.policy == FlacCapturePolicy::RejectOverload
                    && !(-1.0..=1.0).contains(value))
        }) {
            self.failed = true;
            return Err(Error::InvalidSamples);
        }
        let scale = (1i32 << (self.writer.format().encoding.bits() - 1)) as f64;
        for (sample, pcm) in samples.iter().zip(&mut self.pcm) {
            *pcm = (f64::from(sample.clamp(-1., 1.)) * scale)
                .round()
                .clamp(-scale, scale - 1.) as i32;
        }
        if let Err(error) = self.writer.write_frames(&self.pcm[..samples.len()]) {
            self.failed = true;
            return Err(error);
        }
        Ok(frames)
    }
    /// Stop/join the producer first. Drain at most one ring capacity then finalize
    /// the nonempty FLAC writer. Concurrent late publication can reject or lose
    /// late data; stopped producer is the caller's lifecycle obligation. Consumes
    /// the sink, with no retry after final output/header/seek/flush failures.
    pub fn finish(mut self) -> Result<W, Error> {
        let limit = self.consumer.capacity_frames();
        let mut drained = 0;
        while drained < limit {
            let frames = self.pump_frames(
                (limit - drained).min(self.samples.len() / self.consumer.channels()),
            )?;
            if frames == 0 {
                break;
            }
            drained += frames;
        }
        if self.consumer.queued_frames() != 0 {
            return Err(Error::Poisoned);
        }
        self.writer.finish()
    }
}

impl<W: Write + Seek + Send + 'static> crate::RecordingPump for FlacRecorder<W> {
    type Destination = W;
    fn consumer(&self) -> &AudioConsumer {
        &self.consumer
    }
    fn owns(&self, producer: &crate::AudioProducer) -> bool {
        producer.owns(&self.consumer)
    }
    fn pump(&mut self) -> Result<usize, Error> {
        FlacRecorder::pump(self)
    }
    fn frames(&self) -> u64 {
        FlacRecorder::frames(self)
    }
    fn finish(self) -> Result<W, Error> {
        FlacRecorder::finish(self)
    }
}
