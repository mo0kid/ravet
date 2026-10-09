use ravet_foundation::*;
use ravet_instrument::{
    MidiProgramDecoder, MidiProgramRequest, MpeReceiver, NegotiatedVoices, VoiceMode, ZoneSide,
};
fn cc(channel: u8, controller: u8, value: u8) -> MidiMessage {
    MidiMessage::from_bytes(&[0xb0 | channel, controller, value]).unwrap()
}
fn pc(channel: u8, program: u8) -> MidiMessage {
    MidiMessage::from_bytes(&[0xc0 | channel, program]).unwrap()
}

#[test]
fn bank_halves_stage_in_either_order_channels_are_independent_and_only_pc_requests() {
    let mut decoder = MidiProgramDecoder::default();
    for channel in 0..16 {
        assert_eq!(decoder.pending_bank(channel), Some(0));
        assert_eq!(decoder.last_request(channel), None);
        assert_eq!(decoder.apply(cc(channel, 32, channel)), None);
        assert_eq!(decoder.apply(cc(channel, 0, 127 - channel)), None);
        let request = MidiProgramRequest {
            channel,
            bank: u16::from(127 - channel) * 128 + u16::from(channel),
            program: channel,
        };
        assert_eq!(decoder.apply(pc(channel, channel)), Some(request));
        assert_eq!(decoder.apply(pc(channel, channel)), Some(request));
        for controller in [120, 121, 123] {
            decoder.apply(cc(channel, controller, 0));
        }
        assert_eq!(decoder.last_request(channel), Some(request));
        decoder.apply(cc(channel, 0, 0));
        assert_eq!(decoder.pending_bank(channel), Some(u16::from(channel)));
        assert_eq!(decoder.last_request(channel), Some(request));
        decoder.apply(cc(channel, 32, 127));
        assert_eq!(
            decoder.apply(pc(channel, 127)),
            Some(MidiProgramRequest {
                channel,
                bank: 127,
                program: 127
            })
        );
    }
    assert_eq!(decoder.pending_bank(16), None);
    assert_eq!(decoder.last_request(255), None);
    decoder.apply(cc(0, 0, 127));
    decoder.apply(cc(0, 32, 127));
    assert_eq!(decoder.apply(pc(0, 127)).unwrap().bank, 16383);
    decoder.apply(MidiMessage::from_bytes(&[0xff]).unwrap());
    for channel in 0..16 {
        assert_eq!(decoder.pending_bank(channel), Some(0));
        assert_eq!(decoder.last_request(channel), None);
    }
}

