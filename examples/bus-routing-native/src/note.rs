//! Ordinary note routing with explicit channel/key release lowering.
//!
//! Input-none/main mono or stereo audio is silent. Complete Level state is
//! separate from the transient processed-frame counter. Notes preserve sample
//! offsets/source order; richer identities and wildcard scopes are rejected.
use ravet_foundation::{
    Event, EventData, EventError, NoteTarget, ParameterDescriptor, ParameterError, ParameterId,
    ParameterRange, ParameterScale, ParameterSchema, ParameterState, PrepareError, ProcessBlock,
    ProcessConfig, ProcessError, ProcessStatus, Processor, Sample,
};
use ravet_native::{
    NativeAudioParameterProcessor, NativeParameterProcessor, NativeTelemetryDescriptor,
    NativeTelemetryProcessor, NativeTelemetryValueV1,
};
use std::marker::PhantomData;
/// Stable complete global Level identity; no bypass role is declared.
pub const LEVEL_ID: ParameterId = ParameterId(1);
static PARAMETERS: [ParameterDescriptor<'static>; 1] = [ParameterDescriptor {
    id: LEVEL_ID,
    name: "Level",
    unit: "x",
    range: match ParameterRange::new(0., 1., ParameterScale::Linear, None) {
        Ok(x) => x,
        Err(_) => panic!("fixed Level range"),
    },
    default: 1.,
    automatable: true,
}];
static TELEMETRY: [NativeTelemetryDescriptor; 1] = [NativeTelemetryDescriptor {
    id: 1,
    name: "Processed frames",
    unit: "frames",
    display_minimum: 0.,
    display_maximum: 1.,
}];
/// Portable ordinary-note routing. No synthesizer, voices or implicit IDs.
/// Level scales On velocity; Off velocity is retained. Exact channel/key/no-ID
/// scoped release becomes ordinary key Off only under this reference policy.
#[derive(Debug)]
pub struct NoteRouting<S> {
    config: Option<ProcessConfig>,
    level: f32,
    frames: u64,
    precision: PhantomData<S>,
}
impl<S> Default for NoteRouting<S> {
    fn default() -> Self {
        Self {
            config: None,
            level: 1.,
            frames: 0,
            precision: PhantomData,
        }
    }
}
impl<S> NoteRouting<S> {
    /// Transient saturating frame count; complete Level recall preserves it.
    pub fn processed_frames(&self) -> u64 {
        self.frames
    }
    /// Current complete global Level; no per-note state is persisted.
    pub fn level(&self) -> f32 {
        self.level
    }
    fn emitted(data: EventData) -> Option<EventData> {
        match data {
            EventData::NoteOn { .. } | EventData::NoteOff { .. } => Some(data),
            EventData::NoteOffScoped {
                target:
                    NoteTarget {
                        channel: Some(channel),
                        key: Some(key),
                        id: None,
                    },
                velocity,
            } => Some(EventData::NoteOff {
                channel,
                key,
                velocity,
            }),
            _ => None,
        }
    }
}
impl<S: Sample> Processor<S> for NoteRouting<S> {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = None;
        if config.layout().input_channels() != 0
            || !(1..=2).contains(&config.layout().output_channels())
            || !config.auxiliary_buses().is_empty()
            || config.maximum_frames() > 1024
            || config.maximum_events() > 1024
            || !(1..=1024).contains(&config.maximum_output_events())
        {
            return Err(PrepareError::UnsupportedLayout);
        }
        self.frames = 0;
        self.config = Some(config);
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, S>) -> ProcessStatus {
        let check = (|| {
            self.config
                .ok_or(ProcessError::NotPrepared)?
                .validate_block(block)?;
            let mut count = 0;
            for (index, event) in block.events.iter().enumerate() {
                match event.data {
                    EventData::ParameterValue {
                        id: LEVEL_ID,
                        value,
                    } if (0. ..=1.).contains(&value) => {}
                    data if Self::emitted(data).is_some() => count += 1,
                    _ => return Err(ProcessError::Events(EventError::InvalidValue { index })),
                }
            }
            if count > block.output_events.as_ref().map_or(0, |out| out.capacity()) {
                return Err(ProcessError::OutputEventCapacity);
            }
            Ok(())
        })();
        if let Err(error) = check {
            block.clear_outputs();
            return ProcessStatus::Fault(error);
        }
        block.clear_outputs();
        for event in block.events {
            if let EventData::ParameterValue {
                id: LEVEL_ID,
                value,
            } = event.data
            {
                self.level = value;
                continue;
            }
            let mut data = Self::emitted(event.data).expect("complete note preflight");
            if let EventData::NoteOn { velocity, .. } = &mut data {
                *velocity *= self.level;
            }
            let routed = Event {
                frame_offset: event.frame_offset,
                data,
            };
            block
                .output_events
                .as_mut()
                .expect("prepared generated storage")
                .push(routed)
                .expect("whole emitted capacity and order preflight");
        }
        self.frames = self.frames.saturating_add(block.audio.frames() as u64);
        ProcessStatus::Continue
    }
    fn reset(&mut self) {
        self.frames = 0;
    }
    fn deactivate(&mut self) {
        self.config = None;
    }
}
impl<S> NativeParameterProcessor<1> for NoteRouting<S> {
    const STATE_PRODUCT: [u8; 8] = *b"RAVETNTE";
    const STATE_VERSION: u32 = 1;
    fn parameter_schema() -> ParameterSchema<'static, 1> {
        ParameterSchema::new(&PARAMETERS).expect("fixed Level schema")
    }
    fn parameter_snapshot(&self) -> ParameterState<'static, 1> {
        Self::parameter_schema()
            .state([self.level])
            .expect("validated retained Level")
    }
    fn restore_parameter_snapshot(
        &mut self,
        state: &ParameterState<'static, 1>,
    ) -> Result<(), ParameterError> {
        let value = state
            .get(LEVEL_ID)
            .ok_or(ParameterError::WrongParameter(LEVEL_ID))?;
        Self::parameter_schema().state([value])?;
        self.level = value;
        Ok(())
    }
}
impl<S> NativeAudioParameterProcessor<1> for NoteRouting<S> {
    fn audio_parameter_snapshot(&self) -> ParameterState<'static, 1> {
        self.parameter_snapshot()
    }
    fn apply_audio_parameter_snapshot(
        &mut self,
        state: &ParameterState<'static, 1>,
    ) -> Result<(), ParameterError> {
        self.restore_parameter_snapshot(state)
    }
}
impl<S> NativeTelemetryProcessor<1> for NoteRouting<S> {
    fn telemetry_descriptors() -> &'static [NativeTelemetryDescriptor; 1] {
        &TELEMETRY
    }
    fn audio_telemetry_snapshot(&self) -> [NativeTelemetryValueV1; 1] {
        [NativeTelemetryValueV1 {
            id: 1,
            flags: 0,
            value: 0.,
            samples: self.frames,
        }]
    }
}
#[allow(unsafe_code)]
mod exports {
    use super::NoteRouting;
    ravet_native::export_native_processor_v1!(
        ravet_note_routing_processor_v1,
        NoteRouting<f32>,
        NoteRouting<f64>
    );
    ravet_native::export_native_parameters_v1!(
        ravet_note_routing_parameters_v1,
        NoteRouting<f32>,
        NoteRouting<f64>,
        1
    );
    ravet_native::export_native_cached_parameters_v1!(
        ravet_note_routing_cached_parameters_v1,
        NoteRouting<f32>,
        NoteRouting<f64>,
        1
    );
    ravet_native::export_native_audio_state_v1!(
        ravet_note_routing_audio_state_v1,
        NoteRouting<f32>,
        NoteRouting<f64>,
        1
    );
    ravet_native::export_native_state_v1!(
        ravet_note_routing_state_v1,
        NoteRouting<f32>,
        NoteRouting<f64>,
        1
    );
    ravet_native::export_native_telemetry_v1!(
        ravet_note_routing_telemetry_v1,
        NoteRouting<f32>,
        NoteRouting<f64>,
        1
    );
}
pub use exports::*;

