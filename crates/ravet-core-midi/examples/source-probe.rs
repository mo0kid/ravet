//! Synthetic owned-source subscriptions; never opens a hardware endpoint.
#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use ravet_core_midi::{CoreMidiError, CoreUmpInput, CoreUmpOutput, CoreUmpSource, sources};
    use ravet_foundation::UmpPacket;
    use ravet_midi_device::{UmpProtocol, UmpReceiver};
    use std::time::{Duration, Instant};
    fn receive(
        receiver: &mut UmpReceiver,
    ) -> Result<ravet_midi_device::TimedUmp, Box<dyn std::error::Error>> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(record) = receiver.pop() {
                return Ok(record);
            }
            if Instant::now() >= deadline {
                return Err("bounded source delivery timeout".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    for bad in ["", "nul\0name", &"x".repeat(257)] {
        assert!(matches!(
            CoreUmpSource::new(bad, "probe", UmpProtocol::Midi2),
            Err(CoreMidiError::Name)
        ));
        assert!(matches!(
            CoreUmpSource::new("probe", bad, UmpProtocol::Midi2),
            Err(CoreMidiError::Name)
        ));
    }
    let mut delivered = 0;
    for cycle in 0..3 {
        for protocol in [UmpProtocol::Midi1, UmpProtocol::Midi2] {
            let label = format!(
                "RAVET public source probe {} {cycle} {protocol:?}",
                std::process::id()
            );
            let mut source = CoreUmpSource::new(&label, &label, protocol)?;
            let id = source.id();
            assert!(sources()?.iter().any(|s| s.id == id));
            // Direction mismatch must fail without selecting any other endpoint.
            assert!(CoreUmpOutput::connect(&label, id, protocol).is_err());
            let (input1, mut receiver1) = CoreUmpInput::connect(&label, &label, id, protocol, 4)?;
            let (input2, mut receiver2) = CoreUmpInput::connect(&label, &label, id, protocol, 4)?;
            for group in 0..16u32 {
                let words = match protocol {
                    UmpProtocol::Midi1 => [0x20913c7f | (group << 24), 0],
                    UmpProtocol::Midi2 => [0x40913c00 | (group << 24), 0x81230000 | group],
                };
                let packet = UmpPacket::from_words(
                    &words[..if protocol == UmpProtocol::Midi1 { 1 } else { 2 }],
                )
                .unwrap();
                source.send(0, packet)?;
                for receiver in [&mut receiver1, &mut receiver2] {
                    let record = receive(receiver)?;
                    assert_eq!(record.protocol, protocol);
                    assert_eq!(record.packet, packet);
                    delivered += 1;
                }
            }
            for input in [&input1, &input2] {
                assert_eq!(input.invalid_native_metadata(), 0);
                assert_eq!(input.overlapping_callbacks(), 0);
            }
            for receiver in [&receiver1, &receiver2] {
                assert_eq!(receiver.statistics().invalid, 0);
                assert_eq!(receiver.statistics().dropped_full, 0);
            }
            // Disconnect one subscriber; the surviving connection stays usable.
            drop(input1);
            assert!(receiver1.producer_gone());
            let packet = UmpPacket::from_words(&[0x20903c7f]).unwrap();
            // Use a protocol-matching packet (no conversion claim).
            let packet = if protocol == UmpProtocol::Midi2 {
                UmpPacket::from_words(&[0x40903c00, 0xffff0000]).unwrap()
            } else {
                packet
            };
            source.send(0, packet)?;
            assert_eq!(receive(&mut receiver2)?.packet, packet);
            assert_eq!(receiver1.pop(), None);
            delivered += 1;
            drop(source);
            assert!(!sources()?.iter().any(|s| s.id == id));
            // Source disappearance does not destroy the input owner/producer.
            assert!(!receiver2.producer_gone());
            assert!(CoreUmpInput::connect(&label, &label, id, protocol, 4).is_err());
            drop(input2);
            assert!(receiver2.producer_gone());
            assert_eq!(receiver2.pop(), None);
        }
    }
    println!(
        "PASS: owned MIDI1/MIDI2 sources, exact-ID two-subscriber delivery, all groups and independent teardown: {delivered} packets across six lifecycles"
    );
    Ok(())
}
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("CoreMIDI source probe requires macOS");
    std::process::exit(1);
}
