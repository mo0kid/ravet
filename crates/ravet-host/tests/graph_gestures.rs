#[path = "../examples/support/parameter_fixture.rs"]
mod fixture;
use clack_host::prelude::*;
use clack_plugin::entry::SinglePluginEntry;
use ravet_foundation::{
    AudioBuffer, AudioLayout, Event, EventData, OutputEvents, ParameterGesturePhase, ParameterId,
    ProcessBlock, ProcessConfig, ProcessError, ProcessStatus, Processor, Tail, Transport,
};
use ravet_host::{
    HostRequests, HostedGraphNodeBuilder, HostedGraphNodeOptions, HostedGraphParameterMapping,
    RequestHost,
};
fn instance(mode: u8) -> PluginInstance<RequestHost> {
    fixture::set_output_mode(mode);
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<fixture::Fixture>>(c"").unwrap();
    PluginInstance::new(
        |_| HostRequests::new(),
        |_| (),
        &entry,
        c"org.ravet.host-parameter-fixture",
        &HostInfo::new("Synthetic mapped gesture host", "RAVET", "", "0.1").unwrap(),
    )
    .unwrap()
}
fn parameter(offset: usize, id: u32, value: f32) -> Event {
    Event {
        frame_offset: offset,
        data: EventData::ParameterValue {
            id: ParameterId(id),
            value,
        },
    }
}
fn gesture(offset: usize, id: u32, phase: ParameterGesturePhase) -> Event {
    Event {
        frame_offset: offset,
        data: EventData::ParameterGesture {
            id: ParameterId(id),
            phase,
        },
    }
}
fn mapping(native: u32, graph: u32) -> HostedGraphParameterMapping {
    HostedGraphParameterMapping {
        native_id: ClapId::new(native),
        graph_id: ParameterId(graph),
    }
}
macro_rules! mapped_gestures {
    ($name:ident,$sample:ty,$prepare:ident,$value:expr) => {
        #[test]
        fn $name() {
            for (mode, enabled, capacity) in [
                (40, false, 3),
                (40, true, 3),
                (40, true, 2),
                (41, true, 3),
                (42, true, 3),
                (43, true, 3),
                (43, true, 4),
            ] {
                let opts = HostedGraphNodeOptions {
                    configuration: ProcessConfig::new(48000., 8, 1, AudioLayout::STEREO)
                        .unwrap()
                        .with_output_events(capacity),
                    declared_latency: 0,
                    declared_tail: Tail::None,
                };
                let (mut control, mut node) = HostedGraphNodeBuilder::new(opts)
                    .with_parameter_input(vec![mapping(1, 9001)])
                    .with_parameter_output(vec![mapping(1, 9101), mapping(2, 9102)])
                    .with_parameter_gesture_output(enabled)
                    .$prepare(instance(mode), |_, _| ())
                    .unwrap();
                node.prepare(opts.configuration).unwrap();
                let mut node = std::thread::spawn(move || {
                    node.start_audio().unwrap();
                    let inputs_data = [$value as $sample; 8];
                    let inputs = [inputs_data.as_slice(); 2];
                    let mut output = [[0. as $sample; 8]; 2];
                    let mut storage = [parameter(0, 0, 0.); 4];
                    let bad =
                        mode == 41 || mode == 42 || (mode == 43 && capacity == 3) || capacity == 2;
                    let counts = allocation_counter::measure(|| {
                        for _ in 0..1000 {
                            let mut outputs =
                                output.each_mut().map(|samples| samples.as_mut_slice());
                            let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                            let changes = [parameter(0, 9001, 2.)];
                            let mut block =
                                ProcessBlock::new(audio, &changes, Transport::default());
                            block.output_events =
                                Some(OutputEvents::new(&mut storage[..capacity], 8));
                            let status = node.process(&mut block);
                            if bad {
                                assert_eq!(
                                    status,
                                    ProcessStatus::Fault(ProcessError::BackendFailure)
                                );
                                assert!(block.output_events.as_ref().unwrap().is_empty());
                                assert_eq!(output, [[0. as $sample; 8]; 2]);
                            } else {
                                assert_eq!(status, ProcessStatus::Continue);
                                let events = block.output_events.as_ref().unwrap().as_slice();
                                if enabled {
                                    assert_eq!(
                                        &events[..3],
                                        &[
                                            gesture(0, 9101, ParameterGesturePhase::Begin),
                                            parameter(7, 9101, 2.),
                                            gesture(7, 9101, ParameterGesturePhase::End)
                                        ]
                                    );
                                    assert_eq!(events.len(), if mode == 43 { 4 } else { 3 });
                                } else {
                                    assert_eq!(events, &[parameter(7, 9101, 2.)]);
                                }
                                assert_eq!(output, [[($value * 2.) as $sample; 8]; 2]);
                            }
                        }
                        let inputs = [&[] as &[$sample]; 2];
                        let mut a = [];
                        let mut b = [];
                        let mut outputs = [a.as_mut_slice(), b.as_mut_slice()];
                        let audio = AudioBuffer::separate(&inputs, &mut outputs, 0).unwrap();
                        let changes = [parameter(0, 9001, 4.)];
                        let mut block = ProcessBlock::new(audio, &changes, Transport::default());
                        block.output_events = Some(OutputEvents::new(&mut storage[..capacity], 0));
                        let status = node.process(&mut block);
                        let bad_flush =
                            mode == 42 || (mode == 43 && capacity == 3) || capacity == 2;
                        assert_eq!(
                            status,
                            if bad_flush {
                                ProcessStatus::Fault(ProcessError::BackendFailure)
                            } else {
                                ProcessStatus::Continue
                            }
                        );
                        if bad_flush {
                            assert!(block.output_events.as_ref().unwrap().is_empty());
                        } else if enabled {
                            assert_eq!(
                                &block.output_events.as_ref().unwrap().as_slice()[..3],
                                &[
                                    gesture(0, 9101, ParameterGesturePhase::Begin),
                                    parameter(0, 9101, 4.),
                                    gesture(0, 9101, ParameterGesturePhase::End)
                                ]
                            );
                        }
                    });
                    assert_eq!(counts.count_total, 0);
                    assert_eq!(counts.count_current, 0);
                    node.stop_audio();
                    node
                })
                .join()
                .unwrap();
                node.deactivate();
                assert!(control.reclaim());
                assert_eq!(control.statistics().parameter_flushes, 1);
                assert_eq!(
                    control.statistics().filtered_native_events,
                    if enabled { 0 } else { 2002 }
                );
                drop(node);
                let mut instance = control.into_instance().ok().unwrap();
                assert_eq!(
                    ravet_host::HostedParameters::new(&mut instance, 2)
                        .unwrap()
                        .value(ClapId::new(1))
                        .unwrap(),
                    4.
                );
            }
        }
    };
}
mapped_gestures!(
    native_f32_global_gestures_preserve_order_flush_and_nonrollback_faults,
    f32,
    prepare,
    0.25
);
mapped_gestures!(
    native_f64_global_gestures_preserve_order_flush_and_nonrollback_faults,
    f64,
    prepare64,
    1e200
);
#[test]
fn gesture_selection_without_parameter_output_mapping_rejects_before_activation() {
    let opts = HostedGraphNodeOptions {
        configuration: ProcessConfig::new(48000., 8, 0, AudioLayout::STEREO).unwrap(),
        declared_latency: 0,
        declared_tail: Tail::None,
    };
    let error = HostedGraphNodeBuilder::new(opts)
        .with_parameter_gesture_output(true)
        .prepare(instance(40), |_, _| panic!("before activation"))
        .err()
        .unwrap();
    assert!(matches!(
        error.reason,
        ravet_host::HostedGraphNodeBuildFailure::Configuration
    ));
    assert!(!error.instance.is_active());
}

