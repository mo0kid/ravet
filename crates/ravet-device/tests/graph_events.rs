use ravet_device::{GraphEventQueueConfig, GraphEventQueueError, graph_event_queue};
use ravet_foundation::{
    Event, EventData, EventError, MidiMessage, ParameterId, Transport, UmpPacket,
};
fn config() -> GraphEventQueueConfig {
    GraphEventQueueConfig {
        block_frames: 8,
        event_capacity: 4,
        queued_batches: 2,
    }
}
fn event(frame_offset: usize, value: f32) -> Event {
    Event {
        frame_offset,
        data: EventData::ParameterValue {
            id: ParameterId(12),
            value,
        },
    }
}
#[test]
fn complete_batches_retain_until_acknowledged_and_full_publish_has_no_prefix() {
    let (mut producer, mut consumer) = graph_event_queue(config()).unwrap();
    let events = [
        event(0, 2.),
        Event {
            frame_offset: 2,
            data: EventData::NoteOn {
                channel: 2,
                key: 60,
                velocity: 0.5,
            },
        },
        Event {
            frame_offset: 2,
            data: EventData::Midi(MidiMessage::from_bytes(&[0xC2, 7]).unwrap()),
        },
        Event {
            frame_offset: 7,
            data: EventData::Ump(UmpPacket::from_words(&[0x42903C00, 0x80000000]).unwrap()),
        },
    ];
    let context = Transport {
        playing: Some(true),
        sample_position: Some(-32),
        beat_position: Some(-0.5),
        tempo: Some(123.5),
        discontinuity: true,
        details: Some(ravet_foundation::TransportDetails {
            recording: true,
            time_signature: Some((7, 8)),
            ..ravet_foundation::TransportDetails::default()
        }),
    };
    producer.try_publish(1, context, &events).unwrap();
    producer.try_publish(2, Transport::default(), &[]).unwrap();
    assert_eq!(
        producer.try_publish(3, context, &events),
        Err(GraphEventQueueError::Full)
    );
    {
        let batch = consumer.next_batch().unwrap();
        assert_eq!(batch.tag(), 1);
        assert_eq!(batch.transport(), context);
        assert_eq!(batch.events(), &events);
    }
    producer.try_publish(3, context, &events).unwrap();
    let retained = consumer.next_batch().unwrap();
    assert_eq!(retained.tag(), 1);
    assert_eq!(retained.events(), &events);
    retained.acknowledge();
    let next = consumer.next_batch().unwrap();
    assert_eq!(next.tag(), 2);
    assert!(next.events().is_empty());
    next.acknowledge();
    let next = consumer.next_batch().unwrap();
    assert_eq!(next.tag(), 3);
    assert_eq!(next.events(), &events);
    next.acknowledge();
    assert!(consumer.next_batch().is_none());
    drop(producer);
    assert!(consumer.producer_disconnected());
}

