//! Preserve full-resolution MIDI 2.0 fields in framed, sample-timed UMP events.
use ravet_foundation::{
    AudioBuffer, AudioLayout, Event, EventData, Midi2Kind, Midi2Message, ProcessBlock,
    ProcessConfig, Transport, UmpDecoder,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().len() == 1
        || std::env::args_os().nth(1).is_some_and(|a| a == "--ui-test")
    {
        return ravet_examples::launch("midi2");
    }
    if std::env::args_os().len() != 2 || std::env::args_os().nth(1).is_none_or(|a| a != "--offline")
    {
        return Err("usage: midi2 [--offline | --ui-test directory]".into());
    }
    // Group 1, channel 2, key 60: explicit zero-based addressing.
    let messages = [
        (
            0,
            Midi2Kind::NoteOn {
                key: 60,
                velocity: 0xabcd,
                attribute_type: 0,
                attribute: 0,
            },
        ),
        (
            17,
            Midi2Kind::PerNotePitchBend {
                key: 60,
                value: 0x9123_4567,
            },
        ),
        (
            17,
            Midi2Kind::PolyPressure {
                key: 60,
                value: 0xdead_beef,
            },
        ),
        (
            48,
            Midi2Kind::NoteOff {
                key: 60,
                velocity: 0x1234,
                attribute_type: 0,
                attribute: 0,
            },
        ),
    ];
    let mut events = Vec::new();
    let mut decoder = UmpDecoder::new();
    for (frame_offset, kind) in messages {
        let original = Midi2Message::new(1, 2, kind).expect("fixed valid MIDI 2.0 fields");
        let packet = original.packet();
        // A word stream may arrive in fragments; emit only a complete packet.
        let mut framed = None;
        for word in packet.as_words() {
            framed = decoder.push(*word);
        }
        decoder.finish().expect("no partial packet remains");
        let framed = framed.expect("complete two-word MIDI 2.0 packet");
        assert_eq!(
            Midi2Message::from_packet(framed).expect("encoded MIDI 2.0 packet"),
            original
        );
        events.push(Event {
            frame_offset,
            data: EventData::Ump(framed),
        });
    }
    // Foundation validates timing/storage, without imposing MIDI 2.0 synthesis policy.
    let config = ProcessConfig::new(48_000., 64, 4, AudioLayout::STEREO_INSTRUMENT)?;
    let mut left = [0.; 64];
    let mut right = [0.; 64];
    let mut channels = [&mut left[..], &mut right[..]];
    let audio = AudioBuffer::separate(&[], &mut channels, 64)?;
    let block = ProcessBlock::new(audio, &events, Transport::default());
    config.validate_block(&block)?;
    for event in block.events {
        if let EventData::Ump(packet) = event.data {
            let message = Midi2Message::from_packet(packet).expect("encoded MIDI 2.0 packet");
            println!(
                "frame={} group={} channel={} words={:08x?} {:?}",
                event.frame_offset,
                message.group(),
                message.channel(),
                packet.as_words(),
                message.kind()
            );
        }
    }
    println!("16-bit velocity and 32-bit expression preserved; same-frame event order retained.");
    println!("Codec/event walkthrough only: no MIDI 2.0 synth, device or plug-in export.");
    Ok(())
}
