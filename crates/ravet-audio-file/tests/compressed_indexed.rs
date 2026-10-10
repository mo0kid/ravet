#![cfg(feature = "compressed")]
use ravet_audio_file::{
    AudioFileError as E, CompressedReadAhead, CompressedReader, audio_ring, decode_compressed,
};
const FLAC: &[u8] = include_bytes!("fixtures/sine.flac");
#[test]
fn verified_index_seeks_first_last_end_and_preserves_failed_seek_and_tails() {
    let expected = decode_compressed(FLAC, 4096).unwrap();
    let mut reader = CompressedReader::new_indexed_flac(FLAC, 4096).unwrap();
    assert!(reader.indexed_packets().unwrap() > 0);
    assert_eq!((reader.sample_rate(), reader.channels()), (48000, 2));
    assert_eq!(reader.declared_frames(), Some(4096));
    assert_eq!(
        CompressedReader::new(FLAC, 4096).unwrap().indexed_packets(),
        None
    );
    for target in [4095, 0, 4096, 1, 2049, 100, 4094] {
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
    reader.seek_frame(100).unwrap();
    assert!(matches!(reader.seek_frame(4097), Err(E::Capacity)));
    assert_eq!(reader.position_frames(), 100);
    let mut invalid = [77.; 3];
    assert!(matches!(reader.read_frames(&mut invalid), Err(E::Capacity)));
    assert_eq!(invalid, [77.; 3]);
    assert_eq!(reader.position_frames(), 100);
    assert_eq!(reader.read_frames(&mut []).unwrap(), 0);
    let mut output = [0.; 2];
    assert_eq!(reader.read_frames(&mut output).unwrap(), 1);
    assert_eq!(&output, &expected.samples()[200..202]);
    reader.seek_frame(4096).unwrap();
    assert!(reader.end_of_file());
    assert_eq!(reader.read_frames(&mut output).unwrap(), 0);
    reader.seek_frame(0).unwrap();
    assert!(!reader.end_of_file());
}
#[test]
fn index_construction_rejects_lossy_truncated_bad_digest_duration_and_limits() {
    for input in [
        include_bytes!("fixtures/sine.mp3").as_slice(),
        include_bytes!("fixtures/sine.ogg").as_slice(),
        b"not audio",
    ] {
        assert!(matches!(
            CompressedReader::new_indexed_flac(input, 4096),
            Err(E::Unsupported)
        ));
    }
    assert!(matches!(
        CompressedReader::new_indexed_flac(&[], 4096),
        Err(E::Capacity)
    ));
    assert!(matches!(
        CompressedReader::new_indexed_flac(FLAC, 0),
        Err(E::Capacity)
    ));
    assert!(matches!(
        CompressedReader::new_indexed_flac(FLAC, 4095),
        Err(E::Capacity)
    ));
    assert!(CompressedReader::new_indexed_flac(&FLAC[..FLAC.len() * 3 / 4], 4096).is_err());
    // STREAMINFO starts at byte 8; MD5 occupies bytes 26..42.
    let mut wrong_digest = FLAC.to_vec();
    wrong_digest[26] ^= 0x80;
    assert!(matches!(
        CompressedReader::new_indexed_flac(&wrong_digest, 4096),
        Err(E::InvalidRecord)
    ));
    let mut unknown_duration = FLAC.to_vec();
    unknown_duration[21] &= 0xf0;
    unknown_duration[22..26].fill(0);
    assert!(matches!(
        CompressedReader::new_indexed_flac(&unknown_duration, 4096),
        Err(E::Unsupported)
    ));
    let mut wrong_duration = FLAC.to_vec();
    wrong_duration[25] ^= 1;
    assert!(CompressedReader::new_indexed_flac(&wrong_duration, 4097).is_err());
}
#[test]
fn existing_read_ahead_uses_index_with_exact_stopped_generation_and_callback_storage() {
    let expected = decode_compressed(FLAC, 4096).unwrap();
    let reader = CompressedReader::new_indexed_flac(FLAC, 4096).unwrap();
    let (producer, mut consumer) = audio_ring(2, 4).unwrap();
    let mut pump = CompressedReadAhead::new(reader, producer, 11).unwrap();
    assert_eq!(pump.pump().unwrap().pending_frames, 7);
    let (_, mut foreign) = audio_ring(2, 4).unwrap();
    assert!(matches!(
        pump.seek_stopped(&mut foreign, 100),
        Err(E::InvalidSamples)
    ));
    assert!(pump.seek_stopped(&mut consumer, 4097).is_err());
    assert_eq!(pump.reader_position(), 11);
    let mut output = [0.; 6];
    assert_eq!(consumer.pop(&mut output).unwrap(), 3);
    assert_eq!(&output, &expected.samples()[..6]);
    pump.seek_stopped(&mut consumer, 1234).unwrap();
    assert_eq!(consumer.pop(&mut output).unwrap(), 0);
    let mut observed = Vec::new();
    for _ in 0..5000 {
        let progress = pump.pump().unwrap();
        let mut count = 0;
        let allocations = allocation_counter::measure(|| {
            count = consumer.pop(&mut output).unwrap();
        });
        assert_eq!((allocations.count_total, allocations.count_current), (0, 0));
        observed.extend_from_slice(&output[..count * 2]);
        if progress.end_of_file && progress.pending_frames == 0 && count == 0 {
            break;
        }
    }
    assert_eq!(observed, &expected.samples()[1234 * 2..]);
    pump.seek_stopped(&mut consumer, 4096).unwrap();
    assert!(pump.pump().unwrap().end_of_file);
}
#[cfg(feature = "flac-encode")]
#[test]
fn independent_pcm_oracle_matches_all_channel_precisions_and_packet_edges() {
    use ravet_audio_file::{FlacFormat, FlacPcmEncoding, FlacWriter};
    use std::io::Cursor;
    const FRAMES: usize = 12_305;
    for channels in [1, 2, 8] {
        for encoding in [FlacPcmEncoding::Pcm16, FlacPcmEncoding::Pcm24] {
            let limit = 1i32 << (encoding.bits() - 1);
            let samples: Vec<i32> = (0..FRAMES * channels)
                .map(|i| match i % 5 {
                    0 => -limit,
                    1 => limit - 1,
                    2 => 0,
                    _ => ((i as i32 * 13007) % limit) - limit / 2,
                })
                .collect();
            let mut writer = FlacWriter::new(
                Cursor::new(Vec::new()),
                FlacFormat {
                    channels: channels as u16,
                    sample_rate: 48000,
                    encoding,
                },
                FRAMES as u64,
                1_048_576,
            )
            .unwrap();
            writer.write_frames(&samples).unwrap();
            let bytes = writer.finish().unwrap().into_inner();
            let mut reader = CompressedReader::new_indexed_flac(&bytes, FRAMES).unwrap();
            assert_eq!(reader.indexed_packets(), Some(4));
            for target in [
                0,
                1,
                4095,
                4096,
                4097,
                8191,
                8192,
                12287,
                12288,
                FRAMES - 1,
                FRAMES,
                333,
            ] {
                reader.seek_frame(target).unwrap();
                let mut output = vec![99.; channels * 37];
                let count = reader.read_frames(&mut output).unwrap();
                let expected: Vec<f32> = samples[target * channels..(target + count) * channels]
                    .iter()
                    .map(|sample| *sample as f32 / limit as f32)
                    .collect();
                assert_eq!(&output[..count * channels], expected.as_slice());
                assert!(
                    output[count * channels..]
                        .iter()
                        .all(|sample| *sample == 99.)
                );
            }
        }
    }
}
