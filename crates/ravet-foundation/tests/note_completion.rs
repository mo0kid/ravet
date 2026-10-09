use ravet_foundation::{Event, EventData, EventError, NoteId, NoteTarget, OutputEvents};
#[test]
fn completion_targets_and_carrier_offsets_follow_portable_prefix_bounds() {
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
            data: EventData::NoteEnd { target },
        };
        let mut storage = [event; 2];
        let mut output = OutputEvents::new(&mut storage, 0);
        output.push(event).unwrap();
        assert_eq!(output.as_slice(), &[event]);
    }
    let good = Event {
        frame_offset: 2,
        data: EventData::NoteEnd {
            target: NoteTarget::default(),
        },
    };
    let mut storage = [good; 2];
    let mut output = OutputEvents::new(&mut storage, 8);
    output.push(good).unwrap();
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
        assert_eq!(
            output.push(Event {
                data: EventData::NoteEnd { target },
                ..good
            }),
            Err(EventError::InvalidValue { index: 1 })
        );
        assert_eq!(output.as_slice(), &[good]);
    }
    assert!(
        output
            .push(Event {
                frame_offset: 8,
                ..good
            })
            .is_err()
    );
    assert_eq!(output.as_slice(), &[good]);
}
