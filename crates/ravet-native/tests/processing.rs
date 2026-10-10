#![allow(unsafe_code)]
use ravet_foundation::{
    AudioLayout, Event, EventData, ParameterId, PrepareError, ProcessBlock, ProcessConfig,
    ProcessError, ProcessStatus, Processor, Transport,
};
use ravet_native::{NativeAudioAdapter, NativePlanarBus, NativePrepareError};
use std::ptr;
#[derive(Default)]
struct Fixture {
    config: Option<ProcessConfig>,
    gain: f64,
    calls: usize,
    transport: Option<Transport>,
    panic: bool,
    fault: bool,
    refuse: bool,
}
macro_rules! fixture {
    ($sample:ty) => {
        impl Processor<$sample> for Fixture {
            fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
                if self.refuse {
                    return Err(PrepareError::UnsupportedLayout);
                }
                self.config = Some(config);
                Ok(())
            }
            fn deactivate(&mut self) {
                self.config = None;
            }
            fn reset(&mut self) {}
            fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, $sample>) -> ProcessStatus {
                self.config.unwrap().validate_block(block).unwrap();
                self.calls += 1;
                self.transport = Some(block.transport);
                if self.panic {
                    self.panic = false;
                    self.gain = 3.;
                    panic!("synthetic processor panic");
                }
                if self.fault {
                    self.fault = false;
                    self.gain = 3.;
                    return ProcessStatus::Fault(ProcessError::BackendFailure);
                }
                let frames = block.audio.frames();
                let mut index = 0;
                for frame in 0..frames {
                    while index < block.events.len() && block.events[index].frame_offset == frame {
                        if let EventData::ParameterValue { value, .. } = block.events[index].data {
                            self.gain = f64::from(value);
                        }
                        index += 1;
                    }
                    let left = block.audio.input(0).unwrap()[frame] as f64;
                    let right = block.audio.input(1).unwrap()[frame] as f64;
                    let buses = block.auxiliary.as_mut().unwrap();
                    let side = buses[0].input(0).unwrap()[frame] as f64;
                    block.audio.output_mut(0).unwrap()[frame] =
                        (left * self.gain + side * 0.125) as $sample;
                    block.audio.output_mut(1).unwrap()[frame] = (right - side * 0.25) as $sample;
                    buses[1].output_mut(0).unwrap()[frame] = side as $sample;
                }
                if frames == 0 {
                    for event in block.events {
                        if let EventData::ParameterValue { value, .. } = event.data {
                            self.gain = f64::from(value);
                        }
                    }
                }
                if let Some(output) = &mut block.output_events {
                    for event in block.events {
                        output.push(*event).unwrap();
                    }
                }
                ProcessStatus::Continue
            }
        }
    };
}
fixture!(f32);
fixture!(f64);
fn config() -> ProcessConfig {
    ProcessConfig::new(48000., 8, 2, AudioLayout::STEREO)
        .unwrap()
        .with_auxiliary_buses(&[
            AudioLayout::new(1, 0).unwrap(),
            AudioLayout::new(0, 1).unwrap(),
        ])
        .unwrap()
        .with_output_events(2)
}
fn events() -> [Event; 2] {
    [
        Event {
            frame_offset: 0,
            data: EventData::ParameterValue {
                id: ParameterId(7),
                value: 1.,
            },
        },
        Event {
            frame_offset: 2,
            data: EventData::ParameterValue {
                id: ParameterId(7),
                value: 2.,
            },
        },
    ]
}
macro_rules! processing {
    ($name:ident,$sample:ty) => {
        #[test]
        fn $name() {
            let mut processor = Fixture::default();
            let mut adapter =
                NativeAudioAdapter::<$sample>::prepare(&mut processor, config()).unwrap();
            let mut a = [0.25 as $sample; 10];
            let mut b = [0.5 as $sample; 10];
            let mut side = [0.75 as $sample; 10];
            let input = [a.as_ptr(), b.as_ptr()];
            let side_input = [side.as_ptr()];
            // Every output aliases an input on another channel/bus.
            let output = [side.as_mut_ptr(), a.as_mut_ptr()];
            let aux_out = [b.as_mut_ptr()];
            let buses = [
                NativePlanarBus {
                    input_channels: 2,
                    output_channels: 2,
                    inputs: input.as_ptr(),
                    outputs: output.as_ptr(),
                },
                NativePlanarBus {
                    input_channels: 1,
                    output_channels: 0,
                    inputs: side_input.as_ptr(),
                    outputs: ptr::null(),
                },
                NativePlanarBus {
                    input_channels: 0,
                    output_channels: 1,
                    inputs: ptr::null(),
                    outputs: aux_out.as_ptr(),
                },
            ];
            let transport = Transport {
                sample_position: Some(i64::MAX),
                beat_position: Some(123.45678912345678),
                tempo: Some(120.5),
                playing: None,
                discontinuity: true,
                ..Transport::default()
            };
            let changes = events();
            let counts = allocation_counter::measure(|| {
                for _ in 0..1000 {
                    a.fill(0.25 as $sample);
                    b.fill(0.5 as $sample);
                    side.fill(0.75 as $sample);
                    let report = unsafe {
                        adapter.process_native(&mut processor, &buses, 8, &changes, transport)
                    };
                    assert_eq!(report.status, ProcessStatus::Continue);
                    assert_eq!(report.events, &changes);
                    assert_eq!(processor.transport, Some(transport));
                    assert_eq!(&side[..2], &[0.34375 as $sample; 2]);
                    assert_eq!(&side[2..8], &[0.59375 as $sample; 6]);
                    assert_eq!(&a[..8], &[0.3125 as $sample; 8]);
                    assert_eq!(&b[..8], &[0.75 as $sample; 8]);
                    assert_eq!(&a[8..], &[0.25 as $sample; 2]);
                    assert_eq!(&b[8..], &[0.5 as $sample; 2]);
                    assert_eq!(&side[8..], &[0.75 as $sample; 2]);
                    adapter.reset(&mut processor).unwrap();
                }
                let calls = processor.calls;
                let invalid = [
                    changes[0],
                    Event {
                        data: EventData::ParameterValue {
                            id: ParameterId(7),
                            value: f32::NAN,
                        },
                        ..changes[1]
                    },
                ];
                let report = unsafe {
                    adapter.process_native(&mut processor, &buses, 8, &invalid, transport)
                };
                assert!(matches!(
                    report.status,
                    ProcessStatus::Fault(ProcessError::Events(_))
                ));
                assert!(report.events.is_empty());
                assert_eq!(processor.calls, calls);
                assert_eq!(processor.gain, 2.);
                assert_eq!(&side[..8], &[0. as $sample; 8]);
                assert_eq!(&a[..8], &[0. as $sample; 8]);
                assert_eq!(&b[..8], &[0. as $sample; 8]);
                let zero = buses.map(|bus| NativePlanarBus {
                    inputs: ptr::null(),
                    outputs: ptr::null(),
                    ..bus
                });
                let flush = [changes[0]];
                let report = unsafe {
                    adapter.process_native(&mut processor, &zero, 0, &flush, Transport::default())
                };
                assert_eq!(report.status, ProcessStatus::Continue);
                assert_eq!(report.events, &flush);
                assert_eq!(processor.gain, 1.);
            });
            assert_eq!(counts.count_total, 0);
            assert_eq!(counts.count_current, 0);
        }
    };
}
processing!(
    native_f32_cross_bus_aliases_events_transport_zero_frame_and_heap,
    f32
);
processing!(
    native_f64_cross_bus_aliases_events_transport_zero_frame_and_heap,
    f64
);
#[test]
fn bad_shapes_pointers_overlap_and_backend_fault_preserve_contracts() {
    let mut processor = Fixture::default();
    let mut adapter = NativeAudioAdapter::<f64>::prepare(&mut processor, config()).unwrap();
    let a = [0.25; 10];
    let b = [0.5; 10];
    let side = [0.75; 10];
    let input = [a.as_ptr(), b.as_ptr()];
    let side_input = [side.as_ptr()];
    let mut y0 = [91.; 10];
    let mut y1 = [92.; 10];
    let mut y2 = [93.; 10];
    let output = [y0.as_mut_ptr(), y1.as_mut_ptr()];
    let aux_out = [y2.as_mut_ptr()];
    let buses = [
        NativePlanarBus {
            input_channels: 2,
            output_channels: 2,
            inputs: input.as_ptr(),
            outputs: output.as_ptr(),
        },
        NativePlanarBus {
            input_channels: 1,
            output_channels: 0,
            inputs: side_input.as_ptr(),
            outputs: ptr::null(),
        },
        NativePlanarBus {
            input_channels: 0,
            output_channels: 1,
            inputs: ptr::null(),
            outputs: aux_out.as_ptr(),
        },
    ];
    let mut invalid = buses;
    invalid[1].input_channels = 0;
    assert_eq!(
        unsafe {
            adapter.process_native(&mut processor, &invalid, 8, &events(), Transport::default())
        }
        .status,
        ProcessStatus::Fault(ProcessError::LayoutMismatch)
    );
    assert_eq!(&y0[..8], &[0.; 8]);
    assert_eq!(&y1[..8], &[0.; 8]);
    assert_eq!(&y2[..8], &[0.; 8]);
    assert_eq!(processor.calls, 0);
    invalid = buses;
    invalid[1].inputs = ptr::null();
    assert_eq!(
        unsafe { adapter.process_native(&mut processor, &invalid, 8, &[], Transport::default()) }
            .status,
        ProcessStatus::Fault(ProcessError::LayoutMismatch)
    );
    assert_eq!(processor.calls, 0);
    let overlap = [y0.as_mut_ptr(), unsafe { y0.as_mut_ptr().add(1) }];
    invalid = buses;
    invalid[0].outputs = overlap.as_ptr();
    assert_eq!(
        unsafe { adapter.process_native(&mut processor, &invalid, 8, &[], Transport::default()) }
            .status,
        ProcessStatus::Fault(ProcessError::LayoutMismatch)
    );
    assert_eq!(processor.calls, 0);
    processor.fault = true;
    let report = unsafe {
        adapter.process_native(&mut processor, &buses, 8, &events(), Transport::default())
    };
    assert_eq!(
        report.status,
        ProcessStatus::Fault(ProcessError::BackendFailure)
    );
    assert!(report.events.is_empty());
    assert_eq!(processor.gain, 3.);
    assert!(!adapter.is_poisoned());
    processor.panic = true;
    assert_eq!(
        unsafe { adapter.process_native(&mut processor, &buses, 8, &[], Transport::default()) }
            .status,
        ProcessStatus::Fault(ProcessError::BackendFailure)
    );
    assert!(adapter.is_poisoned());
    let calls = processor.calls;
    assert_eq!(
        adapter.reset(&mut processor),
        Err(ProcessError::BackendFailure)
    );
    assert_eq!(
        unsafe { adapter.process_native(&mut processor, &buses, 8, &[], Transport::default()) }
            .status,
        ProcessStatus::Fault(ProcessError::BackendFailure)
    );
    assert_eq!(processor.calls, calls);
    let mut fresh = NativeAudioAdapter::<f64>::prepare(&mut processor, config()).unwrap();
    assert_eq!(
        unsafe { fresh.process_native(&mut processor, &buses, 8, &events(), Transport::default()) }
            .status,
        ProcessStatus::Continue
    );
    assert_eq!(y0[8], 0.);
    assert_eq!(y0[9], 91.);
    assert_eq!(&y1[8..], &[92.; 2]);
    assert_eq!(&y2[8..], &[93.; 2]);
}
#[test]
fn preparation_caps_reject_before_allocation_and_processor_refusal_is_inactive() {
    let mut processor = Fixture::default();
    let large = ProcessConfig::new(48000., 65536, 0, AudioLayout::new(64, 64).unwrap())
        .unwrap()
        .with_auxiliary_buses(&[AudioLayout::new(64, 64).unwrap()])
        .unwrap();
    let invalid = [
        ProcessConfig::new(48000., usize::MAX, 0, AudioLayout::STEREO).unwrap(),
        ProcessConfig::new(48000., 8, usize::MAX, AudioLayout::STEREO).unwrap(),
        ProcessConfig::new(48000., 8, 0, AudioLayout::STEREO)
            .unwrap()
            .with_output_events(1025),
        large,
    ];
    let counts = allocation_counter::measure(|| {
        for config in invalid {
            assert!(matches!(
                NativeAudioAdapter::<f64>::prepare(&mut processor, config),
                Err(NativePrepareError::Capacity)
            ));
            assert!(processor.config.is_none());
        }
    });
    assert_eq!(counts.count_total, 0);
    processor.refuse = true;
    assert!(matches!(
        NativeAudioAdapter::<f64>::prepare(&mut processor, config()),
        Err(NativePrepareError::Processor(
            PrepareError::UnsupportedLayout
        ))
    ));
    assert!(processor.config.is_none());
}
#[test]
fn native_f64_wide_audio_scoped_events_and_extended_transport_are_exact() {
    use ravet_foundation::{NoteExpression, NoteId, NoteTarget, TransportDetails};
    let mut processor = Fixture {
        gain: 1.,
        ..Fixture::default()
    };
    let mut adapter = NativeAudioAdapter::<f64>::prepare(&mut processor, config()).unwrap();
    let x = [1e200; 8];
    let r = [0.5; 8];
    let side = [0.; 8];
    let input = [x.as_ptr(), r.as_ptr()];
    let aux_input = [side.as_ptr()];
    let mut a = [0.; 8];
    let mut b = [0.; 8];
    let mut c = [0.; 8];
    let output = [a.as_mut_ptr(), b.as_mut_ptr()];
    let aux_output = [c.as_mut_ptr()];
    let buses = [
        NativePlanarBus {
            input_channels: 2,
            output_channels: 2,
            inputs: input.as_ptr(),
            outputs: output.as_ptr(),
        },
        NativePlanarBus {
            input_channels: 1,
            output_channels: 0,
            inputs: aux_input.as_ptr(),
            outputs: ptr::null(),
        },
        NativePlanarBus {
            input_channels: 0,
            output_channels: 1,
            inputs: ptr::null(),
            outputs: aux_output.as_ptr(),
        },
    ];
    let target = NoteTarget {
        channel: Some(15),
        key: Some(127),
        id: Some(NoteId(u32::MAX)),
    };
    let events = [
        Event {
            frame_offset: 0,
            data: EventData::ParameterValueScoped {
                id: ParameterId(u32::MAX),
                target,
                value: 1e200,
            },
        },
        Event {
            frame_offset: 2,
            data: EventData::NoteExpression {
                target,
                expression: NoteExpression::Tuning,
                value: 0.12345678912345678,
            },
        },
    ];
    let transport = Transport {
        playing: Some(true),
        sample_position: Some(i64::MAX),
        beat_position: Some(12.5),
        tempo: Some(123.5),
        discontinuity: true,
        details: Some(TransportDetails {
            recording: true,
            loop_active: true,
            within_pre_roll: true,
            time_signature: Some((7, 8)),
            tempo_increment: Some(0.001),
            beat_loop: Some((8., 16.)),
            sample_loop: Some((100, 1000)),
            bar: Some((-4., -2)),
        }),
    };
    let report = unsafe { adapter.process_native(&mut processor, &buses, 8, &events, transport) };
    assert_eq!(report.status, ProcessStatus::Continue);
    assert_eq!(report.events, &events);
    assert_eq!(processor.transport, Some(transport));
    assert_eq!(a, [1e200; 8]);
    assert_eq!(b, [0.5; 8]);
    assert_eq!(c, [0.; 8]);
}
#[test]
fn instrument_without_inputs_generates_completion_and_initializes_every_output() {
    struct Instrument(Option<ProcessConfig>);
    impl Processor for Instrument {
        fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
            self.0 = Some(config);
            Ok(())
        }
        fn deactivate(&mut self) {
            self.0 = None;
        }
        fn reset(&mut self) {}
        fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
            self.0.unwrap().validate_block(block).unwrap();
            assert_eq!(block.audio.input_channels(), 0);
            block.audio.output_mut(0).unwrap().fill(0.25);
            block
                .output_events
                .as_mut()
                .unwrap()
                .push(Event {
                    frame_offset: 0,
                    data: EventData::NoteEnd {
                        target: ravet_foundation::NoteTarget::default(),
                    },
                })
                .unwrap();
            ProcessStatus::Continue
        }
    }
    let mut processor = Instrument(None);
    let config = ProcessConfig::new(48000., 8, 0, AudioLayout::MONO_INSTRUMENT)
        .unwrap()
        .with_output_events(1);
    let mut adapter = NativeAudioAdapter::<f32>::prepare(&mut processor, config).unwrap();
    let mut y = [91.; 10];
    let output = [y.as_mut_ptr()];
    let bus = NativePlanarBus {
        input_channels: 0,
        output_channels: 1,
        inputs: ptr::null(),
        outputs: output.as_ptr(),
    };
    let report =
        unsafe { adapter.process_native(&mut processor, &[bus], 8, &[], Transport::default()) };
    assert_eq!(report.status, ProcessStatus::Continue);
    assert_eq!(report.events.len(), 1);
    assert!(matches!(report.events[0].data, EventData::NoteEnd { .. }));
    assert_eq!(&y[..8], &[0.25; 8]);
    assert_eq!(&y[8..], &[91.; 2]);
}
