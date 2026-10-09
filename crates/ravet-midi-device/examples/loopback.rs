//! Explicit native virtual-port check; sends only to its own temporary endpoint.
#[cfg(unix)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use ravet_midi_device::{open_output, open_virtual_input, output_ports};
    use std::time::{Duration, Instant};
    let name = format!("RAVET public MIDI loopback {}", std::process::id());
    let (input, mut receiver) = open_virtual_input(&name, 16)?;
    let port = output_ports()?
        .into_iter()
        .find(|p| p.name == name)
        .ok_or("new virtual input destination unavailable")?;
    let mut output = open_output(&port.id)?;
    let packets: &[&[u8]] = &[
        &[0x90, 60, 64],
        &[0xb0, 1, 32],
        &[0xe0, 0, 64],
        &[0xf8],
        &[0xf0, 0x7d, 1, 2, 0xf7],
        &[0x80, 60, 0],
    ];
    for packet in packets {
        output.send(packet)?;
    }
    let end = Instant::now() + Duration::from_secs(3);
    for expected in packets {
        let packet = loop {
            if let Some(packet) = receiver.pop() {
                break packet;
            }
            if Instant::now() >= end {
                return Err("virtual loopback timed out".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        if packet.as_bytes() != *expected {
            return Err("virtual packet mismatch".into());
        }
        println!(
            "Received timestamp={} bytes={:?}",
            packet.timestamp_microseconds,
            packet.as_bytes()
        );
    }
    let statistics = input.statistics();
    if statistics.accepted != 6 || statistics.invalid != 0 || statistics.dropped_full != 0 {
        return Err("native callback statistics mismatch".into());
    }
    output.close();
    input.close();
    println!("PASS native virtual input/output, channel/system/SysEx FIFO and teardown");
    Ok(())
}
#[cfg(not(unix))]
fn main() {
    eprintln!("This explicit virtual-port fixture requires a Unix midir backend.");
}
