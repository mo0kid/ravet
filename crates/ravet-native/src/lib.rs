//! Reusable prepared native planar processing, independent of SDKs and products.
//! Control threads prepare/deactivate processors and storage. Native callers own
//! valid foreign memory and serialized lifecycle. Explicit C-layout event/transport codecs
//! use caller-prepared storage; an optional exported lifecycle table composes
//! these for consumer-selected processors. Native facade traits supply bounded
//! global schemas and optional explicit live base-state publication; SDK
//! metadata/registration and delivery remain consumer policy. Normal processing/reset avoid heap actions.
use ravet_foundation::{
    AudioBuffer, Event, EventData, OutputEvents, ParameterId, PrepareError, ProcessBlock,
    ProcessConfig, ProcessError, ProcessStatus, Processor, Sample,
};
use std::panic::{AssertUnwindSafe, catch_unwind};
mod event;
mod transport;
pub use transport::{NativeTransportError, NativeTransportV1, native_transport_presence};
#[allow(unsafe_code)]
mod entry;
#[allow(unsafe_code)]
mod parameter_cache;
#[allow(unsafe_code)]
mod parameters;
pub use parameter_cache::{
    NativeCachedParameterEntryV1, NativeCachedParameters, NativeNormalizedParameterEventV1,
    NativeParameterCache, NativeParameterMappingError,
};
pub use parameters::{
    NativeAudioParameterProcessor, NativeParameterEntryV1, NativeParameterInfoV1,
    NativeParameterProcessor, NativeParameterRoles, NativeParameterValueV1, NativeParameters,
};
#[allow(unsafe_code)]
mod raw;
pub use entry::{
    NativeBusLayoutV1, NativeProcessor, NativeProcessorBusV1, NativeProcessorConfigV1,
    NativeProcessorEntryV1, NativeProcessorResultV1, NativeProcessorTimingV1,
};
pub use event::{
    NativeEventError, NativeEventV1, NativeParameterEvent, decode_native_events,
    decode_parameter_events, encode_native_events, native_event_kind,
};
pub use raw::{NativePlanarBus, NativeProcessReport};
const CHANNELS: usize = 64 * 9;
const PAYLOAD_BYTES: usize = 64 * 1024 * 1024;
/// Preparation is bounded independently of the consumer processor's limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativePrepareError {
    Capacity,
    Processor(PrepareError),
}
impl std::fmt::Display for NativePrepareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "native adapter preparation: {self:?}")
    }
}
impl std::error::Error for NativePrepareError {}
/// Prepared f32/f64 planar scratch and generated-event storage. One main bus and
/// up to eight auxiliaries use preparation order, including input/output-only
/// buses. All inputs copy before host output writes, permitting cross-bus aliases.
/// Owned sample/event payload is at most 64 MiB; frames at most 65536, event
/// capacities at most 1024 each. Processor support remains separately validated.
///
/// Pass the same exclusively owned processor to process/reset as was prepared.
/// Preparation/drop/deactivation belong after callbacks stop. Audio panic poisons
/// this adapter; reset does not recover it. Fresh control-thread preparation is
/// required. No event/state marshalling, SDK registration or deadline is inferred.
#[derive(Debug)]
pub struct NativeAudioAdapter<S: Sample = f32> {
    config: ProcessConfig,
    inputs: Vec<Vec<S>>,
    outputs: Vec<Vec<S>>,
    generated: Vec<Event>,
    payload_bytes: usize,
    poisoned: bool,
}
impl<S: Sample> NativeAudioAdapter<S> {
    /// Deactivate the supplied stopped processor, validate bounds, allocate
    /// scratch, then prepare it. A rejected preparation leaves it inactive. This
    /// Rust control API does not contain panics; foreign control wrappers must.
    pub fn prepare<P: Processor<S>>(
        processor: &mut P,
        config: ProcessConfig,
    ) -> Result<Self, NativePrepareError> {
        processor.deactivate();
        if config.maximum_frames() > 65536
            || config.maximum_events() > 1024
            || config.maximum_output_events() > 1024
        {
            return Err(NativePrepareError::Capacity);
        }
        let (ins, outs) = std::iter::once(config.layout())
            .chain(config.auxiliary_buses().iter().copied())
            .fold((0, 0), |(i, o), bus| {
                (i + bus.input_channels(), o + bus.output_channels())
            });
        let bytes = (ins + outs) * config.maximum_frames() * size_of::<S>()
            + config.maximum_output_events() * size_of::<Event>();
        if bytes > PAYLOAD_BYTES {
            return Err(NativePrepareError::Capacity);
        }
        let adapter = Self {
            config,
            inputs: (0..ins)
                .map(|_| vec![S::default(); config.maximum_frames()])
                .collect(),
            outputs: (0..outs)
                .map(|_| vec![S::default(); config.maximum_frames()])
                .collect(),
            generated: vec![
                Event {
                    frame_offset: 0,
                    data: EventData::ParameterValue {
                        id: ParameterId(0),
                        value: 0.
                    }
                };
                config.maximum_output_events()
            ],
            payload_bytes: bytes,
            poisoned: false,
        };
        if let Err(error) = processor.prepare(config) {
            processor.deactivate();
            return Err(NativePrepareError::Processor(error));
        }
        Ok(adapter)
    }
    pub fn configuration(&self) -> ProcessConfig {
        self.config
    }
    /// Sample and generated-event payload bytes, excluding control metadata.
    pub fn payload_bytes(&self) -> usize {
        self.payload_bytes
    }
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }
    /// Reset the same processor without changing adapter preparation. A panic
    /// poisons this adapter; poisoned instances reject without invoking reset.
    pub fn reset<P: Processor<S>>(&mut self, processor: &mut P) -> Result<(), ProcessError> {
        if self.poisoned {
            return Err(ProcessError::BackendFailure);
        }
        if catch_unwind(AssertUnwindSafe(|| processor.reset())).is_err() {
            self.poisoned = true;
            return Err(ProcessError::BackendFailure);
        }
        Ok(())
    }
    fn run<P: Processor<S>>(
        &mut self,
        processor: &mut P,
        frames: usize,
        events: &[Event],
        transport: ravet_foundation::Transport,
    ) -> Result<(ProcessStatus, usize), ProcessError> {
        let mut input_iter = self.inputs.iter();
        let inputs: [&[S]; CHANNELS] =
            std::array::from_fn(|_| input_iter.next().map_or(&[] as &[S], |v| &v[..frames]));
        let mut output_iter = self.outputs.iter_mut();
        let mut outputs: [&mut [S]; CHANNELS] = std::array::from_fn(|_| {
            output_iter.next().map_or(&mut [] as &mut [S], |v| {
                v[..frames].fill(S::default());
                &mut v[..frames]
            })
        });
        let layout = self.config.layout();
        let (main_outputs, mut remaining_outputs) = outputs.split_at_mut(layout.output_channels());
        let main = AudioBuffer::separate(&inputs[..layout.input_channels()], main_outputs, frames)
            .expect("prepared scratch");
        let mut input_offset = layout.input_channels();
        let mut auxiliary: [AudioBuffer<'_, '_, S>; 8] = std::array::from_fn(|index| {
            let (ins, outs) = self
                .config
                .auxiliary_buses()
                .get(index)
                .map_or((0, 0), |b| (b.input_channels(), b.output_channels()));
            let (out, rest) = std::mem::take(&mut remaining_outputs).split_at_mut(outs);
            remaining_outputs = rest;
            let bus = AudioBuffer::separate(&inputs[input_offset..input_offset + ins], out, frames)
                .expect("prepared auxiliary scratch");
            input_offset += ins;
            bus
        });
        let mut block = ProcessBlock::new(main, events, transport);
        block.auxiliary = Some(&mut auxiliary[..self.config.auxiliary_buses().len()]);
        if !self.generated.is_empty() {
            block.output_events = Some(OutputEvents::new(&mut self.generated, frames));
        }
        self.config.validate_block(&block)?;
        let status = processor.process(&mut block);
        if let ProcessStatus::Fault(error) = status {
            return Err(error);
        }
        Ok((
            status,
            block
                .output_events
                .as_ref()
                .map_or(0, |events| events.len()),
        ))
    }
}

#[cfg(target_has_atomic = "64")]
#[allow(unsafe_code)]
mod state_publication;
#[cfg(target_has_atomic = "64")]
pub use state_publication::{
    NativeState, NativeStateApplyV1, NativeStateEntryV1, NativeStateRevisionV1,
};

#[allow(unsafe_code)]
mod audio_state;
pub use audio_state::{NativeAudioState, NativeAudioStateEntryV1};

#[allow(unsafe_code)]
mod telemetry;
pub use telemetry::{
    NATIVE_TELEMETRY_NONFINITE_SAMPLES, NativeTelemetry, NativeTelemetryCache,
    NativeTelemetryDescriptor, NativeTelemetryEntryV1, NativeTelemetryError, NativeTelemetryInfoV1,
    NativeTelemetryProcessor, NativeTelemetryValueV1,
};
