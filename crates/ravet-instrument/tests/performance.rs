use ravet_foundation::{
    Event, EventData, EventError, NoteExpression, NoteId, NotePhase, NoteTarget,
};
use ravet_instrument::{
    PerformanceBatchError, PerformanceError, PerformanceExpression, PerformancePhase,
    PerformanceVoices,
};

fn on(id: Option<u32>, channel: u8, key: u8, velocity: f32) -> EventData {
    match id {
        Some(id) => EventData::NoteWithId {
            id: NoteId(id),
            phase: NotePhase::On,
            channel,
            key,
            velocity,
        },
        None => EventData::NoteOn {
            channel,
            key,
            velocity,
        },
    }
}

fn target(id: Option<u32>, channel: u8, key: u8) -> NoteTarget {
    NoteTarget {
        channel: Some(channel),
        key: Some(key),
        id: id.map(NoteId),
    }
}

fn expression(target: NoteTarget, expression: NoteExpression, value: f64) -> EventData {
    EventData::NoteExpression {
        target,
        expression,
        value,
    }
}

#[test]
fn full_identity_bits_exact_velocity_and_explicit_retrigger_policy() {
    for id in [0, 1, i32::MAX as u32, 0x8000_0000, 0xffff_d8f0, u32::MAX] {
        for velocity in [0., -0., f32::from_bits(1), 0.12345679, 1.] {
            let mut bank = PerformanceVoices::<2>::new().unwrap();
            let started = bank.apply(on(Some(id), 15, 127, velocity)).unwrap();
            assert_eq!(started.started.unwrap().index, 0);
            assert!(!started.started.unwrap().retriggered);
            assert_eq!(bank.voices()[0].target, Some(target(Some(id), 15, 127)));
            assert_eq!(bank.voices()[0].velocity.to_bits(), velocity.to_bits());
            assert_eq!(bank.voices()[0].phase, PerformancePhase::Held);
            bank.apply(expression(
                NoteTarget::default(),
                NoteExpression::Volume,
                -0.,
            ))
            .unwrap();
            let before = bank.clone();
            assert_eq!(
                bank.apply(on(Some(id), 14, 127, 1.)),
                Err(PerformanceError::IdentityAddressConflict { id: NoteId(id) })
            );
            assert_eq!(bank, before);
            bank.apply(EventData::NoteOffScoped {
                target: target(Some(id), 15, 127),
                velocity: 0.7,
            })
            .unwrap();
            let again = bank.apply(on(Some(id), 15, 127, velocity)).unwrap();
            assert_eq!(again.started.unwrap().index, 0);
            assert!(again.started.unwrap().retriggered);
            assert_eq!(again.completions().count(), 0);
            assert_eq!(bank.voices()[0].phase, PerformancePhase::Held);
            assert_eq!(
                bank.voices()[0].expression,
                PerformanceExpression::default()
            );
            assert_eq!(bank.finish(0).unwrap(), Some(target(Some(id), 15, 127)));
            assert_eq!(bank.finish(0).unwrap(), None);
            assert!(bank.apply(on(Some(id), 14, 126, 1.)).is_ok());
        }
    }
    let mut bank = PerformanceVoices::<3>::new().unwrap();
    for index in 0..3 {
        assert_eq!(
            bank.apply(on(None, 0, 60, 0.))
                .unwrap()
                .started
                .unwrap()
                .index,
            index
        );
    }
    let release = bank
        .apply(EventData::NoteOff {
            channel: 0,
            key: 60,
            velocity: -0.,
        })
        .unwrap();
    assert_eq!(release.released, 7);
    assert_eq!(release.completions().count(), 0);
    let again = bank
        .apply(EventData::NoteOff {
            channel: 0,
            key: 60,
            velocity: 1.,
        })
        .unwrap();
    assert_eq!(again.changed, 0);
    assert!(
        bank.voices()
            .iter()
            .all(|v| v.release_velocity.to_bits() == (-0f32).to_bits())
    );
}

