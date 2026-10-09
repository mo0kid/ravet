use ravet_foundation::{Event, EventData, EventError, NoteId, NotePhase, OutputEvents};
fn note(channel: u8, key: u8, velocity: f32) -> Event {
    Event {
        frame_offset: 0,
        data: EventData::NoteWithId {
            id: NoteId(u32::MAX),
            phase: NotePhase::On,
            channel,
            key,
            velocity,
        },
    }
}
#[test]
fn complete_u32_identity_and_zero_velocity_on_are_preserved_without_voice_policy() {
    let mut storage = [note(0, 0, 0.); 2];
    let mut output = OutputEvents::new(&mut storage, 0);
    output.push(note(15, 127, 0.)).unwrap();
    output.push(note(15, 127, 1.)).unwrap();
    assert_eq!(output.as_slice()[0], note(15, 127, 0.));
    assert_eq!(output.as_slice()[1], note(15, 127, 1.));
}
#[test]
fn invalid_note_scalars_reject_without_changing_accepted_prefix() {
    for event in [
        note(16, 60, 0.5),
        note(0, 128, 0.5),
        note(0, 60, -0.1),
        note(0, 60, 1.1),
        note(0, 60, f32::NAN),
    ] {
        let mut storage = [note(0, 0, 0.); 2];
        let mut output = OutputEvents::new(&mut storage, 8);
        output.push(note(0, 60, 0.5)).unwrap();
        assert_eq!(
            output.push(event),
            Err(EventError::InvalidValue { index: 1 })
        );
        assert_eq!(output.as_slice(), &[note(0, 60, 0.5)]);
    }
}