macro_rules! gesture_input {
    ($name:ident,$sample:ty,$prepare:ident) => {
        #[test]
        fn $name() {
            for enabled in [false, true] {
                let opts = HostedGraphNodeOptions {
                    configuration: ProcessConfig::new(48000., 8, 3, AudioLayout::STEREO)
                        .unwrap()
                        .with_output_events(3),
                    declared_latency: 0,
                    declared_tail: Tail::None,
                };
                let (mut control, mut node) = HostedGraphNodeBuilder::new(opts)
                    .with_parameter_input(vec![mapping(1, 9001)])
                    .with_parameter_output(vec![mapping(1, 9101), mapping(2, 9102)])
                    .with_parameter_gesture_input(enabled)
                    .with_parameter_gesture_output(true)
                    .$prepare(instance(44), |_, _| ())
                    .unwrap();
                node.prepare(opts.configuration).unwrap();
                let mut node = std::thread::spawn(move || {
                    node.start_audio().unwrap();
                    let source = [0.25 as $sample; 8];
                    let inputs = [source.as_slice(); 2];
                    let mut audio_output = [[0. as $sample; 8]; 2];
                    let mut storage = [parameter(0, 0, 0.); 3];
                    let counts = allocation_counter::measure(|| {
                        for _ in 0..1000 {
                            let changes = [
                                parameter(0, 9001, 2.),
                                gesture(2, 9001, ParameterGesturePhase::Begin),
                                gesture(6, 9001, ParameterGesturePhase::End),
                            ];
                            let mut outputs = audio_output
                                .each_mut()
                                .map(|samples| samples.as_mut_slice());
                            let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                            let mut block =
                                ProcessBlock::new(audio, &changes, Transport::default());
                            block.output_events = Some(OutputEvents::new(&mut storage, 8));
                            if enabled {
                                assert_eq!(node.process(&mut block), ProcessStatus::Continue);
                                assert_eq!(
                                    block.output_events.as_ref().unwrap().as_slice(),
                                    &[
                                        gesture(2, 9101, ParameterGesturePhase::Begin),
                                        gesture(6, 9101, ParameterGesturePhase::End),
                                        parameter(7, 9102, 2.)
                                    ]
                                );
                                assert_eq!(audio_output, [[0.5 as $sample; 8]; 2]);
                            } else {
                                assert!(matches!(
                                    node.process(&mut block),
                                    ProcessStatus::Fault(ProcessError::Events(_))
                                ));
                                assert!(block.output_events.as_ref().unwrap().is_empty());
                                assert_eq!(audio_output, [[0. as $sample; 8]; 2]);
                            }
                        }
                        // Valid value prefix then an unknown boundary never reaches native.
                        let invalid = [
                            parameter(0, 9001, 4.),
                            gesture(2, 9999, ParameterGesturePhase::Begin),
                        ];
                        let mut outputs = audio_output
                            .each_mut()
                            .map(|samples| samples.as_mut_slice());
                        let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                        let mut block = ProcessBlock::new(audio, &invalid, Transport::default());
                        block.output_events = Some(OutputEvents::new(&mut storage, 8));
                        assert!(matches!(
                            node.process(&mut block),
                            ProcessStatus::Fault(ProcessError::Events(_))
                        ));
                        // Empty flush observes unchanged base; raw repeated ends are accepted.
                        let inputs = [&[] as &[$sample]; 2];
                        let mut a = [];
                        let mut b = [];
                        let mut outputs = [a.as_mut_slice(), b.as_mut_slice()];
                        let audio = AudioBuffer::separate(&inputs, &mut outputs, 0).unwrap();
                        let ends = [gesture(0, 9001, ParameterGesturePhase::End); 2];
                        let mut block = ProcessBlock::new(
                            audio,
                            if enabled { &ends } else { &[] },
                            Transport::default(),
                        );
                        block.output_events = Some(OutputEvents::new(&mut storage, 0));
                        assert_eq!(node.process(&mut block), ProcessStatus::Continue);
                        let observed = block.output_events.as_ref().unwrap().as_slice();
                        let base = if enabled { 2. } else { 1. };
                        assert_eq!(observed.last(), Some(&parameter(0, 9102, base)));
                        if enabled {
                            assert_eq!(
                                &observed[..2],
                                &[gesture(0, 9101, ParameterGesturePhase::End); 2]
                            );
                        }
                    });
                    assert_eq!(counts.count_total, 0);
                    assert_eq!(counts.count_current, 0);
                    node.stop_audio();
                    node
                })
                .join()
                .unwrap();
                node.deactivate();
                assert!(control.reclaim());
            }
        }
    };
}
gesture_input!(
    native_f32_gesture_input_echo_and_atomic_rejection,
    f32,
    prepare
);
gesture_input!(
    native_f64_gesture_input_echo_and_atomic_rejection,
    f64,
    prepare64
);

#[test]
fn gesture_input_without_mapping_rejects_before_activation() {
    let options = HostedGraphNodeOptions {
        configuration: ProcessConfig::new(48000., 8, 0, AudioLayout::STEREO).unwrap(),
        declared_latency: 0,
        declared_tail: Tail::None,
    };
    let error = HostedGraphNodeBuilder::new(options)
        .with_parameter_gesture_input(true)
        .prepare(instance(44), |_, _| ())
        .err()
        .expect("input gestures require explicit mapping");
    assert!(matches!(
        error.reason,
        ravet_host::HostedGraphNodeBuildFailure::Configuration
    ));
}
