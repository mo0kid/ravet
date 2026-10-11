use super::*;
use ravet_foundation::{ProcessBlock, ProcessConfig, ProcessStatus};
use ravet_signal::FifoClockConfig;
use std::sync::{Mutex, atomic::AtomicUsize};
struct Probe {
    id: usize,
    prepares: Arc<AtomicUsize>,
    deactivates: Arc<AtomicUsize>,
    dropped: Arc<Mutex<Option<String>>>,
    reject: bool,
    panic_process: bool,
    panic_deactivate: bool,
}
impl Probe {
    fn new(id: usize) -> Self {
        Self {
            id,
            prepares: Arc::new(AtomicUsize::new(0)),
            deactivates: Arc::new(AtomicUsize::new(0)),
            dropped: Arc::new(Mutex::new(None)),
            reject: false,
            panic_process: false,
            panic_deactivate: false,
        }
    }
}
impl Processor for Probe {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        self.prepares.fetch_add(1, Ordering::Relaxed);
        if self.reject {
            Err(PrepareError::InvalidSampleRate)
        } else {
            Ok(())
        }
    }
    fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        assert!(!self.panic_process, "injected process panic");
        b.clear_outputs();
        ProcessStatus::Silence
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {
        self.deactivates.fetch_add(1, Ordering::Relaxed);
        assert!(!self.panic_deactivate, "injected deactivate panic");
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        *self.dropped.lock().unwrap() = Some(thread::current().name().unwrap_or("unnamed").into());
    }
}
fn options() -> ClockedDuplexOptions {
    ClockedDuplexOptions {
        maximum_frames: 8,
        taps: 16,
        clock: FifoClockConfig {
            capacity_frames: 32,
            target_frames: 16,
            ..FifoClockConfig::default()
        },
    }
}
fn prepare(probe: Probe) -> StoppedClockedDuplex<Probe> {
    let (capture, raw) = PreparedCapture::new(1, 32).unwrap();
    let renderer =
        PreparedClockedDuplex::new_owned(probe, raw, 48000, 48000, 1, options()).unwrap();
    StoppedClockedDuplex { renderer, capture }
}
fn normal(
    outcome: ReclaimedClockedDuplex<Probe>,
) -> (StoppedClockedDuplex<Probe>, ReclaimableClockedStatistics) {
    match outcome {
        ReclaimedClockedDuplex::Reclaimed { owner, statistics } => (owner, statistics),
        _ => panic!("unexpected fault"),
    }
}
#[test]
fn all_24_orders_preserve_original_processor_matching_endpoints_and_callback_drop_has_no_heap_work()
{
    let mut orders = Vec::new();
    for a in 0..4 {
        for b in 0..4 {
            for c in 0..4 {
                for d in 0..4 {
                    if a != b && a != c && a != d && b != c && b != d && c != d {
                        orders.push([a, b, c, d]);
                    }
                }
            }
        }
    }
    assert_eq!(orders.len(), 24);
    for order in orders {
        let probe = Probe::new(7);
        let deactivates = Arc::clone(&probe.deactivates);
        let ((mut input, input_error, mut output, output_error), _, mut retirement) =
            start_prepared(
                prepare(probe),
                |i, ie, o, oe| Ok((i, ie, o, oe)),
                spawn_worker,
                None,
            )
            .unwrap();
        assert_eq!(
            allocation_counter::measure(|| {
                input.capture(&[0.25; 4]);
                output.render(&mut [9.; 4]);
            })
            .count_total,
            0
        );
        input_error.report();
        output_error.report();
        let mut input = Some(input);
        let mut input_error = Some(input_error);
        let mut output = Some(output);
        let mut output_error = Some(output_error);
        for (n, guard) in order.into_iter().enumerate() {
            let allocation = allocation_counter::measure(|| match guard {
                0 => drop(input.take()),
                1 => drop(input_error.take()),
                2 => drop(output.take()),
                3 => drop(output_error.take()),
                _ => unreachable!(),
            });
            assert_eq!(allocation.count_total, 0);
            assert_eq!(allocation.count_current, 0);
            if n < 3 {
                assert!(retirement.try_reclaim().unwrap().is_none());
            }
        }
        let (owner, stats) = normal(retirement.wait().unwrap());
        assert_eq!(stats.input.captured_frames, 4);
        assert_eq!(stats.input.backend_errors, 1);
        assert_eq!(stats.output.backend_errors, 1);
        assert_eq!(stats.output.frames, 4);
        assert_eq!(deactivates.load(Ordering::Relaxed), 0);
        let (probe, mut raw, mut capture) = owner.into_parts();
        assert_eq!(probe.id, 7);
        assert_eq!(deactivates.load(Ordering::Relaxed), 1);
        let mut pcm = [0.; 5];
        assert_eq!(raw.pop(&mut pcm).unwrap(), 4);
        assert_eq!(pcm[..4], [0.25; 4]);
        capture.capture(&[0.5]);
        assert_eq!(raw.pop(&mut pcm[..1]).unwrap(), 1);
        assert_eq!(pcm[0], 0.5);
        drop(probe);
        assert_eq!(deactivates.load(Ordering::Relaxed), 1);
    }
}
#[test]
fn four_flags_alone_do_not_reclaim_while_final_guard_arc_field_is_live() {
    let ((input, input_error, output, output_error), _, mut retirement) = start_prepared(
        prepare(Probe::new(8)),
        |i, ie, o, oe| Ok((i, ie, o, oe)),
        spawn_worker,
        None,
    )
    .unwrap();
    drop(input);
    drop(input_error);
    drop(output);
    output_error.cell.retired[3].store(true, Ordering::Release);
    thread::sleep(Duration::from_millis(10));
    assert!(retirement.try_reclaim().unwrap().is_none());
    drop(output_error);
    let (owner, _) = normal(retirement.wait().unwrap());
    assert_eq!(owner.into_processor().id, 8);
}
#[test]
fn early_receiver_drop_waits_asynchronous_all_four_capture_owners_and_disposes_on_worker() {
    let probe = Probe::new(9);
    let dropped = Arc::clone(&probe.dropped);
    let ((input, input_error, output, output_error), _, retirement) = start_prepared(
        prepare(probe),
        |i, ie, o, oe| Ok((i, ie, o, oe)),
        spawn_worker,
        None,
    )
    .unwrap();
    drop(retirement);
    drop(input);
    drop(input_error);
    drop(output);
    thread::sleep(Duration::from_millis(10));
    assert!(dropped.lock().unwrap().is_none());
    assert_eq!(
        allocation_counter::measure(|| drop(output_error)).count_total,
        0
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        if dropped.lock().unwrap().is_some() {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        thread::yield_now();
    }
    assert_eq!(
        dropped.lock().unwrap().as_deref(),
        Some("ravet-clocked-retirement")
    );
}
#[test]
fn normal_preparation_errors_retain_exact_processor_and_queued_raw_endpoint_without_deactivation() {
    for reject in [false, true] {
        let mut probe = Probe::new(10);
        probe.reject = reject;
        let prepares = Arc::clone(&probe.prepares);
        let deactivates = Arc::clone(&probe.deactivates);
        let (mut capture, raw) = PreparedCapture::new(1, 32).unwrap();
        capture.capture(&[0.25; 4]);
        let failure = PreparedClockedDuplex::new_owned(
            probe,
            raw,
            if reject { 48000 } else { 0 },
            48000,
            1,
            options(),
        )
        .err()
        .unwrap();
        let (probe, mut raw, error) = failure.into_parts();
        assert_eq!(probe.id, 10);
        assert_eq!(prepares.load(Ordering::Relaxed), usize::from(reject));
        assert_eq!(deactivates.load(Ordering::Relaxed), 0);
        assert!(
            matches!(
                error,
                ClockedDuplexError::Processor(PrepareError::InvalidSampleRate)
            ) || matches!(error, ClockedDuplexError::InvalidConfiguration)
        );
        let mut pcm = [0.; 4];
        assert_eq!(raw.pop(&mut pcm).unwrap(), 4);
        assert_eq!(pcm, [0.25; 4]);
        drop(probe);
        assert_eq!(deactivates.load(Ordering::Relaxed), 0);
    }
}
#[test]
fn original_spawn_error_before_handoff_and_both_async_build_errors_retain_unique_owners() {
    let failure = start_prepared(
        prepare(Probe::new(11)),
        |_, _, _, _| -> Result<(), ReclaimableClockedOpenError> {
            panic!("handoff after spawn failure")
        },
        |job| {
            drop(job);
            Err(std::io::Error::from_raw_os_error(42))
        },
        None,
    )
    .err()
    .unwrap();
    let (error, owner) = failure.into_parts();
    assert!(
        matches!(error, ReclaimableClockedOpenError::WorkerSpawn(e) if e.raw_os_error()==Some(42))
    );
    assert!(matches!(owner, FailedClockedOwner::Immediate(p) if p.id==11));
    for output_failure in [false, true] {
        let mut retained = None;
        let failure = start_prepared(
            prepare(Probe::new(12)),
            |i, ie, o, oe| {
                retained = Some((i, ie, o, oe));
                let e = cpal::Error::with_message(
                    cpal::ErrorKind::DeviceNotAvailable,
                    "original paired error",
                );
                Err::<(), _>(if output_failure {
                    ReclaimableClockedOpenError::OutputBackend(e)
                } else {
                    ReclaimableClockedOpenError::InputBackend(e)
                })
            },
            spawn_worker,
            None,
        )
        .err()
        .unwrap();
        let (error, owner) = failure.into_parts();
        assert!(matches!(
            (&error, output_failure),
            (ReclaimableClockedOpenError::OutputBackend(_), true)
                | (ReclaimableClockedOpenError::InputBackend(_), false)
        ));
        assert!(error.to_string().contains("original paired error"));
        let mut retirement = match owner {
            FailedClockedOwner::Pending(p) => p,
            _ => panic!("build failure requires proof"),
        };
        assert!(retirement.try_reclaim().unwrap().is_none());
        drop(retained.take());
        assert_eq!(normal(retirement.wait().unwrap()).0.into_processor().id, 12);
    }
}
#[test]
fn caught_poll_and_real_processing_panic_preserve_proof_and_disable_only_guard() {
    let mut probe = Probe::new(13);
    probe.panic_process = true;
    let ((mut input, input_error, mut output, output_error), _, mut retirement) = start_prepared(
        prepare(probe),
        |i, ie, o, oe| Ok((i, ie, o, oe)),
        spawn_worker,
        Some(Box::new(|| panic!("injected poll panic"))),
    )
    .unwrap();
    input.capture_with(|_| panic!("injected capture panic"));
    input.capture(&[0.25; 32]);
    let mut pcm = [9.; 4];
    output.render(&mut pcm);
    assert_eq!(pcm, [0.; 4]);
    output.render(&mut pcm);
    assert_eq!(pcm, [0.; 4]);
    assert!(retirement.try_reclaim().unwrap().is_none());
    drop(input);
    drop(input_error);
    drop(output);
    drop(output_error);
    match retirement.wait().unwrap() {
        ReclaimedClockedDuplex::WorkerFault {
            owner,
            statistics,
            error,
        } => {
            assert_eq!(error, ClockedRetirementError::WorkerPollPanicked);
            assert_eq!(statistics.capture_panics, 1);
            assert_eq!(statistics.input.captured_frames, 0);
            assert_eq!(statistics.output.panics, 1);
            assert_eq!(statistics.output.callbacks, 2);
            assert_eq!(owner.into_processor().id, 13);
        }
        _ => panic!("missing explicit worker fault"),
    }
}
#[test]
fn concurrent_actual_capture_and_render_use_disjoint_cell_fields_then_retire_both_endpoints() {
    let ((mut input, input_error, mut output, output_error), _, retirement) = start_prepared(
        prepare(Probe::new(14)),
        |i, ie, o, oe| Ok((i, ie, o, oe)),
        spawn_worker,
        None,
    )
    .unwrap();
    let input_thread = thread::spawn(move || {
        for _ in 0..1000 {
            input.capture(&[0.25; 4]);
        }
        drop(input);
        drop(input_error);
    });
    let output_thread = thread::spawn(move || {
        let mut pcm = [9.; 4];
        for _ in 0..1000 {
            output.render(&mut pcm);
            assert_eq!(pcm, [0.; 4]);
        }
        drop(output);
        drop(output_error);
    });
    input_thread.join().unwrap();
    output_thread.join().unwrap();
    let (owner, statistics) = normal(retirement.wait().unwrap());
    assert_eq!(statistics.input.callbacks, 1000);
    assert_eq!(statistics.output.callbacks, 1000);
    assert_eq!(
        statistics.input.captured_frames + statistics.input.dropped_frames,
        4000
    );
    assert_eq!(statistics.output.frames, 4000);
    assert_eq!(owner.into_processor().id, 14);
}

type Guards<P> = (InputData<P>, InputError<P>, OutputData<P>, OutputError<P>);
pub(crate) struct SimulatedPair<P: Processor> {
    statistics: Arc<Statistics>,
    retirement: Option<ClockedDuplexRetirement<P>>,
    _guards: Arc<Mutex<Option<Guards<P>>>>,
    fail_start: bool,
}
pub(crate) struct SimulatedBackend<P: Processor>(Arc<Mutex<Option<Guards<P>>>>);
impl<P: Processor> SimulatedPair<P> {
    pub(crate) fn fail_start(&mut self) {
        self.fail_start = true;
    }
    pub(crate) fn play(&self) -> Result<(), cpal::Error> {
        if self.fail_start {
            Err(cpal::Error::with_message(
                cpal::ErrorKind::DeviceNotAvailable,
                "original paired start failure",
            ))
        } else {
            Ok(())
        }
    }
    pub(crate) fn statistics(&self) -> ReclaimableClockedStatistics {
        self.statistics.snapshot()
    }
    pub(crate) fn stop(mut self) -> ClockedDuplexRetirement<P> {
        self.retirement.take().unwrap()
    }
}
impl<P: Processor> SimulatedBackend<P> {
    pub(crate) fn release(&self) {
        self.0.lock().unwrap().take();
    }
    pub(crate) fn fault_kind(&self, kind: u8) {
        let guard = self.0.lock().unwrap();
        let (_, input_error, output, output_error) = guard.as_ref().unwrap();
        match kind {
            0 => input_error.report(),
            1 => output_error.report(),
            2 => {
                output
                    .cell
                    .statistics
                    .output
                    .panics
                    .fetch_add(1, Ordering::Relaxed);
            }
            3 => {
                output
                    .cell
                    .statistics
                    .capture_panics
                    .fetch_add(1, Ordering::Relaxed);
            }
            _ => unreachable!(),
        }
    }
    pub(crate) fn fault(&self) {
        self.0.lock().unwrap().as_ref().unwrap().1.report();
    }
}
pub(crate) fn simulated_pair<P: Processor + Send + 'static>(
    processor: P,
) -> (SimulatedPair<P>, SimulatedBackend<P>) {
    let (capture, raw) = PreparedCapture::new(1, 32).unwrap();
    let renderer =
        PreparedClockedDuplex::new_owned(processor, raw, 48000, 48000, 1, options()).unwrap();
    let (guards, statistics, retirement) = start_prepared(
        StoppedClockedDuplex { capture, renderer },
        |i, ie, o, oe| Ok((i, ie, o, oe)),
        spawn_worker,
        None,
    )
    .unwrap();
    let guards = Arc::new(Mutex::new(Some(guards)));
    (
        SimulatedPair {
            statistics,
            retirement: Some(retirement),
            _guards: Arc::clone(&guards),
            fail_start: false,
        },
        SimulatedBackend(guards),
    )
}

