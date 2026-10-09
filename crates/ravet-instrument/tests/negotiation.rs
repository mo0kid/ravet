use ravet_foundation::MidiMessage;
use ravet_instrument::{NegotiatedVoices, RpnDecoder, VoiceMode, VoicePhase, ZoneSide};
fn msg(bytes: &[u8]) -> MidiMessage {
    MidiMessage::from_bytes(bytes).unwrap()
}
fn cc(channel: u8, controller: u8, value: u8) -> MidiMessage {
    msg(&[0xb0 | channel, controller, value])
}
fn rpn<const N: usize>(
    voices: &mut NegotiatedVoices<N>,
    channel: u8,
    parameter: u8,
    value: u8,
) -> ravet_instrument::NegotiatedChanges {
    voices.apply(cc(channel, 100, parameter));
    voices.apply(cc(channel, 101, 0));
    voices.apply(cc(channel, 6, value))
}
#[test]
fn selectors_are_channel_local_and_null_nrpn_reset_cancel_data() {
    let mut decoder = RpnDecoder::default();
    assert!(decoder.apply(cc(0, 6, 12)).is_none());
    decoder.apply(cc(0, 100, 0));
    decoder.apply(cc(0, 101, 0));
    assert!(decoder.apply(cc(1, 6, 12)).is_none());
    assert!(decoder.apply(cc(0, 38, 25)).is_none());
    let coarse = decoder.apply(cc(0, 6, 12)).unwrap();
    assert_eq!(
        (coarse.parameter, coarse.msb, coarse.lsb, coarse.coarse),
        (0, 12, 0, true)
    );
    let fine = decoder.apply(cc(0, 38, 25)).unwrap();
    assert_eq!((fine.msb, fine.lsb, fine.coarse), (12, 25, false));
    assert_eq!(decoder.apply(cc(0, 6, 24)).unwrap().lsb, 0);
    decoder.apply(cc(0, 99, 0));
    assert!(decoder.apply(cc(0, 6, 1)).is_none());
    decoder.apply(cc(0, 101, 0));
    decoder.apply(cc(0, 100, 6));
    assert!(decoder.apply(cc(0, 38, 1)).is_none());
    assert_eq!(decoder.apply(cc(0, 6, 3)).unwrap().parameter, 6);
    decoder.apply(cc(0, 100, 127));
    decoder.apply(cc(0, 101, 127));
    assert!(decoder.apply(cc(0, 6, 1)).is_none());
    decoder.apply(cc(0, 101, 0));
    decoder.apply(cc(0, 100, 0));
    decoder.apply(cc(0, 121, 0));
    assert!(decoder.apply(cc(0, 6, 1)).is_none());
    decoder.apply(cc(0, 101, 0));
    decoder.apply(cc(0, 100, 0));
    decoder.apply(msg(&[0xff]));
    assert!(decoder.apply(cc(0, 6, 1)).is_none());
}
#[test]
fn lower_zone_defaults_live_ranges_tail_freeze_and_reconfiguration() {
    let mut voices = NegotiatedVoices::<4>::new(ZoneSide::Lower).unwrap();
    voices.apply(msg(&[0x90, 60, 127]));
    assert!(!rpn(&mut voices, 15, 6, 2).configuration_changed);
    assert!(!rpn(&mut voices, 0, 6, 16).configuration_changed);
    assert_eq!(voices.voices()[0].phase, VoicePhase::Held);
    let enabled = rpn(&mut voices, 0, 6, 2);
    assert!(enabled.configuration_changed);
    assert_eq!(enabled.voices.changed, 1);
    assert_eq!(voices.bend_ranges(), (48., 2.));
    assert!(voices.voices().iter().all(|v| v.phase == VoicePhase::Idle));
    assert!(voices.apply(msg(&[0x93, 60, 127])).voices.started.is_none());
    voices.apply(msg(&[0x91, 60, 127]));
    voices.apply(msg(&[0x92, 64, 127]));
    voices.apply(msg(&[0xe1, 127, 127]));
    voices.apply(msg(&[0xe2, 0, 0]));
    assert_eq!(rpn(&mut voices, 1, 0, 12).voices.changed, 3);
    assert_eq!(voices.voices()[0].expression.member_bend, 12.);
    assert_eq!(voices.voices()[1].expression.member_bend, -12.);
    voices.apply(msg(&[0xe0, 127, 127]));
    assert_eq!(rpn(&mut voices, 0, 0, 7).voices.changed, 3);
    assert_eq!(voices.voices()[1].expression.master_bend, 7.);
    assert!(!rpn(&mut voices, 3, 0, 24).configuration_changed);
    assert!(!rpn(&mut voices, 0, 0, 97).configuration_changed);
    voices.apply(msg(&[0x81, 60, 0]));
    let tail = voices.voices()[0].expression;
    rpn(&mut voices, 2, 0, 0);
    assert_eq!(voices.voices()[0].expression, tail);
    assert_eq!(voices.voices()[1].expression.member_bend, 0.);
    let disabled = rpn(&mut voices, 0, 6, 0);
    assert_eq!(disabled.voices.changed, 3);
    assert_eq!(voices.mode(), VoiceMode::Polyphonic);
    assert_eq!(voices.bend_ranges(), (2., 2.));
    assert!(voices.apply(msg(&[0x90, 60, 127])).voices.started.is_some());
}
#[test]
fn upper_zone_fine_data_ignored_reset_preserves_configuration_and_no_heap() {
    let mut voices = NegotiatedVoices::<128>::new(ZoneSide::Upper).unwrap();
    rpn(&mut voices, 15, 6, 15);
    assert!(!voices.apply(cc(15, 38, 0)).configuration_changed);
    assert!(voices.apply(msg(&[0x90, 60, 127])).voices.started.is_some());
    assert!(voices.apply(msg(&[0x9f, 60, 127])).voices.started.is_none());
    rpn(&mut voices, 0, 0, 96);
    voices.apply(msg(&[0xe0, 127, 127]));
    voices.apply(cc(0, 38, 99));
    assert_eq!(voices.voices()[0].expression.member_bend, 96.);
    voices.reset();
    assert_eq!(voices.bend_ranges(), (96., 2.));
    assert!(!voices.apply(cc(15, 6, 0)).configuration_changed);
    let counts = allocation_counter::measure(|| {
        for _ in 0..10_000 {
            rpn(&mut voices, 15, 6, 2);
            voices.apply(msg(&[0x9e, 60, 127]));
            voices.apply(msg(&[0xee, 127, 127]));
            voices.apply(cc(15, 64, 127));
            voices.apply(msg(&[0x8e, 60, 0]));
            rpn(&mut voices, 14, 0, 12);
            assert_eq!(voices.voices()[0].phase, VoicePhase::Sustained);
            assert_eq!(voices.voices()[0].expression.member_bend, 12.);
            voices.apply(msg(&[0xff]));
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    assert!(NegotiatedVoices::<0>::new(ZoneSide::Lower).is_err());
}
