use cpal::traits::DeviceTrait;
use ravet_device::{
    ClockedDuplexOptions, ClockedDuplexRetirement, ReclaimableClockedStatistics,
    ReclaimedClockedDuplex, StoppedClockedDuplex, cpal, input_device, input_devices,
    open_reclaimable_clocked_duplex, output_device, output_devices,
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
    id: u64,
    control: std::thread::ThreadId,
    lifecycle: Arc<[AtomicUsize; 4]>,
}
impl Processor for Silence {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        self.lifecycle[0].fetch_add(1, Ordering::Relaxed);
        if std::thread::current().id() != self.control {
            self.lifecycle[3].fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }
    fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        self.lifecycle[2].fetch_add(1, Ordering::Relaxed);
        b.clear_outputs();
        ProcessStatus::Silence
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {
        self.lifecycle[1].fetch_add(1, Ordering::Relaxed);
        if std::thread::current().id() != self.control {
            self.lifecycle[3].fetch_add(1, Ordering::Relaxed);
        }
    }
}
fn reclaim(
    mut proof: ClockedDuplexRetirement<Silence>,
) -> Result<(StoppedClockedDuplex<Silence>, ReclaimableClockedStatistics), Box<dyn std::error::Error>>
{
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(outcome) = proof.try_reclaim()? {
            return match outcome {
                ReclaimedClockedDuplex::Reclaimed { owner, statistics } => Ok((owner, statistics)),
                ReclaimedClockedDuplex::WorkerFault { error, .. } => Err(error.into()),
            };
        }
        if Instant::now() >= deadline {
            return Err(
                "paired retirement still pending after five seconds; no owner receipt".into(),
            );
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.is_empty() || args == ["--list"] {
        for d in input_devices()? {
            println!("INPUT {} [{}]", d.name, d.id);
            for c in d.configurations {
                println!("  {c:?}");
            }
        }
        for d in output_devices()? {
            println!("OUTPUT {} [{}]", d.name, d.id);
            for c in d.configurations {
                println!("  {c:?}");
            }
        }
        return Ok(());
    }
    if args.len() != 3 || args[0] != "--validate-silent" {
        return Err("usage: --list | --validate-silent INPUT_ID OUTPUT_ID".into());
    }
    let input = input_device(Some(&args[1]))?.ok_or("selected input unavailable")?;
    let output = output_device(Some(&args[2]))?.ok_or("selected output unavailable")?;
    let input_supported = input.default_input_config()?;
    let output_supported = output.default_output_config()?;
    if input_supported.sample_format() != cpal::SampleFormat::F32
        || output_supported.sample_format() != cpal::SampleFormat::F32
    {
        return Err("selected current native pair is not f32; no fallback".into());
    }
    println!(
        "SELECTED INPUT {} [{}] {input_supported:?}",
        input.description()?.name(),
        args[1]
    );
    println!(
        "SELECTED OUTPUT {} [{}] {output_supported:?}",
        output.description()?.name(),
        args[2]
    );
    let options = ClockedDuplexOptions {
        maximum_frames: 512,
        taps: 16,
        clock: ravet_signal::FifoClockConfig {
            capacity_frames: 4096,
            target_frames: 2048,
            ..Default::default()
        },
    };
    let lifecycle = Arc::new(std::array::from_fn(|_| AtomicUsize::new(0)));
    let mut processor = Silence {
        id: 0x5241564554218,
        control: std::thread::current().id(),
        lifecycle: Arc::clone(&lifecycle),
    };
    for cycle in 1..=2 {
        let stream = open_reclaimable_clocked_duplex(
            &input,
            &output,
            input_supported.config(),
            output_supported.config(),
            4096,
            processor,
            options,
        )?;
        stream.play()?;
        std::thread::sleep(Duration::from_millis(350));
        stream.pause()?;
        let observed = stream.statistics();
        println!("CYCLE {cycle} live {observed:?}");
        let began = Instant::now();
        let (owner, retired) = reclaim(stream.stop())?;
        println!("CYCLE {cycle} retired {:?} {retired:?}", began.elapsed());
        if retired.input.callbacks == 0
            || retired.input.captured_frames == 0
            || retired.output.callbacks == 0
            || retired.output.frames == 0
            || retired.input.backend_errors != 0
            || retired.output.backend_errors != 0
            || retired.output.processor_faults != 0
            || retired.output.panics != 0
            || retired.capture_panics != 0
        {
            return Err(
                "selected silent pair did not produce clean callback/retirement evidence".into(),
            );
        }
        let (original, raw, capture) = owner.into_parts();
        if original.id != 0x5241564554218
            || !Arc::ptr_eq(&original.lifecycle, &lifecycle)
            || lifecycle[0].load(Ordering::Relaxed) != cycle
            || lifecycle[1].load(Ordering::Relaxed) != cycle
            || lifecycle[3].load(Ordering::Relaxed) != 0
        {
            return Err("original processor identity/control lifecycle mismatch".into());
        }
        println!(
            "CYCLE {cycle} original processor recovered; remaining raw queued={} frames",
            raw.queued_frames()
        );
        drop(raw);
        drop(capture);
        processor = original;
    }
    println!(
        "PASS exact selected pair, two silent lifecycle/retirement/reprepare receipts; prepares={} deactivates={} process_calls={} foreign_control_calls={}",
        lifecycle[0].load(Ordering::Relaxed),
        lifecycle[1].load(Ordering::Relaxed),
        lifecycle[2].load(Ordering::Relaxed),
        lifecycle[3].load(Ordering::Relaxed)
    );
    println!(
        "No acoustic/timestamp/hotplug/drift/deadline claim; no captured PCM persisted or emitted."
    );
    drop(processor);
    Ok(())
}
