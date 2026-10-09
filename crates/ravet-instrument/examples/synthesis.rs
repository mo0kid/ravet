//! Silent offline reference: synthetic note events render only to memory.
use ravet_foundation::{
    AudioBuffer, AudioLayout, Event, EventData, ProcessBlock, ProcessConfig, ProcessStatus,
    Processor, Transport,
};
use ravet_instrument::{SineInstrument, ZoneSide};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut synth = SineInstrument::<8>::new(ZoneSide::Lower)?;
    synth.prepare(ProcessConfig::new(
        48000.,
        4096,
        16,
        AudioLayout::STEREO_INSTRUMENT,
    )?)?;
    let events = [
        Event {
            frame_offset: 0,
            data: EventData::NoteOn {
                channel: 0,
                key: 69,
                velocity: 1.,
            },
        },
        Event {
            frame_offset: 1024,
            data: EventData::NoteOff {
                channel: 0,
                key: 69,
                velocity: 0.,
            },
        },
    ];
    let mut left = [0.; 4096];
    let mut right = [0.; 4096];
    let mut channels = [&mut left[..], &mut right[..]];
    let audio = AudioBuffer::separate(&[], &mut channels, 4096)?;
    assert_eq!(
        synth.process(&mut ProcessBlock::new(audio, &events, Transport::default())),
        ProcessStatus::Continue
    );
    assert_eq!(left, right);
    assert!(left[..1024].iter().any(|s| s.abs() > 0.01));
    assert!(left[3424..].iter().all(|s| *s == 0.));
    synth.deactivate();
    println!("PASS: stereo sample-timed sine note and completed release rendered offline");
    Ok(())
}
