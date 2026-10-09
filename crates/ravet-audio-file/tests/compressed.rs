#![cfg(feature = "compressed")]
use ravet_audio_file::{AudioFileError, WaveReader, decode_compressed};
use std::io::Cursor;
const FLAC: &[u8] = include_bytes!("fixtures/sine.flac");
const MP3: &[u8] = include_bytes!("fixtures/sine.mp3");
const OGG: &[u8] = include_bytes!("fixtures/sine.ogg");
fn source() -> Vec<f32> {
    let mut reader = WaveReader::new(
        Cursor::new(include_bytes!("fixtures/sine.wav").as_slice()),
        4096,
    )
    .unwrap();
    let mut output = vec![0.; 8192];
    assert_eq!(reader.read_frames(&mut output).unwrap(), 4096);
    output
}
#[test]
fn actual_encoded_tones_preserve_rate_channels_gapless_duration_and_signal() {
    let expected = source();
    for (name, encoded, tolerance) in [
        ("FLAC", FLAC, 0.),
        ("MP3", MP3, 0.02),
        ("Ogg/Vorbis", OGG, 0.02),
    ] {
        let clip = decode_compressed(encoded, 4096).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            (clip.sample_rate(), clip.channels(), clip.frames()),
            (48000, 2, 4096),
            "{name}"
        );
        let rms = (clip
            .samples()
            .iter()
            .zip(&expected)
            .map(|(a, b)| f64::from(a - b).powi(2))
            .sum::<f64>()
            / 8192.)
            .sqrt();
        assert!(rms <= tolerance, "{name}: signal RMS error {rms}");
        for channel in 0..2 {
            let actual_rms = (clip
                .samples()
                .iter()
                .skip(channel)
                .step_by(2)
                .map(|v| f64::from(*v).powi(2))
                .sum::<f64>()
                / 4096.)
                .sqrt();
            assert!(
                (actual_rms - if channel == 0 { 0.1768 } else { 0.3536 }).abs() < 0.02,
                "{name} channel {channel}"
            );
        }
        let frames = clip.frames();
        assert_eq!(clip.into_samples().len(), frames * 2);
    }
}
#[test]
fn output_caps_malformed_input_and_truncated_declared_streams_reject() {
    for encoded in [FLAC, MP3, OGG] {
        assert!(matches!(
            decode_compressed(encoded, 4095),
            Err(AudioFileError::Capacity)
        ));
        assert!(decode_compressed(&encoded[..encoded.len() / 2], 4096).is_err());
        assert!(decode_compressed(encoded, 0).is_err());
    }
    for input in [
        &[][..],
        b"not an audio file",
        &FLAC[..4],
        &OGG[..4],
        &MP3[..4],
    ] {
        assert!(decode_compressed(input, 4096).is_err());
    }
}
