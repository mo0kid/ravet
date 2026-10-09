//! Explicit worker clock conversion between prepared audio queues.
use crate::{
    ClockedDuplexError, ClockedDuplexOptions, ClockedDuplexStatistics, OutputStatistics,
    PreparedClockedDuplex,
};
use ravet_audio_file::{AudioConsumer, AudioProducer};
use ravet_foundation::{
    PrepareError, ProcessBlock, ProcessConfig, ProcessError, ProcessStatus, Processor,
};
struct Relay {
    configuration: Option<ProcessConfig>,
}
impl Processor for Relay {
    fn prepare(&mut self, configuration: ProcessConfig) -> Result<(), PrepareError> {
        if configuration.layout().input_channels() != configuration.layout().output_channels()
            || !configuration.auxiliary_buses().is_empty()
            || configuration.maximum_events() != 0
            || configuration.maximum_output_events() != 0
        {
            return Err(PrepareError::UnsupportedLayout);
        }
        self.configuration = Some(configuration);
        Ok(())
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {
        self.configuration = None;
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        let Some(configuration) = self.configuration else {
            block.clear_outputs();
            return ProcessStatus::Fault(ProcessError::NotPrepared);
        };
        if let Err(error) = configuration.validate_block(block) {
            block.clear_outputs();
            return ProcessStatus::Fault(error);
        }
        for frame in 0..block.audio.frames() {
            for channel in 0..block.audio.output_channels() {
                let sample = block
                    .audio
                    .input(channel)
                    .expect("matched converter channels")[frame];
                block
                    .audio
                    .output_mut(channel)
                    .expect("matched converter channels")[frame] = sample;
            }
        }
        ProcessStatus::Continue
    }
}
/// Progress separates a fresh converted block from retained queue publication.
/// Counts describe worker blocks, not capture/playback timestamps or latency.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClockedInputProgress {
    pub converted_frames: usize,
    pub published_frames: usize,
    pub pending_frames: usize,
}
/// Control-prepared worker conversion from raw capture to a graph-rate queue.
/// Matching 1..64 channel counts, 8000..192000 nominal integer rates, existing
/// clock/sinc bounds and 1..65536 fixed frames apply. Output scratch<=16MiB is
/// additional to converter/internal queues and source/destination ring memory.
/// Construction/drop allocate off callbacks. Pumping uses no heap/locks/I/O;
/// worker pacing, queue priming, thread ownership and endpoint shutdown are explicit.
/// No physical clock alignment, universal drift stability or deadline guarantee.
pub struct PreparedClockedInput {
    converter: PreparedClockedDuplex<Relay>,
    producer: AudioProducer,
    scratch: Vec<f32>,
    frames: usize,
    pending: std::ops::Range<usize>,
}
impl PreparedClockedInput {
    /// Construct outside active callbacks, retaining one raw consumer and one
    /// converted producer. Options.clock.capacity_frames matches the raw ring;
    /// target occupancy governs priming, not destination publication. Nominal
    /// rates are explicit, without inferred device/graph configuration or events.
    pub fn new(
        consumer: AudioConsumer,
        producer: AudioProducer,
        input_rate: u32,
        output_rate: u32,
        options: ClockedDuplexOptions,
    ) -> Result<Self, ClockedDuplexError> {
        if consumer.channels() != producer.channels() {
            return Err(ClockedDuplexError::InvalidConfiguration);
        }
        let channels = producer.channels();
        let frames = options.maximum_frames;
        let converter = PreparedClockedDuplex::new(
            Relay {
                configuration: None,
            },
            consumer,
            input_rate,
            output_rate,
            channels,
            options,
        )?;
        Ok(Self {
            converter,
            producer,
            scratch: vec![0.; frames * channels],
            frames,
            pending: 0..0,
        })
    }
    /// Convert one fixed block only after the prior suffix has been published.
    /// Full/partial destination queues retain exact frames without advancing sinc/
    /// controller/capture state again. Priming and shortage produce intentional
    /// zeros under the existing clocked-duplex policy. No waiting or device I/O.
    pub fn pump(&mut self) -> ClockedInputProgress {
        let mut converted_frames = 0;
        let channels = self.producer.channels();
        if self.pending.is_empty() {
            self.converter.render(&mut self.scratch);
            self.pending = 0..self.frames;
            converted_frames = self.frames;
        }
        let published_frames = self
            .producer
            .push(&self.scratch[self.pending.start * channels..self.pending.end * channels])
            .expect("prepared complete converted frames");
        self.pending.start += published_frames;
        ClockedInputProgress {
            converted_frames,
            published_frames,
            pending_frames: self.pending.len(),
        }
    }
    /// Relaxed independent converter counters; converted blocks include priming
    /// zeros. Pending publication makes no additional converter callback.
    pub fn statistics(&self) -> OutputStatistics {
        self.converter.statistics()
    }
    /// Raw queue plus prefetched occupancy/controller correction, sampled during
    /// conversion; not a coherent hardware timestamp or measured stream delay.
    pub fn clock_statistics(&self) -> ClockedDuplexStatistics {
        self.converter.clock_statistics()
    }
}