#[test]
fn every_scope_is_conjunctive_and_release_tail_expressions_are_live() {
    let owned = [
        target(Some(7), 1, 60),
        target(Some(8), 1, 61),
        target(None, 2, 60),
        target(Some(9), 2, 61),
    ];
    for scope in 0..8 {
        let selected = NoteTarget {
            channel: (scope & 1 != 0).then_some(1),
            key: (scope & 2 != 0).then_some(60),
            id: (scope & 4 != 0).then_some(NoteId(7)),
        };
        let expected = owned.iter().enumerate().fold(0u128, |mask, (index, note)| {
            let matches = selected.channel.is_none_or(|v| note.channel == Some(v))
                && selected.key.is_none_or(|v| note.key == Some(v))
                && selected.id.is_none_or(|v| note.id == Some(v));
            mask | if matches { 1 << index } else { 0 }
        });
        let mut bank = PerformanceVoices::<4>::new().unwrap();
        for note in owned {
            bank.apply(on(
                note.id.map(|id| id.0),
                note.channel.unwrap(),
                note.key.unwrap(),
                0.5,
            ))
            .unwrap();
        }
        assert_eq!(
            bank.apply(EventData::NoteOffScoped {
                target: selected,
                velocity: 0.12345679
            })
            .unwrap()
            .released,
            expected
        );
        assert_eq!(
            bank.apply(expression(selected, NoteExpression::Tuning, -120.))
                .unwrap()
                .changed,
            expected
        );
        for (index, note) in bank.voices().iter().enumerate() {
            assert_eq!(
                note.expression.get(NoteExpression::Tuning),
                if expected & (1 << index) != 0 {
                    -120.
                } else {
                    0.
                }
            );
        }
        let changes = bank
            .apply(EventData::NoteChoke { target: selected })
            .unwrap();
        assert_eq!(changes.changed, expected);
        assert_eq!(changes.released, 0);
        let completions: Vec<_> = changes.completions().collect();
        for (index, original) in &completions {
            assert_eq!(*original, owned[*index]);
            assert_eq!(bank.voices()[*index].phase, PerformancePhase::Idle);
        }
        assert_eq!(completions.len(), expected.count_ones() as usize);
    }
    let mut bank = PerformanceVoices::<1>::new().unwrap();
    bank.apply(expression(NoteTarget::default(), NoteExpression::Pan, 1.))
        .unwrap();
    bank.apply(on(None, 0, 60, 1.)).unwrap();
    assert_eq!(bank.voices()[0].expression.get(NoteExpression::Pan), 0.5);
}

#[test]
fn exact_expression_values_defaults_mute_and_complete_invalid_rejection() {
    let kinds = [
        NoteExpression::Volume,
        NoteExpression::Pan,
        NoteExpression::Tuning,
        NoteExpression::Vibrato,
        NoteExpression::Expression,
        NoteExpression::Brightness,
        NoteExpression::Pressure,
    ];
    for (kind, default) in kinds.into_iter().zip([1., 0.5, 0., 0., 1., 1., 0.]) {
        let mut bank = PerformanceVoices::<1>::new().unwrap();
        bank.apply(on(Some(u32::MAX), 0, 0, 1.)).unwrap();
        assert_eq!(bank.voices()[0].expression.get(kind), default);
        for value in [0., -0., f64::from_bits(1), 0.12345678901234567, 1.] {
            bank.apply(expression(NoteTarget::default(), kind, value))
                .unwrap();
            assert_eq!(
                bank.voices()[0].expression.get(kind).to_bits(),
                value.to_bits()
            );
            assert_eq!(bank.voices()[0].phase, PerformancePhase::Held);
            assert_eq!(
                bank.apply(expression(NoteTarget::default(), kind, value))
                    .unwrap()
                    .changed,
                0
            );
        }
        let maximum = match kind {
            NoteExpression::Volume => 4.,
            NoteExpression::Tuning => 120.,
            _ => 1.,
        };
        bank.apply(expression(NoteTarget::default(), kind, maximum))
            .unwrap();
        if kind == NoteExpression::Tuning {
            bank.apply(expression(NoteTarget::default(), kind, -120.))
                .unwrap();
        }
        let invalid_low = if kind == NoteExpression::Tuning {
            -120.00000001
        } else {
            -f64::from_bits(1)
        };
        for invalid in [
            invalid_low,
            maximum + 0.00001,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            let before = bank.clone();
            assert_eq!(
                bank.apply(expression(NoteTarget::default(), kind, invalid)),
                Err(PerformanceError::InvalidValue)
            );
            assert_eq!(bank, before);
        }
    }
    let mut bank = PerformanceVoices::<1>::new().unwrap();
    bank.apply(on(None, 0, 60, 1.)).unwrap();
    for invalid in [
        on(None, 16, 60, 1.),
        on(None, 0, 128, 1.),
        on(None, 0, 60, f32::NAN),
        on(None, 0, 60, -f32::from_bits(1)),
        on(Some(1), 0, 60, 1.0001),
        EventData::NoteOffScoped {
            target: NoteTarget {
                channel: Some(16),
                ..NoteTarget::default()
            },
            velocity: 0.,
        },
        EventData::NoteOff {
            channel: 0,
            key: 60,
            velocity: f32::INFINITY,
        },
        EventData::NoteChoke {
            target: NoteTarget {
                key: Some(128),
                ..NoteTarget::default()
            },
        },
    ] {
        let before = bank.clone();
        assert_eq!(bank.apply(invalid), Err(PerformanceError::InvalidValue));
        assert_eq!(bank, before);
    }
    for unsupported in [
        EventData::NoteEnd {
            target: NoteTarget::default(),
        },
        EventData::Midi(ravet_foundation::MidiMessage::from_bytes(&[0x90, 60, 100]).unwrap()),
        EventData::ParameterValue {
            id: ravet_foundation::ParameterId(1),
            value: 0.5,
        },
    ] {
        let before = bank.clone();
        assert_eq!(
            bank.apply(unsupported),
            Err(PerformanceError::UnsupportedEvent)
        );
        assert_eq!(bank, before);
    }
}

