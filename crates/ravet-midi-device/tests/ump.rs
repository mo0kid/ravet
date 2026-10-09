use ravet_foundation::UmpPacket;
use ravet_midi_device::{PreparedUmpInput, TimedUmp, UmpInputError, UmpProtocol};
#[test]
fn all_framed_types_timestamps_protocol_words_and_atomic_batch_rejection() {
    let (mut input, mut receiver) = PreparedUmpInput::new(32).unwrap();
    let mut words = Vec::new();
    let mut packets = Vec::new();
    for kind in 0..16u32 {
        let mut storage = [0x0001_2345, 0x12345678, 0x89abcdef, 0xabcdef01];
        storage[0] |= kind << 28;
        let packet = UmpPacket::from_words(&storage[..UmpPacket::word_count(storage[0])]).unwrap();
        words.extend_from_slice(packet.as_words());
        packets.push(packet);
    }
    assert_eq!(
        input
            .push_words(u64::MAX, UmpProtocol::Midi2, &words)
            .unwrap()
            .accepted,
        16
    );
    for packet in packets {
        assert_eq!(
            receiver.pop(),
            Some(TimedUmp {
                timestamp: u64::MAX,
                protocol: UmpProtocol::Midi2,
                packet
            })
        );
    }
    assert_eq!(receiver.pop(), None);
    // Valid prefix plus truncated 128-bit packet must publish no prefix.
    assert_eq!(
        input.push_words(0, UmpProtocol::Midi1, &[0x20903c7f, 0xf0000000]),
        Err(UmpInputError::Malformed)
    );
    assert_eq!(receiver.pop(), None);
    assert_eq!(
        input.push_words(0, UmpProtocol::Midi1, &vec![0; 4097]),
        Err(UmpInputError::Capacity)
    );
    assert_eq!(receiver.pop(), None);
    assert_eq!(receiver.statistics().accepted, 16);
    assert_eq!(receiver.statistics().invalid, 2);
    assert!(PreparedUmpInput::new(0).is_err());
    assert!(PreparedUmpInput::new(4097).is_err());
}
#[test]
fn prefix_refusal_drops_suffix_without_replay_abandonment_and_prepared_heap() {
    let (mut input, mut receiver) = PreparedUmpInput::new(1).unwrap();
    let result = input
        .push_words(
            123,
            UmpProtocol::Midi1,
            &[0x20903c7f, 0x20803c00, 0x2090407f],
        )
        .unwrap();
    assert_eq!(
        (result.accepted, result.discarded, result.reason),
        (1, 2, Some(UmpInputError::Full))
    );
    assert_eq!(input.statistics().dropped_full, 2);
    assert_eq!(receiver.pop().unwrap().packet.as_words(), [0x20903c7f]);
    assert_eq!(receiver.pop(), None);
    let counts = allocation_counter::measure(|| {
        for i in 0..10_000 {
            input
                .push_words(i, UmpProtocol::Midi2, &[0x40903c00, 0xffff0000])
                .unwrap();
            assert_eq!(receiver.pop().unwrap().timestamp, i);
            assert!(
                input
                    .push_words(i, UmpProtocol::Midi2, &[0x50900000])
                    .is_err()
            );
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    drop(receiver);
    assert_eq!(
        input
            .push_words(0, UmpProtocol::Midi1, &[0x20f80000])
            .unwrap()
            .reason,
        Some(UmpInputError::ReceiverGone)
    );
    let (mut producer, mut consumer) = PreparedUmpInput::new(1).unwrap();
    producer
        .push_words(0, UmpProtocol::Midi1, &[0x20f80000])
        .unwrap();
    drop(producer);
    assert!(consumer.producer_gone());
    assert!(consumer.pop().is_some());
    assert_eq!(consumer.pop(), None);
}
#[test]
fn separate_thread_packets_remain_complete_ordered_and_heap_free() {
    let (mut input, mut receiver) = PreparedUmpInput::new(8).unwrap();
    let packet = UmpPacket::from_words(&[0x40913c00, 0xa1230000]).unwrap();
    let worker = std::thread::spawn(move || {
        let counts = allocation_counter::measure(|| {
            for timestamp in 0..10_000 {
                let record = TimedUmp {
                    timestamp,
                    protocol: UmpProtocol::Midi2,
                    packet,
                };
                while input.try_push(record) == Err(UmpInputError::Full) {
                    std::hint::spin_loop();
                }
            }
        });
        assert_eq!(counts.count_total, 0);
        assert_eq!(counts.count_current, 0);
    });
    for timestamp in 0..10_000 {
        let record = loop {
            if let Some(record) = receiver.pop() {
                break record;
            }
            if receiver.producer_gone() {
                break receiver
                    .pop()
                    .expect("producer ended before expected packet");
            }
            std::hint::spin_loop();
        };
        assert_eq!(
            record,
            TimedUmp {
                timestamp,
                protocol: UmpProtocol::Midi2,
                packet
            }
        );
    }
    worker.join().unwrap();
    assert_eq!(receiver.statistics().accepted, 10_000);
    assert!(receiver.producer_gone());
}
