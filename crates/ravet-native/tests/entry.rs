#![allow(unsafe_code)]
use ravet_foundation::*;
use ravet_native::*;
use std::{ffi::c_void, ptr};
#[derive(Default)]
struct Fixture {
    gain: f64,
    config: Option<ProcessConfig>,
}
fn transport() -> Transport {
    Transport {
        playing: None,
        sample_position: Some(i64::MAX),
        beat_position: Some(1e200),
        tempo: Some(1e300),
        discontinuity: true,
        details: Some(TransportDetails {
            recording: true,
            loop_active: true,
            within_pre_roll: true,
            time_signature: Some((7, 8)),
            tempo_increment: Some(-1e200),
            beat_loop: Some((-1e200, 2e200)),
            sample_loop: Some((i64::MIN, i64::MAX)),
            bar: Some((-1e200, i32::MIN)),
        }),
    }
}
impl Fixture {
    fn apply(&mut self, event: EventData) -> bool {
        if let EventData::ParameterValue { id, value } = event {
            if id.0 == 77 {
                return false;
            }
            self.gain = f64::from(value);
            if value == 11. {
                panic!("synthetic audio panic");
            }
            return value == 12.;
        }
        if let EventData::ParameterValueScoped { value, .. } = event {
            self.gain = value;
        }
        false
    }
}
macro_rules! fixture {
    ($sample:ty) => {
        impl Processor<$sample> for Fixture {
            fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
                if config.sample_rate() == 1. {
                    panic!("synthetic prepare panic");
                }
                if config.sample_rate() == 2. {
                    return Err(PrepareError::UnsupportedLayout);
                }
                self.config = Some(config);
                self.gain = 1.;
                Ok(())
            }
            fn deactivate(&mut self) {
                self.config = None;
            }
            fn reset(&mut self) {
                if self.gain == 13. {
                    panic!("synthetic reset panic");
                }
            }
            fn latency_samples(&self) -> u32 {
                37
            }
            fn tail(&self) -> Tail {
                Tail::Samples(u64::MAX)
            }
            fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, $sample>) -> ProcessStatus {
                self.config.unwrap().validate_block(block).unwrap();
                if block.events.iter().any(|e| {
                    matches!(
                        e.data,
                        EventData::ParameterValue {
                            id: ParameterId(77),
                            ..
                        }
                    )
                }) {
                    assert_eq!(block.transport, transport());
                }
                let frames = block.audio.frames();
                let mut index = 0;
                if frames == 0 {
                    for e in block.events {
                        if self.apply(e.data) {
                            return ProcessStatus::Fault(ProcessError::BackendFailure);
                        }
                    }
                }
                for frame in 0..frames {
                    while index < block.events.len() && block.events[index].frame_offset == frame {
                        if self.apply(block.events[index].data) {
                            return ProcessStatus::Fault(ProcessError::BackendFailure);
                        }
                        index += 1;
                    }
                    let input = block.audio.input(0).unwrap()[frame] as f64;
                    let buses = block.auxiliary.as_mut().unwrap();
                    let side = buses[0].input(0).unwrap()[frame] as f64;
                    block.audio.output_mut(0).unwrap()[frame] = (input * self.gain) as $sample;
                    buses[1].output_mut(0).unwrap()[frame] = side as $sample;
                }
                for e in block.events {
                    block.output_events.as_mut().unwrap().push(*e).unwrap();
                }
                ProcessStatus::Continue
            }
        }
    };
}
fixture!(f32);
fixture!(f64);
export_native_processor_v1!(ravet_test_native_processor_v1, Fixture, Fixture);
fn config() -> NativeProcessorConfigV1 {
    let mut buses = [NativeBusLayoutV1::default(); 9];
    buses[..3].copy_from_slice(&[
        NativeBusLayoutV1 {
            inputs: 1,
            outputs: 1,
        },
        NativeBusLayoutV1 {
            inputs: 1,
            outputs: 0,
        },
        NativeBusLayoutV1 {
            inputs: 0,
            outputs: 1,
        },
    ]);
    NativeProcessorConfigV1 {
        version: 1,
        maximum_frames: 8,
        maximum_events: 4,
        maximum_output_events: 4,
        bus_count: 3,
        reserved: 0,
        sample_rate: 48000.,
        buses,
    }
}
fn parameter(offset: u32, value: f32) -> NativeEventV1 {
    NativeEventV1 {
        offset,
        kind: 1,
        parameter_id: 1,
        value: f64::from(value),
        ..Default::default()
    }
}
macro_rules! cycle {
    ($name:ident,$sample:ty,$bits:expr) => {
        #[test]
        fn $name() {
            let entry = unsafe { &*ravet_test_native_processor_v1($bits) };
            assert_eq!(entry.version, 1);
            assert_eq!(entry.sample_bits, $bits);
            assert_eq!(
                entry.byte_size as usize,
                size_of::<NativeProcessorEntryV1>()
            );
            let handle = (entry.create)();
            assert!(!handle.is_null());
            let config = config();
            assert_eq!(unsafe { (entry.prepare)(handle, &config) }, 0);
            let mut timing = NativeProcessorTimingV1::default();
            assert_eq!(unsafe { (entry.timing)(handle, &mut timing) }, 0);
            assert_eq!(
                timing,
                NativeProcessorTimingV1 {
                    latency_samples: 37,
                    tail_kind: 1,
                    tail_samples: u64::MAX
                }
            );
            let mut main = [0.5 as $sample; 10];
            let mut side = [0.75 as $sample; 10];
            let input = [main.as_ptr()];
            let side_input = [side.as_ptr()];
            let main_output = [side.as_mut_ptr()];
            let side_output = [main.as_mut_ptr()];
            let buses = [
                NativeProcessorBusV1 {
                    input_channels: 1,
                    output_channels: 1,
                    inputs: input.as_ptr().cast(),
                    outputs: main_output.as_ptr().cast(),
                },
                NativeProcessorBusV1 {
                    input_channels: 1,
                    output_channels: 0,
                    inputs: side_input.as_ptr().cast(),
                    outputs: ptr::null(),
                },
                NativeProcessorBusV1 {
                    input_channels: 0,
                    output_channels: 1,
                    inputs: ptr::null(),
                    outputs: side_output.as_ptr().cast(),
                },
            ];
            let raw_transport = NativeTransportV1::from_transport(transport()).unwrap();
            let records = [
                parameter(0, 2.),
                NativeEventV1 {
                    offset: 2,
                    kind: 12,
                    parameter_id: u32::MAX,
                    note_id: u32::MAX,
                    scope: 4,
                    value: 3.,
                    ..Default::default()
                },
                NativeEventV1 {
                    offset: 5,
                    kind: 1,
                    parameter_id: 77,
                    value: 0.,
                    ..Default::default()
                },
                NativeEventV1 {
                    offset: 7,
                    kind: 10,
                    scope: 0,
                    ..Default::default()
                },
            ];
            let sentinel = NativeEventV1 {
                kind: 999,
                ..Default::default()
            };
            let mut generated = [sentinel; 5];
            let mut result = NativeProcessorResultV1 {
                status: 99,
                event_count: 99,
            };
            let heap = allocation_counter::measure(|| {
                for _ in 0..1000 {
                    main[..8].fill(0.5 as $sample);
                    side[..8].fill(0.75 as $sample);
                    assert_eq!(
                        unsafe {
                            (entry.process)(
                                handle,
                                buses.as_ptr(),
                                3,
                                8,
                                records.as_ptr(),
                                4,
                                &raw_transport,
                                generated.as_mut_ptr(),
                                4,
                                &mut result,
                            )
                        },
                        0
                    );
                    assert_eq!(
                        result,
                        NativeProcessorResultV1 {
                            status: 0,
                            event_count: 4
                        }
                    );
                    assert_eq!(generated[..4], records);
                    assert_eq!(generated[4], sentinel);
                    assert_eq!(side[..2], [1. as $sample; 2]);
                    assert_eq!(side[2..8], [1.5 as $sample; 6]);
                    assert_eq!(main[..8], [0.75 as $sample; 8]);
                    assert_eq!(main[8..], [0.5 as $sample; 2]);
                    assert_eq!(side[8..], [0.75 as $sample; 2]);
                    assert_eq!(unsafe { (entry.reset)(handle) }, 0);
                }
            });
            assert_eq!((heap.count_total, heap.bytes_total), (0, 0));
            // Fresh prepare resets fixture gain; malformed whole inputs/capacity/
            // transport must reject before a valid gain prefix reaches DSP.
            assert_eq!(unsafe { (entry.prepare)(handle, &config) }, 0);
            let invalid = [
                parameter(0, 2.),
                NativeEventV1 {
                    kind: 999,
                    ..Default::default()
                },
            ];
            generated.fill(sentinel);
            assert_eq!(
                unsafe {
                    (entry.process)(
                        handle,
                        buses.as_ptr(),
                        3,
                        8,
                        invalid.as_ptr(),
                        2,
                        ptr::null(),
                        generated.as_mut_ptr(),
                        4,
                        &mut result,
                    )
                },
                1
            );
            assert_eq!(
                result,
                NativeProcessorResultV1 {
                    status: 2,
                    event_count: 0
                }
            );
            assert_eq!(generated, [sentinel; 5]);
            assert_eq!(main[..8], [0. as $sample; 8]);
            assert_eq!(side[..8], [0. as $sample; 8]);
            assert_eq!(
                unsafe {
                    (entry.process)(
                        handle,
                        buses.as_ptr(),
                        3,
                        8,
                        records.as_ptr(),
                        1,
                        ptr::null(),
                        generated.as_mut_ptr(),
                        3,
                        &mut result,
                    )
                },
                1
            );
            let invalid_transport = NativeTransportV1 {
                tempo: -1.,
                ..Default::default()
            };
            assert_eq!(
                unsafe {
                    (entry.process)(
                        handle,
                        buses.as_ptr(),
                        3,
                        8,
                        records.as_ptr(),
                        1,
                        &invalid_transport,
                        generated.as_mut_ptr(),
                        4,
                        &mut result,
                    )
                },
                1
            );
            main[..8].fill(0.5 as $sample);
            side[..8].fill(0.75 as $sample);
            assert_eq!(
                unsafe {
                    (entry.process)(
                        handle,
                        buses.as_ptr(),
                        3,
                        8,
                        ptr::null(),
                        0,
                        ptr::null(),
                        generated.as_mut_ptr(),
                        4,
                        &mut result,
                    )
                },
                0
            );
            assert_eq!(side[..8], [0.5 as $sample; 8]);
            let zero_buses = [
                NativeProcessorBusV1 {
                    inputs: ptr::null(),
                    outputs: ptr::null(),
                    ..buses[0]
                },
                NativeProcessorBusV1 {
                    inputs: ptr::null(),
                    outputs: ptr::null(),
                    ..buses[1]
                },
                NativeProcessorBusV1 {
                    inputs: ptr::null(),
                    outputs: ptr::null(),
                    ..buses[2]
                },
            ];
            let change = [parameter(0, 5.)];
            assert_eq!(
                unsafe {
                    (entry.process)(
                        handle,
                        zero_buses.as_ptr(),
                        3,
                        0,
                        change.as_ptr(),
                        1,
                        ptr::null(),
                        generated.as_mut_ptr(),
                        4,
                        &mut result,
                    )
                },
                0
            );
            assert_eq!(generated[0], change[0]);
            main[..8].fill(1. as $sample);
            assert_eq!(
                unsafe {
                    (entry.process)(
                        handle,
                        buses.as_ptr(),
                        3,
                        8,
                        ptr::null(),
                        0,
                        ptr::null(),
                        generated.as_mut_ptr(),
                        4,
                        &mut result,
                    )
                },
                0
            );
            assert_eq!(side[..8], [5. as $sample; 8]);
            // Backend progression is retained; audio panic poisons/reset refuses.
            let backend = [parameter(0, 12.)];
            assert_eq!(
                unsafe {
                    (entry.process)(
                        handle,
                        buses.as_ptr(),
                        3,
                        8,
                        backend.as_ptr(),
                        1,
                        ptr::null(),
                        generated.as_mut_ptr(),
                        4,
                        &mut result,
                    )
                },
                1
            );
            main[..8].fill(1. as $sample);
            assert_eq!(
                unsafe {
                    (entry.process)(
                        handle,
                        buses.as_ptr(),
                        3,
                        8,
                        ptr::null(),
                        0,
                        ptr::null(),
                        generated.as_mut_ptr(),
                        4,
                        &mut result,
                    )
                },
                0
            );
            assert_eq!(side[..8], [12. as $sample; 8]);
            let panic = [parameter(0, 11.)];
            assert_eq!(
                unsafe {
                    (entry.process)(
                        handle,
                        buses.as_ptr(),
                        3,
                        8,
                        panic.as_ptr(),
                        1,
                        ptr::null(),
                        generated.as_mut_ptr(),
                        4,
                        &mut result,
                    )
                },
                2
            );
            assert_eq!(result.event_count, 0);
            assert_eq!(unsafe { (entry.reset)(handle) }, 2);
            assert_eq!(
                unsafe {
                    (entry.process)(
                        handle,
                        buses.as_ptr(),
                        3,
                        8,
                        ptr::null(),
                        0,
                        ptr::null(),
                        generated.as_mut_ptr(),
                        4,
                        &mut result,
                    )
                },
                2
            );
            assert_eq!(unsafe { (entry.prepare)(handle, &config) }, 0);
            let reset_panic = [parameter(0, 13.)];
            assert_eq!(
                unsafe {
                    (entry.process)(
                        handle,
                        zero_buses.as_ptr(),
                        3,
                        0,
                        reset_panic.as_ptr(),
                        1,
                        ptr::null(),
                        generated.as_mut_ptr(),
                        4,
                        &mut result,
                    )
                },
                0
            );
            assert_eq!(unsafe { (entry.reset)(handle) }, 2);
            assert_eq!(unsafe { (entry.prepare)(handle, &config) }, 0);
            assert_eq!(unsafe { (entry.deactivate)(handle) }, 0);
            let unchanged = timing;
            assert_eq!(unsafe { (entry.timing)(handle, &mut timing) }, 1);
            assert_eq!(timing, unchanged);
            assert_eq!(
                unsafe {
                    (entry.process)(
                        handle,
                        buses.as_ptr(),
                        3,
                        8,
                        ptr::null(),
                        0,
                        ptr::null(),
                        generated.as_mut_ptr(),
                        4,
                        &mut result,
                    )
                },
                1
            );
            unsafe { (entry.destroy)(handle) };
        }
    };
}
cycle!(
    exported_entry_f32_complete_processing_events_transport_lifecycle,
    f32,
    32
);
cycle!(
    exported_entry_f64_complete_processing_events_transport_lifecycle,
    f64,
    64
);
#[test]
fn configuration_limits_and_control_panics_are_contained_and_sticky() {
    let entry = NativeProcessor::<Fixture, f32>::entry();
    let handle = (entry.create)();
    let config = config();
    assert!(config.to_configuration().is_some());
    for bad in [
        NativeProcessorConfigV1 {
            version: 0,
            ..config
        },
        NativeProcessorConfigV1 {
            reserved: 1,
            ..config
        },
        NativeProcessorConfigV1 {
            bus_count: 0,
            ..config
        },
        NativeProcessorConfigV1 {
            bus_count: 10,
            ..config
        },
        NativeProcessorConfigV1 {
            maximum_frames: 0,
            ..config
        },
        NativeProcessorConfigV1 {
            maximum_frames: 65537,
            ..config
        },
        NativeProcessorConfigV1 {
            maximum_events: 1025,
            ..config
        },
        NativeProcessorConfigV1 {
            maximum_output_events: 1025,
            ..config
        },
        NativeProcessorConfigV1 {
            sample_rate: f64::NAN,
            ..config
        },
    ] {
        assert!(bad.to_configuration().is_none());
        assert_eq!(unsafe { (entry.prepare)(handle, &bad) }, 1);
        assert_eq!(unsafe { (entry.reset)(handle) }, 1);
    }
    let mut bad = config;
    bad.buses[3].inputs = 1;
    assert!(bad.to_configuration().is_none());
    assert_eq!(unsafe { (entry.prepare)(handle, &bad) }, 1);
    let mut bad = config;
    bad.buses[0].inputs = 65;
    assert!(bad.to_configuration().is_none());
    assert_eq!(
        unsafe {
            (entry.prepare)(
                handle,
                &NativeProcessorConfigV1 {
                    sample_rate: 2.,
                    ..config
                },
            )
        },
        1
    );
    assert_eq!(
        unsafe {
            (entry.prepare)(
                handle,
                &NativeProcessorConfigV1 {
                    sample_rate: 1.,
                    ..config
                },
            )
        },
        2
    );
    assert_eq!(unsafe { (entry.reset)(handle) }, 2);
    assert_eq!(unsafe { (entry.prepare)(handle, &config) }, 0);
    assert_eq!(unsafe { (entry.prepare)(ptr::null_mut(), &config) }, 1);
    assert_eq!(unsafe { (entry.reset)(ptr::null_mut()) }, 1);
    assert_eq!(unsafe { (entry.prepare)(handle, ptr::null()) }, 1);
    unsafe {
        (entry.destroy)(handle);
        (entry.destroy)(ptr::null_mut());
    }
    assert!(ravet_test_native_processor_v1(16).is_null());
}
#[test]
fn native_entry_record_offsets_and_sizes_match_c_contract() {
    use std::mem::{offset_of, size_of};
    assert_eq!(size_of::<NativeProcessorConfigV1>(), 104);
    assert_eq!(offset_of!(NativeProcessorConfigV1, sample_rate), 24);
    assert_eq!(offset_of!(NativeProcessorConfigV1, buses), 32);
    assert_eq!(
        size_of::<NativeProcessorEntryV1>(),
        16 + 7 * size_of::<*const c_void>()
    );
    assert_eq!(offset_of!(NativeProcessorEntryV1, create), 16);
    assert_eq!(size_of::<NativeProcessorResultV1>(), 8);
    assert_eq!(size_of::<NativeProcessorTimingV1>(), 16);
}

