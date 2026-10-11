//! Local, explicitly silent physical lifecycle receipt; no audible/support claim.
use ravet_device::{
    ReclaimedOutput,
    cpal::{
        self,
        traits::{DeviceTrait, HostTrait},
    },
    open_reclaimable_output,
};
use ravet_foundation::{PrepareError, ProcessBlock, ProcessConfig, ProcessStatus, Processor};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
struct Silence {
    prepares: Arc<AtomicUsize>,
    deactivations: Arc<AtomicUsize>,
}
impl Processor for Silence {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        self.prepares.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        b.clear_outputs();
        ProcessStatus::Silence
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {
        self.deactivations.fetch_add(1, Ordering::Relaxed);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("no default output device")?;
    let supported = device.default_output_config()?;
    if supported.sample_format() != cpal::SampleFormat::F32 {
        return Err("default output profile is not f32; no implicit format fallback".into());
    }
    let config = supported.config();
    let prepares = Arc::new(AtomicUsize::new(0));
    let deactivations = Arc::new(AtomicUsize::new(0));
    let mut processor = Silence {
        prepares: Arc::clone(&prepares),
        deactivations: Arc::clone(&deactivations),
    };
    let mut frames = [0; 2];
    for (cycle, rendered) in frames.iter_mut().enumerate() {
        let owner = open_reclaimable_output(&device, config, 256, processor)?;
        owner.play()?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while owner.statistics().frames == 0 {
            if Instant::now() >= deadline {
                return Err(
                    "silent native callback did not complete within local observation window"
                        .into(),
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let mut retirement = owner.stop();
        processor = loop {
            match retirement.try_reclaim()? {
                Some(ReclaimedOutput::Reclaimed {
                    processor,
                    statistics,
                }) => {
                    *rendered = statistics.frames;
                    if statistics.panics != 0
                        || statistics.backend_errors != 0
                        || statistics.processor_faults != 0
                    {
                        return Err("native callback fault in local receipt".into());
                    }
                    break processor;
                }
                Some(ReclaimedOutput::WorkerFault { error, .. }) => return Err(error.into()),
                None => {
                    if Instant::now() >= deadline {
                        return Err(
                            "native retirement remained pending at local observation window".into(),
                        );
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        };
        assert_eq!(prepares.load(Ordering::Relaxed), cycle + 1);
        assert_eq!(deactivations.load(Ordering::Relaxed), cycle + 1);
    }
    println!(
        "silent_native_receipt host={:?} channels={} rate={} frames={frames:?} prepares={} deactivations={} support_claim=false",
        host.id(),
        config.channels,
        config.sample_rate,
        prepares.load(Ordering::Relaxed),
        deactivations.load(Ordering::Relaxed)
    );
    drop(processor);
    Ok(())
}
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [] => run(),
        [mode] if mode == "--converted" => run_converted(),
        _ => Err(
            "usage: reclaimable-output [--converted]; converted requires resampled-playback".into(),
        ),
    };
    if let Err(error) = result {
        eprintln!("silent_native_receipt_failed: {error}");
        std::process::exit(1)
    }
}

#[cfg(not(feature = "resampled-playback"))]
fn run_converted() -> Result<(), Box<dyn std::error::Error>> {
    Err("--converted requires the explicit resampled-playback feature".into())
}

/// The PCM is zero in every exact default-device channel. This tests ownership,
/// bounded conversion, EOF/drain and stopped seek, not audible quality or clocks.
#[cfg(feature = "resampled-playback")]
fn run_converted() -> Result<(), Box<dyn std::error::Error>> {
    use ravet_audio_file::{
        AudioFileError, ConvertedReadAhead, ConvertedReadAheadOptions, PlaybackConversion,
        ReadAheadStatus, ReadAheadWorker, WaveEncoding, WaveFormat, WaveReader, WaveWriter,
        audio_ring,
    };
    use ravet_device::BufferedPlaybackOutput;
    use std::io::Cursor;
    const INPUT_RATE: u32 = 44100;
    const INPUT_FRAMES: usize = 257;
    const SEEK: u64 = 17;
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("no default output device")?;
    let supported = device.default_output_config()?;
    if supported.sample_format() != cpal::SampleFormat::F32 {
        return Err("default output profile is not f32; no implicit format fallback".into());
    }
    let config = supported.config();
    let channels = usize::from(config.channels);
    if !(1..=64).contains(&channels)
        || !(INPUT_RATE / 16..=INPUT_RATE * 16).contains(&config.sample_rate)
    {
        return Err("default exact layout/rate lies outside bounded converted profile".into());
    }
    // Independent checked integer duration, including the documented causal tail
    // for 16 taps. No decoder trimming, resampling fallback or channel mapping.
    let total =
        ((INPUT_FRAMES as u64 + 14) * u64::from(config.sample_rate)) / u64::from(INPUT_RATE) + 1;
    if total <= SEEK {
        return Err("default profile's retained output is shorter than this seek fixture".into());
    }
    let mut writer = WaveWriter::new(
        Cursor::new(Vec::new()),
        WaveFormat {
            channels: config.channels,
            sample_rate: INPUT_RATE,
            encoding: WaveEncoding::Float32,
        },
        INPUT_FRAMES,
    )?;
    writer.write_frames(&vec![0.0; INPUT_FRAMES * channels])?;
    let bytes = writer.finish()?.into_inner();
    let reader = WaveReader::new(Cursor::new(bytes.clone()), 17)?;
    let factory = move || WaveReader::new(Cursor::new(bytes.clone()), 17);
    let (producer, consumer) = audio_ring(channels, 64)
        .map_err(|error| std::io::Error::other(format!("prepared ring: {error:?}")))?;
    let pump = ConvertedReadAhead::new(
        reader,
        factory,
        producer,
        &consumer,
        ConvertedReadAheadOptions {
            output_rate: config.sample_rate,
            conversion: PlaybackConversion::Sinc { taps: 16 },
            maximum_source_frames: INPUT_FRAMES,
            maximum_output_frames: total,
            source_batch_frames: 17,
            conversion_batch_frames: 7,
        },
    )?;
    let (mut worker, mut playback) = ReadAheadWorker::new(pump, consumer, 32)?;
    worker.start()?;
    playback.play();
    let mut processor = BufferedPlaybackOutput::new(playback);
    let monitor = processor.monitor();
    let mut consumed = [0; 2];
    let mut underflow = [0; 2];
    for cycle in 0..2 {
        let expected_consumed = if cycle == 0 {
            total
        } else {
            total + total - SEEK
        };
        let owner = open_reclaimable_output(&device, config, 32, processor)?;
        owner.play()?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let stats = monitor.statistics();
            if stats.worker_status == ReadAheadStatus::Failed {
                return Err("converted reader worker failed in local native receipt".into());
            }
            if stats.ended && stats.consumed_output_frames >= expected_consumed {
                if stats.consumed_output_frames != expected_consumed
                    || stats.output_source_position != total
                    || stats.worker_status != ReadAheadStatus::EndOfFile
                {
                    return Err(
                        "converted native cursor/count/EOF disagrees with integer duration".into(),
                    );
                }
                consumed[cycle] = stats.consumed_output_frames;
                underflow[cycle] = stats.underflow_output_frames;
                break;
            }
            if Instant::now() >= deadline {
                return Err("converted native drain exceeded local observation window".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let mut retirement = owner.stop();
        let deadline = Instant::now() + Duration::from_secs(5);
        processor = loop {
            match retirement.try_reclaim()? {
                Some(ReclaimedOutput::Reclaimed {
                    processor,
                    statistics,
                }) => {
                    if statistics.panics != 0
                        || statistics.backend_errors != 0
                        || statistics.processor_faults != 0
                    {
                        return Err("converted native callback fault in local receipt".into());
                    }
                    break processor;
                }
                Some(ReclaimedOutput::WorkerFault { error, .. }) => return Err(error.into()),
                None => {
                    if Instant::now() >= deadline {
                        return Err("converted native retirement remained pending at local observation window".into());
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        };
        if cycle == 0 {
            let position = processor.playback().position_frames();
            let queued = processor.playback().queued_frames();
            if !matches!(
                worker.seek_stopped(processor.playback_mut(), total + 1),
                Err(AudioFileError::Capacity)
            ) || processor.playback().position_frames() != position
                || processor.playback().queued_frames() != queued
            {
                return Err("failed stopped seek changed the reclaimed cursor or queue".into());
            }
            worker.seek_stopped(processor.playback_mut(), SEEK)?;
            processor.playback_mut().play();
        }
    }
    drop(processor);
    let exit = worker.shutdown()?;
    if let Some(error) = exit.error {
        return Err(error.into());
    }
    println!(
        "silent_converted_native_receipt host={:?} channels={} source_rate={} target_rate={} input_frames={} retained_output_frames={} consumed={consumed:?} underflow={underflow:?} seek={} cycles=2 support_claim=false",
        host.id(),
        config.channels,
        INPUT_RATE,
        config.sample_rate,
        INPUT_FRAMES,
        total,
        SEEK
    );
    Ok(())
}
