use ravet_examples::synth::{DEFAULT_VALUES, SubtractiveSynth, SynthPatch};
use ravet_foundation::{
    AudioBuffer, AudioLayout, Event, EventData, MidiMessage, ProcessBlock, ProcessConfig,
    ProcessStatus, Processor, Transport,
};
fn event(frame_offset: usize, bytes: &[u8]) -> Event {
    Event {
        frame_offset,
        data: EventData::Midi(MidiMessage::from_bytes(bytes).unwrap()),
    }
}
fn synth(values: [f32; 9]) -> SubtractiveSynth {
    let mut synth = SubtractiveSynth::new();
    synth
        .set_patch(&SynthPatch::new(48_000., values).unwrap())
        .unwrap();
    synth
        .prepare(ProcessConfig::new(48_000., 1024, 128, AudioLayout::MONO_INSTRUMENT).unwrap())
        .unwrap();
    synth
}
fn process(synth: &mut SubtractiveSynth, output: &mut [f32], events: &[Event]) -> ProcessStatus {
    let frames = output.len();
    let mut views = [output];
    let audio = AudioBuffer::separate(&[], &mut views, frames).unwrap();
    synth.process(&mut ProcessBlock::new(audio, events, Transport::default()))
}
#[test]
fn shared_adsr_hits_targets_and_release_terminates_audio() {
    let mut values = DEFAULT_VALUES;
    values[5] = 1.;
    values[6] = 1.;
    values[7] = 0.5;
    values[8] = 1.;
    let mut synth = synth(values);
    let mut output = [0.; 48];
    assert_eq!(
        process(&mut synth, &mut output, &[event(0, &[0x90, 69, 127])]),
        ProcessStatus::Continue
    );
    assert_eq!(synth.envelope_levels()[0], 1.);
    process(&mut synth, &mut output, &[]);
    assert_eq!(synth.envelope_levels()[0], 0.5);
    process(&mut synth, &mut output, &[event(0, &[0x80, 69, 0])]);
    assert_eq!(synth.envelope_levels()[0], 0.);
    assert_eq!(output[47], 0.);
    output.fill(9.);
    process(&mut synth, &mut output, &[]);
    assert_eq!(output, [0.; 48]);
}
#[test]
fn all_waveforms_sound_and_vcf_cutoff_attenuates_high_frequency() {
    let mut outputs = Vec::new();
    for wave in 0..4 {
        let mut values = DEFAULT_VALUES;
        values[0] = wave as f32;
        values[2] = 18_000.;
        values[4] = 0.;
        values[5] = 0.;
        values[6] = 0.;
        values[7] = 1.;
        let mut s = synth(values);
        let mut output = [0.; 1024];
        process(&mut s, &mut output, &[event(0, &[0x90, 93, 127])]);
        assert!(output.iter().all(|v| v.is_finite()));
        assert!(output.iter().any(|v| v.abs() > 0.01));
        outputs.push(output);
    }
    for i in 0..4 {
        for j in i + 1..4 {
            assert!(
                outputs[i]
                    .iter()
                    .zip(outputs[j])
                    .any(|(a, b)| (a - b).abs() > 0.01)
            );
        }
    }
    let energy = |cutoff: f32| {
        let mut values = DEFAULT_VALUES;
        values[0] = 0.;
        values[2] = cutoff;
        values[4] = 0.;
        values[5] = 0.;
        values[6] = 0.;
        values[7] = 1.;
        let mut s = synth(values);
        let mut out = [0.; 1024];
        process(&mut s, &mut out, &[event(0, &[0x90, 93, 127])]);
        for _ in 0..4 {
            process(&mut s, &mut out, &[]);
        }
        out.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>()
    };
    assert!(energy(100.) < energy(10_000.) * 0.001);
}
#[test]
fn patch_validation_latching_and_whole_block_fault_preserve_history() {
    let mut values = DEFAULT_VALUES;
    values[5] = 10.;
    values[6] = 0.;
    values[7] = 1.;
    let mut s = synth(values);
    let mut out = [0.; 48];
    process(&mut s, &mut out, &[event(0, &[0x90, 69, 100])]);
    let level = s.envelope_levels()[0];
    assert!(level > 0.09 && level < 0.11);
    let mut changed = values;
    changed[0] = 2.;
    changed[5] = 0.;
    s.set_patch(&SynthPatch::new(48_000., changed).unwrap())
        .unwrap();
    process(&mut s, &mut out, &[]);
    assert!(s.envelope_levels()[0] > 0.19 && s.envelope_levels()[0] < 0.21);
    let before = s.envelope_levels();
    out.fill(8.);
    assert!(matches!(
        process(&mut s, &mut out, &[event(48, &[0x90, 60, 100])]),
        ProcessStatus::Fault(_)
    ));
    assert_eq!(out, [0.; 48]);
    assert_eq!(s.envelope_levels(), before);
    process(&mut s, &mut out, &[event(0, &[0x90, 72, 100])]);
    assert_eq!(s.envelope_levels()[1], 1.);
    assert!(
        s.set_patch(&SynthPatch::new(96_000., values).unwrap())
            .is_err()
    );
    for (index, value) in [(0, 0.5), (2, f32::NAN), (3, 9.), (5, -1.)] {
        let mut bad = values;
        bad[index] = value;
        assert!(SynthPatch::new(48_000., bad).is_err());
    }
}
#[test]
fn prepared_synthesis_patch_changes_and_reset_have_no_heap_operations() {
    let mut s = synth(DEFAULT_VALUES);
    let patch = SynthPatch::new(48_000., DEFAULT_VALUES).unwrap();
    let mut out = [0.; 512];
    let counts = allocation_counter::measure(|| {
        for _ in 0..1000 {
            s.set_patch(&patch).unwrap();
            process(
                &mut s,
                &mut out,
                &[event(0, &[0x90, 60, 100]), event(256, &[0x80, 60, 0])],
            );
            s.reset();
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}
