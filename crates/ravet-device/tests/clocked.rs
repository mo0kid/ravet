use ravet_audio_file::audio_ring;
use ravet_device::{ClockedDuplexOptions, PreparedClockedDuplex};
use ravet_foundation::{PrepareError, ProcessBlock, ProcessConfig, ProcessStatus, Processor};
use ravet_signal::FifoClockConfig;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
struct Relay {
    config: Option<ProcessConfig>,
    deactivate: Arc<AtomicUsize>,
    fault: bool,
}
impl Processor for Relay {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = Some(config);
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        self.config.unwrap().validate_block(block).unwrap();
        if self.fault {
            return ProcessStatus::Silence;
        }
        for frame in 0..block.audio.frames() {
            for channel in 0..block.audio.output_channels() {
                let sample = block
                    .audio
                    .input(channel % block.audio.input_channels())
                    .unwrap()[frame];
                block.audio.output_mut(channel).unwrap()[frame] = sample;
            }
        }
        ProcessStatus::Continue
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {
        self.deactivate.fetch_add(1, Ordering::SeqCst);
        self.config = None;
    }
}
struct NeverPrepare;
impl Processor for NeverPrepare {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        panic!("invalid options reached processor preparation")
    }
    fn process(&mut self, _: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        panic!("invalid options reached processing")
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {}
}
fn relay(counter: &Arc<AtomicUsize>) -> Relay {
    Relay {
        config: None,
        deactivate: Arc::clone(counter),
        fault: false,
    }
}
fn options(capacity: usize, target: usize, maximum: usize) -> ClockedDuplexOptions {
    ClockedDuplexOptions {
        maximum_frames: maximum,
        taps: 32,
        clock: FifoClockConfig {
            capacity_frames: capacity,
            target_frames: target,
            ..FifoClockConfig::default()
        },
    }
}
#[test]
fn complete_fifo_conversion_tracks_signed_clocks_and_preserves_stereo() {
    for drift in [-500., 500.] {
        let (mut producer, consumer) = audio_ring(2, 1024).unwrap();
        let counter = Arc::new(AtomicUsize::new(0));
        let mut renderer = PreparedClockedDuplex::new(
            relay(&counter),
            consumer,
            44100,
            48000,
            2,
            options(1024, 512, 64),
        )
        .unwrap();
        producer.push(&[0.25, -0.5].repeat(512)).unwrap();
        let mut input_clock = 0.;
        let mut output = vec![99.; 384];
        for callback in 0..6000 {
            let frames = if callback % 3 == 0 { 64 } else { 128 };
            input_clock += frames as f64 * 44100. / 48000. * (1. + drift * 1e-6);
            let captured = input_clock.floor() as usize;
            input_clock -= captured as f64;
            assert_eq!(
                producer.push(&[0.25, -0.5].repeat(captured)).unwrap(),
                captured
            );
            renderer.render(&mut output[..frames * 2]);
            if callback > 4 {
                for pair in output[..frames * 2].as_chunks::<2>().0 {
                    assert!((pair[0] - 0.25).abs() < 1e-6 && (pair[1] + 0.5).abs() < 1e-6);
                }
            }
        }
        let clock = renderer.clock_statistics();
        assert_eq!(clock.underflow_output_frames, 0);
        assert_eq!(clock.priming_output_frames, 0);
        assert!(
            (clock.correction_ppm - drift).abs() < 80.,
            "{drift}: {clock:?}"
        );
        assert!(clock.queued_input_frames > 100 && clock.queued_input_frames < 1000);
        let stats = renderer.statistics();
        assert_eq!(stats.callbacks, 6000);
        assert_eq!(stats.frames, 640000);
        assert_eq!(stats.processor_faults, 0);
        assert_eq!(stats.input_underflow_frames, 0);
        drop(renderer);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
#[test]
fn priming_underflow_reprime_partial_rejection_and_callback_counts() {
    let (mut producer, consumer) = audio_ring(2, 128).unwrap();
    let counter = Arc::new(AtomicUsize::new(0));
    let mut renderer = PreparedClockedDuplex::new(
        relay(&counter),
        consumer,
        48000,
        48000,
        2,
        options(128, 64, 16),
    )
    .unwrap();
    let mut output = [99.; 64];
    renderer.render(&mut output);
    assert!(output.iter().all(|x| *x == 0.));
    assert_eq!(renderer.clock_statistics().priming_output_frames, 32);
    assert_eq!(renderer.statistics().callbacks, 1);
    producer.push(&[0.25, -0.5].repeat(64)).unwrap();
    let mut partial = [99.; 3];
    renderer.render(&mut partial);
    assert_eq!(partial, [0.; 3]);
    assert_eq!(renderer.statistics().processor_faults, 1);
    renderer.render(&mut output);
    assert!(output[62] > 0.24 && output[63] < -0.49);
    let mut long = [99.; 256];
    renderer.render(&mut long);
    assert!(renderer.clock_statistics().underflow_output_frames > 0);
    assert_eq!(
        renderer.clock_statistics().underflow_output_frames,
        renderer.statistics().input_underflow_frames
    );
    assert_eq!(renderer.statistics().callbacks, 4);
    assert_eq!(renderer.statistics().frames, 192);
    producer.push(&[0.125, -0.25].repeat(128)).unwrap();
    renderer.render(&mut output);
    assert!(output[62] > 0.124 && output[63] < -0.249);
    drop(renderer);
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}
#[test]
fn prepared_rendering_is_heap_free_and_invalid_preparation_never_prepares_processor() {
    let counter = Arc::new(AtomicUsize::new(0));
    for (input, output, opts) in [
        (0, 48000, options(128, 64, 16)),
        (48000, 200000, options(128, 64, 16)),
        (8000, 192000, options(128, 64, 16)),
        (48000, 48000, options(256, 64, 16)),
        (48000, 48000, options(128, 0, 16)),
        (48000, 48000, options(128, 64, 0)),
    ] {
        let (_, consumer) = audio_ring(2, 128).unwrap();
        assert!(
            PreparedClockedDuplex::new(NeverPrepare, consumer, input, output, 2, opts).is_err()
        );
    }
    assert_eq!(counter.load(Ordering::SeqCst), 0);
    let (mut producer, consumer) = audio_ring(2, 128).unwrap();
    let mut renderer = PreparedClockedDuplex::new(
        relay(&counter),
        consumer,
        48000,
        48000,
        2,
        options(128, 64, 16),
    )
    .unwrap();
    let mut output = [0.; 32];
    let input = [0.25; 32];
    producer.push(&[0.25; 128]).unwrap();
    let counts = allocation_counter::measure(|| {
        for _ in 0..1000 {
            renderer.render(&mut output);
            producer.push(&input).unwrap();
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.bytes_total, 0);
    assert_eq!(renderer.statistics().panics, 0);
}

#[test]
fn recovery_constructor_rejects_invalid_configs_without_factory_or_native_work() {
    use ravet_device::{DuplexRecoveryStatus, RecoveringClockedDuplex, cpal};
    let input = cpal::StreamConfig {
        channels: 2,
        sample_rate: 96000,
        buffer_size: cpal::BufferSize::Default,
    };
    let output = cpal::StreamConfig {
        sample_rate: 48000,
        ..input
    };
    let factory = || -> NeverPrepare { panic!("constructor/disabled service called factory") };
    for (i, o) in [("", "output"), ("input", "a\0b")] {
        assert!(
            RecoveringClockedDuplex::new(i, o, input, output, options(128, 64, 16), factory)
                .is_err()
        );
    }
    for opts in [
        options(1, 0, 16),
        options(127, 64, 16),
        options(128, 0, 16),
        options(128, 128, 16),
        options(128, 64, 0),
        ClockedDuplexOptions {
            taps: 17,
            ..options(128, 64, 16)
        },
        ClockedDuplexOptions {
            clock: FifoClockConfig {
                maximum_ppm: f64::NAN,
                ..options(128, 64, 16).clock
            },
            ..options(128, 64, 16)
        },
    ] {
        assert!(
            RecoveringClockedDuplex::new("input", "output", input, output, opts, factory).is_err()
        );
    }
    for bad in [
        cpal::StreamConfig {
            channels: 0,
            ..input
        },
        cpal::StreamConfig {
            channels: 65,
            ..input
        },
        cpal::StreamConfig {
            sample_rate: 0,
            ..input
        },
        cpal::StreamConfig {
            sample_rate: 192001,
            ..input
        },
        cpal::StreamConfig {
            sample_rate: 8000,
            ..input
        },
        cpal::StreamConfig {
            buffer_size: cpal::BufferSize::Fixed(0),
            ..input
        },
    ] {
        let high_output = cpal::StreamConfig {
            sample_rate: 192000,
            ..output
        };
        assert!(
            RecoveringClockedDuplex::new(
                "input",
                "output",
                bad,
                high_output,
                options(128, 64, 16),
                factory
            )
            .is_err()
        );
        assert!(
            RecoveringClockedDuplex::new(
                "input",
                "output",
                high_output,
                bad,
                options(128, 64, 16),
                factory
            )
            .is_err()
        );
    }
    let mut owner = RecoveringClockedDuplex::new(
        "input",
        "output",
        input,
        output,
        options(128, 64, 16),
        factory,
    )
    .unwrap();
    assert_eq!(owner.device_ids(), ("input", "output"));
    assert!(owner.statistics().is_none());
    owner.set_enabled(false);
    assert_eq!(owner.service().unwrap(), DuplexRecoveryStatus::Disabled);
    owner.request_reopen();
    assert_eq!(owner.service().unwrap(), DuplexRecoveryStatus::Disabled);
}
