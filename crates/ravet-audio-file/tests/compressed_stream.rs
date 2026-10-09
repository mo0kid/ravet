#![cfg(feature = "compressed")]
use ravet_audio_file::{AudioFileError as E, CompressedReader, decode_compressed};
const FILES: [&[u8]; 3] = [
    include_bytes!("fixtures/sine.flac"),
    include_bytes!("fixtures/sine.mp3"),
    include_bytes!("fixtures/sine.ogg"),
];
#[test]
fn lazy_partitioned_reads_match_whole_decode_and_preserve_output_tails() {
    for encoded in FILES {
        let expected = decode_compressed(encoded, 4096).unwrap();
        let mut reader = CompressedReader::new(encoded, 4096).unwrap();
        assert_eq!((reader.sample_rate(), reader.channels()), (48000, 2));
        assert_eq!(reader.position_frames(), 0);
        assert_eq!(reader.read_frames(&mut []).unwrap(), 0);
        let mut observed = Vec::new();
        for frames in [1, 7, 128, 333, 1024, 65536] {
            let mut output = vec![99.; frames * 2];
            let count = reader.read_frames(&mut output).unwrap();
            observed.extend_from_slice(&output[..count * 2]);
            assert!(output[count * 2..].iter().all(|sample| *sample == 99.));
        }
        assert_eq!(observed, expected.samples());
        assert_eq!(reader.position_frames(), 4096);
        assert_eq!(reader.read_frames(&mut [99.; 2]).unwrap(), 0);
    }
}
#[test]
fn exact_replayed_seeks_cross_packet_boundaries_and_failed_seek_preserves_cursor() {
    for encoded in FILES {
        let expected = decode_compressed(encoded, 4096).unwrap();
        let mut reader = CompressedReader::new(encoded, 4096).unwrap();
        for target in [2049, 0, 4095, 1153, 4096, 1, 3333, 100] {
            reader.seek_frame(target).unwrap();
            assert_eq!(reader.position_frames(), target);
            let mut output = [99.; 34];
            let count = reader.read_frames(&mut output).unwrap();
            assert_eq!(
                &output[..count * 2],
                &expected.samples()[target * 2..(target + count) * 2]
            );
            assert!(output[count * 2..].iter().all(|sample| *sample == 99.));
        }
        let before = reader.position_frames();
        assert!(reader.seek_frame(4097).is_err());
        assert_eq!(reader.position_frames(), before);
        let mut output = [0.; 2];
        assert_eq!(reader.read_frames(&mut output).unwrap(), 1);
        assert_eq!(&output, &expected.samples()[before * 2..before * 2 + 2]);
        let before = reader.position_frames();
        let mut invalid = [77.; 3];
        assert!(matches!(reader.read_frames(&mut invalid), Err(E::Capacity)));
        assert_eq!(invalid, [77.; 3]);
        assert_eq!(reader.position_frames(), before);
    }
}
#[test]
fn limits_corruption_poison_and_successful_replay_recovery_are_explicit() {
    assert!(CompressedReader::new(&[], 4096).is_err());
    assert!(CompressedReader::new(b"not audio", 4096).is_err());
    let mut exercised_poison = 0;
    for encoded in FILES {
        assert!(matches!(
            CompressedReader::new(encoded, 0),
            Err(E::Capacity)
        ));
        assert!(CompressedReader::new(encoded, 4095).is_err());
        let truncated = &encoded[..encoded.len() * 3 / 4];
        if let Ok(mut reader) = CompressedReader::new(truncated, 4096) {
            exercised_poison += 1;
            let mut output = [99.; 8192];
            assert!(reader.read_frames(&mut output).is_err());
            assert!(output.iter().all(|sample| *sample == 0.));
            assert!(matches!(reader.read_frames(&mut output), Err(E::Poisoned)));
            let previous = reader.position_frames();
            assert!(reader.seek_frame(4096).is_err());
            assert_eq!(reader.position_frames(), previous);
            assert!(matches!(reader.read_frames(&mut output), Err(E::Poisoned)));
            reader.seek_frame(0).unwrap();
            assert_eq!(reader.position_frames(), 0);
            assert_eq!(reader.read_frames(&mut [0.; 2]).unwrap(), 1);
        }
    }
    assert!(
        exercised_poison > 0,
        "no deferred malformed-stream fixture exercised"
    );
}
