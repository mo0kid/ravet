//! Synthetic WAV loading happens before preparation; render performs no file I/O.
use ravet_audio_file::{MemoryPlayback, WaveEncoding, WaveFormat, WaveReader, WaveWriter};
use std::io::Cursor;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let format = WaveFormat {
        channels: 2,
        sample_rate: 48000,
        encoding: WaveEncoding::Float32,
    };
    let mut writer = WaveWriter::new(Cursor::new(Vec::new()), format, 4)?;
    writer.write_frames(&[0.25, -0.5, 0.5, -0.25, 0.75, -0.75, 0., 0.])?;
    let mut reader = WaveReader::new(writer.finish()?, 4)?;
    let mut decoded = [0.; 8];
    assert_eq!(reader.read_frames(&mut decoded)?, 4);
    let mut transport = MemoryPlayback::new(&decoded, 2, reader.format().sample_rate, 8)?;
    drop(reader);
    transport.set_loop(Some(1..3))?;
    transport.play();
    let mut output = [0.; 12];
    let progress = transport.render(&mut output)?;
    assert_eq!(progress.loop_wraps, 3);
    assert_eq!(
        output,
        [
            0.5, -0.25, 0.75, -0.75, 0.5, -0.25, 0.75, -0.75, 0.5, -0.25, 0.75, -0.75
        ]
    );
    transport.pause();
    transport.render(&mut output)?;
    assert_eq!(output, [0.; 12]);
    println!("PASS: decoded WAV, looped exact stereo frames and paused silently");
    Ok(())
}