#[test]
fn gesture_boundaries_preserve_identity_phase_offsets_and_order() {
    use ravet_foundation::ParameterGesturePhase;
    let (mut producer, mut consumer) = graph_event_queue(config()).unwrap();
    let events = [
        Event {
            frame_offset: 0,
            data: EventData::ParameterGesture {
                id: ParameterId(12),
                phase: ParameterGesturePhase::Begin,
            },
        },
        event(3, 2.),
        Event {
            frame_offset: 3,
            data: EventData::ParameterGesture {
                id: ParameterId(12),
                phase: ParameterGesturePhase::End,
            },
        },
    ];
    producer
        .try_publish(7, Transport::default(), &events)
        .unwrap();
    let batch = consumer.next_batch().unwrap();
    assert_eq!(batch.events(), &events);
    batch.acknowledge();
}
#[test]
fn invalid_streams_and_disconnected_consumer_never_publish() {
    let (mut producer, mut consumer) = graph_event_queue(config()).unwrap();
    for (events, error) in [
        (
            vec![event(0, 1.); 5],
            EventError::TooMany {
                count: 5,
                maximum: 4,
            },
        ),
        (
            vec![event(0, 1.), event(8, 2.)],
            EventError::OffsetOutOfRange {
                index: 1,
                offset: 8,
                frames: 8,
            },
        ),
        (
            vec![event(3, 1.), event(2, 2.)],
            EventError::Unordered { index: 1 },
        ),
        (
            vec![event(0, 1.), event(1, f32::NAN)],
            EventError::InvalidValue { index: 1 },
        ),
        (
            vec![
                event(0, 1.),
                Event {
                    frame_offset: 1,
                    data: EventData::NoteOff {
                        channel: 16,
                        key: 60,
                        velocity: 0.5,
                    },
                },
            ],
            EventError::InvalidValue { index: 1 },
        ),
    ] {
        assert_eq!(
            producer.try_publish(1, Transport::default(), &events),
            Err(GraphEventQueueError::Events(error))
        );
        assert!(consumer.next_batch().is_none());
    }
    producer
        .try_publish(4, Transport::default(), &[event(0, 2.)])
        .unwrap();
    consumer.next_batch().unwrap().acknowledge();
    drop(consumer);
    assert_eq!(
        producer.try_publish(5, Transport::default(), &[]),
        Err(GraphEventQueueError::Disconnected)
    );
    for bad in [
        GraphEventQueueConfig {
            block_frames: 0,
            ..config()
        },
        GraphEventQueueConfig {
            block_frames: 65537,
            ..config()
        },
        GraphEventQueueConfig {
            event_capacity: 4097,
            ..config()
        },
        GraphEventQueueConfig {
            queued_batches: 0,
            ..config()
        },
        GraphEventQueueConfig {
            queued_batches: 65,
            ..config()
        },
    ] {
        assert!(matches!(
            graph_event_queue(bad),
            Err(GraphEventQueueError::Configuration)
        ));
    }
}
#[test]
fn publication_dequeue_and_acknowledgement_are_heap_free_across_worker_threads() {
    let (mut producer, mut consumer) = graph_event_queue(config()).unwrap();
    let worker = std::thread::spawn(move || {
        let counts = allocation_counter::measure(|| {
            for tag in 0..10000 {
                loop {
                    let Some(batch) = consumer.next_batch() else {
                        std::thread::yield_now();
                        continue;
                    };
                    assert_eq!(batch.tag(), tag);
                    assert_eq!(batch.events(), &[event(3, tag as f32)]);
                    batch.acknowledge();
                    break;
                }
            }
        });
        assert_eq!(counts.count_total, 0);
        assert_eq!(counts.count_current, 0);
        consumer
    });
    let counts = allocation_counter::measure(|| {
        for tag in 0..10000 {
            loop {
                match producer.try_publish(tag, Transport::default(), &[event(3, tag as f32)]) {
                    Ok(()) => break,
                    Err(GraphEventQueueError::Full) => std::thread::yield_now(),
                    Err(error) => panic!("queue: {error:?}"),
                }
            }
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    let mut consumer = worker.join().unwrap();
    assert!(consumer.next_batch().is_none());
}

#[test]
fn modulation_batches_preserve_amounts_and_reject_nonfinite_whole_streams() {
    let (mut producer, mut consumer) = graph_event_queue(config()).unwrap();
    let modulation = |amount| Event {
        frame_offset: 3,
        data: EventData::ParameterModulation {
            id: ParameterId(12),
            amount,
        },
    };
    for amount in [f32::NAN, f32::INFINITY] {
        assert!(matches!(
            producer.try_publish(1, Transport::default(), &[event(0, 1.), modulation(amount)]),
            Err(GraphEventQueueError::Events(EventError::InvalidValue {
                index: 1
            }))
        ));
        assert!(consumer.next_batch().is_none());
    }
    let events = [event(0, 2.), modulation(-3.), modulation(0.25)];
    producer
        .try_publish(2, Transport::default(), &events)
        .unwrap();
    let batch = consumer.next_batch().unwrap();
    assert_eq!(batch.events(), &events);
    batch.acknowledge();
}

#[test]
fn explicit_note_identity_phase_and_velocity_roundtrip_without_quantization() {
    use ravet_foundation::{NoteId, NotePhase};
    let (mut producer, mut consumer) = graph_event_queue(config()).unwrap();
    let events = [NotePhase::On, NotePhase::Off].map(|phase| Event {
        frame_offset: 2,
        data: EventData::NoteWithId {
            id: NoteId(u32::MAX),
            phase,
            channel: 15,
            key: 127,
            velocity: 0.123456,
        },
    });
    let counts = allocation_counter::measure(|| {
        for tag in 0..10000 {
            producer
                .try_publish(tag, Transport::default(), &events)
                .unwrap();
            let batch = consumer.next_batch().unwrap();
            assert_eq!(batch.events(), &events);
            batch.acknowledge();
        }
    });
    assert_eq!(counts.count_total, 0);
}

#[test]
fn expression_batches_preserve_f64_scope_and_reject_invalid_whole_prefixes() {
    use ravet_foundation::{NoteExpression, NoteId, NoteTarget};
    let (mut producer, mut consumer) = graph_event_queue(config()).unwrap();
    let events = [Event {
        frame_offset: 2,
        data: EventData::NoteExpression {
            target: NoteTarget {
                channel: Some(2),
                key: None,
                id: Some(NoteId(u32::MAX)),
            },
            expression: NoteExpression::Pressure,
            value: 0.12345678912345678,
        },
    }];
    let counts = allocation_counter::measure(|| {
        for tag in 0..10000 {
            producer
                .try_publish(tag, Transport::default(), &events)
                .unwrap();
            let batch = consumer.next_batch().unwrap();
            assert_eq!(batch.events(), &events);
            batch.acknowledge();
        }
        let invalid = [
            events[0],
            Event {
                frame_offset: 3,
                data: EventData::NoteExpression {
                    target: NoteTarget::default(),
                    expression: NoteExpression::Volume,
                    value: -f64::from_bits(1),
                },
            },
        ];
        assert!(matches!(
            producer.try_publish(10000, Transport::default(), &invalid),
            Err(GraphEventQueueError::Events(EventError::InvalidValue {
                index: 1
            }))
        ));
        assert!(consumer.next_batch().is_none());
    });
    assert_eq!(counts.count_total, 0);
}

#[test]
fn zero_volume_mute_and_subnormal_survive_complete_portable_handoff() {
    use ravet_foundation::{NoteExpression, NoteId, NoteTarget};
    let (mut producer, mut consumer) = graph_event_queue(config()).unwrap();
    for value in [0_f64, -0., f64::from_bits(1)] {
        let events = [Event {
            frame_offset: 2,
            data: EventData::NoteExpression {
                target: NoteTarget {
                    id: Some(NoteId(u32::MAX)),
                    ..Default::default()
                },
                expression: NoteExpression::Volume,
                value,
            },
        }];
        let counts = allocation_counter::measure(|| {
            for tag in 0..1000 {
                producer
                    .try_publish(tag, Transport::default(), &events)
                    .unwrap();
                let batch = consumer.next_batch().unwrap();
                let EventData::NoteExpression {
                    value: restored,
                    target,
                    ..
                } = batch.events()[0].data
                else {
                    panic!("wrong expression kind");
                };
                assert_eq!(restored.to_bits(), value.to_bits());
                assert_eq!(target.id, Some(NoteId(u32::MAX)));
                batch.acknowledge();
            }
        });
        assert_eq!((counts.count_total, counts.bytes_total), (0, 0));
    }
}

#[test]
fn scoped_release_and_choke_queue_preserve_full_identity_without_heap_or_invalid_prefixes() {
    use ravet_foundation::{NoteId, NoteTarget};
    let (mut producer, mut consumer) = graph_event_queue(config()).unwrap();
    let target = NoteTarget {
        channel: None,
        key: Some(127),
        id: Some(NoteId(u32::MAX)),
    };
    let events = [
        Event {
            frame_offset: 2,
            data: EventData::NoteOffScoped {
                target,
                velocity: 0.123456,
            },
        },
        Event {
            frame_offset: 3,
            data: EventData::NoteChoke {
                target: NoteTarget::default(),
            },
        },
    ];
    let counts = allocation_counter::measure(|| {
        for tag in 0..10000 {
            producer
                .try_publish(tag, Transport::default(), &events)
                .unwrap();
            let batch = consumer.next_batch().unwrap();
            assert_eq!(batch.events(), &events);
            batch.acknowledge();
        }
        let invalid = Event {
            frame_offset: 3,
            data: EventData::NoteChoke {
                target: NoteTarget {
                    channel: Some(16),
                    ..target
                },
            },
        };
        assert!(matches!(
            producer.try_publish(10000, Transport::default(), &[events[0], invalid]),
            Err(GraphEventQueueError::Events(EventError::InvalidValue {
                index: 1
            }))
        ));
        assert!(consumer.next_batch().is_none());
    });
    assert_eq!(counts.count_total, 0);
}

#[test]
fn scoped_parameter_queue_preserves_f64_and_rejects_whole_invalid_batches_without_heap() {
    use ravet_foundation::{NoteId, NoteTarget};
    let (mut producer, mut consumer) = graph_event_queue(config()).unwrap();
    let target = NoteTarget {
        channel: Some(2),
        key: None,
        id: Some(NoteId(u32::MAX)),
    };
    let events = [
        Event {
            frame_offset: 2,
            data: EventData::ParameterValueScoped {
                id: ravet_foundation::ParameterId(7),
                target,
                value: f64::MAX,
            },
        },
        Event {
            frame_offset: 3,
            data: EventData::ParameterModulationScoped {
                id: ravet_foundation::ParameterId(7),
                target: NoteTarget::default(),
                amount: -f64::MAX,
            },
        },
    ];
    let counts = allocation_counter::measure(|| {
        for tag in 0..10000 {
            producer
                .try_publish(tag, Transport::default(), &events)
                .unwrap();
            let batch = consumer.next_batch().unwrap();
            assert_eq!(batch.events(), &events);
            batch.acknowledge();
        }
        let invalid = Event {
            frame_offset: 3,
            data: EventData::ParameterModulationScoped {
                id: ravet_foundation::ParameterId(7),
                target,
                amount: f64::NAN,
            },
        };
        assert!(matches!(
            producer.try_publish(10000, Transport::default(), &[events[0], invalid]),
            Err(GraphEventQueueError::Events(EventError::InvalidValue {
                index: 1
            }))
        ));
        assert!(consumer.next_batch().is_none());
    });
    assert_eq!(counts.count_total, 0);
}

#[test]
fn completion_queue_preserves_identity_and_rejects_invalid_whole_batches() {
    use ravet_foundation::{NoteId, NoteTarget};
    let (mut producer, mut consumer) = graph_event_queue(config()).unwrap();
    let event = Event {
        frame_offset: 3,
        data: EventData::NoteEnd {
            target: NoteTarget {
                channel: Some(15),
                key: Some(127),
                id: Some(NoteId(u32::MAX)),
            },
        },
    };
    let counts = allocation_counter::measure(|| {
        for tag in 0..10000 {
            producer
                .try_publish(tag, Transport::default(), &[event])
                .unwrap();
            let batch = consumer.next_batch().unwrap();
            assert_eq!(batch.events(), &[event]);
            batch.acknowledge();
        }
        let invalid = Event {
            data: EventData::NoteEnd {
                target: NoteTarget {
                    channel: Some(16),
                    ..NoteTarget::default()
                },
            },
            ..event
        };
        assert!(matches!(
            producer.try_publish(10000, Transport::default(), &[event, invalid]),
            Err(GraphEventQueueError::Events(EventError::InvalidValue {
                index: 1
            }))
        ));
        assert!(consumer.next_batch().is_none());
    });
    assert_eq!(counts.count_total, 0);
}