#[test]
fn idle_released_held_priority_and_age_order_survive_retrigger_finish_choke() {
    let mut bank = PerformanceVoices::<3>::new().unwrap();
    for id in 0..3 {
        bank.apply(on(Some(id), 0, 60, 1.)).unwrap();
    }
    bank.apply(on(Some(0), 0, 60, 1.)).unwrap(); // ID 1 now oldest.
    let changes = bank.apply(on(Some(3), 0, 61, 1.)).unwrap();
    assert_eq!(changes.started.unwrap().index, 1);
    assert_eq!(
        changes.completions().collect::<Vec<_>>(),
        vec![(1, target(Some(1), 0, 60))]
    );
    bank.apply(EventData::NoteOffScoped {
        target: NoteTarget {
            id: Some(NoteId(3)),
            ..NoteTarget::default()
        },
        velocity: 0.,
    })
    .unwrap();
    assert_eq!(
        bank.apply(on(Some(4), 0, 62, 1.))
            .unwrap()
            .started
            .unwrap()
            .index,
        1
    );
    bank.finish(0).unwrap();
    assert_eq!(
        bank.apply(on(Some(5), 0, 63, 1.))
            .unwrap()
            .started
            .unwrap()
            .index,
        0
    );
    assert_eq!(
        bank.apply(on(Some(6), 0, 64, 1.))
            .unwrap()
            .started
            .unwrap()
            .index,
        2
    );
    // An independent chronological FIFO oracle runs beyond any u8 sequence.
    let mut bank = PerformanceVoices::<7>::new().unwrap();
    let mut order = std::collections::VecDeque::new();
    for id in 0..10000u32 {
        let changes = bank.apply(on(Some(id), 0, 60, 1.)).unwrap();
        let ended: Vec<_> = changes
            .completions()
            .map(|(_, note)| note.id.unwrap().0)
            .collect();
        if order.len() == 7 {
            assert_eq!(ended, vec![order.pop_front().unwrap()]);
        } else {
            assert!(ended.is_empty());
        }
        order.push_back(id);
        if id % 31 == 0 {
            let oldest = order.pop_front().unwrap();
            let index = bank
                .voices()
                .iter()
                .position(|v| v.target.is_some_and(|n| n.id == Some(NoteId(oldest))))
                .unwrap();
            assert_eq!(
                bank.finish(index).unwrap().unwrap().id,
                Some(NoteId(oldest))
            );
        }
    }
    let before = bank.clone();
    assert_eq!(bank.finish(7), Err(PerformanceError::InvalidIndex));
    assert_eq!(bank, before);
}

