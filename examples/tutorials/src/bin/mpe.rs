//! Negotiate a lower MPE zone and inspect independent per-note expression.
use ravet_foundation::MidiMessage;
use ravet_instrument::{MpeReceiver, VoicePhase};

fn apply(receiver: &mut MpeReceiver<4>, bytes: &[u8]) {
    receiver.apply(MidiMessage::from_bytes(bytes).expect("fixed valid MIDI message"));
}

fn show(label: &str, receiver: &MpeReceiver<4>) {
    println!("{label}");
    for voice in receiver
        .voices()
        .iter()
        .filter(|v| v.phase != VoicePhase::Idle)
    {
        let note = voice.note.expect("occupied voice");
        let expression = voice.expression;
        println!(
            "  channel={} key={} {:?}: member_bend={:.1} master_bend={:.1} pressure={:.3} timbre={:.3}",
            note.channel,
            note.key,
            voice.phase,
            expression.member_bend,
            expression.master_bend,
            expression.pressure,
            expression.timbre,
        );
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().len() == 1
        || std::env::args_os().nth(1).is_some_and(|a| a == "--ui-test")
    {
        return ravet_examples::launch("mpe");
    }
    if std::env::args_os().len() != 2 || std::env::args_os().nth(1).is_none_or(|a| a != "--offline")
    {
        return Err("usage: mpe [--offline | --ui-test directory]".into());
    }
    let mut receiver = MpeReceiver::<4>::new()?;
    // RPN 6: lower master channel 0, two member channels (1 and 2).
    // Send Data Entry, then null the RPN selection to avoid accidental later edits.
    for bytes in [
        [0xb0, 101, 0],
        [0xb0, 100, 6],
        [0xb0, 6, 2],
        [0xb0, 101, 127],
        [0xb0, 100, 127],
        // RPN 0 on a member: set this zone's member bend range to 12 semitones.
        [0xb1, 101, 0],
        [0xb1, 100, 0],
        [0xb1, 6, 12],
        [0xb1, 101, 127],
        [0xb1, 100, 127],
        [0x91, 60, 100],
        [0x92, 64, 100],
        [0xe1, 127, 127], // member 1 maximum bend; member 2 stays centered
        [0xe0, 0, 0],     // master bend -2 semitones affects both notes
    ]
    .iter()
    {
        apply(&mut receiver, bytes);
    }
    apply(&mut receiver, &[0xd1, 100]);
    apply(&mut receiver, &[0xb1, 74, 64]);
    show(
        "Two notes: member expression is independent; master bend is shared.",
        &receiver,
    );
    apply(&mut receiver, &[0x81, 60, 64]);
    apply(&mut receiver, &[0xe1, 0, 64]); // released member expression freezes
    apply(&mut receiver, &[0xe0, 0, 64]); // master expression still follows the zone
    show(
        "After note-off and recenter: released C4 retains its +12 member bend.",
        &receiver,
    );
    // A DSP owner would render a release envelope before freeing this voice.
    receiver.finish(0)?;
    show("After the DSP owner finishes the release tail:", &receiver);
    println!("Snapshot example only; pressure/timbre mapping to sound belongs to your DSP.");
    Ok(())
}