#[cfg(test)]
mod tests {
    use super::*;
    use ravet_foundation::{AudioBuffer, AudioLayout, NoteId, NotePhase, OutputEvents, Transport};
    fn config(capacity: usize) -> ProcessConfig {
        ProcessConfig::new(48000., 1024, 1024, AudioLayout::MONO_INSTRUMENT)
            .unwrap()
            .with_output_events(capacity)
    }
    fn on(offset: usize, velocity: f32) -> Event {
        Event {
            frame_offset: offset,
            data: EventData::NoteOn {
                channel: 3,
                key: 72,
                velocity,
            },
        }
    }
    fn control(offset: usize, value: f32) -> Event {
        Event {
            frame_offset: offset,
            data: EventData::ParameterValue {
                id: LEVEL_ID,
                value,
            },
        }
    }
    #[test]
    fn paired_exports_and_whole_level_recall_are_distinct_from_bus_routing() {
        for bits in [32, 64] {
            assert!(!ravet_note_routing_processor_v1(bits).is_null());
            assert!(!ravet_note_routing_parameters_v1(bits).is_null());
            assert!(!ravet_note_routing_cached_parameters_v1(bits).is_null());
            assert!(!ravet_note_routing_audio_state_v1(bits).is_null());
            assert!(!ravet_note_routing_state_v1(bits).is_null());
            assert!(!ravet_note_routing_telemetry_v1(bits).is_null());
            assert_ne!(
                ravet_note_routing_processor_v1(bits),
                crate::ravet_bus_routing_processor_v1(bits)
            );
        }
        assert!(ravet_note_routing_processor_v1(16).is_null());
        let mut p = NoteRouting::<f32> {
            frames: 17,
            ..Default::default()
        };
        let state = NoteRouting::<f32>::parameter_schema()
            .state([0.25])
            .unwrap();
        p.apply_audio_parameter_snapshot(&state).unwrap();
        assert_eq!(p.level(), 0.25);
        assert_eq!(p.processed_frames(), 17);
        assert_eq!(
            <NoteRouting<f32> as NativeParameterProcessor<1>>::STATE_PRODUCT,
            *b"RAVETNTE"
        );
    }
    #[test]
    fn timed_level_zero_on_and_exact_key_release_preserve_order_and_tail() {
        let mut p = NoteRouting::<f32>::default();
        p.prepare(config(4)).unwrap();
        let events = [
            on(0, 1.),
            control(1, 0.5),
            on(1, 1.),
            Event {
                frame_offset: 1,
                data: EventData::NoteOffScoped {
                    target: NoteTarget {
                        channel: Some(3),
                        key: Some(72),
                        id: None,
                    },
                    velocity: 0.75,
                },
            },
            on(3, 0.),
        ];
        let mut audio = [77.; 5];
        let mut outs = [&mut audio[..]];
        let mut storage = [on(0, 0.); 4];
        let mut block = ProcessBlock::new(
            AudioBuffer::separate(&[], &mut outs, 4).unwrap(),
            &events,
            Transport::default(),
        );
        block.output_events = Some(OutputEvents::new(&mut storage, 4));
        assert_eq!(p.process(&mut block), ProcessStatus::Continue);
        assert_eq!(
            block.output_events.as_ref().unwrap().as_slice(),
            &[
                on(0, 1.),
                on(1, 0.5),
                Event {
                    frame_offset: 1,
                    data: EventData::NoteOff {
                        channel: 3,
                        key: 72,
                        velocity: 0.75
                    }
                },
                on(3, 0.)
            ]
        );
        assert_eq!(audio, [0., 0., 0., 0., 77.]);
        assert_eq!(p.processed_frames(), 4);
        assert_eq!(p.level(), 0.5);
    }
    #[test]
    fn late_identity_wildcard_control_and_capacity_faults_retain_state_history() {
        let rejected = [
            EventData::NoteWithId {
                id: NoteId(0),
                phase: NotePhase::On,
                channel: 3,
                key: 72,
                velocity: 1.,
            },
            EventData::NoteOffScoped {
                target: NoteTarget {
                    channel: None,
                    key: Some(72),
                    id: None,
                },
                velocity: 1.,
            },
            EventData::NoteOffScoped {
                target: NoteTarget {
                    channel: Some(3),
                    key: Some(72),
                    id: Some(NoteId(0)),
                },
                velocity: 1.,
            },
            EventData::ParameterValue {
                id: ParameterId(7),
                value: 0.2,
            },
            EventData::ParameterValue {
                id: LEVEL_ID,
                value: 1.1,
            },
        ];
        for last in rejected {
            let mut p = NoteRouting::<f32>::default();
            p.prepare(config(2)).unwrap();
            p.frames = 13;
            let events = [
                control(0, 0.25),
                on(0, 1.),
                Event {
                    frame_offset: 3,
                    data: last,
                },
            ];
            let mut audio = [77.; 5];
            let mut outs = [&mut audio[..]];
            let mut storage = [on(0, 0.); 2];
            let mut block = ProcessBlock::new(
                AudioBuffer::separate(&[], &mut outs, 4).unwrap(),
                &events,
                Transport::default(),
            );
            block.output_events = Some(OutputEvents::new(&mut storage, 4));
            assert!(matches!(p.process(&mut block), ProcessStatus::Fault(_)));
            assert!(block.output_events.as_ref().unwrap().is_empty());
            assert_eq!(p.level(), 1.);
            assert_eq!(p.processed_frames(), 13);
            assert_eq!(audio, [0., 0., 0., 0., 77.]);
        }
        let mut p = NoteRouting::<f64>::default();
        p.prepare(config(1)).unwrap();
        let events = [control(0, 0.25), on(0, 1.), on(1, 1.)];
        let mut audio = [77.; 3];
        let mut outs = [&mut audio[..]];
        let mut storage = [on(0, 0.); 1];
        let mut block = ProcessBlock::new(
            AudioBuffer::separate(&[], &mut outs, 2).unwrap(),
            &events,
            Transport::default(),
        );
        block.output_events = Some(OutputEvents::new(&mut storage, 2));
        assert_eq!(
            p.process(&mut block),
            ProcessStatus::Fault(ProcessError::OutputEventCapacity)
        );
        assert_eq!(p.level(), 1.);
        assert_eq!(p.processed_frames(), 0);
        assert_eq!(audio, [0., 0., 77.]);
    }
    #[test]
    fn maximum_output_and_zero_frame_empty_blocks_require_exact_prepared_storage() {
        let mut p = NoteRouting::<f64>::default();
        p.prepare(config(1024)).unwrap();
        let events: Vec<_> = (0..1024).map(|i| on(i, 1.)).collect();
        let mut audio = [77.; 1025];
        let mut outs = [&mut audio[..]];
        let mut storage = [on(0, 0.); 1024];
        let mut block = ProcessBlock::new(
            AudioBuffer::separate(&[], &mut outs, 1024).unwrap(),
            &events,
            Transport::default(),
        );
        block.output_events = Some(OutputEvents::new(&mut storage, 1024));
        assert_eq!(p.process(&mut block), ProcessStatus::Continue);
        assert_eq!(
            block.output_events.as_ref().unwrap().as_slice(),
            events.as_slice()
        );
        assert_eq!(audio[1024], 77.);
        assert_eq!(p.processed_frames(), 1024);
        let mut outputs = [];
        let mut block = ProcessBlock::new(
            AudioBuffer::separate(&[], &mut outputs, 0).unwrap(),
            &[],
            Transport::default(),
        );
        // Portable layout still needs its configured channel even at zero frames.
        assert_eq!(
            p.process(&mut block),
            ProcessStatus::Fault(ProcessError::LayoutMismatch)
        );
        let mut empty = [];
        let mut outputs = [&mut empty[..]];
        let mut block = ProcessBlock::new(
            AudioBuffer::separate(&[], &mut outputs, 0).unwrap(),
            &[],
            Transport::default(),
        );
        block.output_events = Some(OutputEvents::new(&mut storage, 0));
        assert_eq!(p.process(&mut block), ProcessStatus::Continue);
        assert_eq!(p.processed_frames(), 1024);
        p.reset();
        assert_eq!(p.processed_frames(), 0);
        assert_eq!(p.prepare(config(0)), Err(PrepareError::UnsupportedLayout));
        assert_eq!(
            p.prepare(config(1025)),
            Err(PrepareError::UnsupportedLayout)
        );
    }
    #[test]
    fn format_stage_subset_vectors_are_authoritative_canonical_native_records() {
        use ravet_native::{NativeEventV1, native_event_kind};
        let words = [
            (0x7f7f8f, 3),
            (0x7f7f9f, 3),
            (0x7f7faf, 3),
            (0x7f00bf, 3),
            (0x7fcf, 2),
            (0x7fdf, 2),
            (0x7f7fef, 3),
        ];
        for (word, length) in words {
            let record = NativeEventV1 {
                kind: native_event_kind::MIDI,
                detail: length,
                words: [word, 0, 0, 0],
                ..Default::default()
            };
            let mut decoded = [on(0, 0.); 1];
            let mut encoded = [NativeEventV1::default(); 1];
            let events = ravet_native::decode_native_events(&[record], 1, &mut decoded).unwrap();
            assert_eq!(
                ravet_native::encode_native_events(events, 1, &mut encoded).unwrap(),
                &[record]
            );
        }
        for velocity in [0., f64::from(0.000001f32), 0.5, 1.] {
            let record = NativeEventV1 {
                kind: native_event_kind::NOTE_ON,
                channel: 15,
                key: 127,
                value: velocity,
                ..Default::default()
            };
            let mut decoded = [on(0, 0.); 1];
            assert!(ravet_native::decode_native_events(&[record], 1, &mut decoded).is_ok());
        }
        let lossy = NativeEventV1 {
            kind: native_event_kind::NOTE_ON,
            channel: 15,
            key: 127,
            value: 0.1,
            ..Default::default()
        };
        let mut decoded = [on(0, 0.); 1];
        assert!(ravet_native::decode_native_events(&[lossy], 1, &mut decoded).is_err());
    }
}