#[test]
fn unavailable_program_preserves_consumer_state_and_prepared_selection_is_heap_free() {
    let descriptors = [ParameterDescriptor {
        id: ParameterId(1),
        name: "Level",
        unit: "",
        default: 0.2,
        automatable: true,
        range: ParameterRange::new(0., 1., ParameterScale::Linear, None).unwrap(),
    }];
    let schema = ParameterSchema::new(&descriptors).unwrap();
    let programs = [ParameterProgram {
        id: ProgramId(7),
        name: "Quiet",
        midi_bank: 0,
        midi_number: 1,
        values: [0.05],
    }];
    let mut state = schema.defaults();
    let mut decoder = MidiProgramDecoder::default();
    let counts = allocation_counter::measure(|| {
        let bank = ProgramBank::new(schema, &programs).unwrap();
        for _ in 0..10_000 {
            let request = decoder.apply(pc(0, 1)).unwrap();
            state = bank
                .state(bank.midi_index(request.bank, request.program).unwrap())
                .unwrap();
            assert_eq!(state.values(), &[0.05]);
            decoder.apply(cc(0, 0, 127));
            let unavailable = decoder.apply(pc(0, 1)).unwrap();
            assert!(
                bank.midi_index(unavailable.bank, unavailable.program)
                    .is_none()
            );
            assert_eq!(state.values(), &[0.05]);
            decoder.reset();
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}

fn configure<const N: usize>(receiver: &mut MpeReceiver<N>, master: u8, members: u8) {
    receiver.apply(cc(master, 100, 6));
    receiver.apply(cc(master, 101, 0));
    receiver.apply(cc(master, 6, members));
}

#[test]
fn two_zone_program_requests_are_master_only_independent_and_never_modify_voices() {
    let mut receiver = MpeReceiver::<4>::new().unwrap();
    assert_eq!(receiver.apply(pc(7, 3)).program_request.unwrap().channel, 7);
    configure(&mut receiver, 0, 2);
    assert!(receiver.last_program_request(7).is_none());
    configure(&mut receiver, 15, 2);
    receiver.apply(MidiMessage::from_bytes(&[0x91, 60, 127]).unwrap());
    receiver.apply(MidiMessage::from_bytes(&[0x9e, 64, 127]).unwrap());
    let before = *receiver.voices();
    for channel in [1, 2, 7, 13, 14] {
        receiver.apply(cc(channel, 0, 127));
        assert!(receiver.apply(pc(channel, 1)).program_request.is_none());
        assert!(receiver.last_program_request(channel).is_none());
    }
    for (master, bank, program) in [(0, 129, 3), (15, 16383, 127)] {
        receiver.apply(cc(master, 32, (bank & 127) as u8));
        receiver.apply(cc(master, 0, (bank >> 7) as u8));
        let request = MidiProgramRequest {
            channel: master,
            bank,
            program,
        };
        let change = receiver.apply(pc(master, program));
        assert_eq!(change.program_request, Some(request));
        assert_eq!(change.voices.changed, 0);
        assert!(!change.configuration_changed);
        assert_eq!(receiver.last_program_request(master), Some(request));
        receiver.apply(cc(master, 121, 0));
        assert_eq!(
            receiver.apply(pc(master, program)).program_request,
            Some(request)
        );
    }
    assert_eq!(receiver.voices(), &before);
    assert!(receiver.last_program_request(16).is_none());
    configure(&mut receiver, 0, 2); // Repeated zone changes clear both banks.
    assert!(receiver.last_program_request(15).is_none());
    assert_eq!(receiver.apply(pc(15, 1)).program_request.unwrap().bank, 0);
    receiver.apply(MidiMessage::from_bytes(&[0xff]).unwrap());
    assert!(receiver.last_program_request(15).is_none());
    assert!(receiver.zones().1.is_some());
    configure(&mut receiver, 0, 0);
    configure(&mut receiver, 15, 0);
    assert_eq!(receiver.apply(pc(7, 3)).program_request.unwrap().channel, 7);
}

#[test]
fn single_zone_program_routing_range_preservation_reset_and_heap_free_recall_requests() {
    for (side, master, member) in [(ZoneSide::Lower, 0, 1), (ZoneSide::Upper, 15, 14)] {
        let mut receiver = NegotiatedVoices::<2>::new(side).unwrap();
        assert!(receiver.apply(pc(member, 1)).program_request.is_some());
        receiver.apply(cc(master, 100, 6));
        receiver.apply(cc(master, 101, 0));
        receiver.apply(cc(master, 6, 2));
        assert!(matches!(receiver.mode(), VoiceMode::Mpe(_)));
        assert!(receiver.last_program_request(member).is_none());
        let counts = allocation_counter::measure(|| {
            for _ in 0..10_000 {
                receiver.apply(cc(master, 32, 5));
                receiver.apply(cc(master, 0, 2));
                let expected = MidiProgramRequest {
                    channel: master,
                    bank: 261,
                    program: 127,
                };
                assert_eq!(
                    receiver.apply(pc(master, 127)).program_request,
                    Some(expected)
                );
                assert!(receiver.apply(pc(member, 1)).program_request.is_none());
                receiver.apply(cc(master, 101, 0));
                receiver.apply(cc(master, 100, 0));
                assert!(receiver.apply(cc(master, 6, 3)).configuration_changed); // RPN0 range only.
                assert_eq!(receiver.last_program_request(master), Some(expected));
                receiver.apply(cc(master, 121, 0));
                assert_eq!(
                    receiver.apply(pc(master, 127)).program_request,
                    Some(expected)
                );
                receiver.reset();
                assert!(receiver.last_program_request(master).is_none());
                assert_eq!(
                    receiver.apply(pc(master, 0)).program_request.unwrap().bank,
                    0
                );
            }
        });
        assert_eq!(counts.count_total, 0);
        assert_eq!(counts.count_current, 0);
        receiver.apply(cc(master, 0, 127));
        receiver.apply(MidiMessage::from_bytes(&[0xff]).unwrap());
        assert_eq!(
            receiver.apply(pc(master, 0)).program_request.unwrap().bank,
            0
        );
        assert!(receiver.last_program_request(255).is_none());
    }
}
