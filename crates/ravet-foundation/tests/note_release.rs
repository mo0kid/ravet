use ravet_foundation::{Event, EventData, EventError, NoteId, NoteTarget, OutputEvents};
#[test]
fn matching_scope_velocity_and_complete_output_prefix_contract() {
    for target in [
        NoteTarget::default(),
        NoteTarget {
            channel: Some(15),
            key: Some(127),
            id: Some(NoteId(u32::MAX)),
        },
    ] {
        for data in [
            EventData::NoteOffScoped {
                target,
                velocity: 0.,
            },
            EventData::NoteOffScoped {
                target,
                velocity: 1.,
            },
            EventData::NoteChoke { target },
        ] {
            let event = Event {
                frame_offset: 0,
                data,
            };
            let mut storage = [event; 2];
            let mut output = OutputEvents::new(&mut storage, 0);
            output.push(event).unwrap();
            assert_eq!(output.as_slice(), &[event]);
        }
    }
    let good = Event {
        frame_offset: 0,
        data: EventData::NoteChoke {
            target: NoteTarget::default(),
        },
    };
    let invalid_targets = [
        NoteTarget {
            channel: Some(16),
            ..NoteTarget::default()
        },
        NoteTarget {
            key: Some(128),
            ..NoteTarget::default()
        },
    ];
    let bad = invalid_targets
        .into_iter()
        .flat_map(|target| {
            [
                EventData::NoteChoke { target },
                EventData::NoteOffScoped {
                    target,
                    velocity: 0.5,
                },
            ]
        })
        .chain(
            [f32::NAN, f32::INFINITY, -0.1, 1.01].map(|velocity| EventData::NoteOffScoped {
                target: NoteTarget::default(),
                velocity,
            }),
        );
    for data in bad {
        let mut storage = [good; 2];
        let mut output = OutputEvents::new(&mut storage, 8);
        output.push(good).unwrap();
        assert_eq!(
            output.push(Event { data, ..good }),
            Err(EventError::InvalidValue { index: 1 })
        );
        assert_eq!(output.as_slice(), &[good]);
    }
}
