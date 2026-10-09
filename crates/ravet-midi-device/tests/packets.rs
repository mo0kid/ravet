use ravet_midi_device::{CaptureStatus, PreparedMidiInput, validate_packet};

#[test]
fn rejects_malformed_packets_and_preserves_full_queue_contents() {
    for bytes in [
        &[][..],
        &[0x90, 60],
        &[0x90, 60, 128],
        &[60, 1],
        &[0xf0, 1],
        &[0xf0, 0x90, 0xf7],
        &[0xf0, 1, 0xf7, 0],
        &[0xf9],
    ] {
        assert!(validate_packet(bytes).is_err());
    }
    for capacity in [0, 3, 1025, usize::MAX] {
        assert!(PreparedMidiInput::new(capacity).is_err());
    }
    let (mut input, mut receiver) = PreparedMidiInput::new(2).unwrap();
    assert_eq!(input.capture(7, &[0x90, 60, 1]), CaptureStatus::Accepted);
    assert_eq!(
        input.capture(8, &[0xf0, 0x7d, 1, 0xf7]),
        CaptureStatus::Accepted
    );
    assert_eq!(input.capture(9, &[0x80, 60, 0]), CaptureStatus::Full);
    assert_eq!(
        input.capture(10, &[0xf0, 0x80, 0xf7]),
        CaptureStatus::Invalid
    );
    let first = receiver.pop().unwrap();
    assert_eq!(
        (first.timestamp_microseconds, first.as_bytes()),
        (7, &[0x90, 60, 1][..])
    );
    let second = receiver.pop().unwrap();
    assert_eq!(
        (second.timestamp_microseconds, second.as_bytes()),
        (8, &[0xf0, 0x7d, 1, 0xf7][..])
    );
    assert!(receiver.pop().is_none());
    assert_eq!(receiver.statistics().accepted, 2);
    assert_eq!(receiver.statistics().dropped_full, 1);
    assert_eq!(receiver.statistics().invalid, 1);
}

#[test]
fn maximum_packets_survive_wrap_and_threaded_fifo_handoff() {
    let (mut input, mut receiver) = PreparedMidiInput::new(8).unwrap();
    let producer = std::thread::spawn(move || {
        let mut bytes = [0; 1024];
        bytes[0] = 0xf0;
        bytes[1023] = 0xf7;
        for time in 0..10000 {
            bytes[1..1023].fill((time % 128) as u8);
            while input.capture(time, &bytes) == CaptureStatus::Full {
                std::thread::yield_now();
            }
        }
    });
    for time in 0..10000 {
        let packet = loop {
            if let Some(packet) = receiver.pop() {
                break packet;
            }
            std::thread::yield_now();
        };
        assert_eq!(packet.timestamp_microseconds, time);
        assert_eq!(packet.as_bytes().len(), 1024);
        assert!(
            packet.as_bytes()[1..1023]
                .iter()
                .all(|b| *b == (time % 128) as u8)
        );
    }
    producer.join().unwrap();
    assert_eq!(receiver.statistics().accepted, 10000);
    assert!(receiver.pop().is_none());
    let mut oversized = [0; 1025];
    oversized[0] = 0xf0;
    oversized[1024] = 0xf7;
    assert!(validate_packet(&oversized).is_err());
}

#[test]
fn prepared_callback_and_consumer_do_not_allocate_or_deallocate() {
    let (mut input, mut receiver) = PreparedMidiInput::new(1).unwrap();
    let counts = allocation_counter::measure(|| {
        for time in 0..10000 {
            assert_eq!(input.capture(time, &[0x90, 60, 1]), CaptureStatus::Accepted);
            assert_eq!(input.capture(time, &[0xf8]), CaptureStatus::Full);
            assert_eq!(input.capture(time, &[0x90, 60]), CaptureStatus::Invalid);
            std::hint::black_box(receiver.pop().unwrap());
            assert!(receiver.pop().is_none());
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    assert_eq!(counts.bytes_current, 0);
}
