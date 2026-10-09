//! Explicit synthetic AIFF creation for independent decoder validation.
use ravet_audio_file::{AiffEncoding, AiffFormat, AiffWriter};
use std::fs::OpenOptions;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: aiff <new-output.aiff>")?;
    let destination = OpenOptions::new().write(true).create_new(true).open(path)?;
    let mut writer = AiffWriter::new(
        destination,
        AiffFormat {
            channels: 2,
            sample_rate: 48000,
            encoding: AiffEncoding::Pcm24,
        },
        256,
    )?;
    for _ in 0..4 {
        writer.write_frames(&[0.25, -0.5].repeat(256))?;
    }
    writer.finish()?;
    println!("Wrote 1024 stereo PCM24 frames at 48000 Hz.");
    Ok(())
}
