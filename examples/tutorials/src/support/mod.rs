//! Offline teaching helpers. File I/O and whole-clip storage belong outside audio callbacks.
use ravet_audio_file::{WaveEncoding, WaveFormat, WaveWriter};
use ravet_foundation::{
    AudioBuffer, AudioLayout, Event, EventData, ParameterId, ProcessBlock, ProcessConfig,
    ProcessStatus, Processor, Transport,
};
use std::{fs::File, io, path::PathBuf};

pub const SAMPLE_RATE: usize = 48_000;
const BLOCK_FRAMES: usize = 256;
const MAX_EVENTS: usize = 32;
pub type Stereo = [Vec<f32>; 2];
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

// The only optional argument is the output file; no device or host is opened.
pub fn output_path(default: &str) -> Result<PathBuf> {
    let mut args = std::env::args_os().skip(1);
    if !args.next().is_some_and(|a| a == "--render") {
        return Err("Rendering requires --render".into());
    }
    let selected = args.next();
    let path = selected.map_or_else(|| PathBuf::from(default), PathBuf::from);
    if args.next().is_some() {
        return Err(
            "usage: cargo run -p ravet-examples --bin <name> -- --render [output.wav]".into(),
        );
    }
    Ok(path)
}

// Events here use absolute clip offsets. Each callback receives block-local offsets.
pub fn render(
    processor: &mut impl Processor,
    input: Option<Stereo>,
    frames: usize,
    events: &[Event],
) -> Result<Stereo> {
    if frames == 0
        || events.len() > MAX_EVENTS
        || events.iter().any(|event| event.frame_offset >= frames)
        || events
            .windows(2)
            .any(|pair| pair[0].frame_offset > pair[1].frame_offset)
        || input
            .as_ref()
            .is_some_and(|channels| channels.iter().any(|c| c.len() != frames))
    {
        return Err("invalid offline clip or event schedule".into());
    }
    let effect = input.is_some();
    let mut output = input.unwrap_or_else(|| [vec![0.; frames], vec![0.; frames]]);
    processor.prepare(ProcessConfig::new(
        SAMPLE_RATE as f64,
        BLOCK_FRAMES,
        MAX_EVENTS,
        if effect {
            AudioLayout::STEREO
        } else {
            AudioLayout::STEREO_INSTRUMENT
        },
    )?)?;
    let mut pending = events.iter().peekable();
    let mut local = [Event {
        frame_offset: 0,
        data: EventData::ParameterValue {
            id: ParameterId(0),
            value: 0.,
        },
    }; MAX_EVENTS];
    for start in (0..frames).step_by(BLOCK_FRAMES) {
        let end = (start + BLOCK_FRAMES).min(frames);
        let mut count = 0;
        while pending.peek().is_some_and(|event| event.frame_offset < end) {
            let event = *pending.next().expect("peeked event");
            local[count] = Event {
                frame_offset: event.frame_offset - start,
                ..event
            };
            count += 1;
        }
        let [left, right] = &mut output;
        let mut channels = [&mut left[start..end], &mut right[start..end]];
        let audio = if effect {
            AudioBuffer::in_place(&mut channels, end - start)?
        } else {
            AudioBuffer::separate(&[], &mut channels, end - start)?
        };
        let status = processor.process(&mut ProcessBlock::new(
            audio,
            &local[..count],
            Transport::default(),
        ));
        if status != ProcessStatus::Continue {
            processor.deactivate();
            return Err(io::Error::other(format!("offline processing failed: {status:?}")).into());
        }
    }
    processor.deactivate();
    Ok(output)
}

pub fn write_wave(path: &std::path::Path, channels: &Stereo) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    // Refuse to overwrite existing files. Use a new path for another render.
    let file = File::options().write(true).create_new(true).open(path)?;
    let mut writer = WaveWriter::new(
        file,
        WaveFormat {
            channels: 2,
            sample_rate: SAMPLE_RATE as u32,
            encoding: WaveEncoding::Pcm16,
        },
        BLOCK_FRAMES,
    )?;
    let mut interleaved = [0.; BLOCK_FRAMES * 2];
    for start in (0..channels[0].len()).step_by(BLOCK_FRAMES) {
        let count = BLOCK_FRAMES.min(channels[0].len() - start);
        for frame in 0..count {
            interleaved[2 * frame] = channels[0][start + frame];
            interleaved[2 * frame + 1] = channels[1][start + frame];
        }
        writer.write_frames(&interleaved[..count * 2])?;
    }
    writer.finish()?;
    println!(
        "Wrote {} ({} frames, stereo PCM16, 48 kHz)",
        path.display(),
        channels[0].len()
    );
    Ok(())
}
