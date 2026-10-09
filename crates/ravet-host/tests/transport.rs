use clack_host::{events::event_types::TransportFlags, utils::BeatTime};
use ravet_foundation::{ProcessError, Transport};
use ravet_host::hosted_transport;
#[test]
fn absent_partial_and_known_playing_context_preserve_flags() {
    assert!(
        hosted_transport(Transport::default(), 48000.)
            .unwrap()
            .is_none()
    );
    assert!(
        hosted_transport(
            Transport {
                discontinuity: true,
                ..Transport::default()
            },
            48000.
        )
        .unwrap()
        .is_none()
    );
    let event = hosted_transport(
        Transport {
            playing: Some(false),
            ..Transport::default()
        },
        48000.,
    )
    .unwrap()
    .unwrap();
    assert!(event.flags.is_empty());
    assert_eq!(event.header.time(), 0);
    assert_eq!(event.tempo_inc, 0.);
    assert_eq!(event.time_signature_denominator, 0);
    let event = hosted_transport(
        Transport {
            playing: Some(true),
            sample_position: Some(-48000),
            beat_position: Some(-0.25),
            tempo: Some(123.5),
            discontinuity: true,
            ..Transport::default()
        },
        48000.,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        event.flags,
        TransportFlags::IS_PLAYING
            | TransportFlags::HAS_SECONDS_TIMELINE
            | TransportFlags::HAS_BEATS_TIMELINE
            | TransportFlags::HAS_TEMPO
    );
    assert_eq!(event.song_pos_seconds.to_bits(), -BeatTime::FACTOR);
    assert_eq!(event.song_pos_beats.to_bits(), -BeatTime::FACTOR / 4);
    assert_eq!(event.tempo, 123.5);
    assert_eq!(event.loop_end_seconds.to_bits(), 0);
    assert_eq!(event.bar_start.to_bits(), 0);
}
#[test]
fn invalid_values_and_fixed_point_limits_reject_without_saturating() {
    for rate in [0., -1., f64::NAN, f64::INFINITY] {
        assert_eq!(
            hosted_transport(Transport::default(), rate),
            Err(ProcessError::InvalidTransport)
        );
    }
    for tempo in [0., -1., f64::NAN, f64::INFINITY] {
        assert_eq!(
            hosted_transport(
                Transport {
                    playing: Some(true),
                    tempo: Some(tempo),
                    ..Transport::default()
                },
                48000.
            ),
            Err(ProcessError::InvalidTransport)
        );
    }
    for beats in [f64::NAN, f64::INFINITY, 4294967296., -4294967297.] {
        assert_eq!(
            hosted_transport(
                Transport {
                    playing: Some(true),
                    beat_position: Some(beats),
                    ..Transport::default()
                },
                48000.
            ),
            Err(ProcessError::InvalidTransport)
        );
    }
    assert_eq!(
        hosted_transport(
            Transport {
                playing: Some(true),
                beat_position: Some(-4294967296.),
                ..Transport::default()
            },
            48000.
        )
        .unwrap()
        .unwrap()
        .song_pos_beats
        .to_bits(),
        i64::MIN
    );
    for context in [
        Transport {
            tempo: Some(120.),
            ..Transport::default()
        },
        Transport {
            beat_position: Some(0.),
            ..Transport::default()
        },
        Transport {
            sample_position: Some(0),
            ..Transport::default()
        },
        Transport {
            playing: Some(true),
            sample_position: Some(i64::MAX),
            ..Transport::default()
        },
    ] {
        assert_eq!(
            hosted_transport(context, 48000.),
            Err(ProcessError::InvalidTransport)
        );
    }
}
#[test]
fn one_sample_positions_quantize_to_native_ticks_and_mapping_has_no_heap_action() {
    let counts = allocation_counter::measure(|| {
        for samples in [-48001, -1, 0, 1, 47999, 48000] {
            let event = hosted_transport(
                Transport {
                    playing: Some(true),
                    sample_position: Some(samples),
                    ..Transport::default()
                },
                48000.,
            )
            .unwrap()
            .unwrap();
            let expected = ((samples as f64 / 48000.) * 2147483648.).round() as i64;
            assert_eq!(event.song_pos_seconds.to_bits(), expected);
            assert!(
                (event.song_pos_seconds.to_float() * 48000. - samples as f64).abs()
                    <= 48000. / 4294967296.
            );
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}

fn extended_context() -> Transport {
    use ravet_foundation::TransportDetails;
    Transport {
        playing: Some(true),
        sample_position: Some(-24000),
        beat_position: Some(-0.25),
        tempo: Some(123.5),
        discontinuity: false,
        details: Some(TransportDetails {
            recording: true,
            loop_active: true,
            within_pre_roll: true,
            time_signature: Some((7, 8)),
            tempo_increment: Some(-0.001),
            beat_loop: Some((-4., 8.)),
            sample_loop: Some((-48000, 96000)),
            bar: Some((-4., -1)),
        }),
    }
}
#[test]
fn extended_metadata_preserves_native_fields_without_heap_actions() {
    let counts = allocation_counter::measure(|| {
        for _ in 0..10000 {
            let event = hosted_transport(extended_context(), 48000.)
                .unwrap()
                .unwrap();
            assert_eq!(event.flags, TransportFlags::all());
            assert_eq!(event.time_signature_numerator, 7);
            assert_eq!(event.time_signature_denominator, 8);
            assert_eq!(event.tempo_inc, -0.001);
            assert_eq!(event.loop_start_beats.to_float(), -4.);
            assert_eq!(event.loop_end_beats.to_float(), 8.);
            assert_eq!(event.loop_start_seconds.to_float(), -1.);
            assert_eq!(event.loop_end_seconds.to_float(), 2.);
            assert_eq!(event.bar_start.to_float(), -4.);
            assert_eq!(event.bar_number, -1);
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}
#[test]
fn missing_validity_dependencies_and_collapsed_loops_reject() {
    let good = extended_context();
    let mut invalid = [good; 13];
    invalid[0].playing = None;
    invalid[1].details.as_mut().unwrap().time_signature = Some((0, 8));
    invalid[2].details.as_mut().unwrap().time_signature = Some((7, 0));
    invalid[3].details.as_mut().unwrap().tempo_increment = Some(f64::NAN);
    invalid[4].tempo = None;
    invalid[5].beat_position = None;
    invalid[6].sample_position = None;
    invalid[7].details.as_mut().unwrap().beat_loop = None;
    invalid[8].details.as_mut().unwrap().sample_loop = None;
    invalid[9].details.as_mut().unwrap().loop_active = false;
    invalid[10].details.as_mut().unwrap().beat_loop = Some((0., 1e-20));
    invalid[11].details.as_mut().unwrap().beat_loop = Some((8., 4.));
    invalid[12].details.as_mut().unwrap().bar = Some((f64::INFINITY, 0));
    for context in invalid {
        assert_eq!(
            hosted_transport(context, 48000.),
            Err(ProcessError::InvalidTransport)
        );
    }
    let mut context = good;
    context.details.as_mut().unwrap().sample_loop = Some((0, 1));
    assert_eq!(
        hosted_transport(context, 1e20),
        Err(ProcessError::InvalidTransport)
    );
    context = good;
    context.sample_position = None;
    context.beat_position = None;
    context.details.as_mut().unwrap().sample_loop = None;
    context.details.as_mut().unwrap().beat_loop = None;
    context.details.as_mut().unwrap().bar = None;
    assert!(
        hosted_transport(context, 48000.)
            .unwrap()
            .unwrap()
            .flags
            .contains(TransportFlags::IS_LOOP_ACTIVE)
    );
    context.details.as_mut().unwrap().time_signature = Some((5, 3));
    assert_eq!(
        hosted_transport(context, 48000.)
            .unwrap()
            .unwrap()
            .time_signature_denominator,
        3
    );
}
