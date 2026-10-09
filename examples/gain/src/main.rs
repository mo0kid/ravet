//! Deterministic offline fixture using the same processor as future adapters.
use ravet::foundation::{
    AudioBuffer, AudioLayout, Event, EventData, ProcessBlock, ProcessConfig, ProcessStatus,
    Processor, Transport,
};
use ravet_gain::{GAIN_ID, GainProcessor};

fn main() {
    let mut processor = GainProcessor::new(0.0).expect("valid smoothing time");
    let config =
        ProcessConfig::new(48_000.0, 8, 4, AudioLayout::STEREO).expect("valid configuration");
    processor.prepare(config).expect("supported gain layout");
    let mut left = [1.0, -1.0, 0.5, -0.5, 1.0, -1.0, 0.5, -0.5];
    let mut right = [0.25; 8];
    let events = [
        Event {
            frame_offset: 0,
            data: EventData::ParameterValue {
                id: GAIN_ID,
                value: 0.5,
            },
        },
        Event {
            frame_offset: 4,
            data: EventData::ParameterValue {
                id: GAIN_ID,
                value: 2.0,
            },
        },
    ];
    let mut channels: [&mut [f32]; 2] = [&mut left, &mut right];
    let audio = AudioBuffer::in_place(&mut channels, 8).expect("valid buffers");
    let mut block = ProcessBlock::new(audio, &events, Transport::default());
    assert_eq!(processor.process(&mut block), ProcessStatus::Continue);
    assert_eq!(left, [0.5, -0.5, 0.25, -0.25, 2.0, -2.0, 1.0, -1.0]);
    assert_eq!(right, [0.125, 0.125, 0.125, 0.125, 0.5, 0.5, 0.5, 0.5]);
    processor.deactivate();
    println!("RAVET gain: stereo, 48 kHz, 8 frames, gain events at frames 0 and 4");
    println!("Left:  {left:?}\nRight: {right:?}");
    println!("Offline fixture passed.");
}
