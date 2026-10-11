use super::*;
use ravet_foundation::{ProcessBlock, ProcessConfig, ProcessStatus};
use std::sync::{Mutex, atomic::AtomicUsize};

struct Probe {
    id: usize,
    prepared: Arc<AtomicUsize>,
    deactivated: Arc<AtomicUsize>,
    dropped: Arc<Mutex<Option<String>>>,
    fail_prepare: bool,
    panic_process: bool,
    panic_deactivate: bool,
}
impl Probe {
    fn new(id: usize) -> Self {
        Self {
            id,
            prepared: Arc::new(AtomicUsize::new(0)),
            deactivated: Arc::new(AtomicUsize::new(0)),
            dropped: Arc::new(Mutex::new(None)),
            fail_prepare: false,
            panic_process: false,
            panic_deactivate: false,
        }
    }
}
impl Processor for Probe {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        self.prepared.fetch_add(1, Ordering::Relaxed);
        if self.fail_prepare {
            Err(PrepareError::UnsupportedLayout)
        } else {
            Ok(())
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        assert!(!self.panic_process, "injected DSP panic");
        block.clear_outputs();
        ProcessStatus::Silence
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {
        self.deactivated.fetch_add(1, Ordering::Relaxed);
        assert!(!self.panic_deactivate, "injected deactivation panic");
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        *self.dropped.lock().unwrap() =
            Some(thread::current().name().unwrap_or("unnamed").to_owned());
    }
}
fn prepare(probe: Probe) -> PreparedOutput<Probe> {
    PreparedOutput::new_owned(probe, 48000, 2, 32).unwrap()
}
fn split(retirement: ReclaimedOutput<Probe>) -> (Probe, OutputStatistics) {
    match retirement {
        ReclaimedOutput::Reclaimed {
            processor,
            statistics,
        } => (processor, statistics),
        _ => panic!("unexpected worker fault"),
    }
}

#[test]
fn both_retirement_orders_wait_for_last_closure_fields_without_callback_destruction() {
    for data_first in [true, false] {
        let probe = Probe::new(7);
        let dropped = Arc::clone(&probe.dropped);
        let inactive = Arc::clone(&probe.deactivated);
        let ((mut data, errors), _, mut retirement) =
            start_prepared(prepare(probe), |d, e| Ok((d, e)), spawn_worker, None).unwrap();
        data.render(&mut [0.0; 64]);
        errors.report();
        assert!(retirement.try_reclaim().unwrap().is_none());
        if data_first {
            drop(data);
            assert!(retirement.try_reclaim().unwrap().is_none());
            drop(errors);
        } else {
            drop(errors);
            assert!(retirement.try_reclaim().unwrap().is_none());
            drop(data);
        }
        let (probe, stats) = split(retirement.wait().unwrap());
        assert_eq!(probe.id, 7);
        assert_eq!(stats.frames, 32);
        assert_eq!(stats.backend_errors, 1);
        assert_eq!(inactive.load(Ordering::Relaxed), 1);
        assert!(dropped.lock().unwrap().is_none());
        drop(probe);
    }
}
#[test]
fn release_flag_alone_cannot_reclaim_before_guard_arc_field_drop() {
    let ((data, errors), _, mut retirement) = start_prepared(
        prepare(Probe::new(1)),
        |d, e| Ok((d, e)),
        spawn_worker,
        None,
    )
    .unwrap();
    // Emulate an observed Drop body before Rust runs that guard's Arc field Drop.
    data.cell.data_retired.store(true, Ordering::Release);
    errors.cell.error_retired.store(true, Ordering::Release);
    thread::sleep(Duration::from_millis(5));
    assert!(retirement.try_reclaim().unwrap().is_none());
    drop(data);
    thread::sleep(Duration::from_millis(5));
    assert!(retirement.try_reclaim().unwrap().is_none());
    drop(errors);
    let _ = split(retirement.wait().unwrap());
}
#[test]
fn receiver_early_drop_and_asynchronous_backend_owners_retire_on_control_worker() {
    let probe = Probe::new(9);
    let dropped = Arc::clone(&probe.dropped);
    let inactive = Arc::clone(&probe.deactivated);
    let ((data, errors), _, retirement) =
        start_prepared(prepare(probe), |d, e| Ok((d, e)), spawn_worker, None).unwrap();
    drop(retirement);
    thread::sleep(Duration::from_millis(5));
    assert!(dropped.lock().unwrap().is_none());
    thread::Builder::new()
        .name("simulated-audio-callback".into())
        .spawn(move || drop(data))
        .unwrap()
        .join()
        .unwrap();
    assert!(dropped.lock().unwrap().is_none());
    thread::Builder::new()
        .name("simulated-error-callback".into())
        .spawn(move || drop(errors))
        .unwrap()
        .join()
        .unwrap();
    for _ in 0..1000 {
        if dropped.lock().unwrap().is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        dropped.lock().unwrap().as_deref(),
        Some("ravet-output-retirement")
    );
    assert_eq!(inactive.load(Ordering::Relaxed), 1);
}
#[test]
fn caught_poll_panic_keeps_owner_until_both_callbacks_retire() {
    let probe = Probe::new(13);
    let dropped = Arc::clone(&probe.dropped);
    let ((data, errors), _, mut retirement) = start_prepared(
        prepare(probe),
        |d, e| Ok((d, e)),
        spawn_worker,
        Some(Box::new(|| panic!("poll fault"))),
    )
    .unwrap();
    thread::sleep(Duration::from_millis(5));
    assert!(retirement.try_reclaim().unwrap().is_none());
    assert!(dropped.lock().unwrap().is_none());
    drop(data);
    drop(errors);
    match retirement.wait().unwrap() {
        ReclaimedOutput::WorkerFault {
            processor, error, ..
        } => {
            assert_eq!(processor.id, 13);
            assert_eq!(error, OutputRetirementError::WorkerPollPanicked);
        }
        _ => panic!("fault receipt required"),
    }
}
#[test]
fn process_panic_disables_render_but_stopped_original_owner_is_explicit() {
    let mut probe = Probe::new(2);
    probe.panic_process = true;
    let ((mut data, errors), _, retirement) =
        start_prepared(prepare(probe), |d, e| Ok((d, e)), spawn_worker, None).unwrap();
    let mut output = [1.0; 64];
    data.render(&mut output);
    assert_eq!(output, [0.0; 64]);
    output.fill(1.0);
    data.render(&mut output);
    assert_eq!(output, [0.0; 64]);
    drop(data);
    drop(errors);
    let (_, stats) = split(retirement.wait().unwrap());
    assert_eq!(stats.panics, 1);
    assert_eq!(stats.callbacks, 2);
    assert_eq!(stats.frames, 0);
}
#[test]
fn original_spawn_and_backend_errors_return_exact_ownership_without_hidden_retry() {
    let probe = Probe::new(42);
    let inactive = Arc::clone(&probe.deactivated);
    let failure = start_prepared(
        prepare(probe),
        |_, _| -> Result<(), cpal::Error> { panic!("must not hand off") },
        |job| {
            drop(job);
            Err(std::io::Error::other("spawn original"))
        },
        None,
    )
    .err()
    .unwrap();
    let (error, owner) = failure.into_parts();
    match error {
        ReclaimableOutputOpenError::WorkerSpawn(error) => {
            assert_eq!(error.to_string(), "spawn original")
        }
        _ => panic!(),
    };
    match owner {
        FailedOutputOwner::Immediate(probe) => assert_eq!(probe.id, 42),
        _ => panic!(),
    };
    assert_eq!(inactive.load(Ordering::Relaxed), 1);
    let held = Arc::new(Mutex::new(None));
    let capture = Arc::clone(&held);
    let failure = start_prepared(
        prepare(Probe::new(73)),
        move |d, e| {
            *capture.lock().unwrap() = Some((d, e));
            Err::<(), _>(cpal::Error::with_message(
                cpal::ErrorKind::DeviceNotAvailable,
                "original backend message",
            ))
        },
        spawn_worker,
        None,
    )
    .err()
    .unwrap();
    let (error, owner) = failure.into_parts();
    match error {
        ReclaimableOutputOpenError::Backend(error) => {
            assert_eq!(error.kind(), cpal::ErrorKind::DeviceNotAvailable);
            assert_eq!(error.message(), Some("original backend message"));
        }
        _ => panic!("original backend error required"),
    }
    let FailedOutputOwner::Pending(mut retirement) = owner else {
        panic!()
    };
    assert!(retirement.try_reclaim().unwrap().is_none());
    drop(held.lock().unwrap().take());
    let (probe, _) = split(retirement.wait().unwrap());
    assert_eq!(probe.id, 73);
}
#[test]
fn preparation_preflight_and_scratch_fault_preserve_untouched_owner_then_reprepare() {
    let probe = Probe::new(99);
    let prepared = Arc::clone(&probe.prepared);
    let failure = PreparedOutput::new_owned(probe, 0, 2, 32).err().unwrap();
    let (probe, error) = failure.into_parts();
    assert_eq!(error, PrepareError::InvalidSampleRate);
    assert_eq!(prepared.load(Ordering::Relaxed), 0);
    let failure = PreparedOutput::new_owned_with_allocator(probe, 48000, 2, 32, |_, _| {
        Err(PrepareError::InvalidMaximumFrames)
    })
    .err()
    .unwrap();
    let (mut probe, error) = failure.into_parts();
    assert_eq!(error, PrepareError::InvalidMaximumFrames);
    assert_eq!(prepared.load(Ordering::Relaxed), 0);
    probe.fail_prepare = true;
    let failure = PreparedOutput::new_owned(probe, 48000, 2, 32)
        .err()
        .unwrap();
    let (mut probe, error) = failure.into_parts();
    assert_eq!(error, PrepareError::UnsupportedLayout);
    assert_eq!(prepared.load(Ordering::Relaxed), 1);
    probe.fail_prepare = false;
    let inactive = Arc::clone(&probe.deactivated);
    let renderer = prepare(probe);
    let probe = renderer.into_processor();
    assert_eq!(inactive.load(Ordering::Relaxed), 1);
    let renderer = prepare(probe);
    drop(renderer);
    assert_eq!(inactive.load(Ordering::Relaxed), 2);
}

#[test]
fn complete_normal_callback_captures_render_and_drop_without_allocation_or_deallocation() {
    for data_first in [true, false] {
        let ((mut data, errors), counters, retirement) = start_prepared(
            prepare(Probe::new(41)),
            |d, e| Ok((d, e)),
            spawn_worker,
            None,
        )
        .unwrap();
        // These are the full framework captures of the native data/error closures;
        // backend closure allocation itself has a separate native teardown policy.
        let mut data_callback = move |output: &mut [f32]| data.render(output);
        let error_callback = move || errors.report();
        let mut output = [1.0; 64];
        let info = allocation_counter::measure(|| {
            data_callback(&mut output);
            error_callback();
        });
        assert_eq!(info.count_total, 0);
        assert_eq!(info.count_current, 0);
        drop(counters); // early control statistics owner must not change proof
        if data_first {
            let info = allocation_counter::measure(|| drop(data_callback));
            assert_eq!(info.count_total, 0);
            assert_eq!(info.count_current, 0);
            let info = allocation_counter::measure(|| drop(error_callback));
            assert_eq!(info.count_total, 0);
            assert_eq!(info.count_current, 0);
        } else {
            let info = allocation_counter::measure(|| drop(error_callback));
            assert_eq!(info.count_total, 0);
            assert_eq!(info.count_current, 0);
            let info = allocation_counter::measure(|| drop(data_callback));
            assert_eq!(info.count_total, 0);
            assert_eq!(info.count_current, 0);
        }
        let _ = split(retirement.wait().unwrap());
    }
}

#[cfg(feature = "resampled-playback")]
#[test]
fn retired_actual_converted_processor_preserves_unique_consumer_for_failed_seek_and_reopen() {
    use crate::BufferedPlaybackOutput;
    use ravet_audio_file::{
        AudioFileError, ConvertedReadAhead, ConvertedReadAheadOptions, FilePlaybackOptions,
        PlaybackConversion, ReadAheadWorker, WaveEncoding, WaveFormat, WaveReader, WaveWriter,
        audio_ring, prepare_file_playback,
    };
    use std::{io::Cursor, time::Instant};
    let mut writer = WaveWriter::new(
        Cursor::new(Vec::new()),
        WaveFormat {
            channels: 2,
            sample_rate: 44100,
            encoding: WaveEncoding::Float32,
        },
        113,
    )
    .unwrap();
    let samples: Vec<f32> = (0..257)
        .flat_map(|i| [(i as f32 * 0.11).sin() * 0.4, (i as f32 * 0.07).sin() * 0.4])
        .collect();
    for chunk in samples.chunks(226) {
        writer.write_frames(chunk).unwrap();
    }
    let bytes = writer.finish().unwrap().into_inner();
    let mut reference = prepare_file_playback(
        WaveReader::new(Cursor::new(bytes.clone()), 17).unwrap(),
        FilePlaybackOptions {
            output_rate: 48000,
            conversion: PlaybackConversion::Sinc { taps: 16 },
            maximum_source_frames: 257,
            maximum_output_frames: 512,
            source_batch_frames: 17,
            maximum_render_frames: 32,
        },
    )
    .unwrap();
    let mut expected = vec![0.0; reference.info().prepared_frames * 2];
    reference.playback_mut().play();
    for chunk in expected.chunks_mut(64) {
        reference.playback_mut().render(chunk).unwrap();
    }
    let reader = WaveReader::new(Cursor::new(bytes.clone()), 17).unwrap();
    let factory = move || WaveReader::new(Cursor::new(bytes.clone()), 17);
    let (producer, consumer) = audio_ring(2, 32).unwrap();
    let pump = ConvertedReadAhead::new(
        reader,
        factory,
        producer,
        &consumer,
        ConvertedReadAheadOptions {
            output_rate: 48000,
            conversion: PlaybackConversion::Sinc { taps: 16 },
            maximum_source_frames: 257,
            maximum_output_frames: 512,
            source_batch_frames: 17,
            conversion_batch_frames: 7,
        },
    )
    .unwrap();
    let (mut worker, mut playback) = ReadAheadWorker::new(pump, consumer, 32).unwrap();
    worker.start().unwrap();
    playback.play();
    let processor = BufferedPlaybackOutput::new(playback);
    let monitor = processor.monitor();
    let renderer = PreparedOutput::new_owned(processor, 48000, 2, 32).unwrap();
    let ((mut data, errors), _, retirement) =
        start_prepared(renderer, |d, e| Ok((d, e)), spawn_worker, None).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut output = [0.0; 14];
    while monitor.statistics().consumed_output_frames < 7 {
        assert!(
            Instant::now() < deadline,
            "monitor={:?}, output={:?}",
            monitor.statistics(),
            data.cell.counters.snapshot()
        );
        data.render(&mut output);
        thread::yield_now();
    }
    drop(data);
    drop(errors);
    let mut processor = match retirement.wait().unwrap() {
        ReclaimedOutput::Reclaimed { processor, .. } => processor,
        _ => panic!(),
    };
    let before = processor.playback().position_frames();
    assert_eq!(before, 7);
    assert!(matches!(
        worker.seek_stopped(processor.playback_mut(), 513),
        Err(AudioFileError::Capacity)
    ));
    assert_eq!(processor.playback().position_frames(), before);
    worker.seek_stopped(processor.playback_mut(), 17).unwrap();
    processor.playback_mut().play();
    let renderer = PreparedOutput::new_owned(processor, 48000, 2, 32).unwrap();
    let ((mut data, errors), _, retirement) =
        start_prepared(renderer, |d, e| Ok((d, e)), spawn_worker, None).unwrap();
    let mut actual = Vec::new();
    let mut output = [0.0; 64];
    let mut prior = monitor.statistics().consumed_output_frames;
    loop {
        data.render(&mut output);
        let stats = monitor.statistics();
        let n = (stats.consumed_output_frames - prior) as usize;
        actual.extend_from_slice(&output[..n * 2]);
        prior = stats.consumed_output_frames;
        if stats.ended {
            break;
        }
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
    assert_eq!(actual, expected[34..]);
    drop(errors);
    drop(data);
    let processor = match retirement.wait().unwrap() {
        ReclaimedOutput::Reclaimed { processor, .. } => processor,
        _ => panic!(),
    };
    drop(processor);
    let exit = worker.shutdown().unwrap();
    assert!(exit.error.is_none());
}

#[test]
fn explicit_poll_receipt_is_single_use_and_never_restores_a_taken_owner() {
    let ((data, errors), _, mut retirement) = start_prepared(
        prepare(Probe::new(108)),
        |d, e| Ok((d, e)),
        spawn_worker,
        None,
    )
    .unwrap();
    drop(data);
    drop(errors);
    let receipt = loop {
        if let Some(receipt) = retirement.try_reclaim().unwrap() {
            break receipt;
        }
        thread::yield_now();
    };
    let (probe, _) = split(receipt);
    assert_eq!(probe.id, 108);
    assert!(matches!(
        retirement.try_reclaim(),
        Err(OutputRetirementError::AlreadyReclaimed)
    ));
    assert!(matches!(
        retirement.wait(),
        Err(OutputRetirementError::AlreadyReclaimed)
    ));
}
#[test]
fn exceptional_deactivation_panic_loses_recovery_without_false_owner_or_double_deactivation() {
    let mut probe = Probe::new(109);
    probe.panic_deactivate = true;
    let deactivations = Arc::clone(&probe.deactivated);
    let dropped = Arc::clone(&probe.dropped);
    let ((data, errors), _, retirement) =
        start_prepared(prepare(probe), |d, e| Ok((d, e)), spawn_worker, None).unwrap();
    drop(data);
    drop(errors);
    assert!(matches!(
        retirement.wait(),
        Err(OutputRetirementError::WorkerPanicked)
    ));
    assert_eq!(deactivations.load(Ordering::Relaxed), 1);
    assert_eq!(
        dropped.lock().unwrap().as_deref(),
        Some("ravet-output-retirement")
    );
}

#[test]
fn disconnected_unfinished_worker_poll_does_not_join_before_unwind_finishes() {
    let (sender, receiver) = mpsc::channel::<ReclaimedOutput<Probe>>();
    let (ready_send, ready_receive) = mpsc::channel();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let held = Arc::clone(&barrier);
    let worker = thread::spawn(move || {
        drop(sender);
        ready_send.send(()).unwrap();
        held.wait();
    });
    let mut retirement = OutputRetirement {
        receiver,
        worker: Some(worker),
        taken: false,
    };
    ready_receive.recv().unwrap();
    assert!(retirement.try_reclaim().unwrap().is_none()); // would deadlock if this joined
    barrier.wait();
    assert!(matches!(
        retirement.wait(),
        Err(OutputRetirementError::Disconnected)
    ));
}
#[test]
fn every_invalid_preparation_shape_returns_owner_before_allocator_or_processor() {
    for (rate, channels, frames, error) in [
        (0, 2, 32, PrepareError::InvalidSampleRate),
        (48000, 0, 32, PrepareError::UnsupportedLayout),
        (48000, 65, 32, PrepareError::UnsupportedLayout),
        (48000, 2, 0, PrepareError::InvalidMaximumFrames),
        (
            48000,
            2,
            crate::MAXIMUM_FRAMES + 1,
            PrepareError::InvalidMaximumFrames,
        ),
    ] {
        let probe = Probe::new(120);
        let prepared = Arc::clone(&probe.prepared);
        let failure =
            PreparedOutput::new_owned_with_allocator(probe, rate, channels, frames, |_, _| {
                panic!("preflight must precede allocation")
            })
            .err()
            .unwrap();
        let (probe, actual) = failure.into_parts();
        assert_eq!(probe.id, 120);
        assert_eq!(actual, error);
        assert_eq!(prepared.load(Ordering::Relaxed), 0);
    }
}
