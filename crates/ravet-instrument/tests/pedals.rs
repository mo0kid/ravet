use ravet_foundation::MidiMessage;
use ravet_instrument::{MpeReceiver, MpeZone, VoiceAllocator, VoiceMode, VoicePhase, ZoneSide};

fn message(bytes: &[u8]) -> MidiMessage {
    MidiMessage::from_bytes(bytes).unwrap()
}
fn cc(channel: u8, controller: u8, value: u8) -> MidiMessage {
    message(&[0xb0 | channel, controller, value])
}
fn on(channel: u8, key: u8) -> MidiMessage {
    message(&[0x90 | channel, key, 100])
}
fn off(channel: u8, key: u8) -> MidiMessage {
    message(&[0x80 | channel, key, 50])
}
fn zone<const N: usize>(receiver: &mut MpeReceiver<N>, master: u8, members: u8) {
    receiver.apply(cc(master, 100, 6));
    receiver.apply(cc(master, 101, 0));
    receiver.apply(cc(master, 6, members));
}

#[test]
fn rising_edge_captures_held_duplicates_only_and_preserves_channel_isolation() {
    let mut voices = VoiceAllocator::<8>::new(VoiceMode::Polyphonic, 2., 2.).unwrap();
    voices.apply(on(0, 60));
    voices.apply(on(0, 60));
    voices.apply(on(1, 64));
    voices.apply(on(0, 67));
    voices.apply(cc(0, 64, 127));
    voices.apply(off(0, 67)); // Sustain-held keys are not physically held.
    assert_eq!(voices.apply(cc(0, 66, 63)).changed, 0);
    assert_eq!(voices.apply(cc(0, 66, 64)).changed, 3);
    assert!(voices.voices()[0].sostenuto_held());
    assert!(voices.voices()[1].sostenuto_held());
    assert!(!voices.voices()[2].sostenuto_held());
    assert!(!voices.voices()[3].sostenuto_held());
    voices.apply(on(0, 72));
    assert_eq!(voices.apply(cc(0, 66, 127)).changed, 0); // Repeated on never recaptures.
    voices.apply(off(0, 60));
    voices.apply(off(0, 60));
    voices.apply(off(0, 72));
    voices.apply(off(1, 64));
    assert_eq!(voices.apply(cc(0, 64, 63)).changed, (1 << 3) | (1 << 4));
    assert_eq!(voices.voices()[0].phase, VoicePhase::Sustained);
    assert_eq!(voices.voices()[1].phase, VoicePhase::Sustained);
    assert_eq!(voices.voices()[0].release_velocity, 50. / 127.);
    assert_eq!(voices.voices()[2].phase, VoicePhase::Released);
    assert_eq!(voices.voices()[3].phase, VoicePhase::Released);
    assert_eq!(voices.voices()[4].phase, VoicePhase::Released);
    assert_eq!(voices.apply(cc(0, 66, 63)).changed, 3);
    assert_eq!(voices.voices()[0].phase, VoicePhase::Released);
    voices.apply(cc(0, 66, 64));
    assert!(!voices.voices()[0].sostenuto_held()); // A pedal cannot revive a tail.
}

#[test]
fn either_pedal_retains_keys_reset_all_notes_and_reused_slots_clear_latches() {
    for first_off in [64, 66] {
        let mut voices = VoiceAllocator::<2>::new(VoiceMode::Polyphonic, 2., 2.).unwrap();
        voices.apply(on(0, 60));
        voices.apply(cc(0, 66, 127));
        voices.apply(cc(0, 64, 127));
        voices.apply(off(0, 60));
        voices.apply(cc(0, first_off, 0));
        assert_eq!(voices.voices()[0].phase, VoicePhase::Sustained);
        voices.apply(cc(0, if first_off == 64 { 66 } else { 64 }, 0));
        assert_eq!(voices.voices()[0].phase, VoicePhase::Released);
    }
    let mut voices = VoiceAllocator::<1>::new(VoiceMode::Polyphonic, 2., 2.).unwrap();
    voices.apply(on(0, 60));
    voices.apply(cc(0, 66, 127));
    voices.apply(cc(0, 123, 0));
    assert_eq!(voices.voices()[0].phase, VoicePhase::Sustained);
    let stolen = voices.apply(on(0, 64)).started.unwrap().stolen.unwrap();
    assert!(stolen.sostenuto_held());
    assert!(!voices.voices()[0].sostenuto_held());
    voices.apply(off(0, 64));
    assert_eq!(voices.voices()[0].phase, VoicePhase::Released);
    voices.finish(0).unwrap();
    voices.apply(cc(0, 66, 0));
    voices.apply(on(0, 67));
    voices.apply(cc(0, 66, 127));
    assert_eq!(voices.apply(cc(0, 121, 0)).changed, 1);
    assert_eq!(voices.voices()[0].phase, VoicePhase::Held);
    assert!(!voices.voices()[0].sostenuto_held());
    voices.apply(off(0, 67));
    assert_eq!(voices.voices()[0].phase, VoicePhase::Released);
    voices.reset();
    voices.apply(on(0, 72));
    voices.apply(cc(0, 66, 127));
    voices.apply(off(0, 72));
    assert_eq!(voices.apply(cc(0, 121, 0)).changed, 1);
    assert_eq!(voices.voices()[0].phase, VoicePhase::Released);
    voices.reset();
    voices.apply(on(0, 76));
    voices.apply(cc(0, 66, 127));
    voices.apply(cc(0, 120, 0));
    assert_eq!(voices.voices()[0].phase, VoicePhase::Idle);
    assert!(!voices.voices()[0].sostenuto_held());
}