#[derive(Default)]
struct Instrument {
    panic_timing: bool,
}
impl Processor<f64> for Instrument {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        if config.layout() != AudioLayout::MONO_INSTRUMENT {
            return Err(PrepareError::UnsupportedLayout);
        }
        self.panic_timing = false;
        Ok(())
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {}
    fn latency_samples(&self) -> u32 {
        assert!(!self.panic_timing, "synthetic timing panic");
        u32::MAX
    }
    fn tail(&self) -> Tail {
        Tail::Infinite
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, f64>) -> ProcessStatus {
        let silent = block.events.iter().any(|e| {
            matches!(
                e.data,
                EventData::ParameterValue {
                    id: ParameterId(2),
                    value: 1.
                }
            )
        });
        if block.events.iter().any(|e| {
            matches!(
                e.data,
                EventData::ParameterValue {
                    id: ParameterId(3),
                    value: 1.
                }
            )
        }) {
            self.panic_timing = true;
        }
        let offset = block.audio.frames().saturating_sub(1);
        for e in block.events {
            if let EventData::NoteWithId {
                id, channel, key, ..
            } = e.data
            {
                block
                    .output_events
                    .as_mut()
                    .unwrap()
                    .push(Event {
                        frame_offset: offset,
                        data: EventData::NoteEnd {
                            target: NoteTarget {
                                id: Some(id),
                                channel: Some(channel),
                                key: Some(key),
                            },
                        },
                    })
                    .unwrap();
            }
        }
        block
            .audio
            .output_mut(0)
            .unwrap()
            .fill(if silent { 0. } else { 0.25 });
        if silent {
            ProcessStatus::Silence
        } else {
            ProcessStatus::Continue
        }
    }
}
#[test]
fn exported_instrument_completion_silence_timing_panic_and_foreign_rejection() {
    let entry = NativeProcessor::<Instrument, f64>::entry();
    let handle = (entry.create)();
    let mut config = config();
    config.bus_count = 1;
    config.buses.fill(NativeBusLayoutV1::default());
    config.buses[0].outputs = 1;
    config.maximum_output_events = 1;
    assert_eq!(unsafe { (entry.prepare)(handle, &config) }, 0);
    let mut timing = NativeProcessorTimingV1::default();
    assert_eq!(unsafe { (entry.timing)(handle, &mut timing) }, 0);
    assert_eq!(
        timing,
        NativeProcessorTimingV1 {
            latency_samples: u32::MAX,
            tail_kind: 2,
            tail_samples: 0
        }
    );
    let mut samples = [91.; 10];
    let pointers = [samples.as_mut_ptr()];
    let bus = NativeProcessorBusV1 {
        input_channels: 0,
        output_channels: 1,
        inputs: ptr::null(),
        outputs: pointers.as_ptr().cast(),
    };
    let input = NativeEventV1 {
        kind: 6,
        note_id: u32::MAX,
        channel: 15,
        key: 127,
        value: 0.5,
        ..Default::default()
    };
    let sentinel = NativeEventV1 {
        kind: 99,
        ..Default::default()
    };
    let mut output = [sentinel; 2];
    let mut result = NativeProcessorResultV1 {
        status: 99,
        event_count: 99,
    };
    assert_eq!(
        unsafe {
            (entry.process)(
                handle,
                &bus,
                1,
                8,
                &input,
                1,
                ptr::null(),
                output.as_mut_ptr(),
                1,
                &mut result,
            )
        },
        0
    );
    assert_eq!(
        result,
        NativeProcessorResultV1 {
            status: 0,
            event_count: 1
        }
    );
    assert_eq!(
        output[0],
        NativeEventV1 {
            offset: 7,
            kind: 10,
            note_id: u32::MAX,
            scope: 7,
            channel: 15,
            key: 127,
            ..Default::default()
        }
    );
    assert_eq!(output[1], sentinel);
    assert_eq!(samples[..8], [0.25; 8]);
    assert_eq!(samples[8..], [91.; 2]);
    output.fill(sentinel);
    assert_eq!(
        unsafe {
            (entry.process)(
                handle,
                &bus,
                1,
                8,
                ptr::null(),
                1,
                ptr::null(),
                output.as_mut_ptr(),
                1,
                &mut result,
            )
        },
        1
    );
    assert_eq!(output, [sentinel; 2]);
    assert_eq!(result.event_count, 0);
    assert_eq!(samples[..8], [0.; 8]);
    assert_eq!(
        unsafe {
            (entry.process)(
                handle,
                &bus,
                1,
                8,
                &input,
                1,
                ptr::null(),
                ptr::null_mut(),
                0,
                &mut result,
            )
        },
        1
    );
    let silent = NativeEventV1 {
        kind: 1,
        parameter_id: 2,
        value: 1.,
        ..Default::default()
    };
    assert_eq!(
        unsafe {
            (entry.process)(
                handle,
                &bus,
                1,
                8,
                &silent,
                1,
                ptr::null(),
                output.as_mut_ptr(),
                1,
                &mut result,
            )
        },
        0
    );
    assert_eq!(result.status, 1);
    assert_eq!(samples[..8], [0.; 8]);
    assert_eq!(
        unsafe {
            (entry.process)(
                ptr::null_mut(),
                &bus,
                1,
                8,
                ptr::null(),
                0,
                ptr::null(),
                output.as_mut_ptr(),
                1,
                &mut result,
            )
        },
        1
    );
    assert_eq!(
        unsafe {
            (entry.process)(
                handle,
                ptr::null(),
                10,
                8,
                ptr::null(),
                0,
                ptr::null(),
                output.as_mut_ptr(),
                1,
                &mut result,
            )
        },
        1
    );
    let panic = NativeEventV1 {
        parameter_id: 3,
        ..silent
    };
    assert_eq!(
        unsafe {
            (entry.process)(
                handle,
                &bus,
                1,
                8,
                &panic,
                1,
                ptr::null(),
                output.as_mut_ptr(),
                1,
                &mut result,
            )
        },
        0
    );
    let previous = timing;
    assert_eq!(unsafe { (entry.timing)(handle, &mut timing) }, 2);
    assert_eq!(timing, previous);
    assert_eq!(unsafe { (entry.reset)(handle) }, 2);
    assert_eq!(unsafe { (entry.prepare)(handle, &config) }, 0);
    assert_eq!(unsafe { (entry.timing)(handle, &mut timing) }, 0);
    unsafe { (entry.destroy)(handle) };
}
struct FactoryPanic;
impl Default for FactoryPanic {
    fn default() -> Self {
        panic!("synthetic factory panic")
    }
}
impl Processor<f32> for FactoryPanic {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        Ok(())
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {}
    fn process(&mut self, _: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        ProcessStatus::Continue
    }
}
#[test]
fn factory_panics_return_null_without_crossing_c_boundary() {
    assert!((NativeProcessor::<FactoryPanic, f32>::entry().create)().is_null());
}
