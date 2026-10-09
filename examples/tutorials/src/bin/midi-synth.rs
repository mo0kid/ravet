//! Live subtractive synth UI; explicit rendering remains an offline test fixture.
#[path = "../support/mod.rs"]
mod support;
use ravet_examples::synth::{DEFAULT_VALUES, SubtractiveSynth, SynthPatch};
use ravet_foundation::{Event, EventData, MidiMessage};

fn midi(frame_offset: usize, bytes: &[u8]) -> Event {
    Event {
        frame_offset,
        data: EventData::Midi(MidiMessage::from_bytes(bytes).expect("fixed valid MIDI message")),
    }
}

fn main() -> support::Result<()> {
    if !std::env::args_os().nth(1).is_some_and(|a| a == "--render") {
        return ravet_examples::launch("synth");
    }
    let path = support::output_path("target/examples/midi-synth.wav")?;
    let mut synth = SubtractiveSynth::new();
    let mut values = DEFAULT_VALUES;
    values[0] = 0.;
    values[1] = 0.8;
    values[2] = 20_000.;
    values[4] = 0.;
    values[5] = 5.;
    values[6] = 10.;
    values[7] = 0.8;
    values[8] = 50.;
    synth.set_patch(&SynthPatch::new(48_000., values)?)?;
    // MIDI channels and keys are zero-based. Notes start after 50 ms of silence.
    // The synth supplies its shared ADSR and VCF; this runner supplies MIDI events.
    let events = [
        midi(2_400, &[0x90, 60, 100]), // C4 on, channel 0
        midi(12_000, &[0x80, 60, 64]),
        midi(16_000, &[0x90, 64, 100]), // E4
        midi(28_000, &[0x80, 64, 64]),
        midi(32_000, &[0x90, 67, 100]), // G4
        midi(44_000, &[0xb0, 64, 127]), // sustain down
        midi(46_000, &[0x80, 67, 64]),  // retained by sustain
        midi(60_000, &[0xb0, 64, 0]),   // sustain up, release tail
    ];
    let audio = support::render(&mut synth, None, 72_000, &events)?;
    support::write_wave(&path, &audio)?;
    println!("C4, E4, G4; the final note continues after note-off until pedal-up.");
    println!("Launch without arguments for the live UI, waveform switch, shared ADSR and VCF.");
    Ok(())
}
