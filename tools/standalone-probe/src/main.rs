//! Evidence helper for standalone audio/MIDI runs on a named device.
//!
//! It drives and observes a standalone application from outside, so the
//! application itself stays unmodified:
//!
//! - `list` prints audio outputs (`ravet_device::output_devices`) and MIDI
//!   input/output ports (`ravet_midi_device`) with their IDs.
//! - `notes <midi-out-id>` sends a fixed pattern: A2 (note 45, 110 Hz) for
//!   1 s, 0.7 s rest, A3 (note 57, 220 Hz) for 1 s, at velocity 100.
//! - `capture-notes <audio-out-id|default> <midi-out-id> <wav>` records the
//!   output device through a loopback input stream (WASAPI loopback on
//!   Windows) while sending that pattern after 1 s of silence, then writes a
//!   32-bit float WAV.
//! - `capture <audio-out-id|default> <seconds> <wav>` records the output
//!   device the same way without sending MIDI (at most 60 s).
//! - `analyse <wav>` prints RMS and an autocorrelation pitch estimate for
//!   each 100 ms window of the first channel of a 32-bit float WAV.
//!
//! The capture callback only pushes samples into a preallocated ring; it is a
//! test helper, not a real-time product path. Loopback capture of an output
//! device is a CPAL WASAPI feature; other hosts need an external recorder
//! (for example `pw-record` on PipeWire).

use ravet_device::cpal::{
    self,
    traits::{DeviceTrait, StreamTrait},
};
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const PATTERN: [(f64, [u8; 3]); 4] = [
    (0.0, [0x90, 45, 100]),
    (1.0, [0x80, 45, 0]),
    (1.7, [0x90, 57, 100]),
    (2.7, [0x80, 57, 0]),
];

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        [] | ["list"] => list(),
        ["notes", midi] => {
            let mut output = ravet_midi_device::open_output(midi)?;
            send_pattern(&mut output, Instant::now())?;
            output.close();
            Ok(())
        }
        ["capture-notes", audio, midi, wav] => capture(audio, Some(midi), 5.0, wav),
        ["capture", audio, seconds, wav] => {
            let seconds: f64 = seconds.parse()?;
            if !(0.1..=60.0).contains(&seconds) {
                return Err("capture length must be 0.1 to 60 s".into());
            }
            capture(audio, None, seconds, wav)
        }
        ["analyse", wav] => analyse(wav),
        _ => Err(
            "usage: ravet-standalone-probe [list | notes <midi-out-id> | \
                  capture-notes <audio-out-id|default> <midi-out-id> <wav> | \
                  capture <audio-out-id|default> <seconds> <wav> | analyse <wav>]"
                .into(),
        ),
    }
}

fn list() -> Result<()> {
    println!("audio outputs:");
    for device in ravet_device::output_devices()? {
        println!("  {} [{}]", device.name, device.id);
    }
    if let Some(device) = ravet_device::output_device(None)? {
        println!("default output: {}", device.id()?);
    }
    println!("MIDI inputs:");
    for port in ravet_midi_device::input_ports()? {
        println!("  {} [{}]", port.name, port.id);
    }
    println!("MIDI outputs:");
    for port in ravet_midi_device::output_ports()? {
        println!("  {} [{}]", port.name, port.id);
    }
    Ok(())
}

fn send_pattern(output: &mut ravet_midi_device::MidiOutputStream, start: Instant) -> Result<()> {
    for (at, bytes) in PATTERN {
        let due = start + Duration::from_secs_f64(at);
        std::thread::sleep(due.saturating_duration_since(Instant::now()));
        output.send(&bytes)?;
        println!("{:.3} s sent {:02X?}", start.elapsed().as_secs_f64(), bytes);
    }
    Ok(())
}

fn capture(audio: &str, midi: Option<&str>, seconds: f64, wav: &str) -> Result<()> {
    let device = ravet_device::output_device((audio != "default").then_some(audio))?
        .ok_or("audio output unavailable")?;
    let supported = device.default_output_config()?;
    if supported.sample_format() != cpal::SampleFormat::F32 {
        return Err("loopback capture supports f32 devices only".into());
    }
    let config: cpal::StreamConfig = supported.into();
    let channels = usize::from(config.channels);
    let rate = config.sample_rate;
    println!(
        "capturing {} ({}) at {rate} Hz, {channels} channels",
        device.description()?.name(),
        device.id()?
    );
    let capacity = (f64::from(rate) * seconds) as usize * channels + rate as usize * channels;
    let (mut producer, mut consumer) = rtrb::RingBuffer::<f32>::new(capacity);
    let callbacks = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let counter = callbacks.clone();
    // On WASAPI an input stream built on a render endpoint is a loopback stream.
    let stream = device.build_input_stream(
        config,
        move |data: &[f32], _| {
            counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            for &sample in data {
                if producer.push(sample).is_err() {
                    break;
                }
            }
        },
        |error| eprintln!("capture stream error: {error}"),
        None,
    )?;
    let mut output = midi.map(ravet_midi_device::open_output).transpose()?;
    stream.play()?;
    let start = Instant::now();
    if let Some(output) = &mut output {
        send_pattern(output, start + Duration::from_secs(1))?;
    }
    std::thread::sleep(
        (start + Duration::from_secs_f64(seconds)).saturating_duration_since(Instant::now()),
    );
    drop(stream);
    if let Some(output) = output {
        output.close();
    }
    let mut samples = Vec::with_capacity(consumer.slots());
    while let Ok(sample) = consumer.pop() {
        samples.push(sample);
    }
    println!(
        "captured {} frames in {} callbacks",
        samples.len() / channels,
        callbacks.load(std::sync::atomic::Ordering::Relaxed)
    );
    write_wav(wav, rate, channels as u16, &samples)
}

