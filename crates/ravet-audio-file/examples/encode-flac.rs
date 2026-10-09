//! Explicit public synthetic PCM/FLAC interoperability artifacts, no devices.
use ravet_audio_file::{FlacFormat, FlacPcmEncoding, FlacWriter, encode_flac};
use std::{io::Write, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(
        args.next()
            .ok_or("usage: encode-flac <new-output-directory>")?,
    );
    if args.next().is_some() {
        return Err("too many arguments".into());
    }
    std::fs::create_dir(&root)?;
    for encoding in [FlacPcmEncoding::Pcm16, FlacPcmEncoding::Pcm24] {
        let bits = encoding.bits();
        let limit = 1_i32 << (bits - 1);
        for frames in [1, 4097, 4113] {
            let samples: Vec<i32> = (0..frames * 2)
                .map(|index| match index % 7 {
                    0 => -limit,
                    1 => limit - 1,
                    2 => 0,
                    3 => -1,
                    4 => 1,
                    _ => ((index * 13007) % (limit * 2)) - limit,
                })
                .collect();
            let encoded = encode_flac(
                &samples,
                FlacFormat {
                    channels: 2,
                    sample_rate: 48000,
                    encoding,
                },
            )?;
            let mut flac = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(root.join(format!("synthetic-{bits}-{frames}.flac")))?;
            flac.write_all(&encoded)?;
            let stream_file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(root.join(format!("streamed-{bits}-{frames}.flac")))?;
            let mut writer = FlacWriter::new(
                stream_file,
                FlacFormat {
                    channels: 2,
                    sample_rate: 48000,
                    encoding,
                },
                frames as u64,
                1_048_576,
            )?;
            for chunk in samples.chunks(226) {
                writer.write_frames(chunk)?;
            }
            writer.finish()?;
            let mut pcm = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(root.join(format!("synthetic-{bits}-{frames}.pcm")))?;
            for sample in samples {
                pcm.write_all(&sample.to_le_bytes()[..bits / 8])?;
            }
            println!("Wrote public synthetic PCM{bits}/FLAC, stereo 48000 Hz/{frames} frames");
        }
    }
    // High-dynamic-range five-channel fixture exercises prediction fallback.
    let limit = 1i32 << 23;
    let samples: Vec<i32> = (0..4096 * 5)
        .map(|i| match i % 5 {
            0 => -limit,
            1 => limit - 1,
            2 => 0,
            _ => (i * 13007) % limit,
        })
        .collect();
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join("streamed-24-4096-fivechannel.flac"))?;
    let mut writer = FlacWriter::new(
        file,
        FlacFormat {
            channels: 5,
            sample_rate: 48000,
            encoding: FlacPcmEncoding::Pcm24,
        },
        4096,
        1_048_576,
    )?;
    for chunk in samples.chunks(113 * 5) {
        writer.write_frames(chunk)?;
    }
    writer.finish()?;
    let mut pcm = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join("synthetic-24-4096-fivechannel.pcm"))?;
    for sample in samples {
        pcm.write_all(&sample.to_le_bytes()[..3])?;
    }
    println!(
        "Wrote public synthetic PCM24/FLAC prediction-fallback fixture, five channels/4096 frames"
    );
    Ok(())
}