#[cfg(feature = "resampled-playback")]
mod converted {
    use super::*;
    use crate::BufferedFileDuplexProcessor;
    use ravet_audio_file::{
        AudioFileError, BufferedPlayback, ConvertedReadAheadOptions, FilePlaybackOptions,
        IndexedConvertedReadAhead, PlaybackConversion, ReadAheadStatus, ReadAheadWorker,
        WaveEncoding, WaveFormat, WaveReader, WaveWriter, audio_ring, prepare_file_playback,
    };
    use std::{
        io::Cursor,
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
    struct Mix {
        prepares: Arc<AtomicUsize>,
        deactivates: Arc<AtomicUsize>,
    }
    impl Processor for Mix {
        fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
            assert_eq!(
                c.layout(),
                ravet_foundation::AudioLayout::new(2, 1).unwrap()
            );
            self.prepares.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
        fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
            for n in 0..b.audio.frames() {
                let v = b.audio.input(0).unwrap()[n] + b.audio.input(1).unwrap()[n];
                b.audio.output_mut(0).unwrap()[n] = v;
            }
            ProcessStatus::Continue
        }
        fn reset(&mut self) {}
        fn deactivate(&mut self) {
            self.deactivates.fetch_add(1, Ordering::Relaxed);
        }
    }
    struct Relay;
    impl Processor for Relay {
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
    #[test]
    fn actual_indexed_file_reclaims_all_four_callbacks_then_failed_seek_and_successful_reprepare_preserve_suffix()
     {
        let (mut worker, playback, expected) = file();
        let prepares = Arc::new(AtomicUsize::new(0));
        let deactivates = Arc::new(AtomicUsize::new(0));
        let child = Mix {
            prepares: Arc::clone(&prepares),
            deactivates: Arc::clone(&deactivates),
        };
        let processor = BufferedFileDuplexProcessor::new(playback, child);
        let monitor = processor.monitor();
        let (capture, raw) = PreparedCapture::new(1, 32).unwrap();
        let renderer =
            PreparedClockedDuplex::new_owned(processor, raw, 48000, 48000, 1, options()).unwrap();
        let ((mut input, ie, mut output, oe), _, mut retirement) = start_prepared(
            StoppedClockedDuplex { renderer, capture },
            |i, ie, o, oe| Ok((i, ie, o, oe)),
            spawn_worker,
            None,
        )
        .unwrap();
        let (mut reference_capture, reference_raw) = PreparedCapture::new(1, 32).unwrap();
        reference_capture.capture(&[0.25; 32]);
        input.capture(&[0.25; 32]);
        let mut reference =
            PreparedClockedDuplex::new(Relay, reference_raw, 48000, 48000, 1, options()).unwrap();
        let mut capture_prefix = [0.; 7];
        reference.render(&mut capture_prefix);
        let mut prefix = [9.; 7];
        assert_eq!(
            allocation_counter::measure(|| output.render(&mut prefix)).count_total,
            0
        );
        for n in 0..7 {
            assert_eq!(prefix[n], expected[n] + capture_prefix[n]);
        }
        assert_eq!(monitor.statistics().file_position, 7);
        drop(input);
        drop(ie);
        drop(output);
        assert!(retirement.try_reclaim().unwrap().is_none());
        assert_eq!(deactivates.load(Ordering::Relaxed), 0);
        drop(oe);
        let stopped = match retirement.wait().unwrap() {
            ReclaimedClockedDuplex::Reclaimed { owner, .. } => owner,
            _ => panic!("fault"),
        };
        let (processor, mut raw, mut capture) = stopped.into_parts();
        assert_eq!(deactivates.load(Ordering::Relaxed), 1);
        // Prove original matching endpoints remain usable after all callback owners.
        let mut raw_tail = [0.; 32];
        let _actual_remaining = raw.pop(&mut raw_tail).unwrap();
        capture.capture(&[0.5]);
        assert_eq!(raw.pop(&mut raw_tail[..1]).unwrap(), 1);
        assert_eq!(raw_tail[0], 0.5);
        drop(raw);
        drop(capture);
        drop(reference);
        drop(reference_capture);
        let (mut playback, child) = processor.into_parts();
        assert_eq!(deactivates.load(Ordering::Relaxed), 1);
        let queued = playback.queued_frames();
        assert!(worker.seek_stopped(&mut playback, 129).is_err());
        assert_eq!(playback.position_frames(), 7);
        assert_eq!(playback.queued_frames(), queued);
        worker.seek_stopped(&mut playback, 17).unwrap();
        ready(&playback);
        let processor = BufferedFileDuplexProcessor::new(playback, child);
        let monitor = processor.monitor();
        let (capture, raw) = PreparedCapture::new(1, 32).unwrap();
        let renderer =
            PreparedClockedDuplex::new_owned(processor, raw, 48000, 48000, 1, options()).unwrap();
        let ((mut input, ie, mut output, oe), _, retirement) = start_prepared(
            StoppedClockedDuplex { renderer, capture },
            |i, ie, o, oe| Ok((i, ie, o, oe)),
            spawn_worker,
            None,
        )
        .unwrap();
        let (mut reference_capture, reference_raw) = PreparedCapture::new(1, 32).unwrap();
        reference_capture.capture(&[0.25; 32]);
        input.capture(&[0.25; 32]);
        let mut reference =
            PreparedClockedDuplex::new(Relay, reference_raw, 48000, 48000, 1, options()).unwrap();
        let mut reference_pcm = [0.; 24];
        let mut resumed = [9.; 24];
        reference.render(&mut reference_pcm);
        assert_eq!(
            allocation_counter::measure(|| output.render(&mut resumed)).count_total,
            0
        );
        for n in 0..24 {
            assert_eq!(
                resumed[n],
                expected.get(17 + n).copied().unwrap_or(0.) + reference_pcm[n]
            );
        }
        assert_eq!(monitor.statistics().file_position, expected.len() as u64);
        assert!(monitor.statistics().ended);
        drop(input);
        drop(ie);
        drop(output);
        drop(oe);
        let processor = match retirement.wait().unwrap() {
            ReclaimedClockedDuplex::Reclaimed { owner, .. } => owner.into_processor(),
            _ => panic!("fault"),
        };
        assert_eq!(prepares.load(Ordering::Relaxed), 2);
        assert_eq!(deactivates.load(Ordering::Relaxed), 2);
        drop(processor);
        assert_eq!(deactivates.load(Ordering::Relaxed), 2);
        drop(reference);
        drop(reference_capture);
        let exit = worker.shutdown().unwrap();
        assert!(exit.error.is_none());
        assert_eq!(exit.pump.generation(), 1);
        assert_eq!(exit.pump.output_origin(), 17);
    }
}

pub(crate) fn simulated_open_failure<P: Processor + Send + 'static>(
    processor: P,
    pending: bool,
) -> (ReclaimableClockedFailure<P>, Option<SimulatedBackend<P>>) {
    let error = ReclaimableClockedOpenError::OutputBackend(cpal::Error::with_message(
        cpal::ErrorKind::DeviceNotAvailable,
        "original recovery open failure",
    ));
    if pending {
        let (pair, backend) = simulated_pair(processor);
        (
            ReclaimableClockedFailure {
                error,
                owner: FailedClockedOwner::Pending(pair.stop()),
            },
            Some(backend),
        )
    } else {
        (
            ReclaimableClockedFailure {
                error,
                owner: FailedClockedOwner::Immediate(processor),
            },
            None,
        )
    }
}

