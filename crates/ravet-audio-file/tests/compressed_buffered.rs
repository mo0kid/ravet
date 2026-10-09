#![cfg(feature = "compressed")]
use ravet_audio_file::{
    AudioFileError as E, CompressedReadAhead, CompressedReader, audio_ring, decode_compressed,
};
const FILES: [&[u8]; 3] = [
    include_bytes!("fixtures/sine.flac"),
    include_bytes!("fixtures/sine.mp3"),
    include_bytes!("fixtures/sine.ogg"),
];
#[test]
fn retained_backpressure_and_stopped_seek_preserve_exact_generation() {
    for encoded in FILES {
        let expected = decode_compressed(encoded, 4096).unwrap();
        let reader = CompressedReader::new(encoded, 4096).unwrap();
        let (producer, mut consumer) = audio_ring(2, 4).unwrap();
        let mut pump = CompressedReadAhead::new(reader, producer, 11).unwrap();
        let initial = pump.pump().unwrap();
        assert_eq!(
            (
                initial.read_frames,
                initial.queued_frames,
                initial.pending_frames
            ),
            (11, 4, 7)
        );
        let blocked = pump.pump().unwrap();
        assert_eq!(
            (
                blocked.read_frames,
                blocked.queued_frames,
                blocked.pending_frames
            ),
            (0, 0, 7)
        );
        assert_eq!(pump.reader_position(), 11);
        let (_, mut foreign) = audio_ring(2, 4).unwrap();
        assert!(matches!(
            pump.seek_stopped(&mut foreign, 100),
            Err(E::InvalidSamples)
        ));
        assert!(pump.seek_stopped(&mut consumer, 4097).is_err());
        assert_eq!(pump.reader_position(), 11);
        let mut output = [99.; 6];
        assert_eq!(consumer.pop(&mut output).unwrap(), 3);
        assert_eq!(&output, &expected.samples()[..6]);
        pump.seek_stopped(&mut consumer, 1234).unwrap();
        assert_eq!(pump.reader_position(), 1234);
        assert_eq!(consumer.pop(&mut output).unwrap(), 0);
        let mut observed = Vec::new();
        for _ in 0..5000 {
            let progress = pump.pump().unwrap();
            let count = consumer.pop(&mut output).unwrap();
            observed.extend_from_slice(&output[..count * 2]);
            if progress.end_of_file && progress.pending_frames == 0 && count == 0 {
                break;
            }
        }
        assert_eq!(observed, &expected.samples()[1234 * 2..]);
        pump.seek_stopped(&mut consumer, 4096).unwrap();
        let progress = pump.pump().unwrap();
        assert!(progress.end_of_file);
        assert_eq!(progress.queued_frames, 0);
    }
    let reader = CompressedReader::new(FILES[0], 4096).unwrap();
    let (producer, _) = audio_ring(1, 8).unwrap();
    assert!(matches!(
        CompressedReadAhead::new(reader, producer, 8),
        Err(E::InvalidSamples)
    ));
}
#[test]
fn worker_decode_and_prepared_consumer_transfer_exact_order_without_callback_heap() {
    for encoded in FILES {
        let expected = decode_compressed(encoded, 4096).unwrap();
        let reader = CompressedReader::new(encoded, 4096).unwrap();
        let (producer, mut consumer) = audio_ring(2, 8).unwrap();
        let mut pump = CompressedReadAhead::new(reader, producer, 127).unwrap();
        let worker = std::thread::spawn(move || {
            for _ in 0..1_000_000 {
                let progress = pump.pump().unwrap();
                if progress.end_of_file && progress.pending_frames == 0 {
                    return;
                }
                std::thread::yield_now();
            }
            panic!("bounded compressed worker did not finish");
        });
        let mut observed = Vec::new();
        let mut output = [0.; 14];
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while observed.len() < 8192 {
            let mut count = 0;
            let measured = allocation_counter::measure(|| {
                count = consumer.pop(&mut output).unwrap();
            });
            assert_eq!(measured.count_total, 0);
            assert_eq!(measured.count_current, 0);
            observed.extend_from_slice(&output[..count * 2]);
            assert!(
                std::time::Instant::now() < deadline,
                "bounded consumer deadline"
            );
            std::thread::yield_now();
        }
        worker.join().unwrap();
        assert_eq!(observed, expected.samples());
        assert_eq!(consumer.pop(&mut output).unwrap(), 0);
    }
}

#[test]
fn decoder_fault_poison_rejects_publication_until_successful_stopped_seek() {
    let mut faults = 0;
    for encoded in FILES {
        let truncated = &encoded[..encoded.len() * 3 / 4];
        let Ok(reader) = CompressedReader::new(truncated, 4096) else {
            continue;
        };
        let (producer, mut consumer) = audio_ring(2, 8).unwrap();
        let mut pump = CompressedReadAhead::new(reader, producer, 16).unwrap();
        let mut output = [0.; 16];
        for _ in 0..2000 {
            match pump.pump() {
                Ok(progress) => {
                    consumer.pop(&mut output).unwrap();
                    if progress.end_of_file {
                        panic!("truncated fixture unexpectedly completed");
                    }
                }
                Err(_) => {
                    faults += 1;
                    assert!(matches!(pump.pump(), Err(E::Poisoned)));
                    assert!(pump.seek_stopped(&mut consumer, 4096).is_err());
                    assert!(matches!(pump.pump(), Err(E::Poisoned)));
                    pump.seek_stopped(&mut consumer, 0).unwrap();
                    assert_eq!(consumer.pop(&mut output).unwrap(), 0);
                    assert!(pump.pump().unwrap().read_frames > 0);
                    break;
                }
            }
        }
    }
    assert!(faults > 0, "no deferred malformed-stream fixture exercised");
}
