#![cfg(feature = "resampled-playback")]
use ravet_audio_file::{
    AudioFileError, BufferedPlayback, ConvertedReadAheadOptions, FilePlaybackOptions,
    IndexedConvertedReadAhead, PlaybackConversion, ReadAheadStatus, ReadAheadWorker, WaveEncoding,
    WaveFormat, WaveReader, WaveWriter, audio_ring, prepare_file_playback,
};
use ravet_device::{BufferedFileDuplexProcessor, ClockedDuplexOptions, PreparedClockedDuplex};
use ravet_foundation::{
    AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessError,
    ProcessStatus, Processor, Transport,
};
use ravet_signal::FifoClockConfig;
use std::{
    io::Cursor,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
type Reader = WaveReader<Cursor<Vec<u8>>>;
type Factory = Box<dyn FnMut() -> Result<Reader, AudioFileError> + Send>;
type Worker = ReadAheadWorker<IndexedConvertedReadAhead<Reader, Factory>>;
fn file() -> (Worker, BufferedPlayback, Vec<f32>) {
    let mut writer = WaveWriter::new(
        Cursor::new(Vec::new()),
        WaveFormat {
            channels: 1,
            sample_rate: 44100,
            encoding: WaveEncoding::Float32,
        },
        17,
    )
    .unwrap();
    writer
        .write_frames(&(0..17).map(|n| n as f32 * 0.02).collect::<Vec<_>>())
        .unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    let conversion = PlaybackConversion::IntegerSinc { taps: 16 };
    let mut reference = prepare_file_playback(
        WaveReader::new(Cursor::new(bytes.clone()), 17).unwrap(),
        FilePlaybackOptions {
            output_rate: 48000,
            conversion,
            maximum_source_frames: 17,
            maximum_output_frames: 128,
            source_batch_frames: 17,
            maximum_render_frames: 128,
        },
    )
    .unwrap();
    let mut expected = vec![0.; reference.info().prepared_frames];
    reference.playback_mut().play();
    reference.playback_mut().render(&mut expected).unwrap();
    let reader = WaveReader::new(Cursor::new(bytes.clone()), 17).unwrap();
    let factory: Factory = Box::new(move || WaveReader::new(Cursor::new(bytes.clone()), 17));
    let (producer, consumer) = audio_ring(1, 128).unwrap();
    let pump = IndexedConvertedReadAhead::new(
        reader,
        factory,
        producer,
        &consumer,
        ConvertedReadAheadOptions {
            output_rate: 48000,
            conversion,
            maximum_source_frames: 17,
            maximum_output_frames: 128,
            source_batch_frames: 7,
            conversion_batch_frames: 7,
        },
    )
    .unwrap();
    let (mut worker, mut playback) = ReadAheadWorker::new(pump, consumer, 128).unwrap();
    worker.start().unwrap();
    ready(&playback);
    playback.play();
    (worker, playback, expected)
}
fn ready(p: &BufferedPlayback) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while p.worker_status() != ReadAheadStatus::EndOfFile {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct Call {
    frames: usize,
    transport: Transport,
}
#[derive(Default)]
struct Observe {
    prepare: AtomicUsize,
    deactivate: AtomicUsize,
    reset: AtomicUsize,
    calls: AtomicUsize,
    started: AtomicUsize,
    stopped: AtomicUsize,
}
struct Mix {
    config: Option<ProcessConfig>,
    observe: Arc<Observe>,
    trace: Vec<Call>,
    fail_call: usize,
}
impl Mix {
    fn new(fail_call: usize) -> Self {
        Self {
            config: None,
            observe: Arc::default(),
            trace: Vec::with_capacity(20000),
            fail_call,
        }
    }
}
impl Processor for Mix {
    fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
        assert_eq!(c.layout(), AudioLayout::new(2, 1).unwrap());
        self.observe.prepare.fetch_add(1, Ordering::Relaxed);
        self.config = Some(c);
        Ok(())
    }
    fn start_audio(&mut self) -> Result<(), ProcessError> {
        self.observe.started.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    fn stop_audio(&mut self) {
        self.observe.stopped.fetch_add(1, Ordering::Relaxed);
    }
    fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        self.config.unwrap().validate_block(b).unwrap();
        assert!(self.trace.len() < self.trace.capacity());
        self.trace.push(Call {
            frames: b.audio.frames(),
            transport: b.transport,
        });
        let call = self.observe.calls.fetch_add(1, Ordering::Relaxed) + 1;
        if call == self.fail_call {
            return ProcessStatus::Fault(ProcessError::InvalidTransport);
        }
        for n in 0..b.audio.frames() {
            let v = b.audio.input(0).unwrap()[n] + b.audio.input(1).unwrap()[n];
            b.audio.output_mut(0).unwrap()[n] = v;
        }
        ProcessStatus::Continue
    }
    fn reset(&mut self) {
        self.observe.reset.fetch_add(1, Ordering::Relaxed);
    }
    fn deactivate(&mut self) {
        self.observe.deactivate.fetch_add(1, Ordering::Relaxed);
        self.config = None;
    }
}
fn config() -> ProcessConfig {
    ProcessConfig::new(48000., 128, 0, AudioLayout::new(1, 1).unwrap()).unwrap()
}
fn render(
    p: &mut BufferedFileDuplexProcessor<Mix>,
    capture: &[f32],
    output: &mut [f32],
    position: i64,
) -> ProcessStatus {
    let inputs = [capture];
    let mut outputs = [output];
    let audio = AudioBuffer::separate(&inputs, &mut outputs, capture.len()).unwrap();
    p.process(&mut ProcessBlock::new(
        audio,
        &[],
        Transport {
            sample_position: Some(position),
            ..Transport::default()
        },
    ))
}
#[test]
fn eof_split_keeps_capture_active_file_device_coordinates_separate_and_extracts_once() {
    let (worker, playback, expected) = file();
    let mut p = BufferedFileDuplexProcessor::new(playback, Mix::new(usize::MAX));
    let monitor = p.monitor();
    p.prepare(config()).unwrap();
    let total = expected.len();
    let capture = vec![0.25; total + 5];
    let mut output = vec![9.; total + 5];
    let allocations = allocation_counter::measure(|| {
        assert_eq!(
            render(&mut p, &capture, &mut output, 1000),
            ProcessStatus::Continue
        )
    });
    assert_eq!(allocations.count_total, 0);
    for n in 0..total {
        assert_eq!(output[n], expected[n] + 0.25);
    }
    assert_eq!(output[total..], [0.25; 5]);
    assert_eq!(p.statistics(), monitor.statistics());
    assert_eq!(p.statistics().file_position, total as u64);
    assert_eq!(p.statistics().device_render_frames, (total + 5) as u64);
    assert_eq!(p.statistics().last_device_sample_position, Some(1000));
    assert!(p.statistics().ended);
    let (playback, child) = p.into_parts();
    assert_eq!(child.observe.deactivate.load(Ordering::Relaxed), 1);
    assert_eq!(child.trace.len(), 2);
    assert_eq!(
        child.trace[0],
        Call {
            frames: total,
            transport: Transport {
                playing: Some(true),
                sample_position: Some(0),
                discontinuity: true,
                ..Transport::default()
            }
        }
    );
    assert_eq!(
        child.trace[1],
        Call {
            frames: 5,
            transport: Transport {
                playing: Some(false),
                sample_position: Some(total as i64),
                ..Transport::default()
            }
        }
    );
    assert_eq!(playback.position_frames(), total as u64);
    drop(playback);
    drop(child);
    assert!(worker.shutdown().unwrap().error.is_none());
}
#[test]
fn second_segment_fault_commits_file_once_silences_all_and_poison_stalls_until_reset() {
    let (worker, playback, expected) = file();
    let mut p = BufferedFileDuplexProcessor::new(playback, Mix::new(2));
    p.prepare(config()).unwrap();
    let total = expected.len();
    let capture = vec![0.25; total + 5];
    let mut output = vec![9.; total + 5];
    assert_eq!(
        render(&mut p, &capture, &mut output, 100),
        ProcessStatus::Fault(ProcessError::BackendFailure)
    );
    assert!(output.iter().all(|v| *v == 0.));
    let first = p.statistics();
    assert_eq!(first.child_calls, 2);
    assert_eq!(first.child_faults, 1);
    assert_eq!(first.consumed_file_frames, total as u64);
    assert_eq!(p.first_error(), Some(ProcessError::InvalidTransport));
    output.fill(9.);
    assert_eq!(
        render(&mut p, &capture, &mut output, 200),
        ProcessStatus::Fault(ProcessError::BackendFailure)
    );
    assert!(output.iter().all(|v| *v == 0.));
    assert_eq!(p.statistics().child_calls, 2);
    assert_eq!(p.statistics().consumed_file_frames, total as u64);
    assert_eq!(p.statistics().device_render_frames, 2 * (total + 5) as u64);
    p.reset();
    assert!(!p.playback().is_playing());
    assert_eq!(p.first_error(), None);
    assert_eq!(
        render(&mut p, &[0.25; 3], &mut [9.; 3], 300),
        ProcessStatus::Continue
    );
    assert_eq!(p.statistics().file_position, total as u64);
    let (playback, child) = p.into_parts();
    assert_eq!(child.trace.len(), 3);
    assert!(child.trace[2].transport.discontinuity);
    assert_eq!(child.trace[2].transport.playing, Some(false));
    assert_eq!(child.observe.reset.load(Ordering::Relaxed), 1);
    assert_eq!(child.observe.deactivate.load(Ordering::Relaxed), 1);
    drop(playback);
    drop(child);
    worker.shutdown().unwrap();
}
#[test]
fn preparation_rejection_and_zero_blocks_preserve_source_then_reprepare_deactivates_once() {
    let (worker, playback, _) = file();
    let queued = playback.queued_frames();
    let mut p = BufferedFileDuplexProcessor::new(playback, Mix::new(usize::MAX));
    let wrong = ProcessConfig::new(44100., 128, 0, AudioLayout::new(1, 1).unwrap()).unwrap();
    assert_eq!(p.prepare(wrong), Err(PrepareError::UnsupportedLayout));
    assert_eq!(p.playback().queued_frames(), queued);
    p.prepare(config()).unwrap();
    let before = p.statistics();
    assert_eq!(render(&mut p, &[], &mut [], 77), ProcessStatus::Continue);
    assert_eq!(p.statistics(), before);
    assert_eq!(
        render(&mut p, &[0.25; 3], &mut [9.; 3], 0),
        ProcessStatus::Continue
    );
    p.prepare(config()).unwrap();
    let (playback, child) = p.into_parts();
    assert_eq!(child.observe.prepare.load(Ordering::Relaxed), 2);
    assert_eq!(child.observe.deactivate.load(Ordering::Relaxed), 2);
    assert_eq!(child.trace.len(), 1);
    assert!(child.trace[0].transport.discontinuity);
    assert_eq!(playback.position_frames(), 3);
    drop(playback);
    drop(child);
    worker.shutdown().unwrap();
}

struct CaptureRelay;
impl Processor for CaptureRelay {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        Ok(())
    }
    fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        for n in 0..b.audio.frames() {
            let v = b.audio.input(0).unwrap()[n];
            b.audio.output_mut(0).unwrap()[n] = v;
        }
        ProcessStatus::Continue
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {}
}
fn clock_options() -> ClockedDuplexOptions {
    ClockedDuplexOptions {
        maximum_frames: 64,
        taps: 32,
        clock: FifoClockConfig {
            capacity_frames: 1024,
            target_frames: 512,
            ..FifoClockConfig::default()
        },
    }
}
#[test]
fn canonical_clocked_duplex_keeps_file_and_capture_distinct_under_both_signed_clock_drifts() {
    for ppm in [-500_i64, 500] {
        let (worker, playback, expected) = file();
        let processor = BufferedFileDuplexProcessor::new(playback, Mix::new(usize::MAX));
        let monitor = processor.monitor();
        let (mut raw, consumer) = audio_ring(1, 1024).unwrap();
        let (mut reference_raw, reference_consumer) = audio_ring(1, 1024).unwrap();
        let mut actual =
            PreparedClockedDuplex::new(processor, consumer, 44100, 48000, 1, clock_options())
                .unwrap();
        let mut reference = PreparedClockedDuplex::new(
            CaptureRelay,
            reference_consumer,
            44100,
            48000,
            1,
            clock_options(),
        )
        .unwrap();
        assert_eq!(raw.push(&[0.25; 512]).unwrap(), 512);
        assert_eq!(reference_raw.push(&[0.25; 512]).unwrap(), 512);
        let mut phase = 0_u128;
        let divisor = 48000_u128 * 1_000_000;
        let mut actual_output = [0.; 128];
        let mut capture_only = [0.; 128];
        let mut device_frames = 0_u64;
        for callback in 0..6000 {
            let frames = if callback % 3 == 0 { 64 } else { 128 };
            phase += frames as u128 * 44100 * (1_000_000 + ppm) as u128;
            let captured = (phase / divisor) as usize;
            phase %= divisor;
            assert_eq!(raw.push(&[0.25; 256][..captured]).unwrap(), captured);
            assert_eq!(
                reference_raw.push(&[0.25; 256][..captured]).unwrap(),
                captured
            );
            reference.render(&mut capture_only[..frames]);
            assert_eq!(
                allocation_counter::measure(|| actual.render(&mut actual_output[..frames]))
                    .count_total,
                0
            );
            for n in 0..frames {
                let file = expected
                    .get(device_frames as usize + n)
                    .copied()
                    .unwrap_or(0.);
                assert_eq!(actual_output[n], file + capture_only[n]);
                if callback > 4 {
                    assert!((capture_only[n] - 0.25).abs() < 1e-6);
                }
            }
            device_frames += frames as u64;
        }
        let file = monitor.statistics();
        assert_eq!(file.file_position, expected.len() as u64);
        assert_eq!(file.consumed_file_frames, expected.len() as u64);
        assert_eq!(file.file_underflow_frames, 0);
        assert_eq!(file.device_render_frames, device_frames);
        assert!(file.ended);
        assert_eq!(file.child_faults, 0);
        let clock = actual.clock_statistics();
        assert_eq!(clock, reference.clock_statistics());
        assert_eq!(clock.underflow_output_frames, 0);
        assert_eq!(clock.priming_output_frames, 0);
        assert!(
            (clock.correction_ppm - ppm as f64).abs() < 80.,
            "{ppm}: {clock:?}"
        );
        assert!(clock.queued_input_frames > 100 && clock.queued_input_frames < 1000);
        assert_eq!(actual.statistics().processor_faults, 0);
        drop(actual);
        drop(reference);
        worker.shutdown().unwrap();
    }
}
#[test]
fn stopped_indexed_seek_and_reprepare_feed_exact_new_suffix_to_canonical_clocked_duplex() {
    let (mut worker, playback, expected) = file();
    let mut processor = BufferedFileDuplexProcessor::new(playback, Mix::new(usize::MAX));
    processor.prepare(config()).unwrap();
    let mut prefix = [0.; 7];
    assert_eq!(
        render(&mut processor, &[0.25; 7], &mut prefix, 0),
        ProcessStatus::Continue
    );
    for n in 0..7 {
        assert_eq!(prefix[n], expected[n] + 0.25);
    }
    let (mut playback, child) = processor.into_parts();
    let queued = playback.queued_frames();
    assert!(worker.seek_stopped(&mut playback, 129).is_err());
    assert_eq!(playback.position_frames(), 7);
    assert_eq!(playback.queued_frames(), queued);
    worker.seek_stopped(&mut playback, 17).unwrap();
    ready(&playback);
    assert_eq!(playback.position_frames(), 17);
    let observed = Arc::clone(&child.observe);
    let processor = BufferedFileDuplexProcessor::new(playback, child);
    let monitor = processor.monitor();
    let (mut raw, consumer) = audio_ring(1, 1024).unwrap();
    let (mut reference_raw, reference_consumer) = audio_ring(1, 1024).unwrap();
    let mut actual =
        PreparedClockedDuplex::new(processor, consumer, 44100, 48000, 1, clock_options()).unwrap();
    let mut reference = PreparedClockedDuplex::new(
        CaptureRelay,
        reference_consumer,
        44100,
        48000,
        1,
        clock_options(),
    )
    .unwrap();
    raw.push(&[0.25; 512]).unwrap();
    reference_raw.push(&[0.25; 512]).unwrap();
    let mut capture = [0.; 64];
    let mut resumed = [0.; 64];
    reference.render(&mut capture);
    assert_eq!(
        allocation_counter::measure(|| actual.render(&mut resumed)).count_total,
        0
    );
    for n in 0..64 {
        assert_eq!(
            resumed[n],
            expected.get(17 + n).copied().unwrap_or(0.) + capture[n]
        );
    }
    assert_eq!(monitor.statistics().file_position, expected.len() as u64);
    assert_eq!(
        monitor.statistics().consumed_file_frames,
        (expected.len() - 17) as u64
    );
    assert!(monitor.statistics().ended);
    assert_eq!(monitor.statistics().device_render_frames, 64);
    drop(actual);
    assert_eq!(observed.prepare.load(Ordering::Relaxed), 2);
    assert_eq!(observed.deactivate.load(Ordering::Relaxed), 2);
    drop(reference);
    let exit = worker.shutdown().unwrap();
    assert!(exit.error.is_none());
    assert_eq!(exit.pump.generation(), 1);
    assert_eq!(exit.pump.output_origin(), 17);
}

