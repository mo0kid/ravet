use ravet_foundation::MidiMessage;
use ravet_instrument::{MpeZone, VoiceAllocator, VoiceMode, VoicePhase, ZoneSide};
fn message(bytes: &[u8]) -> MidiMessage {
    MidiMessage::from_bytes(bytes).unwrap()
}
#[test]
fn duplicate_notes_release_oldest_sustain_and_deterministic_stealing() {
    let mut voices = VoiceAllocator::<3>::new(VoiceMode::Polyphonic, 2., 2.).unwrap();
    assert_eq!(
        voices
            .apply(message(&[0x90, 60, 100]))
            .started
            .unwrap()
            .index,
        0
    );
    assert_eq!(
        voices
            .apply(message(&[0x90, 60, 120]))
            .started
            .unwrap()
            .index,
        1
    );
    voices.apply(message(&[0xb0, 64, 127]));
    voices.apply(message(&[0x80, 60, 50]));
    assert_eq!(voices.voices()[0].phase, VoicePhase::Sustained);
    assert_eq!(voices.voices()[1].phase, VoicePhase::Held);
    voices.apply(message(&[0xb0, 64, 0]));
    assert_eq!(voices.voices()[0].phase, VoicePhase::Released);
    assert_eq!(
        voices
            .apply(message(&[0x90, 64, 100]))
            .started
            .unwrap()
            .index,
        2
    ); // idle beats released
    let start = voices.apply(message(&[0x90, 67, 100])).started.unwrap();
    assert_eq!(start.index, 0);
    assert_eq!(start.stolen.unwrap().note.unwrap().key, 60);
    assert_eq!(
        voices
            .apply(message(&[0x90, 70, 100]))
            .started
            .unwrap()
            .index,
        1
    ); // oldest held
    voices.finish(0).unwrap();
    assert_eq!(voices.voices()[0].phase, VoicePhase::Idle);
    assert_eq!(
        voices
            .apply(message(&[0x90, 72, 100]))
            .started
            .unwrap()
            .index,
        0
    );
    assert!(voices.finish(3).is_err());
}
#[test]
fn expression_endpoints_poly_pressure_and_released_tail_freeze() {
    let mut voices = VoiceAllocator::<2>::new(VoiceMode::Polyphonic, 12., 2.).unwrap();
    voices.apply(message(&[0x90, 60, 127]));
    voices.apply(message(&[0x91, 64, 127]));
    voices.apply(message(&[0xe0, 0, 0]));
    assert_eq!(voices.voices()[0].expression.member_bend, -12.);
    assert_eq!(voices.voices()[1].expression.member_bend, 0.);
    voices.apply(message(&[0xa0, 60, 127]));
    voices.apply(message(&[0xe0, 127, 127]));
    assert_eq!(voices.voices()[0].expression.pressure, 1.);
    assert_eq!(voices.voices()[0].expression.member_bend, 12.);
    voices.apply(message(&[0xb0, 74, 64]));
    assert_eq!(voices.voices()[0].expression.timbre, 64. / 127.);
    voices.apply(message(&[0x80, 60, 12]));
    let tail = voices.voices()[0].expression;
    voices.apply(message(&[0xe0, 0, 64]));
    voices.apply(message(&[0xd0, 0]));
    assert_eq!(voices.voices()[0].expression, tail);
    voices.apply(message(&[0xb1, 120, 0]));
    assert_eq!(voices.voices()[1].phase, VoicePhase::Idle);
}
#[test]
fn configured_mpe_master_member_sustain_reuse_and_zone_filtering() {
    let zone = MpeZone::new(ZoneSide::Lower, 2).unwrap();
    let mut voices = VoiceAllocator::<4>::new(VoiceMode::Mpe(zone), 48., 2.).unwrap();
    assert!(voices.apply(message(&[0x90, 60, 127])).started.is_none());
    assert!(voices.apply(message(&[0x93, 60, 127])).started.is_none());
    voices.apply(message(&[0xe1, 127, 127]));
    voices.apply(message(&[0x91, 60, 127]));
    voices.apply(message(&[0x92, 64, 100]));
    assert_eq!(voices.voices()[0].expression.member_bend, 48.);
    voices.apply(message(&[0xe0, 0, 0]));
    assert_eq!(voices.voices()[0].expression.master_bend, -2.);
    assert_eq!(voices.voices()[1].expression.master_bend, -2.);
    voices.apply(message(&[0xd1, 127]));
    assert_eq!(voices.voices()[0].expression.pressure, 1.);
    assert_eq!(voices.voices()[1].expression.pressure, 0.);
    voices.apply(message(&[0xb0, 64, 127]));
    voices.apply(message(&[0x81, 60, 0]));
    assert_eq!(voices.voices()[0].phase, VoicePhase::Sustained);
    let old = voices.voices()[0].expression;
    let next = voices.apply(message(&[0x91, 67, 100])).started.unwrap();
    assert_eq!(next.index, 2);
    assert_eq!(voices.voices()[0].phase, VoicePhase::Released);
    voices.apply(message(&[0xe1, 0, 64]));
    assert_eq!(voices.voices()[0].expression, old);
    assert_eq!(voices.voices()[2].expression.member_bend, 0.);
    voices.apply(message(&[0xb0, 123, 0]));
    assert_eq!(voices.voices()[1].phase, VoicePhase::Sustained);
    voices.apply(message(&[0xb0, 121, 0]));
    assert_eq!(voices.voices()[1].phase, VoicePhase::Released);
    let upper = MpeZone::new(ZoneSide::Upper, 3).unwrap();
    assert_eq!(upper.master(), 15);
    assert!(upper.contains(12));
    assert!(!upper.contains(11));
    assert!(!upper.contains(15));
}
#[test]
fn zero_velocity_reset_and_heap_boundaries() {
    assert!(VoiceAllocator::<0>::new(VoiceMode::Polyphonic, 2., 2.).is_err());
    assert!(VoiceAllocator::<129>::new(VoiceMode::Polyphonic, 2., 2.).is_err());
    assert!(MpeZone::new(ZoneSide::Lower, 0).is_err());
    let mut voices = VoiceAllocator::<128>::new(VoiceMode::Polyphonic, 2., 2.).unwrap();
    let note = message(&[0x90, 60, 127]);
    let off = message(&[0x90, 60, 0]);
    let counts = allocation_counter::measure(|| {
        for i in 0..10000 {
            voices.apply(note);
            voices.apply(off);
            if i % 100 == 0 {
                voices.reset();
            }
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    assert_eq!(voices.apply(message(&[0xff])).changed, u128::MAX);
    assert!(voices.voices().iter().all(|v| v.phase == VoicePhase::Idle));
}

#[test]
fn separate_master_pressure_timbre_refresh_active_voices_and_freeze_tails() {
    for (side, master, member) in [(ZoneSide::Lower, 0, 1), (ZoneSide::Upper, 15, 14)] {
        let mut voices =
            VoiceAllocator::<2>::new(VoiceMode::Mpe(MpeZone::new(side, 2).unwrap()), 48., 2.)
                .unwrap();
        voices.apply(message(&[0xd0 | master, 100]));
        voices.apply(message(&[0xb0 | master, 74, 50]));
        voices.apply(message(&[0x90 | member, 60, 127]));
        voices.apply(message(&[0xd0 | member, 20]));
        voices.apply(message(&[0xb0 | member, 74, 30]));
        let expression = voices.voices()[0].expression;
        assert_eq!(expression.pressure, 20. / 127.);
        assert_eq!(expression.timbre, 30. / 127.);
        assert_eq!(expression.master_pressure, 100. / 127.);
        assert_eq!(expression.master_timbre, 50. / 127.);
        voices.apply(message(&[0xb0 | master, 64, 127]));
        voices.apply(message(&[0x80 | member, 60, 0]));
        assert_eq!(voices.voices()[0].phase, VoicePhase::Sustained);
        assert_eq!(voices.apply(message(&[0xd0 | master, 127])).changed, 1);
        assert_eq!(voices.voices()[0].expression.master_pressure, 1.);
        voices.apply(message(&[0xb0 | master, 64, 0]));
        let tail = voices.voices()[0].expression;
        voices.apply(message(&[0xd0 | master, 0]));
        voices.apply(message(&[0xb0 | master, 74, 0]));
        assert_eq!(voices.voices()[0].expression, tail);
        voices.apply(message(&[0x90 | member, 64, 127]));
        assert_eq!(voices.voices()[1].expression.master_pressure, 0.);
        assert_eq!(voices.voices()[1].expression.master_timbre, 0.);
        voices.apply(message(&[0xb0 | master, 121, 0]));
        assert_eq!(voices.voices()[1].expression, Default::default());
    }
    let mut poly = VoiceAllocator::<1>::new(VoiceMode::Polyphonic, 2., 2.).unwrap();
    poly.apply(message(&[0x90, 60, 127]));
    poly.apply(message(&[0xd0, 127]));
    assert_eq!(poly.voices()[0].expression.master_pressure, 0.);
}