#[test]
fn mpe_master_only_two_zone_compatibility_and_legacy_member_reuse() {
    let mut receiver = MpeReceiver::<8>::new().unwrap();
    zone(&mut receiver, 0, 2);
    zone(&mut receiver, 15, 2);
    for (channel, key) in [(1, 60), (0, 64), (14, 67), (15, 72)] {
        receiver.apply(on(channel, key));
    }
    assert_eq!(receiver.apply(cc(1, 66, 127)).voices.changed, 0);
    assert_eq!(receiver.apply(cc(0, 66, 127)).voices.changed, 3);
    assert_eq!(receiver.apply(cc(15, 66, 127)).voices.changed, 12);
    for (channel, key) in [(1, 60), (0, 64), (14, 67), (15, 72)] {
        receiver.apply(off(channel, key));
    }
    assert!(
        receiver.voices()[..4]
            .iter()
            .all(|v| v.phase == VoicePhase::Sustained)
    );
    receiver.apply(cc(0, 66, 0));
    assert_eq!(receiver.voices()[0].phase, VoicePhase::Released);
    assert_eq!(receiver.voices()[1].phase, VoicePhase::Released);
    assert_eq!(receiver.voices()[2].phase, VoicePhase::Sustained);
    receiver.apply(cc(14, 121, 0));
    assert!(receiver.voices()[2].sostenuto_held());
    receiver.apply(cc(15, 121, 0));
    assert_eq!(receiver.voices()[2].phase, VoicePhase::Released);
    receiver.apply(on(1, 60));
    receiver.apply(cc(0, 66, 127));
    zone(&mut receiver, 0, 1);
    assert!(receiver.voices().iter().all(|v| !v.sostenuto_held()));
    receiver.apply(on(1, 60));
    receiver.apply(cc(0, 66, 127)); // Configuration cleared the pedal edge state too.
    assert!(receiver.voices()[0].sostenuto_held());
    for (side, master, member) in [(ZoneSide::Lower, 0, 1), (ZoneSide::Upper, 15, 14)] {
        let mut legacy =
            VoiceAllocator::<2>::new(VoiceMode::Mpe(MpeZone::new(side, 2).unwrap()), 48., 2.)
                .unwrap();
        legacy.apply(on(member, 60));
        legacy.apply(cc(master, 66, 127));
        legacy.apply(off(member, 60));
        legacy.apply(on(member, 64));
        assert_eq!(legacy.voices()[0].phase, VoicePhase::Released);
        assert!(!legacy.voices()[0].sostenuto_held());
        assert!(!legacy.voices()[1].sostenuto_held());
    }
}

#[test]
fn full_capacity_pedal_cycles_are_heap_free_and_clear_every_slot() {
    let mut receiver = MpeReceiver::<128>::new().unwrap();
    let counts = allocation_counter::measure(|| {
        for _ in 0..100 {
            for key in 0..128 {
                receiver.apply(on(0, key));
            }
            assert_eq!(receiver.apply(cc(0, 66, 127)).voices.changed, u128::MAX);
            receiver.apply(cc(0, 64, 127));
            receiver.apply(cc(0, 123, 0));
            receiver.apply(cc(0, 64, 0));
            assert!(
                receiver
                    .voices()
                    .iter()
                    .all(|v| v.phase == VoicePhase::Sustained)
            );
            assert_eq!(receiver.apply(cc(0, 66, 0)).voices.changed, u128::MAX);
            assert!(
                receiver
                    .voices()
                    .iter()
                    .all(|v| v.phase == VoicePhase::Released)
            );
            receiver.apply(message(&[0xff]));
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}
