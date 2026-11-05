use ravet_foundation::{
    AudioBuffer, AudioLayout, Event, EventData, MidiMessage, OutputEvents, ProcessBlock,
    ProcessConfig, ProcessError, ProcessStatus, Processor, Transport,
};
use ravet_synth_plugin::dsp::EvidenceSynth;

fn note(offset: usize) -> Event {
    Event {
        frame_offset: offset,
        data: EventData::Midi(MidiMessage::from_bytes(&[0x90, 60, 127]).unwrap()),
    }
}
fn process(
    synth: &mut EvidenceSynth,
    frames: usize,
    events: &[Event],
    capacity: usize,
) -> (Vec<f32>, ProcessStatus) {
    let mut left = vec![99.0; frames];
    let mut right = left.clone();
    let mut channels = [left.as_mut_slice(), right.as_mut_slice()];
    let mut storage = vec![note(0); capacity];
    let audio = AudioBuffer::separate(&[], &mut channels, frames).unwrap();
    let mut block = ProcessBlock::new(audio, events, Transport::default());
    if capacity > 0 {
        block.output_events = Some(OutputEvents::new(&mut storage, frames));
    }
    let status = synth.process(&mut block);
    assert_eq!(left, right);
    (left, status)
}
#[test]
fn click_history_is_independent_of_block_boundaries() {
    let mut synth = EvidenceSynth::default();
    synth
        .prepare(ProcessConfig::new(48000.0, 31, 8, AudioLayout::STEREO_INSTRUMENT).unwrap())
        .unwrap();
    let mut audio = Vec::new();
    for block in 0..5 {
        let events = if block == 0 { vec![note(7)] } else { vec![] };
        let (samples, status) = process(&mut synth, 31, &events, 0);
        assert_eq!(status, ProcessStatus::Continue);
        audio.extend(samples);
    }
    for (f, &value) in audio.iter().enumerate() {
        let expected = if (39..103).contains(&f) {
            if (f - 39) % 2 == 0 { 0.25 } else { -0.25 }
        } else {
            0.0
        };
        assert_eq!(value, expected, "sample {f}");
    }
}
#[test]
fn output_overflow_rejects_before_retriggering_the_voice() {
    let mut synth = EvidenceSynth::default();
    let config = ProcessConfig::new(48000.0, 128, 8, AudioLayout::STEREO_INSTRUMENT)
        .unwrap()
        .with_output_events(1);
    synth.prepare(config).unwrap();
    let (audio, status) = process(&mut synth, 128, &[note(0), note(1)], 1);
    assert_eq!(
        status,
        ProcessStatus::Fault(ProcessError::OutputEventCapacity)
    );
    assert!(audio.iter().all(|v| *v == 0.0));
    let (audio, status) = process(&mut synth, 128, &[], 1);
    assert_eq!(status, ProcessStatus::Continue);
    assert!(audio.iter().all(|v| *v == 0.0));
}
