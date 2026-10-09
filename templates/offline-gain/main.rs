//! Offline starter; no device or plug-in installation is performed.
use ravet::foundation::{
    AudioBuffer, AudioLayout, Event, EventData, ProcessBlock, ProcessConfig, ProcessStatus,
    Processor, Transport,
};
use ravet_gain::{GAIN_ID, GainProcessor};

fn main() {
    let mut processor = GainProcessor::new(0.0).expect("finite smoothing");
    processor
        .prepare(
            ProcessConfig::new(48_000.0, 4, 1, AudioLayout::STEREO)
                .expect("valid prepared configuration"),
        )
        .expect("supported reference layout");
    let mut left = [0.25; 4];
    let mut right = [-0.5; 4];
    let events = [Event {
        frame_offset: 0,
        data: EventData::ParameterValue {
            id: GAIN_ID,
            value: 2.0,
        },
    }];
    let mut channels: [&mut [f32]; 2] = [&mut left, &mut right];
    let audio = AudioBuffer::in_place(&mut channels, 4).expect("valid borrowed audio");
    let mut block = ProcessBlock::new(audio, &events, Transport::default());
    assert_eq!(processor.process(&mut block), ProcessStatus::Continue);
    assert_eq!(left, [0.5; 4]);
    assert_eq!(right, [-1.0; 4]);
    processor.deactivate();
    println!("RAVET starter passed: four synthetic stereo frames, gain=2");
}
