//! Explicit worker decoding and independently inspectable Float32 WAV export.
use ravet_audio_file::{WaveEncoding, WaveFormat, WaveWriter, decode_compressed};
use std::{
    fs::{File, OpenOptions},
    io::Read,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let input = args
        .next()
        .ok_or("usage: decode <input> <new-output.wav>")?;
    let output = args.next().ok_or("missing output")?;
    if args.next().is_some() {
        return Err("too many arguments".into());
    }
    let mut encoded = Vec::new();
    File::open(input)?
        .take(67_108_865)
        .read_to_end(&mut encoded)?;
    let clip = decode_compressed(&encoded, 16_777_216)?;
    let destination = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    let mut writer = WaveWriter::new(
        destination,
        WaveFormat {
            channels: clip.channels(),
            sample_rate: clip.sample_rate(),
            encoding: WaveEncoding::Float32,
        },
        1024,
    )?;
    for chunk in clip.samples().chunks(1024 * usize::from(clip.channels())) {
        writer.write_frames(chunk)?;
    }
    writer.finish()?;
    println!(
        "Decoded {} frames, {} channels, {} Hz",
        clip.frames(),
        clip.channels(),
        clip.sample_rate()
    );
    Ok(())
}