struct PrefixOnly {
    inner: ravet_audio_file::WaveReadAhead<Cursor<Vec<u8>>>,
    submitted: bool,
}
impl ravet_audio_file::ReadAheadPump for PrefixOnly {
    fn pump(&mut self) -> Result<ravet_audio_file::ReadAheadProgress, AudioFileError> {
        if self.submitted {
            return Ok(ravet_audio_file::ReadAheadProgress::default());
        }
        self.submitted = true;
        self.inner.pump()
    }
    fn owns(&self, c: &ravet_audio_file::AudioConsumer) -> bool {
        ravet_audio_file::ReadAheadPump::owns(&self.inner, c)
    }
    fn pending_frames(&self) -> usize {
        ravet_audio_file::ReadAheadPump::pending_frames(&self.inner)
    }
    fn reader_position(&self) -> u64 {
        self.inner.reader_position()
    }
    fn sample_rate(&self) -> u32 {
        48000
    }
    fn seek_stopped(
        &mut self,
        _: &mut ravet_audio_file::AudioConsumer,
        _: u64,
    ) -> Result<(), AudioFileError> {
        Err(AudioFileError::Unsupported)
    }
}
#[test]
fn actual_worker_underflow_splits_only_available_file_prefix_without_stopping_capture() {
    let mut writer = WaveWriter::new(
        Cursor::new(Vec::new()),
        WaveFormat {
            channels: 1,
            sample_rate: 48000,
            encoding: WaveEncoding::Float32,
        },
        64,
    )
    .unwrap();
    writer.write_frames(&[0.1; 64]).unwrap();
    let reader = WaveReader::new(writer.finish().unwrap(), 4).unwrap();
    let (producer, consumer) = audio_ring(1, 4).unwrap();
    let inner = ravet_audio_file::WaveReadAhead::new(reader, producer, 4).unwrap();
    let (mut worker, mut playback) = ReadAheadWorker::new(
        PrefixOnly {
            inner,
            submitted: false,
        },
        consumer,
        128,
    )
    .unwrap();
    worker.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while playback.queued_frames() != 4 {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    playback.play();
    let mut p = BufferedFileDuplexProcessor::new(playback, Mix::new(usize::MAX));
    p.prepare(config()).unwrap();
    let mut output = [9.; 10];
    assert_eq!(
        allocation_counter::measure(|| assert_eq!(
            render(&mut p, &[0.25; 10], &mut output, 700),
            ProcessStatus::Continue
        ))
        .count_total,
        0
    );
    assert_eq!(output[..4], [0.35; 4]);
    assert_eq!(output[4..], [0.25; 6]);
    let stats = p.statistics();
    assert_eq!(stats.file_position, 4);
    assert_eq!(stats.file_underflow_frames, 6);
    assert_eq!(stats.worker_status, ReadAheadStatus::Running);
    assert!(!stats.ended);
    assert_eq!(stats.last_device_sample_position, Some(700));
    assert_eq!(stats.device_render_frames, 10);
    let (playback, child) = p.into_parts();
    assert_eq!(child.trace.len(), 2);
    assert_eq!(child.trace[0].frames, 4);
    assert_eq!(child.trace[1].frames, 6);
    assert_eq!(child.trace[1].transport.sample_position, Some(4));
    assert_eq!(child.trace[1].transport.playing, Some(false));
    drop(playback);
    drop(child);
    worker.shutdown().unwrap();
}

#[test]
fn actual_wave_worker_failure_drains_prefix_without_false_eof_or_disabling_capture() {
    let (reference_worker, reference_playback, expected) = file();
    drop(reference_playback);
    reference_worker.shutdown().unwrap();
    let mut writer = WaveWriter::new(
        Cursor::new(Vec::new()),
        WaveFormat {
            channels: 1,
            sample_rate: 44100,
            encoding: WaveEncoding::Float32,
        },
        17,
    )
    .unwrap();
    let samples: Vec<_> = (0..17)
        .map(|n| if n == 10 { f32::NAN } else { n as f32 * 0.02 })
        .collect();
    // Writer correctly rejects NaN: encode a valid public file, then replace this
    // fixture's original f32 sample bytes to exercise reader integrity failure.
    let valid: Vec<_> = (0..17).map(|n| n as f32 * 0.02).collect();
    writer.write_frames(&valid).unwrap();
    let mut bytes = writer.finish().unwrap().into_inner();
    let data = bytes.windows(4).position(|v| v == b"data").unwrap() + 8;
    bytes[data + 10 * 4..data + 11 * 4].copy_from_slice(&samples[10].to_le_bytes());
    let reader = WaveReader::new(Cursor::new(bytes.clone()), 17).unwrap();
    let factory: Factory = Box::new(move || WaveReader::new(Cursor::new(bytes.clone()), 17));
    let (producer, consumer) = audio_ring(1, 128).unwrap();
    let pump = IndexedConvertedReadAhead::new(
        reader,
        factory,
        producer,
        &consumer,
        ConvertedReadAheadOptions {
            output_rate: 48000,
            conversion: PlaybackConversion::IntegerSinc { taps: 16 },
            maximum_source_frames: 17,
            maximum_output_frames: 128,
            source_batch_frames: 7,
            conversion_batch_frames: 7,
        },
    )
    .unwrap();
    let (mut worker, mut playback) = ReadAheadWorker::new(pump, consumer, 128).unwrap();
    worker.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while playback.worker_status() != ReadAheadStatus::Failed {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    let queued = playback.queued_frames();
    assert!(queued > 0 && queued < expected.len());
    playback.play();
    let mut p = BufferedFileDuplexProcessor::new(playback, Mix::new(usize::MAX));
    p.prepare(config()).unwrap();
    let mut output = [9.; 128];
    assert_eq!(
        render(&mut p, &[0.25; 128], &mut output, 900),
        ProcessStatus::Continue
    );
    for n in 0..queued {
        assert_eq!(output[n], expected[n] + 0.25);
    }
    assert!(output[queued..].iter().all(|v| *v == 0.25));
    assert_eq!(p.statistics().worker_status, ReadAheadStatus::Failed);
    assert!(!p.statistics().ended);
    assert_eq!(p.statistics().file_position, queued as u64);
    assert_eq!(p.first_error(), None);
    assert_eq!(p.statistics().file_underflow_frames, 0);
    assert_eq!(
        render(&mut p, &[0.25; 3], &mut [9.; 3], 1028),
        ProcessStatus::Continue
    );
    assert_eq!(p.statistics().file_position, queued as u64);
    drop(p);
    assert!(matches!(
        worker.shutdown().unwrap().error,
        Some(AudioFileError::InvalidSamples)
    ));
}
struct RejectPrepare {
    observe: Arc<Observe>,
}
impl Processor for RejectPrepare {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        self.observe.prepare.fetch_add(1, Ordering::Relaxed);
        Err(PrepareError::InvalidSampleRate)
    }
    fn process(&mut self, _: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        panic!("inactive rejected child executed")
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {
        self.observe.deactivate.fetch_add(1, Ordering::Relaxed);
    }
}
#[test]
fn failed_child_preparation_retains_both_original_owners_inactive_without_deactivation_or_reads() {
    let (worker, playback, _) = file();
    let queued = playback.queued_frames();
    let observe = Arc::new(Observe::default());
    let mut p = BufferedFileDuplexProcessor::new(
        playback,
        RejectPrepare {
            observe: Arc::clone(&observe),
        },
    );
    assert_eq!(p.prepare(config()), Err(PrepareError::InvalidSampleRate));
    assert_eq!(p.playback().queued_frames(), queued);
    assert_eq!(p.playback().position_frames(), 0);
    let mut output = [9.; 3];
    let inputs = [&[0.25; 3][..]];
    let mut outputs = [&mut output[..]];
    let audio = AudioBuffer::separate(&inputs, &mut outputs, 3).unwrap();
    assert_eq!(
        p.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
        ProcessStatus::Fault(ProcessError::NotPrepared)
    );
    assert_eq!(output, [0.; 3]);
    let (playback, child) = p.into_parts();
    assert_eq!(observe.prepare.load(Ordering::Relaxed), 1);
    assert_eq!(observe.deactivate.load(Ordering::Relaxed), 0);
    assert!(Arc::ptr_eq(&observe, &child.observe));
    drop(playback);
    drop(child);
    worker.shutdown().unwrap();
}

#[test]
fn invalid_complete_block_preserves_queue_child_and_device_context_then_explicit_session_and_backing_tail()
 {
    let (worker, playback, expected) = file();
    let queued = playback.queued_frames();
    let mut p = BufferedFileDuplexProcessor::new(playback, Mix::new(usize::MAX));
    assert_eq!(p.start_audio(), Err(ProcessError::NotPrepared));
    p.prepare(config()).unwrap();
    let before = p.statistics();
    let bad_inputs = [&[0.25; 3][..], &[0.5; 3][..]];
    let mut output = [9.; 5];
    let mut outputs = [&mut output[..]];
    let audio = AudioBuffer::separate(&bad_inputs, &mut outputs, 3).unwrap();
    assert_eq!(
        p.process(&mut ProcessBlock::new(
            audio,
            &[],
            Transport {
                sample_position: Some(900),
                ..Transport::default()
            }
        )),
        ProcessStatus::Fault(ProcessError::LayoutMismatch)
    );
    assert_eq!(output, [0., 0., 0., 9., 9.]);
    assert_eq!(p.statistics(), before);
    assert_eq!(p.playback().queued_frames(), queued);
    assert_eq!(p.playback().position_frames(), 0);
    p.start_audio().unwrap();
    output.fill(9.);
    assert_eq!(
        allocation_counter::measure(|| assert_eq!(
            render(&mut p, &[0.25; 3], &mut output, 700),
            ProcessStatus::Continue
        ))
        .count_total,
        0
    );
    assert_eq!(output[3..], [9., 9.]);
    for n in 0..3 {
        assert_eq!(output[n], expected[n] + 0.25);
    }
    p.stop_audio();
    let (playback, child) = p.into_parts();
    assert_eq!(child.trace.len(), 1);
    assert_eq!(child.observe.started.load(Ordering::Relaxed), 1);
    assert_eq!(child.observe.stopped.load(Ordering::Relaxed), 1);
    assert_eq!(child.observe.deactivate.load(Ordering::Relaxed), 1);
    assert_eq!(child.trace[0].transport.sample_position, Some(0));
    assert_eq!(child.trace[0].transport.playing, Some(true));
    assert!(child.trace[0].transport.discontinuity);
    drop(playback);
    drop(child);
    worker.shutdown().unwrap();
}

struct Stereo {
    mix: bool,
}
impl Processor for Stereo {
    fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
        assert_eq!(
            c.layout(),
            AudioLayout::new(if self.mix { 4 } else { 2 }, 2).unwrap()
        );
        Ok(())
    }
    fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        for c in 0..2 {
            for n in 0..b.audio.frames() {
                let v = b.audio.input(c).unwrap()[n]
                    + if self.mix {
                        b.audio.input(c + 2).unwrap()[n]
                    } else {
                        0.
                    };
                b.audio.output_mut(c).unwrap()[n] = v;
            }
        }
        ProcessStatus::Continue
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {}
}
#[test]
fn canonical_stereo_clock_conversion_preserves_ordered_file_and_capture_channel_groups() {
    let mut writer = WaveWriter::new(
        Cursor::new(Vec::new()),
        WaveFormat {
            channels: 2,
            sample_rate: 48000,
            encoding: WaveEncoding::Float32,
        },
        64,
    )
    .unwrap();
    writer.write_frames(&[0.1, 0.2].repeat(64)).unwrap();
    let reader = WaveReader::new(writer.finish().unwrap(), 4).unwrap();
    let (producer, consumer) = audio_ring(2, 4).unwrap();
    let inner = ravet_audio_file::WaveReadAhead::new(reader, producer, 4).unwrap();
    let (mut worker, mut file) = ReadAheadWorker::new(
        PrefixOnly {
            inner,
            submitted: false,
        },
        consumer,
        128,
    )
    .unwrap();
    worker.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while file.queued_frames() != 4 {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    file.play();
    let p = BufferedFileDuplexProcessor::new(file, Stereo { mix: true });
    let monitor = p.monitor();
    let (mut raw, consumer) = audio_ring(2, 1024).unwrap();
    let (mut raw_reference, reference_consumer) = audio_ring(2, 1024).unwrap();
    raw.push(&[0.25, -0.5].repeat(512)).unwrap();
    raw_reference.push(&[0.25, -0.5].repeat(512)).unwrap();
    let mut actual =
        PreparedClockedDuplex::new(p, consumer, 44100, 48000, 2, clock_options()).unwrap();
    let mut reference = PreparedClockedDuplex::new(
        Stereo { mix: false },
        reference_consumer,
        44100,
        48000,
        2,
        clock_options(),
    )
    .unwrap();
    let mut capture = [0.; 20];
    let mut output = [9.; 20];
    reference.render(&mut capture);
    assert_eq!(
        allocation_counter::measure(|| actual.render(&mut output)).count_total,
        0
    );
    for n in 0..10 {
        for c in 0..2 {
            assert_eq!(
                output[n * 2 + c],
                capture[n * 2 + c] + if n < 4 { [0.1, 0.2][c] } else { 0. }
            );
        }
    }
    assert_eq!(monitor.statistics().consumed_file_frames, 4);
    assert_eq!(monitor.statistics().file_underflow_frames, 6);
    assert_eq!(monitor.statistics().child_calls, 2);
    drop(actual);
    drop(reference);
    worker.shutdown().unwrap();
}
struct NeverPrepare;
impl Processor for NeverPrepare {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        panic!("capacity rejection reached child")
    }
    fn process(&mut self, _: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        panic!("inactive child executed")
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {
        panic!("inactive child deactivated")
    }
}
#[test]
fn total_working_storage_rejects_above_32mib_before_allocating_or_preparing_child() {
    let mut writer = WaveWriter::new(
        Cursor::new(Vec::new()),
        WaveFormat {
            channels: 32,
            sample_rate: 48000,
            encoding: WaveEncoding::Float32,
        },
        1,
    )
    .unwrap();
    writer.write_frames(&[0.; 32]).unwrap();
    let reader = WaveReader::new(writer.finish().unwrap(), 1).unwrap();
    let (producer, consumer) = audio_ring(32, 1).unwrap();
    let pump = ravet_audio_file::WaveReadAhead::new(reader, producer, 1).unwrap();
    let (worker, file) = ReadAheadWorker::new(pump, consumer, 65536).unwrap();
    let mut p = BufferedFileDuplexProcessor::new(file, NeverPrepare);
    let wide = ProcessConfig::new(48000., 65536, 0, AudioLayout::new(32, 64).unwrap()).unwrap();
    assert_eq!(
        allocation_counter::measure(|| assert_eq!(
            p.prepare(wide),
            Err(PrepareError::InvalidMaximumFrames)
        ))
        .count_total,
        0
    );
    assert_eq!(p.playback().position_frames(), 0);
    assert_eq!(p.playback().queued_frames(), 0);
    drop(p);
    worker.shutdown().unwrap();
}

#[test]
fn zero_output_layout_rejects_before_allocation_child_or_file_consumption() {
    let (worker, playback, _) = file();
    let queued = playback.queued_frames();
    let mut p = BufferedFileDuplexProcessor::new(playback, NeverPrepare);
    let before = p.statistics();
    let no_output = ProcessConfig::new(48000., 128, 0, AudioLayout::new(1, 0).unwrap()).unwrap();
    assert_eq!(
        allocation_counter::measure(|| assert_eq!(
            p.prepare(no_output),
            Err(PrepareError::UnsupportedLayout)
        ))
        .count_total,
        0
    );
    assert_eq!(p.playback().queued_frames(), queued);
    assert_eq!(p.playback().position_frames(), 0);
    assert_eq!(p.statistics(), before);
    let (playback, _child) = p.into_parts();
    assert_eq!(playback.queued_frames(), queued);
    assert_eq!(playback.position_frames(), 0);
    drop(playback);
    worker.shutdown().unwrap();
}
