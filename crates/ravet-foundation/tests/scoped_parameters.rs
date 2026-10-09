use ravet_foundation::{
    Event, EventData, EventError, NoteId, NoteTarget, OutputEvents, ParameterId,
};
#[test]
fn scoped_f64_values_amounts_and_targets_preserve_range_and_reject_invalid_output_prefixes() {
    let targets = [
        NoteTarget::default(),
        NoteTarget {
            channel: Some(15),
            key: Some(127),
            id: Some(NoteId(u32::MAX)),
        },
    ];
    for target in targets {
        for value in [
            -f64::MAX,
            -0.,
            f64::from_bits(1),
            0.12345678912345678,
            f64::MAX,
        ] {
            for data in [
                EventData::ParameterValueScoped {
                    id: ParameterId(7),
                    target,
                    value,
                },
                EventData::ParameterModulationScoped {
                    id: ParameterId(7),
                    target,
                    amount: value,
                },
            ] {
                let event = Event {
                    frame_offset: 0,
                    data,
                };
                let mut storage = [event; 1];
                let mut output = OutputEvents::new(&mut storage, 0);
                output.push(event).unwrap();
                assert_eq!(output.as_slice(), &[event]);
            }
        }
    }
    let good = Event {
        frame_offset: 0,
        data: EventData::ParameterValue {
            id: ParameterId(7),
            value: 0.5,
        },
    };
    for target in [
        NoteTarget {
            channel: Some(16),
            ..NoteTarget::default()
        },
        NoteTarget {
            key: Some(128),
            ..NoteTarget::default()
        },
    ] {
        for data in [
            EventData::ParameterValueScoped {
                id: ParameterId(7),
                target,
                value: 0.5,
            },
            EventData::ParameterModulationScoped {
                id: ParameterId(7),
                target,
                amount: 0.5,
            },
        ] {
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
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for data in [
            EventData::ParameterValueScoped {
                id: ParameterId(7),
                target: NoteTarget::default(),
                value,
            },
            EventData::ParameterModulationScoped {
                id: ParameterId(7),
                target: NoteTarget::default(),
                amount: value,
            },
        ] {
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
}
