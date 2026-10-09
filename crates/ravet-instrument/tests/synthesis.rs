use ravet_foundation::*;
use ravet_instrument::{INSTRUMENT_LEVEL_ID, SineInstrument, VoicePhase, ZoneSide};
fn event(offset: usize, data: EventData) -> Event {
    Event {
        frame_offset: offset,
        data,
    }
}
fn note(offset: usize, on: bool) -> Event {
    event(
        offset,
        if on {
            EventData::NoteOn {
                channel: 0,
                key: 69,
                velocity: 1.,
            }
        } else {
            EventData::NoteOff {
                channel: 0,
                key: 69,
                velocity: 0.,
            }
        },
    )
}
fn render<const N: usize>(
    synth: &mut SineInstrument<N>,
    output: &mut [f32],
    events: &[Event],
) -> ProcessStatus {
    let frames = output.len();
    let mut channels = [output];
    let audio = AudioBuffer::separate(&[], &mut channels, frames).unwrap();
    synth.process(&mut ProcessBlock::new(audio, events, Transport::default()))
}
fn prepared<const N: usize>() -> SineInstrument<N> {
    let mut synth = SineInstrument::new(ZoneSide::Lower).unwrap();
    synth
        .prepare(ProcessConfig::new(48000., 4096, 16, AudioLayout::MONO_INSTRUMENT).unwrap())
        .unwrap();
    synth
}
#[test]
fn sample_timing_analytic_attack_split_blocks_release_and_reset() {
    let mut synth = prepared::<1>();
    let mut output = [99.; 300];
    let events = [
        note(3, true),
        event(
            3,
            EventData::ParameterValue {
                id: INSTRUMENT_LEVEL_ID,
                value: 1.,
            },
        ),
    ];
    assert_eq!(
        render(&mut synth, &mut output, &events),
        ProcessStatus::Continue
    );
    assert_eq!(output[..4], [0.; 4]);
    for (i, &sample) in output[3..243].iter().enumerate() {
        let expected =
            (std::f64::consts::TAU * 440. / 48000. * i as f64).sin() * (i + 1) as f64 / 240.;
        assert!((f64::from(sample) - expected).abs() < 1e-6);
    }
    let mut split = prepared::<1>();
    let mut partitioned = [0.; 300];
    render(&mut split, &mut partitioned[..100], &events);
    render(&mut split, &mut partitioned[100..], &[]);
    assert_eq!(partitioned, output);
    let mut release = [99.; 2401];
    render(&mut synth, &mut release, &[note(0, false)]);
    assert!(release[..2399].iter().any(|s| s.abs() > 0.01));
    assert_eq!(release[2399..], [0., 0.]);
    assert_eq!(synth.voices()[0].phase, VoicePhase::Idle);
    synth.reset();
    render(&mut synth, &mut output, &[]);
    assert_eq!(output, [0.; 300]);
    assert_eq!(synth.level(), 1.);
}
#[test]
fn zero_events_fault_atomicity_stereo_and_inactive_failure() {
    let mut synth = prepared::<2>();
    render(&mut synth, &mut [], &[note(0, true)]);
    assert_eq!(synth.voices()[0].phase, VoicePhase::Held);
    let mut control = prepared::<2>();
    render(&mut control, &mut [], &[note(0, true)]);
    let bad = [
        event(
            0,
            EventData::ParameterValue {
                id: INSTRUMENT_LEVEL_ID,
                value: 0.9,
            },
        ),
        note(4, false),
    ];
    let mut rejected = [99.; 4];
    assert!(matches!(
        render(&mut synth, &mut rejected, &bad),
        ProcessStatus::Fault(_)
    ));
    assert_eq!(rejected, [0.; 4]);
    assert_eq!(synth.level(), 0.2);
    let mut a = [0.; 50];
    let mut b = [0.; 50];
    render(&mut synth, &mut a, &[]);
    render(&mut control, &mut b, &[]);
    assert_eq!(a, b);
    synth
        .prepare(ProcessConfig::new(48000., 8, 16, AudioLayout::STEREO_INSTRUMENT).unwrap())
        .unwrap();
    let mut left = [99.; 10];
    let mut right = [88.; 10];
    let mut channels = [&mut left[..], &mut right[..]];
    let audio = AudioBuffer::separate(&[], &mut channels, 8).unwrap();
    assert_eq!(
        synth.process(&mut ProcessBlock::new(
            audio,
            &[note(0, true)],
            Transport::default()
        )),
        ProcessStatus::Continue
    );
    assert_eq!(left[..8], right[..8]);
    assert_eq!(left[8..], [99.; 2]);
    assert_eq!(right[8..], [88.; 2]);
    assert!(
        synth
            .prepare(ProcessConfig::new(48000., 8, 1, AudioLayout::MONO).unwrap())
            .is_err()
    );
    assert!(matches!(
        render(&mut synth, &mut rejected, &[]),
        ProcessStatus::Fault(ProcessError::NotPrepared)
    ));
    assert!(SineInstrument::<0>::new(ZoneSide::Lower).is_err());
}
#[test]
fn negotiated_bend_sustain_stealing_and_prepared_heap_contract() {
    let mut synth = prepared::<1>();
    let midi = |bytes: &[u8]| event(0, EventData::Midi(MidiMessage::from_bytes(bytes).unwrap()));
    render(
        &mut synth,
        &mut [],
        &[
            midi(&[0xb0, 101, 0]),
            midi(&[0xb0, 100, 6]),
            midi(&[0xb0, 6, 2]),
            midi(&[0x91, 69, 127]),
            midi(&[0xe1, 127, 127]),
            midi(&[0xb0, 64, 127]),
            midi(&[0x81, 69, 0]),
        ],
    );
    assert_eq!(synth.voices()[0].phase, VoicePhase::Sustained);
    assert_eq!(synth.voices()[0].expression.member_bend, 48.);
    let mut output = [0.; 128];
    render(&mut synth, &mut output, &[]);
    assert!(output.iter().any(|s| s.abs() > 0.01));
    render(&mut synth, &mut [], &[midi(&[0x92, 60, 127])]);
    assert_eq!(synth.voices()[0].note.unwrap().channel, 2);
    let counts = allocation_counter::measure(|| {
        for _ in 0..10_000 {
            synth.reset();
            render(
                &mut synth,
                &mut output,
                &[midi(&[0x91, 69, 127]), midi(&[0xe0, 127, 127])],
            );
            render(&mut synth, &mut output, &[midi(&[0x81, 69, 0])]);
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}

#[test]
fn host_rate_boundaries_prepare_without_panics_and_reject_envelope_overflow() {
    let mut synth = SineInstrument::<1>::new(ZoneSide::Lower).unwrap();
    for rate in [1., 8000., 48000., 384000., 768000.] {
        synth
            .prepare(ProcessConfig::new(rate, 16, 4, AudioLayout::MONO_INSTRUMENT).unwrap())
            .unwrap();
        let mut samples = [0.; 16];
        assert_eq!(
            render(&mut synth, &mut samples, &[note(0, true)]),
            ProcessStatus::Continue
        );
        assert!(samples.iter().all(|s| s.is_finite()));
    }
    assert_eq!(
        synth.prepare(ProcessConfig::new(1e20, 16, 4, AudioLayout::MONO_INSTRUMENT).unwrap()),
        Err(PrepareError::InvalidSmoothingTime)
    );
    assert!(matches!(
        render(&mut synth, &mut [1.; 16], &[]),
        ProcessStatus::Fault(ProcessError::NotPrepared)
    ));
}

#[test]
fn sample_timed_sostenuto_keeps_audio_until_pedal_release_and_matches_split_blocks() {
    let midi = |offset, bytes: &[u8]| {
        event(
            offset,
            EventData::Midi(MidiMessage::from_bytes(bytes).unwrap()),
        )
    };
    let mut synth = prepared::<1>();
    let mut split = prepared::<1>();
    let events = [note(0, true), midi(10, &[0xb0, 66, 64]), note(20, false)];
    let mut prelude = [0.; 512];
    render(&mut synth, &mut prelude, &events);
    render(&mut split, &mut prelude, &events);
    assert_eq!(synth.voices()[0].phase, VoicePhase::Sustained);
    assert!(synth.voices()[0].sostenuto_held());
    let mut held = [0.; 3000];
    render(&mut synth, &mut held, &[]);
    render(&mut split, &mut held, &[]);
    assert!(held[2500..].iter().any(|s| s.abs() > 0.01));
    let mut whole = [99.; 2500];
    let mut partitioned = [99.; 2500];
    let end = [midi(7, &[0xb0, 66, 63])];
    let counts = allocation_counter::measure(|| {
        render(&mut synth, &mut whole, &end);
        render(&mut split, &mut partitioned[..101], &end);
        render(&mut split, &mut partitioned[101..], &[]);
    });
    assert_eq!(whole, partitioned);
    assert!(whole[..2406].iter().any(|s| s.abs() > 0.01));
    assert!(whole[2406..].iter().all(|s| *s == 0.));
    assert_eq!(synth.voices()[0].phase, VoicePhase::Idle);
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}

#[test]
fn factory_programs_switch_level_at_exact_samples_without_restarting_voices() {
    let midi = |offset, bytes: &[u8]| {
        event(
            offset,
            EventData::Midi(MidiMessage::from_bytes(bytes).unwrap()),
        )
    };
    let mut synth = prepared::<1>();
    let mut split = prepared::<1>();
    let mut full = prepared::<1>();
    let events = [
        note(0, true),
        midi(100, &[0xc0, 2]),
        midi(200, &[0xc0, 1]),
        midi(300, &[0xc0, 0]),
    ];
    let mut output = [0.; 512];
    let mut partitioned = [0.; 512];
    let mut reference = [0.; 512];
    render(
        &mut full,
        &mut reference,
        &[
            note(0, true),
            event(
                0,
                EventData::ParameterValue {
                    id: INSTRUMENT_LEVEL_ID,
                    value: 1.,
                },
            ),
        ],
    );
    let counts = allocation_counter::measure(|| {
        render(&mut synth, &mut output, &events);
        render(&mut split, &mut partitioned[..101], &events[..2]);
        let later = [midi(99, &[0xc0, 1]), midi(199, &[0xc0, 0])];
        render(&mut split, &mut partitioned[101..], &later);
    });
    assert_eq!(partitioned, output);
    for (i, &value) in output.iter().enumerate() {
        let level = match i {
            0..100 | 300.. => 0.2,
            100..200 => 1.,
            _ => 0.05,
        };
        assert!((value - reference[i] * level).abs() < 1e-7);
    }
    assert_eq!(synth.voices(), split.voices());
    assert_eq!(synth.voices()[0].phase, VoicePhase::Held);
    assert_eq!(synth.level(), 0.2);
    assert_eq!(synth.programs().programs().len(), 3);
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}

#[test]
fn chosen_master_program_routing_bank_reset_order_and_fault_preservation() {
    let midi = |bytes: &[u8]| event(0, EventData::Midi(MidiMessage::from_bytes(bytes).unwrap()));
    for (side, master, member) in [(ZoneSide::Lower, 0, 1), (ZoneSide::Upper, 15, 14)] {
        let mut synth = SineInstrument::<1>::new(side).unwrap();
        synth
            .prepare(ProcessConfig::new(48000., 4096, 16, AudioLayout::MONO_INSTRUMENT).unwrap())
            .unwrap();
        render(&mut synth, &mut [], &[midi(&[0xc0 | member, 2])]);
        assert_eq!(synth.level(), 0.2);
        render(&mut synth, &mut [], &[midi(&[0xc0 | master, 1])]);
        assert_eq!(synth.level(), 0.05);
        render(
            &mut synth,
            &mut [],
            &[
                midi(&[0xb0 | master, 0, 1]),
                midi(&[0xc0 | master, 2]),
                midi(&[0xb0 | master, 121, 0]),
                midi(&[0xc0 | master, 2]),
            ],
        );
        assert_eq!(synth.level(), 0.05); // Bank 128 is unavailable, CC121 preserves it.
        synth.reset();
        render(&mut synth, &mut [], &[midi(&[0xc0 | master, 2])]);
        assert_eq!(synth.level(), 1.);
        render(
            &mut synth,
            &mut [],
            &[
                midi(&[0xb0 | master, 101, 0]),
                midi(&[0xb0 | master, 100, 6]),
                midi(&[0xb0 | master, 6, 2]),
                midi(&[0x90 | member, 69, 127]),
                midi(&[0xc0 | member, 1]),
            ],
        );
        assert_eq!(synth.level(), 1.);
        assert_eq!(synth.voices()[0].phase, VoicePhase::Held);
        let voice = synth.voices()[0];
        render(&mut synth, &mut [], &[midi(&[0xc0 | master, 1])]);
        assert_eq!(synth.level(), 0.05);
        assert_eq!(synth.voices()[0], voice);
        render(
            &mut synth,
            &mut [],
            &[
                midi(&[0xc0 | master, 2]),
                event(
                    0,
                    EventData::ParameterValue {
                        id: INSTRUMENT_LEVEL_ID,
                        value: 0.7,
                    },
                ),
            ],
        );
        assert_eq!(synth.level(), 0.7);
        render(
            &mut synth,
            &mut [],
            &[
                event(
                    0,
                    EventData::ParameterValue {
                        id: INSTRUMENT_LEVEL_ID,
                        value: 0.7,
                    },
                ),
                midi(&[0xc0 | master, 0]),
            ],
        );
        assert_eq!(synth.level(), 0.2);
        let bad = [
            midi(&[0xc0 | master, 2]),
            Event {
                frame_offset: 1,
                data: EventData::Midi(MidiMessage::from_bytes(&[0xb0 | master, 0, 127]).unwrap()),
            },
        ];
        assert!(matches!(
            render(&mut synth, &mut [99.], &bad),
            ProcessStatus::Fault(_)
        ));
        assert_eq!(synth.level(), 0.2);
        render(&mut synth, &mut [], &[midi(&[0xc0 | master, 1])]);
        assert_eq!(synth.level(), 0.05); // Bad block did not stage its bank.
        render(
            &mut synth,
            &mut [],
            &[
                midi(&[0xb0 | master, 32, 1]),
                midi(&[0xff]),
                midi(&[0xc0 | master, 2]),
            ],
        );
        assert_eq!(synth.level(), 1.);
        assert_eq!(synth.voices()[0].phase, VoicePhase::Idle);
    }
}
