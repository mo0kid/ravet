#![allow(unsafe_code)]
use ravet_foundation::*;
use ravet_native::*;
const EMPTY: Event = Event {
    frame_offset: 0,
    data: EventData::ParameterValue {
        id: ParameterId(0),
        value: 0.,
    },
};
fn event(data: EventData) -> Event {
    Event {
        frame_offset: 0,
        data,
    }
}
fn corpus() -> Vec<Event> {
    let mut events = vec![
        event(EventData::ParameterValue {
            id: ParameterId(u32::MAX),
            value: f32::MAX,
        }),
        event(EventData::ParameterModulation {
            id: ParameterId(9),
            amount: -f32::MAX,
        }),
    ];
    for phase in [ParameterGesturePhase::Begin, ParameterGesturePhase::End] {
        events.push(event(EventData::ParameterGesture {
            id: ParameterId(9),
            phase,
        }));
    }
    events.push(event(EventData::NoteOn {
        channel: 15,
        key: 127,
        velocity: 0.,
    }));
    events.push(event(EventData::NoteOff {
        channel: 0,
        key: 0,
        velocity: 1.,
    }));
    for phase in [NotePhase::On, NotePhase::Off] {
        events.push(event(EventData::NoteWithId {
            id: NoteId(u32::MAX),
            phase,
            channel: 15,
            key: 127,
            velocity: 0.5,
        }));
    }
    for bits in 0..8 {
        let target = NoteTarget {
            channel: (bits & 1 != 0).then_some(15),
            key: (bits & 2 != 0).then_some(127),
            id: (bits & 4 != 0).then_some(NoteId(u32::MAX)),
        };
        events.extend([
            event(EventData::NoteOffScoped {
                target,
                velocity: 0.75,
            }),
            event(EventData::NoteChoke { target }),
            event(EventData::NoteEnd { target }),
            event(EventData::ParameterValueScoped {
                id: ParameterId(u32::MAX),
                target,
                value: 1e200,
            }),
            event(EventData::ParameterModulationScoped {
                id: ParameterId(0),
                target,
                amount: -1e200,
            }),
        ]);
        for (expression, value) in [
            (NoteExpression::Volume, 0.),
            (NoteExpression::Volume, -0.),
            (NoteExpression::Volume, f64::from_bits(1)),
            (NoteExpression::Volume, 4.),
            (NoteExpression::Pan, 0.1234567890123),
            (NoteExpression::Tuning, -119.123456789),
            (NoteExpression::Vibrato, 0.1234567890123),
            (NoteExpression::Expression, 1.),
            (NoteExpression::Brightness, 0.),
            (NoteExpression::Pressure, 0.5),
        ] {
            events.push(event(EventData::NoteExpression {
                target,
                expression,
                value,
            }));
        }
    }
    for bytes in [
        &[0xff][..],
        &[0xc7, 127][..],
        &[0x9f, 127, 0][..],
        &[0xf2, 1, 127][..],
    ] {
        events.push(event(EventData::Midi(
            MidiMessage::from_bytes(bytes).unwrap(),
        )));
    }
    // All sixteen message types, including reserved framing and arbitrary bits.
    for kind in 0..16 {
        let first = (kind << 28) | 0x0fedcba9;
        let words = [first, 0xffffffff, 0x81234567, 0x76543210];
        events.push(event(EventData::Ump(
            UmpPacket::from_words(&words[..UmpPacket::word_count(first)]).unwrap(),
        )));
    }
    events
}
#[test]
fn complete_corpus_preserves_all_scopes_scalars_ids_framing_and_equal_time_order() {
    let events = corpus();
    let tail = NativeEventV1 {
        kind: 999,
        ..Default::default()
    };
    let mut records = vec![tail; events.len() + 1];
    assert_eq!(
        encode_native_events(&events, 0, &mut records)
            .unwrap()
            .len(),
        events.len()
    );
    assert_eq!(records[events.len()], tail);
    let mut decoded = vec![EMPTY; events.len() + 1];
    assert_eq!(
        decode_native_events(&records[..events.len()], 0, &mut decoded).unwrap(),
        events
    );
    assert_eq!(decoded[events.len()], EMPTY);
    for (source, record) in events.iter().zip(&records) {
        if let EventData::NoteExpression { value, .. } = source.data {
            assert_eq!(record.value.to_bits(), value.to_bits());
            let EventData::NoteExpression {
                value: restored, ..
            } = record.to_event().unwrap().data
            else {
                panic!("wrong expression kind");
            };
            assert_eq!(restored.to_bits(), value.to_bits());
        }
    }
    let golden = NativeEventV1 {
        offset: 7,
        kind: 12,
        parameter_id: u32::MAX,
        note_id: u32::MAX,
        scope: 7,
        channel: 15,
        key: 127,
        detail: 0,
        value: 1e200,
        words: [0; 4],
    };
    assert_eq!(
        golden.to_event().unwrap(),
        Event {
            frame_offset: 7,
            data: EventData::ParameterValueScoped {
                id: ParameterId(u32::MAX),
                target: NoteTarget {
                    channel: Some(15),
                    key: Some(127),
                    id: Some(NoteId(u32::MAX))
                },
                value: 1e200
            }
        }
    );
    let midi = NativeEventV1 {
        kind: 14,
        detail: 3,
        words: [0x007f129f, 0, 0, 0],
        ..Default::default()
    };
    assert_eq!(
        midi.to_event().unwrap().data,
        EventData::Midi(MidiMessage::from_bytes(&[0x9f, 0x12, 0x7f]).unwrap())
    );
    let minus_zero = event(EventData::ParameterModulation {
        id: ParameterId(1),
        amount: -0.,
    });
    let record = NativeEventV1::from_event(minus_zero).unwrap();
    assert_eq!(record.value.to_bits(), (-0_f64).to_bits());
    if let EventData::ParameterModulation { amount, .. } = record.to_event().unwrap().data {
        assert_eq!(amount.to_bits(), (-0_f32).to_bits());
    } else {
        panic!("wrong kind");
    }
}
#[test]
fn malformed_records_reject_entire_prefix_and_destination_without_lossy_conversion() {
    let valid = NativeEventV1::from_event(event(EventData::ParameterValue {
        id: ParameterId(1),
        value: 1.,
    }))
    .unwrap();
    let mut bad = vec![
        NativeEventV1::default(),
        NativeEventV1 { kind: 16, ..valid },
        NativeEventV1 {
            value: f64::NAN,
            ..valid
        },
        NativeEventV1 {
            value: f64::INFINITY,
            ..valid
        },
        NativeEventV1 {
            value: 1e200,
            ..valid
        },
        NativeEventV1 {
            value: 0.1,
            ..valid
        },
        NativeEventV1 { scope: 1, ..valid },
        NativeEventV1 {
            channel: 256,
            ..valid
        },
        NativeEventV1 { key: 1, ..valid },
        NativeEventV1 {
            note_id: 1,
            ..valid
        },
        NativeEventV1 { detail: 1, ..valid },
        NativeEventV1 {
            words: [1, 0, 0, 0],
            ..valid
        },
    ];
    let choke = NativeEventV1::from_event(event(EventData::NoteChoke {
        target: NoteTarget::default(),
    }))
    .unwrap();
    bad.extend([
        NativeEventV1 { scope: 8, ..choke },
        NativeEventV1 {
            scope: 1,
            channel: 16,
            ..choke
        },
        NativeEventV1 {
            scope: 2,
            key: 128,
            ..choke
        },
        NativeEventV1 {
            value: -0.,
            ..choke
        },
        NativeEventV1 { value: 1., ..choke },
        NativeEventV1 {
            kind: 11,
            detail: 7,
            ..choke
        },
        NativeEventV1 {
            kind: 11,
            detail: 0,
            value: -f64::from_bits(1),
            ..choke
        },
        NativeEventV1 {
            kind: 11,
            detail: 2,
            value: 121.,
            ..choke
        },
        NativeEventV1 {
            kind: 3,
            detail: 2,
            ..Default::default()
        },
    ]);
    bad.extend([
        NativeEventV1 {
            kind: 14,
            detail: 0,
            ..Default::default()
        },
        NativeEventV1 {
            kind: 14,
            detail: 3,
            words: [0x00800090, 0, 0, 0],
            ..Default::default()
        },
        NativeEventV1 {
            kind: 14,
            detail: 1,
            words: [0x000001ff, 0, 0, 0],
            ..Default::default()
        },
        NativeEventV1 {
            kind: 15,
            detail: 2,
            words: [0x50000000, 0, 0, 0],
            ..Default::default()
        },
        NativeEventV1 {
            kind: 15,
            detail: 1,
            words: [0, 1, 0, 0],
            ..Default::default()
        },
        NativeEventV1 {
            kind: 15,
            detail: 5,
            ..Default::default()
        },
    ]);
    for record in bad {
        let mut destination = [EMPTY; 3];
        assert_eq!(
            decode_native_events(&[valid, record], 8, &mut destination),
            Err(NativeEventError::InvalidRecord { index: 1 })
        );
        assert_eq!(destination, [EMPTY; 3]);
    }
    let invalid = event(EventData::NoteOn {
        channel: 16,
        key: 0,
        velocity: 0.5,
    });
    let sentinel = NativeEventV1 {
        kind: 99,
        ..Default::default()
    };
    let mut destination = [sentinel; 3];
    assert_eq!(
        encode_native_events(&[EMPTY, invalid], 8, &mut destination),
        Err(NativeEventError::InvalidRecord { index: 1 })
    );
    assert_eq!(destination, [sentinel; 3]);
}
#[test]
fn block_bounds_order_capacity_and_legacy_parameter_preflight_are_atomic() {
    let valid = NativeEventV1::from_event(EMPTY).unwrap();
    let mut destination = [EMPTY; 3];
    for (records, frames, error) in [
        (
            [valid, NativeEventV1 { offset: 1, ..valid }],
            0,
            NativeEventError::OffsetOutOfRange { index: 1 },
        ),
        (
            [valid, NativeEventV1 { offset: 8, ..valid }],
            8,
            NativeEventError::OffsetOutOfRange { index: 1 },
        ),
        (
            [NativeEventV1 { offset: 2, ..valid }, valid],
            8,
            NativeEventError::Unordered { index: 1 },
        ),
    ] {
        assert_eq!(
            decode_native_events(&records, frames, &mut destination),
            Err(error)
        );
        assert_eq!(destination, [EMPTY; 3]);
    }
    assert_eq!(
        decode_native_events(&[valid; 4], 8, &mut destination),
        Err(NativeEventError::Capacity)
    );
    let many = vec![valid; 1025];
    let mut many_destination = vec![EMPTY; 1025];
    assert_eq!(
        decode_native_events(&many, 8, &mut many_destination),
        Err(NativeEventError::Capacity)
    );
    let legacy = NativeParameterEvent {
        offset: 0,
        id: u32::MAX,
        value: f32::MAX,
    };
    let mut legacy_destination = [EMPTY; 3];
    assert_eq!(
        decode_parameter_events(
            &[
                legacy,
                NativeParameterEvent {
                    value: f32::NAN,
                    ..legacy
                }
            ],
            0,
            &mut legacy_destination
        ),
        Err(NativeEventError::InvalidRecord { index: 1 })
    );
    assert_eq!(legacy_destination, [EMPTY; 3]);
    assert_eq!(
        decode_parameter_events(&[legacy], 0, &mut legacy_destination).unwrap(),
        [event(EventData::ParameterValue {
            id: ParameterId(u32::MAX),
            value: f32::MAX
        })]
    );
    assert_eq!(legacy_destination[1..], [EMPTY; 2]);
    let too_wide = Event {
        frame_offset: usize::MAX,
        data: EMPTY.data,
    };
    if usize::BITS > 32 {
        assert_eq!(
            NativeEventV1::from_event(too_wide),
            Err(NativeEventError::OffsetOutOfRange { index: 0 })
        );
    }
    let max = NativeEventV1 {
        offset: u32::MAX,
        ..valid
    };
    assert_eq!(max.to_event().unwrap().frame_offset, u32::MAX as usize);
    let sentinel = NativeEventV1 {
        kind: 99,
        ..Default::default()
    };
    let mut encoded = [sentinel; 3];
    assert_eq!(
        encode_native_events(
            &[
                Event {
                    frame_offset: 2,
                    ..EMPTY
                },
                EMPTY
            ],
            8,
            &mut encoded
        ),
        Err(NativeEventError::Unordered { index: 1 })
    );
    assert_eq!(encoded, [sentinel; 3]);
    assert_eq!(
        encode_native_events(
            &[Event {
                frame_offset: 1,
                ..EMPTY
            }],
            0,
            &mut encoded
        ),
        Err(NativeEventError::OffsetOutOfRange { index: 0 })
    );
    assert_eq!(encoded, [sentinel; 3]);
    assert!(
        decode_native_events(&[], 0, &mut destination)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn c_layout_and_bounded_conversion_workloads_are_heap_free() {
    use std::mem::{align_of, offset_of, size_of};
    assert_eq!(size_of::<NativeEventV1>(), 56);
    assert_eq!(align_of::<NativeEventV1>(), align_of::<f64>());
    assert_eq!(offset_of!(NativeEventV1, value), 32);
    assert_eq!(offset_of!(NativeEventV1, words), 40);
    assert_eq!(size_of::<NativeParameterEvent>(), 12);
    assert_eq!(offset_of!(NativeParameterEvent, value), 8);
    let corpus = corpus();
    let mut events = vec![EMPTY; 1024];
    for (out, input) in events.iter_mut().zip(corpus.iter().cycle()) {
        *out = *input;
    }
    let mut records = vec![NativeEventV1::default(); 1025];
    let mut decoded = vec![EMPTY; 1025];
    let legacy = [NativeParameterEvent {
        offset: 0,
        id: 1,
        value: 1.,
    }; 1024];
    let info = allocation_counter::measure(|| {
        for _ in 0..1000 {
            encode_native_events(&events, 0, &mut records).unwrap();
            assert_eq!(
                decode_native_events(&records[..1024], 0, &mut decoded).unwrap(),
                events
            );
            decode_parameter_events(&legacy, 0, &mut decoded).unwrap();
            let rejected = [
                records[0],
                NativeEventV1 {
                    kind: 999,
                    ..records[1]
                },
            ];
            assert!(decode_native_events(&rejected, 0, &mut decoded).is_err());
        }
    });
    assert_eq!(
        (info.count_total, info.count_max, info.bytes_total),
        (0, 0, 0)
    );
}
#[derive(Default)]
struct Echo {
    calls: usize,
}
macro_rules! native_cycle {
    ($name:ident,$sample:ty) => {
        impl Processor<$sample> for Echo {
            fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
                Ok(())
            }
            fn reset(&mut self) {}
            fn deactivate(&mut self) {}
            fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, $sample>) -> ProcessStatus {
                self.calls += 1;
                for event in block.events {
                    block.output_events.as_mut().unwrap().push(*event).unwrap();
                }
                for sample in block.audio.output_mut(0).unwrap() {
                    *sample = 0.25;
                }
                ProcessStatus::Continue
            }
        }
        #[test]
        fn $name() {
            let mut processor = Echo::default();
            let config = ProcessConfig::new(48000., 8, 1024, AudioLayout::MONO_INSTRUMENT)
                .unwrap()
                .with_output_events(1024);
            let mut adapter =
                NativeAudioAdapter::<$sample>::prepare(&mut processor, config).unwrap();
            let mut events = corpus();
            let count = events.len();
            for (index, event) in events.iter_mut().enumerate() {
                event.frame_offset = index * 8 / count;
            }
            let mut records = vec![NativeEventV1::default(); events.len()];
            encode_native_events(&events, 8, &mut records).unwrap();
            let mut decoded = vec![EMPTY; events.len()];
            let mut encoded = vec![NativeEventV1::default(); events.len()];
            let mut samples = [91. as $sample; 10];
            let outputs = [samples.as_mut_ptr()];
            let bus = NativePlanarBus {
                input_channels: 0,
                output_channels: 1,
                inputs: std::ptr::null(),
                outputs: outputs.as_ptr(),
            };
            let info = allocation_counter::measure(|| {
                for _ in 0..1000 {
                    let input = decode_native_events(&records, 8, &mut decoded).unwrap();
                    let report = unsafe {
                        adapter.process_native(
                            &mut processor,
                            &[bus],
                            8,
                            input,
                            Transport::default(),
                        )
                    };
                    assert_eq!(report.status, ProcessStatus::Continue);
                    assert_eq!(
                        encode_native_events(report.events, 8, &mut encoded).unwrap(),
                        records
                    );
                    assert_eq!(samples[..8], [0.25 as $sample; 8]);
                    assert_eq!(samples[8..], [91. as $sample; 2]);
                }
            });
            assert_eq!((info.count_total, info.bytes_total), (0, 0));
            assert_eq!(processor.calls, 1000);
        }
    };
}
native_cycle!(native_record_f32_instrument_roundtrip, f32);
native_cycle!(native_record_f64_instrument_roundtrip, f64);
