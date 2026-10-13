use cpal::traits::DeviceTrait;
use ravet_device::{
    cpal, input_device, input_devices, open_duplex, open_output, output_device, output_devices,
};
use ravet_foundation::{PrepareError, ProcessBlock, ProcessConfig, ProcessStatus, Processor};

// Stream snapshots include xruns/route_changes as diagnostics; failure checks below
// use backend_errors and panics, preserving the stream recovery contract.
struct Silence;
impl Processor for Silence {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        block.clear_outputs();
        ProcessStatus::Silence
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {}
}
struct RecoverySilence {
    lifecycle: std::sync::Arc<[std::sync::atomic::AtomicUsize; 3]>,
    control: std::thread::ThreadId,
}
impl Processor for RecoverySilence {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        self.lifecycle[0].fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if std::thread::current().id() != self.control {
            self.lifecycle[2].fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        block.clear_outputs();
        ProcessStatus::Silence
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {
        self.lifecycle[1].fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if std::thread::current().id() != self.control {
            self.lifecycle[2].fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.is_empty() || args == ["--list"] {
        for device in output_devices()? {
            println!("{} [{}]", device.name, device.id);
            for config in device.configurations {
                println!("  {config:?}");
            }
        }
    } else if args.first().map(String::as_str) == Some("--validate-silence") && args.len() <= 2 {
        let device =
            output_device(args.get(1).map(String::as_str))?.ok_or("output device unavailable")?;
        let supported = device.default_output_config()?;
        if supported.sample_format() != cpal::SampleFormat::F32 {
            return Err("default output is not f32; select/configure a supported device".into());
        }
        println!(
            "Validating silent output: {} {:?}",
            device.description()?.name(),
            supported
        );
        let stream = open_output(&device, supported.config(), 1024, Silence)?;
        stream.play()?;
        std::thread::sleep(std::time::Duration::from_secs(1));
        stream.pause()?;
        let stats = stream.statistics();
        println!("{stats:?}");
        if stats.callbacks == 0
            || stats.frames == 0
            || stats.processor_faults != 0
            || stats.backend_errors != 0
            || stats.panics != 0
        {
            return Err("silent callback validation failed".into());
        }
    } else if args.first().map(String::as_str) == Some("--validate-recovery-silence")
        && args.len() <= 2
    {
        use ravet_device::{OutputRecoveryStatus, RecoveringOutput};
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let device =
            output_device(args.get(1).map(String::as_str))?.ok_or("output device unavailable")?;
        let supported = device.default_output_config()?;
        if supported.sample_format() != cpal::SampleFormat::F32 {
            return Err("default output is not f32".into());
        }
        let id = device.id()?.to_string();
        println!(
            "Validating silent recovery: {} [{id}] {supported:?}",
            device.description()?.name()
        );
        let lifecycle = Arc::new([
            AtomicUsize::new(0),
            AtomicUsize::new(0),
            AtomicUsize::new(0),
        ]);
        let control = std::thread::current().id();
        let factory = lifecycle.clone();
        let mut recovery =
            RecoveringOutput::new(id, supported.config(), 1024, move || RecoverySilence {
                lifecycle: factory.clone(),
                control,
            })?;
        for generation in 1..=3 {
            if generation == 2 {
                recovery.request_reopen();
            }
            if generation == 3 {
                recovery.set_enabled(false);
                assert_eq!(recovery.service()?, OutputRecoveryStatus::Disabled);
                assert!(recovery.statistics().is_none());
                recovery.set_enabled(true);
            }
            assert_eq!(
                recovery.service()?,
                OutputRecoveryStatus::Opened { generation }
            );
            std::thread::sleep(std::time::Duration::from_millis(250));
            assert_eq!(recovery.service()?, OutputRecoveryStatus::Running);
            let stats = recovery.statistics().ok_or("missing running output")?;
            println!("Generation {generation}: {stats:?}");
            if stats.callbacks == 0
                || stats.frames == 0
                || stats.processor_faults != 0
                || stats.backend_errors != 0
                || stats.panics != 0
            {
                return Err("recovery callback validation failed".into());
            }
        }
        drop(recovery);
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        while lifecycle[1].load(Ordering::SeqCst) != 3 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if lifecycle[0].load(Ordering::SeqCst) != 3
            || lifecycle[1].load(Ordering::SeqCst) != 3
            || lifecycle[2].load(Ordering::SeqCst) != 0
        {
            return Err("recovery processor lifecycle count failed".into());
        }
        println!(
            "PASS three silent output generations with exact control-thread prepare/deactivate counts; physical disconnect not exercised"
        );
    } else if args.first().map(String::as_str) == Some("--validate-recovery-duplex-silence")
        && args.len() <= 3
    {
        use ravet_device::{DuplexRecoveryStatus, RecoveringDuplex};
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let input = input_device(args.get(1).map(String::as_str))?.ok_or("input unavailable")?;
        let output = output_device(args.get(2).map(String::as_str))?.ok_or("output unavailable")?;
        let input_config = input.default_input_config()?;
        let output_config = output.default_output_config()?;
        if input_config.sample_format() != cpal::SampleFormat::F32
            || output_config.sample_format() != cpal::SampleFormat::F32
        {
            return Err("default input/output must use f32".into());
        }
        let input_id = input.id()?.to_string();
        let output_id = output.id()?.to_string();
        println!(
            "Validating silent pair recovery: {} {input_config:?} / {} {output_config:?}",
            input.description()?.name(),
            output.description()?.name()
        );
        // Exercise native exact-ID absence without changing connected hardware.
        for (i, o) in [
            ("ravet-validation-missing-input", output_id.as_str()),
            (input_id.as_str(), "ravet-validation-missing-output"),
        ] {
            let mut missing = RecoveringDuplex::new(
                i,
                o,
                input_config.config(),
                output_config.config(),
                1024,
                8192,
                || Silence,
            )?;
            assert_eq!(missing.service()?, DuplexRecoveryStatus::Waiting);
            assert!(missing.statistics().is_none());
        }
        let lifecycle = Arc::new([
            AtomicUsize::new(0),
            AtomicUsize::new(0),
            AtomicUsize::new(0),
        ]);
        let factory = lifecycle.clone();
        let control = std::thread::current().id();
        let mut recovery = RecoveringDuplex::new(
            input_id,
            output_id,
            input_config.config(),
            output_config.config(),
            1024,
            8192,
            move || RecoverySilence {
                lifecycle: factory.clone(),
                control,
            },
        )?;
        for generation in 1..=3 {
            if generation == 2 {
                recovery.request_reopen();
            }
            if generation == 3 {
                recovery.set_enabled(false);
                assert_eq!(recovery.service()?, DuplexRecoveryStatus::Disabled);
                assert!(recovery.statistics().is_none());
                recovery.set_enabled(true);
            }
            assert_eq!(
                recovery.service()?,
                DuplexRecoveryStatus::Opened { generation }
            );
            std::thread::sleep(std::time::Duration::from_millis(250));
            assert_eq!(recovery.service()?, DuplexRecoveryStatus::Running);
            let (captured, rendered) = recovery.statistics().ok_or("missing running pair")?;
            println!("Generation {generation}: input {captured:?} / output {rendered:?}");
            if captured.callbacks == 0
                || captured.captured_frames == 0
                || captured.invalid_buffers != 0
                || captured.backend_errors != 0
                || rendered.callbacks == 0
                || rendered.frames == 0
                || rendered.processor_faults != 0
                || rendered.backend_errors != 0
                || rendered.panics != 0
            {
                return Err("pair recovery callback validation failed".into());
            }
        }
        drop(recovery);
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        while lifecycle[1].load(Ordering::SeqCst) != 3 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if lifecycle[0].load(Ordering::SeqCst) != 3
            || lifecycle[1].load(Ordering::SeqCst) != 3
            || lifecycle[2].load(Ordering::SeqCst) != 0
        {
            return Err("pair processor lifecycle count failed".into());
        }
        println!(
            "PASS missing input/output IDs and three silent pair generations with control-thread prepare/deactivate; physical disconnect not exercised"
        );
    } else if args.first().map(String::as_str) == Some("--validate-recovery-clocked-duplex-silence")
        && args.len() == 3
    {
        use ravet_device::{ClockedDuplexOptions, DuplexRecoveryStatus, RecoveringClockedDuplex};
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let input = input_device(Some(&args[1]))?.ok_or("input unavailable")?;
        let output = output_device(Some(&args[2]))?.ok_or("output unavailable")?;
        let input_config = input.default_input_config()?;
        let output_config = output.default_output_config()?;
        if input_config.sample_format() != cpal::SampleFormat::F32
            || output_config.sample_format() != cpal::SampleFormat::F32
        {
            return Err("default devices must use f32".into());
        }
        let input_id = input.id()?.to_string();
        let output_id = output.id()?.to_string();
        println!(
            "Clocked recovery {input_id} {input_config:?} / silent {output_id} {output_config:?}"
        );
        for (i, o) in [
            ("ravet-validation-missing-input", output_id.as_str()),
            (input_id.as_str(), "ravet-validation-missing-output"),
        ] {
            let mut missing = RecoveringClockedDuplex::new(
                i,
                o,
                input_config.config(),
                output_config.config(),
                ClockedDuplexOptions::default(),
                || -> RecoverySilence { panic!("missing device invoked factory") },
            )?;
            assert_eq!(missing.service()?, DuplexRecoveryStatus::Waiting);
            assert!(missing.statistics().is_none());
        }
        let lifecycle = Arc::new([
            AtomicUsize::new(0),
            AtomicUsize::new(0),
            AtomicUsize::new(0),
        ]);
        let control = std::thread::current().id();
        let counts = lifecycle.clone();
        let mut owner = RecoveringClockedDuplex::new(
            input_id,
            output_id,
            input_config.config(),
            output_config.config(),
            ClockedDuplexOptions::default(),
            move || RecoverySilence {
                lifecycle: counts.clone(),
                control,
            },
        )?;
        for generation in 1..=3 {
            if generation == 2 {
                owner.request_reopen();
                assert!(owner.statistics().is_none());
            }
            if generation == 3 {
                owner.set_enabled(false);
                assert_eq!(owner.service()?, DuplexRecoveryStatus::Disabled);
                assert!(owner.statistics().is_none());
                owner.set_enabled(true);
            }
            assert_eq!(
                owner.service()?,
                DuplexRecoveryStatus::Opened { generation }
            );
            std::thread::sleep(std::time::Duration::from_millis(500));
            assert_eq!(owner.service()?, DuplexRecoveryStatus::Running);
            let (captured, rendered, clock) = owner.statistics().ok_or("missing clocked pair")?;
            println!("Generation {generation}: {captured:?} / {rendered:?} / {clock:?}");
            if captured.callbacks == 0
                || captured.captured_frames == 0
                || captured.invalid_buffers != 0
                || captured.backend_errors != 0
                || rendered.callbacks == 0
                || rendered.frames == 0
                || rendered.processor_faults != 0
                || rendered.backend_errors != 0
                || rendered.panics != 0
            {
                return Err("clocked recovery callbacks failed".into());
            }
        }
        drop(owner);
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        while lifecycle[1].load(Ordering::SeqCst) != 3 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if lifecycle[0].load(Ordering::SeqCst) != 3
            || lifecycle[1].load(Ordering::SeqCst) != 3
            || lifecycle[2].load(Ordering::SeqCst) != 0
        {
            return Err("clocked recovery control-thread lifecycle failed".into());
        }
        println!(
            "PASS missing exact IDs, manual reopen and disable/reenable across three silent clocked generations; physical unplug/backend failure not exercised"
        );
    } else if args.first().map(String::as_str) == Some("--validate-clocked-duplex-silence")
        && (3..=4).contains(&args.len())
    {
        use ravet_device::{ClockedDuplexOptions, open_clocked_duplex};
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let seconds = args
            .get(3)
            .map(|value| value.parse::<u64>())
            .transpose()?
            .unwrap_or(1);
        if !(1..=60).contains(&seconds) {
            return Err("clocked probe seconds must be 1..60 per generation".into());
        }
        let input = input_device(Some(&args[1]))?.ok_or("input unavailable")?;
        let output = output_device(Some(&args[2]))?.ok_or("output unavailable")?;
        let input_config = input.default_input_config()?;
        let output_config = output.default_output_config()?;
        if input_config.sample_format() != cpal::SampleFormat::F32
            || output_config.sample_format() != cpal::SampleFormat::F32
        {
            return Err("default devices must use f32".into());
        }
        println!(
            "Clocked capture {} [{}] {input_config:?} / silent output {} [{}] {output_config:?}",
            input.description()?.name(),
            input.id()?,
            output.description()?.name(),
            output.id()?
        );
        let lifecycle = Arc::new([
            AtomicUsize::new(0),
            AtomicUsize::new(0),
            AtomicUsize::new(0),
        ]);
        let control = std::thread::current().id();
        for generation in 1..=3 {
            let options = ClockedDuplexOptions::default();
            let stream = open_clocked_duplex(
                &input,
                &output,
                input_config.config(),
                output_config.config(),
                options.clock.capacity_frames,
                RecoverySilence {
                    lifecycle: lifecycle.clone(),
                    control,
                },
                options,
            )?;
            stream.play()?;
            std::thread::sleep(std::time::Duration::from_secs(seconds));
            stream.pause()?;
            let captured = stream.input_statistics();
            let rendered = stream.output_statistics();
            let clock = stream.clock_statistics();
            println!(
                "Generation {generation}: input {captured:?} / output {rendered:?} / clock {clock:?}"
            );
            if captured.callbacks == 0
                || captured.captured_frames == 0
                || captured.invalid_buffers != 0
                || captured.backend_errors != 0
                || rendered.callbacks == 0
                || rendered.frames == 0
                || rendered.processor_faults != 0
                || rendered.backend_errors != 0
                || rendered.panics != 0
                || !clock.correction_ppm.is_finite()
                || clock.correction_ppm.abs() > options.clock.maximum_ppm
            {
                return Err("clocked pair callback validation failed".into());
            }
            // Pause/resume retains histories; explicit reconstruction starts fresh.
            stream.play()?;
            std::thread::sleep(std::time::Duration::from_millis(100));
            stream.pause()?;
            let resumed_input = stream.input_statistics();
            let resumed = stream.output_statistics();
            if resumed.frames <= rendered.frames
                || resumed.processor_faults != 0
                || resumed.backend_errors != 0
                || resumed.panics != 0
                || resumed_input.invalid_buffers != 0
                || resumed_input.backend_errors != 0
            {
                return Err("clocked pair resume validation failed".into());
            }
            drop(stream);
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        while lifecycle[1].load(Ordering::SeqCst) != 3 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if lifecycle[0].load(Ordering::SeqCst) != 3
            || lifecycle[1].load(Ordering::SeqCst) != 3
            || lifecycle[2].load(Ordering::SeqCst) != 0
        {
            return Err("clocked pair control-thread lifecycle failed".into());
        }
        println!(
            "PASS three silent clocked pair generations and pause/resume with control-thread prepare/deactivate; no long-session drift/deadline claim"
        );
    } else if args == ["--list-input"] {
        for device in input_devices()? {
            println!("{} [{}]", device.name, device.id);
            for config in device.configurations {
                println!("  {config:?}");
            }
        }
    } else if args.first().map(String::as_str) == Some("--validate-duplex-silence")
        && args.len() <= 3
    {
        let input =
            input_device(args.get(1).map(String::as_str))?.ok_or("input device unavailable")?;
        let output =
            output_device(args.get(2).map(String::as_str))?.ok_or("output device unavailable")?;
        let input_config = input.default_input_config()?;
        let output_config = output.default_output_config()?;
        if input_config.sample_format() != cpal::SampleFormat::F32
            || output_config.sample_format() != cpal::SampleFormat::F32
        {
            return Err("default devices must use f32".into());
        }
        println!(
            "Validating input {} {:?} / silent output {} {:?}",
            input.description()?.name(),
            input_config,
            output.description()?.name(),
            output_config
        );
        let stream = open_duplex(
            &input,
            &output,
            input_config.config(),
            output_config.config(),
            1024,
            8192,
            Silence,
        )?;
        stream.play()?;
        std::thread::sleep(std::time::Duration::from_secs(1));
        stream.pause()?;
        let captured = stream.input_statistics();
        let rendered = stream.output_statistics();
        println!("Input {captured:?} / output {rendered:?}");
        if captured.callbacks == 0
            || captured.captured_frames == 0
            || captured.invalid_buffers != 0
            || captured.backend_errors != 0
            || rendered.frames == 0
            || rendered.processor_faults != 0
            || rendered.backend_errors != 0
            || rendered.panics != 0
        {
            return Err("duplex callback validation failed".into());
        }
    } else {
        return Err("usage: ravet-device-probe [--list | --list-input | --validate-silence [output-id] | --validate-recovery-silence [output-id] | --validate-duplex-silence [input-id [output-id]] | --validate-recovery-duplex-silence [input-id [output-id]] | --validate-clocked-duplex-silence input-id output-id [seconds] | --validate-recovery-clocked-duplex-silence input-id output-id]".into());
    }
    Ok(())
}
