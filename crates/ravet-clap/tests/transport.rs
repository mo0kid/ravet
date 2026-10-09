use clack_plugin::{
    events::{
        EventHeader,
        event_types::{TransportEvent, TransportFlags},
    },
    utils::{BeatTime, SecondsTime},
};
use ravet_clap::{BridgeError, translate_transport};
use ravet_foundation::{Transport, TransportDetails};
fn context() -> TransportEvent {
    TransportEvent {
        header: EventHeader::default(),
        flags: TransportFlags::IS_PLAYING
            | TransportFlags::IS_RECORDING
            | TransportFlags::IS_LOOP_ACTIVE
            | TransportFlags::IS_WITHIN_PRE_ROLL
            | TransportFlags::HAS_TIME_SIGNATURE
            | TransportFlags::HAS_TEMPO
            | TransportFlags::HAS_BEATS_TIMELINE
            | TransportFlags::HAS_SECONDS_TIMELINE,
        song_pos_beats: BeatTime::from_float(-0.25),
        song_pos_seconds: SecondsTime::from_float(-0.5),
        tempo: 123.5,
        tempo_inc: -0.001,
        loop_start_beats: BeatTime::from_float(-4.),
        loop_end_beats: BeatTime::from_float(8.),
        loop_start_seconds: SecondsTime::from_float(-1.),
        loop_end_seconds: SecondsTime::from_float(2.),
        bar_start: BeatTime::from_float(-4.),
        bar_number: -1,
        time_signature_numerator: 7,
        time_signature_denominator: 8,
    }
}
#[test]
fn extended_block_context_preserves_flags_units_and_has_no_heap_action() {
    let native = context();
    let expected = Transport {
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
    };
    let counts = allocation_counter::measure(|| {
        for _ in 0..10000 {
            assert_eq!(
                translate_transport(Some(&native), 48000.).unwrap(),
                expected
            );
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    assert_eq!(translate_transport(None, 48000.).unwrap().details, None);
}
#[test]
fn advertised_invalid_metadata_rejects_and_unadvertised_metadata_is_ignored() {
    let good = context();
    let mut invalid = [good; 7];
    invalid[0].time_signature_numerator = 0;
    invalid[1].time_signature_denominator = 0;
    invalid[2].tempo_inc = f64::NAN;
    invalid[3].tempo_inc = f64::INFINITY;
    invalid[4].loop_end_beats = invalid[4].loop_start_beats;
    invalid[5].loop_end_seconds = invalid[5].loop_start_seconds;
    // Ordered native endpoints collapse to the same sample at 48k.
    invalid[6].loop_start_seconds = SecondsTime::from_bits(0);
    invalid[6].loop_end_seconds = SecondsTime::from_bits(1);
    for native in invalid {
        assert_eq!(
            translate_transport(Some(&native), 48000.),
            Err(BridgeError::InvalidTransport)
        );
    }
    let mut native = invalid[2];
    native.flags = TransportFlags::empty();
    native.time_signature_numerator = 0;
    assert_eq!(
        translate_transport(Some(&native), 48000.).unwrap(),
        Transport {
            playing: Some(false),
            details: Some(TransportDetails::default()),
            ..Transport::default()
        }
    );
    native = good;
    native.flags.remove(TransportFlags::IS_LOOP_ACTIVE);
    native.loop_end_beats = native.loop_start_beats;
    native.loop_end_seconds = native.loop_start_seconds;
    let details = translate_transport(Some(&native), 48000.)
        .unwrap()
        .details
        .unwrap();
    assert_eq!(details.beat_loop, None);
    assert_eq!(details.sample_loop, None);
    // A loop state with neither timeline remains observable without guessed bounds.
    native.flags = TransportFlags::IS_LOOP_ACTIVE;
    let details = translate_transport(Some(&native), 48000.)
        .unwrap()
        .details
        .unwrap();
    assert!(details.loop_active);
    assert_eq!(details.beat_loop, None);
}