#[test]
fn disconnected_unfinished_worker_poll_does_not_join_before_unwind_finishes() {
    let (sender, receiver) = mpsc::channel::<ReclaimedClockedDuplex<Probe>>();
    let (started_send, started) = mpsc::channel();
    let (release_send, release) = mpsc::channel();
    let worker = thread::spawn(move || {
        drop(sender);
        started_send.send(()).unwrap();
        release.recv().unwrap();
        panic!("injected worker unwind");
    });
    started.recv().unwrap();
    let mut retirement = ClockedDuplexRetirement {
        receiver,
        worker: Some(worker),
        taken: false,
    };
    assert!(retirement.try_reclaim().unwrap().is_none());
    release_send.send(()).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while !retirement.worker.as_ref().unwrap().is_finished() {
        assert!(std::time::Instant::now() < deadline);
        thread::yield_now();
    }
    assert_eq!(
        retirement.try_reclaim().err(),
        Some(ClockedRetirementError::WorkerPanicked)
    );
    assert_eq!(
        retirement.try_reclaim().err(),
        Some(ClockedRetirementError::AlreadyReclaimed)
    );
}
#[test]
fn exceptional_deactivation_panic_after_proof_has_no_successful_owner_or_duplicate_deactivation() {
    let mut probe = Probe::new(20);
    probe.panic_deactivate = true;
    let deactivates = Arc::clone(&probe.deactivates);
    let ((input, ie, output, oe), _, retirement) = start_prepared(
        prepare(probe),
        |i, ie, o, oe| Ok((i, ie, o, oe)),
        spawn_worker,
        None,
    )
    .unwrap();
    drop(input);
    drop(ie);
    drop(output);
    drop(oe);
    let owner = normal(retirement.wait().unwrap()).0;
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| owner.into_parts())).is_err());
    assert_eq!(deactivates.load(Ordering::Relaxed), 1);
}