fn write_wav(path: &str, rate: u32, channels: u16, samples: &[f32]) -> Result<()> {
    let data_len = u32::try_from(samples.len() * 4)?;
    let mut bytes = Vec::with_capacity(44 + samples.len() * 4);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * u32::from(channels) * 4).to_le_bytes());
    bytes.extend_from_slice(&(channels * 4).to_le_bytes());
    bytes.extend_from_slice(&32u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

/// Read a 32-bit float WAV (format tag 3, or WAVE_FORMAT_EXTENSIBLE).
fn read_wav(path: &str) -> Result<(u32, usize, Vec<f32>)> {
    let bytes = std::fs::read(path)?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a RIFF/WAVE file".into());
    }
    let (mut position, mut format, mut data) = (12, None, None);
    while position + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[position + 4..position + 8].try_into()?) as usize;
        let body = bytes
            .get(position + 8..position + 8 + size)
            .ok_or("truncated chunk")?;
        match &bytes[position..position + 4] {
            b"fmt " if size >= 16 => format = Some(body),
            b"data" => data = Some(body),
            _ => {}
        }
        position += 8 + size + (size & 1);
    }
    let format = format.ok_or("missing fmt chunk")?;
    let tag = u16::from_le_bytes([format[0], format[1]]);
    let channels = usize::from(u16::from_le_bytes([format[2], format[3]]));
    let rate = u32::from_le_bytes(format[4..8].try_into()?);
    let bits = u16::from_le_bytes([format[14], format[15]]);
    if !(tag == 3 || tag == 0xFFFE) || bits != 32 || channels == 0 {
        return Err(format!("expected 32-bit float WAV, found tag {tag}, {bits} bits").into());
    }
    let samples = data
        .ok_or("missing data chunk")?
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| f32::from_le_bytes(*bytes))
        .collect();
    Ok((rate, channels, samples))
}

fn analyse(path: &str) -> Result<()> {
    let (rate, channels, samples) = read_wav(path)?;
    let first: Vec<f32> = samples.iter().step_by(channels).copied().collect();
    let identical = channels < 2
        || samples
            .chunks_exact(channels)
            .all(|frame| frame.iter().all(|&s| s == frame[0]));
    let peak = samples.iter().fold(0f32, |m, s| m.max(s.abs()));
    println!(
        "{rate} Hz, {channels} channels, {} frames ({:.2} s), peak {peak:.4}, channels identical: {identical}",
        first.len(),
        first.len() as f64 / f64::from(rate)
    );
    let hop = rate as usize / 10;
    for (index, window) in first.chunks_exact(hop).enumerate() {
        let rms = (window.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / hop as f64).sqrt();
        let level = if rms > 0.0 {
            20.0 * rms.log10()
        } else {
            f64::NEG_INFINITY
        };
        let pitch = (rms > 1e-3).then(|| pitch(window, rate)).flatten();
        match pitch {
            Some(f0) => println!(
                "{:5.1} s {level:7.1} dBFS  f0 ~ {f0:6.1} Hz",
                index as f64 / 10.0
            ),
            None => println!("{:5.1} s {level:7.1} dBFS", index as f64 / 10.0),
        }
    }
    Ok(())
}

/// Lag of the strongest autocorrelation between 50 Hz and 1 kHz.
fn pitch(window: &[f32], rate: u32) -> Option<f64> {
    let mean = window.iter().sum::<f32>() / window.len() as f32;
    let centred: Vec<f32> = window.iter().map(|s| s - mean).collect();
    let (low, high) = (rate as usize / 1000, rate as usize / 50);
    (low..high.min(centred.len() / 2))
        .map(|lag| {
            let c: f32 = centred
                .iter()
                .zip(&centred[lag..])
                .map(|(a, b)| a * b)
                .sum();
            (lag, c)
        })
        .filter(|&(_, c)| c > 0.0)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(lag, _)| f64::from(rate) / lag as f64)
}
