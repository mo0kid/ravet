use ravet::foundation::*;
use ravet_gain::state::{GainState, decode_parameters, encode_parameters};
use ravet_gain::{BYPASS_ID, GAIN_ID, GainProcessor, parameter_schema};

fn event(frame_offset: usize, id: ParameterId, value: f32) -> Event {
    Event {
        frame_offset,
        data: EventData::ParameterValue { id, value },
    }
}
fn processor(smoothing: f64) -> GainProcessor {
    let mut p = GainProcessor::new(smoothing).unwrap();
    p.prepare(ProcessConfig::new(1000.0, 8, 8, AudioLayout::MONO).unwrap())
        .unwrap();
    p
}
fn process(p: &mut GainProcessor, samples: &mut [f32], events: &[Event]) -> ProcessStatus {
    let len = samples.len();
    let mut channels = [samples];
    let audio = AudioBuffer::in_place(&mut channels, len).unwrap();
    p.process(&mut ProcessBlock::new(audio, events, Transport::default()))
}

#[test]
fn bypass_events_are_sample_timed_and_smoothing_keeps_advancing() {
    let mut p = processor(4.0);
    let mut samples = [1.0; 4];
    assert_eq!(
        process(
            &mut p,
            &mut samples,
            &[
                event(0, GAIN_ID, 0.0),
                event(0, BYPASS_ID, 1.0),
                event(2, BYPASS_ID, 0.0)
            ]
        ),
        ProcessStatus::Continue
    );
    assert_eq!(samples, [1.0, 1.0, 0.25, 0.0]);
    assert_eq!(p.requested_gain(), 0.0);
    assert!(!p.bypassed());
    let mut samples = [1.0; 4];
    process(
        &mut p,
        &mut samples,
        &[event(1, BYPASS_ID, 0.5), event(3, BYPASS_ID, 0.49)],
    );
    assert_eq!(samples, [0.0, 1.0, 1.0, 0.0]);
}

#[test]
fn meters_measure_input_and_output_and_retain_on_empty_or_fault() {
    let mut p = processor(0.0);
    let mut samples = [1.0, -1.0, 0.0, 0.0];
    process(&mut p, &mut samples, &[event(0, GAIN_ID, 2.0)]);
    let meters = p.meters();
    assert_eq!(meters.input[0].peak, 1.0);
    assert_eq!(meters.output[0].peak, 2.0);
    assert!((meters.output[0].rms - 2.0_f32.sqrt()).abs() < 1e-6);
    assert_eq!(meters.input[0].samples, 4);
    assert_eq!(meters.output[1].samples, 0);
    process(&mut p, &mut [], &[event(0, BYPASS_ID, 1.0)]);
    assert!(p.bypassed());
    assert_eq!(p.meters(), meters);
    assert!(matches!(
        process(
            &mut p,
            &mut samples,
            &[event(0, BYPASS_ID, 0.0), event(3, GAIN_ID, f32::NAN)]
        ),
        ProcessStatus::Fault(_)
    ));
    assert!(p.bypassed());
    assert_eq!(p.meters(), meters);
    assert_eq!(samples, [0.0; 4]);
    p.reset();
    assert!(p.bypassed());
    assert_eq!(p.meters(), Default::default());
}

#[test]
fn stereo_separate_meters_preserve_tails_and_audio_nonfinites() {
    let mut p = GainProcessor::new(0.0).unwrap();
    p.prepare(ProcessConfig::new(48_000.0, 4, 2, AudioLayout::STEREO).unwrap())
        .unwrap();
    let left = [0.5, -0.5, 7.0];
    let right = [f32::NAN, 2.0, 7.0];
    let inputs = [&left[..], &right[..]];
    let mut out_left = [99.0; 3];
    let mut out_right = [99.0; 3];
    let mut outputs = [&mut out_left[..], &mut out_right[..]];
    let audio = AudioBuffer::separate(&inputs, &mut outputs, 2).unwrap();
    p.process(&mut ProcessBlock::new(audio, &[], Transport::default()));
    assert_eq!(out_left, [0.5, -0.5, 99.0]);
    assert!(out_right[0].is_nan());
    assert_eq!(out_right[2], 99.0);
    assert_eq!(p.meters().input[0].rms, 0.5);
    assert!(p.meters().output[1].nonfinite);
    assert!((p.meters().output[1].rms - 2.0_f32.sqrt()).abs() < 1e-6);
}

