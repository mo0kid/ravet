#![cfg(all(feature = "flac-encode", feature = "compressed"))]
use ravet_audio_file::{
    AudioFileError as E, FlacFormat, FlacPcmEncoding as P, decode_compressed, encode_flac,
};
#[test]
fn encoded_integer_extrema_multichannel_and_frame_boundaries_roundtrip_exactly() {
    for channels in 1..=8 {
        for encoding in [P::Pcm16, P::Pcm24] {
            let limit = 1_i32 << (encoding.bits() - 1);
            for frames in [1, 17, 4097, 4113] {
                let samples: Vec<i32> = (0..frames * usize::from(channels))
                    .map(|index| match index % 7 {
                        0 => -limit,
                        1 => limit - 1,
                        2 => 0,
                        3 => -1,
                        4 => 1,
                        _ => ((index as i32 * 13007) % (limit * 2)) - limit,
                    })
                    .collect();
                let rate = if channels % 2 == 0 { 96000 } else { 44100 };
                let encoded = encode_flac(
                    &samples,
                    FlacFormat {
                        channels,
                        sample_rate: rate,
                        encoding,
                    },
                )
                .unwrap();
                assert_eq!(&encoded[..4], b"fLaC");
                let decoded = decode_compressed(&encoded, frames).unwrap_or_else(|error| {
                    panic!("{channels}ch/{encoding:?}/{frames}frames: {error}")
                });
                assert_eq!(
                    (decoded.channels(), decoded.sample_rate(), decoded.frames()),
                    (channels, rate, frames)
                );
                let expected: Vec<f32> = samples
                    .iter()
                    .map(|sample| *sample as f32 / limit as f32)
                    .collect();
                assert_eq!(decoded.samples(), expected);
            }
        }
    }
}
#[test]
fn invalid_precision_ranges_shapes_and_format_reject_without_source_mutation() {
    let valid = FlacFormat {
        channels: 2,
        sample_rate: 48000,
        encoding: P::Pcm16,
    };
    assert!(matches!(encode_flac(&[], valid), Err(E::Capacity)));
    for samples in [vec![0], vec![-32769, 0], vec![32768, 0]] {
        let before = samples.clone();
        assert!(matches!(
            encode_flac(&samples, valid),
            Err(E::InvalidSamples)
        ));
        assert_eq!(samples, before);
    }
    for format in [
        FlacFormat {
            channels: 0,
            ..valid
        },
        FlacFormat {
            channels: 9,
            ..valid
        },
        FlacFormat {
            sample_rate: 0,
            ..valid
        },
        FlacFormat {
            sample_rate: 96001,
            ..valid
        },
    ] {
        assert!(matches!(encode_flac(&[0, 0], format), Err(E::Unsupported)));
    }
    let encoding24 = FlacFormat {
        encoding: P::Pcm24,
        ..valid
    };
    for samples in [[-8_388_609, 0], [8_388_608, 0]] {
        assert!(matches!(
            encode_flac(&samples, encoding24),
            Err(E::InvalidSamples)
        ));
    }
}

#[test]
fn declared_rate_edges_and_fixed_streaminfo_bounds_include_short_final_frames() {
    for rate in [1, 8000, 48000, 65535, 65536, 96000] {
        let encoded = encode_flac(
            &[-32768, 32767, 0],
            FlacFormat {
                channels: 1,
                sample_rate: rate,
                encoding: P::Pcm16,
            },
        )
        .unwrap();
        assert_eq!(&encoded[8..12], &[0x10, 0, 0x10, 0]);
        let decoded = decode_compressed(&encoded, 3).unwrap();
        assert_eq!(decoded.sample_rate(), rate);
        assert_eq!(decoded.frames(), 3);
        assert_eq!(decoded.samples(), &[-1., 32767. / 32768., 0.]);
    }
}
