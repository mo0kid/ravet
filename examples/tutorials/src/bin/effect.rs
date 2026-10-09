//! Live stereo delay, or an explicit hardware-free impulse/automation render.
#[path = "../support/mod.rs"]
mod support;
use ravet_examples::delay::DelayProcessor;
use ravet_foundation::{Event, EventData, ParameterId};

fn parameter(frame_offset: usize, id: ParameterId, value: f32) -> Event {
    Event {
        frame_offset,
        data: EventData::ParameterValue { id, value },
    }
}

fn main() -> support::Result<()> {
    if !std::env::args_os().nth(1).is_some_and(|a| a == "--render") {
        return ravet_examples::launch("effect");
    }
    let path = support::output_path("target/examples/effect.wav")?;
    let frames = 2 * support::SAMPLE_RATE;
    let mut input = [vec![0.; frames], vec![0.; frames]];
    input[0][0] = 0.5;
    input[1][2400] = 0.25;
    input[0][73_600] = 0.125; // dry impulse after sample-timed bypass
    let events = [
        parameter(60_013, ParameterId(2), 0.25), // inside a 256-frame block
        parameter(72_007, ParameterId(4), 1.),   // dry from this exact sample
    ];
    let mut delay = DelayProcessor::default();
    delay.set_controls([250., 0.5, 1., 0., 1.])?;
    let audio = support::render(&mut delay, Some(input), frames, &events)?;
    support::write_wave(&path, &audio)?;
    println!("250 ms ping-pong delay; feedback 0.5 -> 0.25 with 5 ms smoothing, then dry bypass.");
    Ok(())
}
