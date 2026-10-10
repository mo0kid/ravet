#![cfg(all(feature = "mp3-encode", feature = "compressed"))]
use ravet_audio_file::{Mp3Bitrate, Mp3Format, Mp3Writer, decode_compressed, encode_mp3};
use std::io::Cursor;
fn source(channels: u16, rate: u32, frames: usize) -> Vec<f32> {
    (0..frames)
        .flat_map(|f| {
            (0..channels).map(move |c| {
                (0.55
                    * (std::f64::consts::TAU * (if c == 0 { 337. } else { 977. }) * f as f64
                        / f64::from(rate))
                    .sin()) as f32
            })
        })
        .collect()
}
#[test]
fn exact_independent_gapless_matrix() {
    for bit_rate in [
        Mp3Bitrate::Kbps192,
        Mp3Bitrate::Kbps256,
        Mp3Bitrate::Kbps320,
    ] {
        for channels in [1, 2] {
            for sample_rate in [32000, 44100, 48000] {
                for frames in [1, 17, 4096, 8193] {
                    let format = Mp3Format {
                        channels,
                        sample_rate,
                        bit_rate,
                    };
                    let source = source(channels, sample_rate, frames);
                    let bytes = encode_mp3(format, &source).unwrap();
                    let decoded = decode_compressed(&bytes, frames).unwrap();
                    assert_eq!(
                        (decoded.frames(), decoded.channels(), decoded.sample_rate()),
                        (frames, channels, sample_rate)
                    );
                    let mut writer =
                        Mp3Writer::new(Cursor::new(Vec::new()), format, frames as u64, 65536)
                            .ok()
                            .unwrap();
                    for chunk in source.chunks(17 * usize::from(channels)) {
                        writer.write_samples(chunk).unwrap();
                    }
                    assert_eq!(writer.accepted_frames(), frames as u64);
                    let incremental = writer.finish().unwrap().into_inner();
                    let decoded = decode_compressed(&incremental, frames).unwrap();
                    assert_eq!(decoded.frames(), frames);
                }
            }
        }
    }
}
#[test]
#[ignore = "requires caller-selected independent FFmpeg native MP3 decoder"]
fn ffmpeg_native_file_decoder_agrees_on_all_bitrates_timing_pcm_and_gain_policy() {
    use std::{fs, process::Command};
    let executable =
        std::env::var_os("RAVET_TEST_FFMPEG").expect("explicit RAVET_TEST_FFMPEG executable");
    let root = std::env::temp_dir().join(format!("ravet-mp3-independent-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    let mut measured = 0;
    for bit_rate in [
        Mp3Bitrate::Kbps192,
        Mp3Bitrate::Kbps256,
        Mp3Bitrate::Kbps320,
    ] {
        for channels in [1, 2] {
            for sample_rate in [32000, 44100, 48000] {
                for frames in [1, 17, 4096, 8193] {
                    let format = Mp3Format {
                        channels,
                        sample_rate,
                        bit_rate,
                    };
                    let original = source(channels, sample_rate, frames);
                    let encoded = encode_mp3(format, &original).unwrap();
                    let independent = decode_compressed(&encoded, frames).unwrap();
                    let file = root.join(format!(
                        "{}-{channels}-{sample_rate}-{frames}.mp3",
                        bit_rate.kbps()
                    ));
                    fs::write(&file, encoded).unwrap();
                    let output = Command::new(&executable)
                        .args(["-hide_banner", "-loglevel", "error", "-c:a", "mp3", "-i"])
                        .arg(&file)
                        .args(["-f", "f32le", "pipe:1"])
                        .output()
                        .unwrap();
                    assert!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    assert_eq!(output.stdout.len(), original.len() * 4);
                    let pcm: Vec<f32> = output
                        .stdout
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|s| f32::from_le_bytes(*s))
                        .collect();
                    assert!(pcm.iter().all(|x| x.is_finite()));
                    let maximum = pcm
                        .iter()
                        .zip(independent.samples())
                        .map(|(a, b)| (a - b).abs())
                        .fold(0., f32::max);
                    assert!(
                        maximum < 0.0001,
                        "{format:?} {frames}: decoder difference{maximum}"
                    );
                    if frames == 8193 {
                        for channel in 0..usize::from(channels) {
                            let x: Vec<f64> = original
                                .iter()
                                .skip(channel)
                                .step_by(usize::from(channels))
                                .map(|v| f64::from(*v))
                                .collect();
                            let y: Vec<f64> = pcm
                                .iter()
                                .skip(channel)
                                .step_by(usize::from(channels))
                                .map(|v| f64::from(*v))
                                .collect();
                            let power: f64 = x.iter().map(|v| v * v).sum();
                            let error: f64 = x.iter().zip(&y).map(|(a, b)| (a - b).powi(2)).sum();
                            let snr = 10. * (power / error).log10();
                            let gain = x.iter().zip(&y).map(|(a, b)| a * b).sum::<f64>() / power;
                            let residual = x
                                .iter()
                                .zip(&y)
                                .map(|(a, b)| (b / gain - a).powi(2))
                                .sum::<f64>();
                            let fitted = 10. * (power / residual).log10();
                            let expected = if bit_rate == Mp3Bitrate::Kbps192 {
                                0.97
                            } else {
                                1.
                            };
                            assert!(
                                (gain - expected).abs() < 0.005,
                                "{format:?} channel{channel}: gain{gain}"
                            );
                            assert!(
                                snr > if bit_rate == Mp3Bitrate::Kbps192 {
                                    25.
                                } else {
                                    45.
                                } && fitted > 45.,
                                "{format:?} channel{channel}: raw{snr}, fitted{fitted}"
                            );
                            println!(
                                "{format:?} channel{channel}: raw SNR{snr:.4}dB fitted{fitted:.4}dB gain{gain:.7} max decoder delta{maximum:.8}"
                            );
                        }
                    }
                    measured += 1;
                }
            }
        }
    }
    assert_eq!(measured, 72);
}