#[test]
fn complete_state_roundtrip_legacy_migration_and_failed_restore_are_atomic() {
    let mut p = processor(4.0);
    let state = parameter_schema().state([0.5, 1.0]).unwrap();
    let bytes = encode_parameters(&state).unwrap();
    let restored = decode_parameters(&bytes).unwrap();
    p.restore_parameters(&restored).unwrap();
    assert_eq!(p.effective_gain(), 0.5);
    assert!(p.bypassed());
    let legacy = GainState::new(2.0).unwrap().encode();
    let migrated = decode_parameters(&legacy).unwrap();
    assert_eq!(migrated.values(), &[2.0, 0.0]);
    p.restore_parameters(&migrated).unwrap();
    assert_eq!(p.effective_gain(), 2.0);
    assert!(!p.bypassed());
    let mut damaged = bytes;
    damaged[28..32].copy_from_slice(&0.5_f32.to_le_bytes());
    assert!(decode_parameters(&damaged).is_err());
    let mut descriptors = *parameter_schema().descriptors();
    descriptors[0].range = ParameterRange::new(0.0, 10.0, ParameterScale::Linear, None).unwrap();
    let alternate = ParameterSchema::new(&descriptors).unwrap();
    let invalid = alternate.state([5.0, 1.0]).unwrap();
    assert!(p.restore_parameters(&invalid).is_err());
    assert_eq!(p.parameter_state().values(), &[2.0, 0.0]);
    let mut samples = [1.0];
    process(&mut p, &mut samples, &[]);
    assert_eq!(samples, [2.0]);
}

#[cfg(target_has_atomic = "64")]
#[test]
fn bypass_restore_meter_publication_and_codec_do_not_allocate() {
    use allocation_counter::measure;
    use ravet_gain::GainMeterMailbox;
    let mut p = processor(4.0);
    let mailbox = GainMeterMailbox::default();
    let state = parameter_schema().state([0.25, 1.0]).unwrap();
    let mut samples = [1.0; 8];
    let info = measure(|| {
        for _ in 0..1000 {
            let bytes = encode_parameters(&state).unwrap();
            let state = decode_parameters(&bytes).unwrap();
            p.restore_parameters(&state).unwrap();
            samples.fill(1.0);
            process(&mut p, &mut samples, &[event(3, BYPASS_ID, 0.0)]);
            mailbox.try_publish(p.meters()).unwrap();
            assert_eq!(mailbox.try_read().unwrap(), p.meters());
            p.reset();
        }
    });
    assert_eq!(info.count_total, 0);
    assert_eq!(info.count_current, 0);
    assert_eq!(info.bytes_current, 0);
}

#[test]
fn bypass_same_frame_order_partitioning_and_reactivation_are_consistent() {
    let events = [
        event(0, GAIN_ID, 0.0),
        event(1, BYPASS_ID, 1.0),
        event(1, BYPASS_ID, 0.0),
        event(2, BYPASS_ID, 1.0),
        event(5, BYPASS_ID, 0.0),
    ];
    let mut whole = processor(4.0);
    let mut expected = [1.0; 8];
    process(&mut whole, &mut expected, &events);
    assert_eq!(expected, [0.75, 0.5, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0]);
    let mut split = processor(4.0);
    let mut actual = [1.0; 8];
    let mut start = 0;
    for len in [2, 1, 3, 2] {
        let block_events: Vec<_> = events
            .iter()
            .filter(|e| e.frame_offset >= start && e.frame_offset < start + len)
            .map(|e| Event {
                frame_offset: e.frame_offset - start,
                data: e.data,
            })
            .collect();
        process(&mut split, &mut actual[start..start + len], &block_events);
        start += len;
    }
    assert_eq!(actual, expected);
    process(&mut split, &mut [], &[event(0, BYPASS_ID, 1.0)]);
    split.deactivate();
    split
        .prepare(ProcessConfig::new(1000.0, 8, 8, AudioLayout::MONO).unwrap())
        .unwrap();
    assert!(split.bypassed());
    assert_eq!(split.meters(), Default::default());
    let mut samples = [0.25];
    process(&mut split, &mut samples, &[]);
    assert_eq!(samples, [0.25]);
}
