//! Explicit threaded synthetic capture-ring/FLAC interoperability, no devices.
use ravet_audio_file::{FlacCapturePolicy, FlacFormat, FlacPcmEncoding, FlacRecorder, audio_ring};
use std::{io::Write, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(
        args.next()
            .ok_or("usage: capture-flac <new-output-directory>")?,
    );
    if args.next().is_some() {
        return Err("too many arguments".into());
    }
    std::fs::create_dir(&root)?;
    const FRAMES: usize = 8193;
    for encoding in [FlacPcmEncoding::Pcm16, FlacPcmEncoding::Pcm24] {
        let bits = encoding.bits();
        let scale = 1i32 << (bits - 1);
        let pcm: Vec<i32> = (0..FRAMES * 2)
            .map(|i| match i % 7 {
                0 => -scale,
                1 => scale - 1,
                2 => 0,
                3 => -1,
                4 => 1,
                _ => (i as i32 * 13007) % (scale * 2) - scale,
            })
            .collect();
        let mut reference = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join(format!("capture-{bits}.pcm")))?;
        for sample in &pcm {
            reference.write_all(&sample.to_le_bytes()[..bits / 8])?;
        }
        reference.flush()?;
        let path = root.join(format!("capture-{bits}.flac"));
        let (mut producer, consumer) =
            audio_ring(2, 128).map_err(|_| "invalid synthetic ring configuration")?;
        let (stopped_tx, stopped_rx) = std::sync::mpsc::sync_channel(0);
        let worker = std::thread::spawn(move || -> Result<(), ravet_audio_file::AudioFileError> {
            let file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
            let mut recorder = FlacRecorder::new(
                file,
                FlacFormat {
                    channels: 2,
                    sample_rate: 48000,
                    encoding,
                },
                31,
                FRAMES as u64,
                1_048_576,
                FlacCapturePolicy::RejectOverload,
                consumer,
            )?;
            while recorder.frames() < FRAMES as u64 {
                if recorder.pump()? == 0 {
                    std::thread::yield_now();
                }
            }
            stopped_rx.recv().map_err(std::io::Error::other)?;
            recorder.finish()?;
            Ok(())
        });
        // Synthetic producer converts exact source PCM; no native audio callback.
        let samples: Vec<f32> = pcm.iter().map(|v| *v as f32 / scale as f32).collect();
        let mut at = 0;
        while at < samples.len() {
            let end = (at + 254).min(samples.len());
            let queued = producer
                .push(&samples[at..end])
                .map_err(|_| "invalid synthetic frame shape")?;
            at += queued * 2;
            if queued == 0 {
                if worker.is_finished() {
                    return Err("capture worker stopped prematurely".into());
                }
                std::thread::yield_now();
            }
        }
        drop(producer);
        stopped_tx.send(())?;
        worker.join().map_err(|_| "capture worker panicked")??;
        println!(
            "PASS synthetic PCM{bits} capture-to-FLAC: {FRAMES} stereo frames, stopped producer before finalization"
        );
    }
    Ok(())
}
