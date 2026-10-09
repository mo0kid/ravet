use ravet_foundation::MidiMessage;
use ravet_instrument::{MpeReceiver, NegotiatedChanges, VoicePhase, ZoneSide};
fn message(bytes: &[u8]) -> MidiMessage {
    MidiMessage::from_bytes(bytes).unwrap()
}
fn cc(channel: u8, controller: u8, value: u8) -> MidiMessage {
    message(&[0xb0 | channel, controller, value])
}
fn rpn<const N: usize>(
    r: &mut MpeReceiver<N>,
    channel: u8,
    parameter: u8,
    value: u8,
) -> NegotiatedChanges {
    r.apply(cc(channel, 100, parameter));
    r.apply(cc(channel, 101, 0));
    r.apply(cc(channel, 6, value))
}
#[test]
fn every_zone_size_order_overlap_and_disabled_master_reuse() {
    for lower in 0..=15 {
        for upper in 0..=15 {
            let mut receiver = MpeReceiver::<4>::new().unwrap();
            rpn(&mut receiver, 0, 6, lower);
            rpn(&mut receiver, 15, 6, upper);
            let (l, u) = receiver.zones();
            assert_eq!(u.map_or(0, |z| z.member_channels()), upper);
            assert_eq!(
                l.map_or(0, |z| z.member_channels()),
                if upper == 0 {
                    lower
                } else {
                    lower.min(14u8.saturating_sub(upper))
                }
            );
            if let (Some(l), Some(u)) = (l, u) {
                for channel in 0..16 {
                    assert!(!(l.contains(channel) && u.contains(channel)));
                    assert!(!(l.contains(channel) && channel == u.master()));
                    assert!(!(u.contains(channel) && channel == l.master()));
                }
            }
            let mut reverse = MpeReceiver::<4>::new().unwrap();
            rpn(&mut reverse, 15, 6, upper);
            rpn(&mut reverse, 0, 6, lower);
            let (l, u) = reverse.zones();
            assert_eq!(l.map_or(0, |z| z.member_channels()), lower);
            assert_eq!(
                u.map_or(0, |z| z.member_channels()),
                if lower == 0 {
                    upper
                } else {
                    upper.min(14u8.saturating_sub(lower))
                }
            );
        }
    }
    let mut receiver = MpeReceiver::<4>::new().unwrap();
    rpn(&mut receiver, 15, 6, 15);
    assert!(
        receiver
            .apply(message(&[0x90, 60, 127]))
            .voices
            .started
            .is_some()
    );
    let change = rpn(&mut receiver, 0, 6, 15);
    assert_eq!(change.voices.changed, 1);
    assert!(receiver.zones().1.is_none());
    assert!(
        receiver
            .apply(message(&[0x9f, 64, 127]))
            .voices
            .started
            .is_some()
    );
    assert!(!rpn(&mut receiver, 1, 6, 2).configuration_changed);
    assert!(!rpn(&mut receiver, 0, 6, 16).configuration_changed);
    assert_eq!(receiver.voices()[0].note.unwrap().channel, 15);
    assert!(MpeReceiver::<0>::new().is_err());
    assert!(MpeReceiver::<129>::new().is_err());
}
#[test]
fn shared_channel_polyphony_master_compatibility_and_independent_zone_ranges() {
    let mut receiver = MpeReceiver::<8>::new().unwrap();
    rpn(&mut receiver, 0, 6, 2);
    rpn(&mut receiver, 15, 6, 2);
    receiver.apply(message(&[0x91, 60, 127]));
    receiver.apply(message(&[0x91, 64, 127]));
    receiver.apply(message(&[0x9e, 67, 127]));
    receiver.apply(message(&[0x90, 72, 127]));
    assert!(
        receiver.voices()[..4]
            .iter()
            .all(|v| v.phase == VoicePhase::Held)
    );
    receiver.apply(message(&[0xe1, 127, 127]));
    receiver.apply(message(&[0xe0, 127, 127]));
    receiver.apply(message(&[0xef, 0, 0]));
    assert_eq!(receiver.voices()[0].expression.member_bend, 48.);
    assert_eq!(receiver.voices()[1].expression.member_bend, 48.);
    assert_eq!(receiver.voices()[2].expression.master_bend, -2.);
    assert_eq!(receiver.voices()[3].expression.member_bend, 0.);
    assert_eq!(receiver.voices()[3].expression.master_bend, 2.);
    rpn(&mut receiver, 1, 0, 12);
    rpn(&mut receiver, 0, 0, 7);
    rpn(&mut receiver, 14, 0, 24);
    rpn(&mut receiver, 15, 0, 3);
    assert_eq!(receiver.bend_ranges(ZoneSide::Lower), (12., 7.));
    assert_eq!(receiver.bend_ranges(ZoneSide::Upper), (24., 3.));
    assert_eq!(receiver.voices()[1].expression.member_bend, 12.);
    assert_eq!(receiver.voices()[1].expression.master_bend, 7.);
    assert_eq!(receiver.voices()[2].expression.master_bend, -3.);
    assert!(!rpn(&mut receiver, 14, 0, 97).configuration_changed);
    assert!(!receiver.apply(cc(14, 38, 50)).configuration_changed);
    receiver.apply(message(&[0xd0, 64]));
    receiver.apply(cc(0, 74, 127));
    receiver.apply(message(&[0xdf, 127]));
    assert_eq!(receiver.voices()[0].expression.master_pressure, 64. / 127.);
    assert_eq!(receiver.voices()[0].expression.master_timbre, 1.);
    assert_eq!(receiver.voices()[2].expression.master_pressure, 1.);
    receiver.apply(message(&[0x81, 60, 0]));
    assert_eq!(receiver.voices()[0].phase, VoicePhase::Released);
    assert_eq!(receiver.voices()[1].phase, VoicePhase::Held);
    assert!(
        receiver
            .apply(message(&[0x97, 40, 127]))
            .voices
            .started
            .is_none()
    );
    assert!(receiver.finish(8).is_err());
    receiver.finish(0).unwrap();
    assert_eq!(receiver.voices()[0].phase, VoicePhase::Idle);
}
#[test]
fn note_off_freezes_members_but_master_expression_reaches_sustained_and_release_tails() {
    let mut receiver = MpeReceiver::<4>::new().unwrap();
    rpn(&mut receiver, 0, 6, 2);
    receiver.apply(message(&[0xe1, 127, 127]));
    receiver.apply(message(&[0xd1, 64]));
    receiver.apply(cc(1, 74, 32));
    receiver.apply(message(&[0x91, 60, 127]));
    receiver.apply(cc(0, 64, 127));
    receiver.apply(message(&[0x81, 60, 0]));
    assert_eq!(receiver.voices()[0].phase, VoicePhase::Sustained);
    let old = receiver.voices()[0].expression;
    receiver.apply(message(&[0xe1, 0, 0]));
    receiver.apply(message(&[0xd1, 127]));
    receiver.apply(cc(1, 74, 127));
    receiver.apply(message(&[0xa1, 60, 127]));
    assert_eq!(receiver.voices()[0].expression, old);
    receiver.apply(message(&[0xe0, 127, 127]));
    receiver.apply(message(&[0xd0, 127]));
    receiver.apply(cc(0, 74, 127));
    let expr = receiver.voices()[0].expression;
    assert_eq!(expr.member_bend, old.member_bend);
    assert_eq!(expr.pressure, old.pressure);
    assert_eq!(expr.timbre, old.timbre);
    assert_eq!(expr.master_bend, 2.);
    assert_eq!(expr.master_pressure, 1.);
    assert_eq!(expr.master_timbre, 1.);
    receiver.apply(cc(1, 64, 0));
    receiver.apply(cc(1, 120, 0));
    receiver.apply(cc(1, 123, 0));
    receiver.apply(cc(1, 121, 0));
    assert_eq!(receiver.voices()[0].phase, VoicePhase::Sustained);
    receiver.apply(cc(0, 64, 0));
    assert_eq!(receiver.voices()[0].phase, VoicePhase::Released);
    receiver.apply(message(&[0xe0, 0, 0]));
    assert_eq!(receiver.voices()[0].expression.master_bend, -2.);
    assert_eq!(receiver.voices()[0].expression.member_bend, 48.);
    receiver.apply(message(&[0x91, 64, 127]));
    assert_eq!(receiver.voices()[1].expression.member_bend, -48.);
    receiver.apply(cc(0, 120, 0));
    assert!(
        receiver
            .voices()
            .iter()
            .all(|v| v.phase == VoicePhase::Idle)
    );
    receiver.reset();
    assert!(receiver.zones().0.is_some());
    assert_eq!(receiver.bend_ranges(ZoneSide::Lower), (48., 2.));
}
#[test]
fn prepared_receiver_heap_reconfiguration_polyphonic_fallback_and_zone_isolation() {
    let mut receiver = MpeReceiver::<128>::new().unwrap();
    let counts = allocation_counter::measure(|| {
        for _ in 0..10_000 {
            rpn(&mut receiver, 0, 6, 2);
            rpn(&mut receiver, 15, 6, 2);
            receiver.apply(message(&[0x91, 60, 127]));
            receiver.apply(message(&[0x9e, 64, 127]));
            receiver.apply(cc(0, 120, 0));
            assert_eq!(receiver.voices()[1].phase, VoicePhase::Held);
            receiver.apply(cc(15, 121, 0));
            receiver.apply(message(&[0xff]));
            rpn(&mut receiver, 0, 6, 0);
            rpn(&mut receiver, 15, 6, 0);
            assert_eq!(receiver.zones(), (None, None));
            assert!(
                receiver
                    .apply(message(&[0x97, 72, 127]))
                    .voices
                    .started
                    .is_some()
            );
            receiver.finish(0).unwrap();
            receiver.reset();
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}
