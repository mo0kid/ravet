//! Headless no_std consumer library. Board startup/drivers stay in the consumer.
#![no_std]
use ravet_embedded::FixedBlockAdapter;
use ravet_embedded_gain::{GAIN_ID, Gain};
use ravet_foundation::{AudioLayout, Event, EventData, ProcessConfig, Transport};
pub type Audio = FixedBlockAdapter<Gain, 2, 2, 64>;
pub fn prepared_audio(sample_rate: f64) -> Audio {
    let mut audio = Audio::new(Gain::new());
    audio
        .prepare(ProcessConfig::new(sample_rate, 64, 8, AudioLayout::STEREO).unwrap())
        .unwrap();
    audio
}
pub fn offline_fixture() -> bool {
    let mut audio = prepared_audio(48000.0);
    let event = Event {
        frame_offset: 0,
        data: EventData::ParameterValue {
            id: GAIN_ID,
            value: 2.0,
        },
    };
    let mut output = [0.0; 4];
    let passed = audio
        .process_interleaved(
            &[0.25, -0.25, 0.5, -0.5],
            &mut output,
            2,
            &[event],
            Transport::default(),
        )
        .is_ok()
        && output == [0.5, -0.5, 1.0, -1.0];
    audio.deactivate();
    passed
}
