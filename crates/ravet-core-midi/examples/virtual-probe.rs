//! Explicit synthetic self-delivery to owned virtual destinations; no hardware output.
#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use ravet_core_midi::{CoreUmpInput, CoreUmpOutput, destinations};
    use ravet_foundation::UmpPacket;
    use ravet_midi_device::UmpProtocol;
    use std::time::{Duration, Instant};
    let mut packets = 0;
    for cycle in 0..3 {
        for protocol in [UmpProtocol::Midi1, UmpProtocol::Midi2] {
            let label = format!(
                "RAVET public UMP probe {} {cycle} {protocol:?}",
                std::process::id()
            );
            let (input, mut receiver) =
                CoreUmpInput::virtual_destination(&label, &label, protocol, 8)?;
            let id = input.virtual_destination_id().ok_or("missing virtual ID")?;
            assert!(destinations()?.iter().any(|endpoint| endpoint.id == id));
            let mut output = CoreUmpOutput::connect(&label, id, protocol)?;
            for group in 0..16u32 {
                let storage = match protocol {
                    UmpProtocol::Midi1 => [0x20913c7f | (group << 24), 0],
                    UmpProtocol::Midi2 => [0x40913c00 | (group << 24), 0x81230000 | group],
                };
                let count = if protocol == UmpProtocol::Midi1 { 1 } else { 2 };
                let packet = UmpPacket::from_words(&storage[..count]).unwrap();
                output.send(0, packet)?;
                let deadline = Instant::now() + Duration::from_secs(2);
                let received = loop {
                    if let Some(record) = receiver.pop() {
                        break record;
                    }
                    if Instant::now() >= deadline {
                        return Err("bounded CoreMIDI self-delivery timeout".into());
                    }
                    std::thread::sleep(Duration::from_millis(1));
                };
                assert_eq!(received.protocol, protocol);
                assert_eq!(received.packet, packet);
                packets += 1;
            }
            assert_eq!(input.invalid_native_metadata(), 0);
            assert_eq!(input.overlapping_callbacks(), 0);
            assert_eq!(receiver.statistics().invalid, 0);
            assert_eq!(receiver.statistics().dropped_full, 0);
            drop(output);
            drop(input);
            assert!(receiver.producer_gone());
            assert_eq!(receiver.pop(), None);
            assert!(!destinations()?.iter().any(|endpoint| endpoint.id == id));
        }
    }
    for cycle in 0..10 {
        let label = format!("RAVET public UMP close {} {cycle}", std::process::id());
        let (input, mut receiver) =
            CoreUmpInput::virtual_destination(&label, &label, UmpProtocol::Midi2, 64)?;
        let id = input
            .virtual_destination_id()
            .ok_or("missing close-test ID")?;
        let packet = UmpPacket::from_words(&[0x40913c00, 0x81230000]).unwrap();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(0);
        let worker = std::thread::spawn(move || -> Result<(), ravet_core_midi::CoreMidiError> {
            let mut output = CoreUmpOutput::connect(&label, id, UmpProtocol::Midi2)?;
            output.send(0, packet)?;
            ready_tx.send(()).unwrap();
            for _ in 0..1000 {
                if output.send(0, packet).is_err() {
                    break;
                }
            }
            Ok(())
        });
        ready_rx.recv_timeout(Duration::from_secs(2))?;
        drop(input);
        assert!(receiver.producer_gone());
        worker.join().map_err(|_| "native send worker panicked")??;
        let mut retained = 0;
        while let Some(record) = receiver.pop() {
            assert_eq!(record.protocol, UmpProtocol::Midi2);
            assert_eq!(record.packet, packet);
            retained += 1;
        }
        assert!(retained <= 64);
        assert!(!destinations()?.iter().any(|endpoint| endpoint.id == id));
    }
    println!(
        "PASS: CoreMIDI MIDI1/MIDI2 UMP exact groups/words and owned lifecycle, {packets} packets across six endpoint cycles and ten concurrent-close cycles"
    );
    Ok(())
}
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("CoreMIDI virtual probe requires macOS");
    std::process::exit(1);
}
