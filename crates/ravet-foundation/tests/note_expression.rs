use ravet_foundation::{
    Event, EventData, EventError, NoteExpression as Expr, NoteId, NoteTarget, OutputEvents,
};
#[test]
fn standard_ranges_and_wildcard_scope_validate_without_clamping() {
    let cases = [
        (Expr::Volume, 0., 4.),
        (Expr::Pan, 0., 1.),
        (Expr::Tuning, -120., 120.),
        (Expr::Vibrato, 0., 1.),
        (Expr::Expression, 0., 1.),
        (Expr::Brightness, 0., 1.),
        (Expr::Pressure, 0., 1.),
    ];
    for (expression, low, high) in cases {
        for value in [low, high] {
            for target in [
                NoteTarget::default(),
                NoteTarget {
                    channel: Some(15),
                    key: Some(127),
                    id: Some(NoteId(u32::MAX)),
                },
            ] {
                let event = Event {
                    frame_offset: 0,
                    data: EventData::NoteExpression {
                        target,
                        expression,
                        value,
                    },
                };
                let mut storage = [event; 1];
                let mut output = OutputEvents::new(&mut storage, 0);
                output.push(event).unwrap();
                assert_eq!(output.as_slice(), &[event]);
            }
        }
        for value in [f64::NAN, f64::INFINITY, high + 1., low - 1.] {
            assert!(!expression.is_valid_value(value));
        }
    }
    assert!(Expr::Volume.is_valid_value(-0.));
    assert!(Expr::Volume.is_valid_value(f64::from_bits(1)));
    assert!(!Expr::Volume.is_valid_value(-f64::from_bits(1)));
    assert!(
        !NoteTarget {
            channel: Some(16),
            ..NoteTarget::default()
        }
        .is_valid()
    );
    assert!(
        !NoteTarget {
            key: Some(128),
            ..NoteTarget::default()
        }
        .is_valid()
    );
    let good = Event {
        frame_offset: 0,
        data: EventData::NoteExpression {
            target: NoteTarget::default(),
            expression: Expr::Pressure,
            value: 0.12345678912345678,
        },
    };
    let invalid = Event {
        data: EventData::NoteExpression {
            target: NoteTarget::default(),
            expression: Expr::Volume,
            value: -f64::from_bits(1),
        },
        ..good
    };
    let mut storage = [good; 2];
    let mut output = OutputEvents::new(&mut storage, 8);
    output.push(good).unwrap();
    assert_eq!(
        output.push(invalid),
        Err(EventError::InvalidValue { index: 1 })
    );
    assert_eq!(output.as_slice(), &[good]);
}
