use ravet_audio_file::{WaveEncoding, WaveFormat, WaveReader, WaveRecorder};
use ravet_device::{PreparedCapture, PreparedDuplex};
use ravet_foundation::{
    PrepareError, ProcessBlock, ProcessConfig, ProcessError, ProcessStatus, Processor,
};

#[test]
fn prepared_input_capture_records_exact_synthetic_frames_on_worker() {
    let (mut capture, consumer) = PreparedCapture::new(2, 8).unwrap();
    let format = WaveFormat {
        channels: 2,
        sample_rate: 48000,
        encoding: WaveEncoding::Float32,
    };
    let mut recorder =
        WaveRecorder::new(std::io::Cursor::new(Vec::new()), format, 2, consumer).unwrap();
    for _ in 0..100 {
        capture.capture(&[0.25, -0.5, 0.125, -0.25]);
        assert_eq!(recorder.pump().unwrap(), 2);
    }
    capture.capture(&[0.75, -1.]);
    assert_eq!(capture.statistics().dropped_frames, 0);
    drop(capture);
    let mut reader = WaveReader::new(recorder.finish().unwrap(), 256).unwrap();
    let mut samples = [99.; 402];
    assert_eq!(reader.read_frames(&mut samples).unwrap(), 201);
    for frame in samples[..400].as_chunks::<4>().0 {
        assert_eq!(*frame, [0.25, -0.5, 0.125, -0.25]);
    }
    assert_eq!(&samples[400..], &[0.75, -1.]);
}
struct PassThrough {
    config: Option<ProcessConfig>,
    fault: bool,
}
impl Processor for PassThrough {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = Some(config);
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        self.config.unwrap().validate_block(block).unwrap();
        if self.fault {
            return ProcessStatus::Fault(ProcessError::NotPrepared);
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
        self.config = None;
    }
}
fn processor(fault: bool) -> PassThrough {
    PassThrough {
        config: None,
        fault,
    }
}
#[test]
fn input_capture_overflow_partial_rejection_and_queue_order() {
    let (mut capture, mut consumer) = PreparedCapture::new(2, 4).unwrap();
    capture.capture(&[1., -1., 2., -2., 3., -3., 4., -4., 5., -5.]);
    capture.capture(&[99., 99., 99.]); // malformed buffer publishes nothing
    let mut output = [99.; 10];
    assert_eq!(consumer.pop(&mut output).unwrap(), 4);
    assert_eq!(output, [1., -1., 2., -2., 3., -3., 4., -4., 99., 99.]);
    let stats = capture.statistics();
    assert_eq!(stats.callbacks, 2);
    assert_eq!(stats.captured_frames, 4);
    assert_eq!(stats.dropped_frames, 2);
    assert_eq!(stats.invalid_buffers, 1);
}
#[test]
fn duplex_partitions_variable_callbacks_and_silences_input_underflow() {
    let (mut capture, consumer) = PreparedCapture::new(2, 8).unwrap();
    let mut renderer = PreparedDuplex::new(processor(false), 48000, consumer, 2, 3).unwrap();
    capture.capture(&[1., -1., 2., -2., 3., -3., 4., -4.]);
    let mut invalid = [99.; 3];
    renderer.render(&mut invalid);
    assert_eq!(invalid, [0.; 3]);
    let mut output = [99.; 12];
    renderer.render(&mut output);
    assert_eq!(output, [1., -1., 2., -2., 3., -3., 4., -4., 0., 0., 0., 0.]);
    let stats = renderer.statistics();
    assert_eq!(stats.frames, 6);
    assert_eq!(stats.input_underflow_frames, 2);
    assert_eq!(stats.processor_faults, 1);
}
#[test]
fn duplex_fault_silence_and_prepared_heap_instrumentation() {
    let (mut capture, consumer) = PreparedCapture::new(2, 128).unwrap();
    let mut renderer = PreparedDuplex::new(processor(false), 48000, consumer, 2, 64).unwrap();
    let input = [0.5; 256];
    let mut output = [99.; 512];
    let counts = allocation_counter::measure(|| {
        for _ in 0..1000 {
            capture.capture(&input);
            renderer.render(&mut output);
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    assert_eq!(renderer.statistics().input_underflow_frames, 128000);
    let (mut capture, consumer) = PreparedCapture::new(1, 4).unwrap();
    capture.capture(&[1.; 4]);
    let mut renderer = PreparedDuplex::new(processor(true), 48000, consumer, 1, 3).unwrap();
    let mut output = [99.; 4];
    renderer.render(&mut output);
    assert_eq!(output, [0.; 4]);
    assert_eq!(renderer.statistics().processor_faults, 2);
}
#[test]
fn invalid_capture_and_duplex_limits_reject() {
    assert!(PreparedCapture::new(0, 4).is_err());
    assert!(PreparedCapture::new(65, 4).is_err());
    assert!(PreparedCapture::new(2, 3).is_err());
    for (rate, channels, maximum) in [
        (0, 2, 64),
        (48000, 0, 64),
        (48000, 65, 64),
        (48000, 2, 0),
        (48000, 2, 65537),
    ] {
        let (_, consumer) = PreparedCapture::new(2, 4).unwrap();
        assert!(PreparedDuplex::new(processor(false), rate, consumer, channels, maximum).is_err());
    }
}