#[test]
fn complete_batch_semantic_timing_capacity_preflight_is_observational() {
    let bank = PerformanceVoices::<1>::new().unwrap();
    let events = [
        Event {
            frame_offset: 0,
            data: on(Some(1), 0, 60, 1.),
        },
        Event {
            frame_offset: 0,
            data: expression(NoteTarget::default(), NoteExpression::Volume, 0.),
        },
        Event {
            frame_offset: 1,
            data: on(Some(2), 0, 61, 0.),
        },
        Event {
            frame_offset: 1,
            data: EventData::NoteChoke {
                target: NoteTarget::default(),
            },
        },
    ];
    let before = bank.clone();
    assert_eq!(bank.preflight(&events, 2).unwrap().immediate_completions, 2);
    assert_eq!(bank, before);
    let mut candidate = bank.clone();
    let mut count = 0;
    for event in events {
        count += candidate.apply(event.data).unwrap().completions().count();
    }
    assert_eq!(count, 2);
    let mut conflict = events;
    conflict[2].data = on(Some(1), 0, 61, 0.);
    assert_eq!(
        bank.preflight(&conflict, 2),
        Err(PerformanceBatchError::Semantic {
            index: 2,
            error: PerformanceError::IdentityAddressConflict { id: NoteId(1) }
        })
    );
    let mut invalid = events;
    invalid[3].data = expression(NoteTarget::default(), NoteExpression::Pressure, f64::NAN);
    assert!(matches!(
        bank.preflight(&invalid, 2),
        Err(PerformanceBatchError::Semantic {
            index: 3,
            error: PerformanceError::InvalidValue
        })
    ));
    invalid = events;
    invalid[3].frame_offset = 0;
    assert_eq!(
        bank.preflight(&invalid, 2),
        Err(PerformanceBatchError::Event(EventError::Unordered {
            index: 3
        }))
    );
    assert!(matches!(
        bank.preflight(&events, 1),
        Err(PerformanceBatchError::Event(EventError::OffsetOutOfRange {
            index: 2,
            ..
        }))
    ));
    let zero = [Event {
        frame_offset: 0,
        data: on(None, 0, 0, -0.),
    }; 1024];
    assert_eq!(
        bank.preflight(&zero, 0).unwrap().immediate_completions,
        1023
    );
    assert_eq!(
        bank.preflight(&zero, 65536).unwrap().immediate_completions,
        1023
    );
    assert!(matches!(
        bank.preflight(&zero, 65537),
        Err(PerformanceBatchError::FramesOutOfRange { .. })
    ));
    assert!(matches!(
        bank.preflight(&[zero[0]; 1025], 0),
        Err(PerformanceBatchError::Event(EventError::TooMany {
            maximum: 1024,
            ..
        }))
    ));
    assert!(
        bank.preflight(
            &[Event {
                frame_offset: 1,
                ..zero[0]
            }],
            0
        )
        .is_err()
    );
    assert_eq!(bank.preflight(&[], 0).unwrap().immediate_completions, 0);
    assert_eq!(bank, before);
}

#[test]
fn maximum_voice_masks_original_completions_reset_and_heap_free_workloads() {
    assert!(matches!(
        PerformanceVoices::<0>::new(),
        Err(PerformanceError::InvalidConfiguration)
    ));
    assert!(matches!(
        PerformanceVoices::<129>::new(),
        Err(PerformanceError::InvalidConfiguration)
    ));
    let mut bank = PerformanceVoices::<128>::new().unwrap();
    for id in 0..128 {
        bank.apply(on(
            Some(u32::MAX - id),
            (id % 16) as u8,
            id as u8,
            0.12345679,
        ))
        .unwrap();
    }
    let release = bank
        .apply(EventData::NoteOffScoped {
            target: NoteTarget::default(),
            velocity: 0.5,
        })
        .unwrap();
    assert_eq!(release.released, u128::MAX);
    let change = bank
        .apply(expression(
            NoteTarget::default(),
            NoteExpression::Volume,
            -0.,
        ))
        .unwrap();
    assert_eq!(change.changed, u128::MAX);
    let ended = bank
        .apply(EventData::NoteChoke {
            target: NoteTarget::default(),
        })
        .unwrap();
    assert_eq!(ended.changed, u128::MAX);
    for (index, note) in ended.completions() {
        assert_eq!(note.id, Some(NoteId(u32::MAX - index as u32)));
    }
    let mut checksum = 0usize;
    let counts = allocation_counter::measure(|| {
        for id in 0..10000 {
            let changes = bank.apply(on(Some(id), 0, 60, f32::from_bits(1))).unwrap();
            checksum += changes.completions().count();
            bank.apply(expression(
                NoteTarget::default(),
                NoteExpression::Pressure,
                f64::from_bits(1),
            ))
            .unwrap();
            bank.apply(EventData::NoteOffScoped {
                target: NoteTarget::default(),
                velocity: -0.,
            })
            .unwrap();
            if id % 100 == 0 {
                bank.reset();
            }
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    assert_eq!(checksum, 0); // Reset every100 prevents exhausting128 slots.
    bank.reset();
    assert!(
        bank.voices()
            .iter()
            .all(|v| v.target.is_none() && v.expression == PerformanceExpression::default())
    );
    let events = [Event {
        frame_offset: 0,
        data: on(None, 0, 60, 0.),
    }; 1024];
    let counts = allocation_counter::measure(|| {
        for _ in 0..1000 {
            checksum += bank.preflight(&events, 0).unwrap().immediate_completions;
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    assert_eq!(checksum, 896000);
}