#[test]
fn direct_owned_duplex_preflight_preserves_raw_queue_and_valid_extraction_deactivates_once() {
    for (frames, outputs) in [(0, 1), (8, 0)] {
        let probe = Probe::new(21);
        let prepares = Arc::clone(&probe.prepares);
        let deactivates = Arc::clone(&probe.deactivates);
        let (mut capture, raw) = PreparedCapture::new(1, 32).unwrap();
        capture.capture(&[0.25; 4]);
        let mut failure = None;
        assert_eq!(
            allocation_counter::measure(|| failure =
                crate::PreparedDuplex::new_owned(probe, 48000, raw, outputs, frames).err())
            .count_total,
            0
        );
        let (probe, mut raw, error) = failure.unwrap().into_parts();
        assert_eq!(probe.id, 21);
        assert_eq!(prepares.load(Ordering::Relaxed), 0);
        assert_eq!(deactivates.load(Ordering::Relaxed), 0);
        assert!(matches!(
            error,
            PrepareError::InvalidMaximumFrames | PrepareError::UnsupportedLayout
        ));
        let mut pcm = [0.; 4];
        assert_eq!(raw.pop(&mut pcm).unwrap(), 4);
        assert_eq!(pcm, [0.25; 4]);
        let prepared = crate::PreparedDuplex::new_owned(probe, 48000, raw, 1, 8).unwrap();
        let (probe, mut raw) = prepared.into_parts();
        assert_eq!(prepares.load(Ordering::Relaxed), 1);
        assert_eq!(deactivates.load(Ordering::Relaxed), 1);
        capture.capture(&[0.5]);
        assert_eq!(raw.pop(&mut pcm[..1]).unwrap(), 1);
        assert_eq!(pcm[0], 0.5);
        drop(probe);
        assert_eq!(deactivates.load(Ordering::Relaxed), 1);
    }
}
