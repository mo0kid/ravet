#![cfg(feature = "mp3-encode")]
use ravet_audio_file::{
    AudioFileError as E, Mp3Format, Mp3RecordingCompletion as C, Mp3RecordingStatus as S,
    Mp3RecordingWorker, audio_ring,
};
use std::{
    io::{self, Cursor, Seek, SeekFrom, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, ThreadId},
    time::{Duration, Instant},
};
#[derive(Default)]
struct Counts {
    writes: AtomicUsize,
    flushes: AtomicUsize,
    threads: Mutex<Vec<ThreadId>>,
    fail: AtomicBool,
    flush_fail: AtomicBool,
    panic: AtomicBool,
    byte_limit: Mutex<Option<usize>>,
    seeks: AtomicUsize,
    seek_fail: AtomicBool,
    rewrite_fail: AtomicBool,
    physical: AtomicUsize,
    original: Arc<()>,
}
#[derive(Debug)]
struct Original {
    id: Arc<()>,
    message: &'static str,
}
impl std::fmt::Display for Original {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message)
    }
}
impl std::error::Error for Original {}
fn injected(counts: &Counts, message: &'static str) -> io::Error {
    io::Error::other(Original {
        id: Arc::clone(&counts.original),
        message,
    })
}
fn assert_original(error: &io::Error, counts: &Counts) {
    let payload = error.get_ref().unwrap().downcast_ref::<Original>().unwrap();
    assert!(Arc::ptr_eq(&payload.id, &counts.original));
}
struct Sink {
    bytes: Cursor<Vec<u8>>,
    counts: Arc<Counts>,
}
impl Sink {
    fn new() -> (Self, Arc<Counts>) {
        let counts = Arc::new(Counts::default());
        (
            Self {
                bytes: Cursor::new(Vec::new()),
                counts: Arc::clone(&counts),
            },
            counts,
        )
    }
}
impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.counts.writes.fetch_add(1, Ordering::Relaxed);
        self.counts
            .threads
            .lock()
            .unwrap()
            .push(thread::current().id());
        assert!(
            !self.counts.panic.load(Ordering::Acquire),
            "selected worker sink panic"
        );
        if self.counts.fail.load(Ordering::Acquire) {
            return Err(injected(&self.counts, "original worker sink error"));
        }
        let limit = *self.counts.byte_limit.lock().unwrap();
        let count = limit.map_or(bytes.len(), |limit| {
            bytes
                .len()
                .min(limit.saturating_sub(self.counts.physical.load(Ordering::Relaxed)))
        });
        if count == 0 && !bytes.is_empty() {
            return Err(injected(&self.counts, "original partial worker sink error"));
        }
        let written = self.bytes.write(&bytes[..count])?;
        self.counts.physical.fetch_add(written, Ordering::Relaxed);
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.counts.flushes.fetch_add(1, Ordering::Relaxed);
        self.counts
            .threads
            .lock()
            .unwrap()
            .push(thread::current().id());
        if self.counts.flush_fail.load(Ordering::Acquire) {
            return Err(injected(&self.counts, "original worker flush error"));
        }
        Ok(())
    }
}
impl Seek for Sink {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.counts.seeks.fetch_add(1, Ordering::Relaxed);
        self.counts
            .threads
            .lock()
            .unwrap()
            .push(thread::current().id());
        if self.counts.seek_fail.load(Ordering::Acquire) {
            return Err(injected(&self.counts, "original startup seek error"));
        }
        if self.counts.rewrite_fail.load(Ordering::Acquire) && position == SeekFrom::Start(0) {
            return Err(injected(&self.counts, "original metadata seek error"));
        }
        self.bytes.seek(position)
    }
}
fn format() -> Mp3Format {
    Mp3Format {
        channels: 2,
        sample_rate: 48000,
        bit_rate: ravet_audio_file::Mp3Bitrate::Kbps256,
    }
}
fn signal(frames: usize) -> Vec<f32> {
    (0..frames)
        .flat_map(|frame| {
            [337., 977.].map(move |frequency| {
                (0.5 * (std::f64::consts::TAU * frequency * frame as f64 / 48000.).sin()) as f32
            })
        })
        .collect()
}
fn wait(mut condition: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < end, "worker did not reach expected state");
        thread::yield_now();
    }
}
#[test]
fn prepared_abort_shape_rejection_and_send_controller_do_not_create_codec_output() {
    fn assert_send<T: Send>() {}
    assert_send::<Mp3RecordingWorker<Sink>>();
    let (mut producer, consumer) = audio_ring(2, 4).unwrap();
    let (sink, counts) = Sink::new();
    producer.push(&[0.25, -0.25]).unwrap();
    let worker = Mp3RecordingWorker::new(sink, format(), consumer, 2, 4096, 65536).unwrap();
    assert_eq!(worker.status(), S::Prepared);
    let exit = worker.abort().unwrap();
    assert!(matches!(exit.completion(), C::Aborted));
    assert_eq!(exit.consumer.queued_frames(), 1);
    assert!(exit.destination.bytes.get_ref().is_empty());
    assert_eq!(counts.writes.load(Ordering::Relaxed), 0);
    let (_, consumer) = audio_ring(1, 4).unwrap();
    let (sink, counts) = Sink::new();
    assert!(matches!(
        Mp3RecordingWorker::new(sink, format(), consumer, 2, 4096, 65536),
        Err(E::InvalidSamples)
    ));
    assert_eq!(counts.writes.load(Ordering::Relaxed), 0);
    assert_eq!(counts.seeks.load(Ordering::Relaxed), 0);
}
#[test]
fn concurrent_bounded_capture_uses_only_codec_owning_worker_and_finalizes_exact_timing() {
    let (mut producer, consumer) = audio_ring(2, 64).unwrap();
    let (sink, counts) = Sink::new();
    let mut worker = Mp3RecordingWorker::new(sink, format(), consumer, 17, 8193, 65536).unwrap();
    worker.start().unwrap();
    let source = signal(8193);
    let expected = source.clone();
    let capture = thread::spawn(move || {
        let mut frame = 0;
        let end = Instant::now() + Duration::from_secs(10);
        while frame < 8193 {
            let to = (frame + 103).min(8193);
            let mut accepted = 0;
            let heap = allocation_counter::measure(|| {
                accepted = producer.push(&source[frame * 2..to * 2]).unwrap();
            });
            assert_eq!((heap.count_total, heap.count_current), (0, 0));
            frame += accepted;
            assert!(Instant::now() < end);
            thread::yield_now();
        }
        producer
    });
    let mut producer = capture.join().unwrap();
    let exit = worker.finish_stopped(&mut producer).unwrap();
    assert!(matches!(exit.completion(), C::Finalized));
    assert_eq!(worker.status(), S::Finalized);
    assert_eq!((exit.accepted_frames, exit.dequeued_frames), (8193, 8193));
    assert_eq!(
        exit.written_bytes,
        counts.physical.load(Ordering::Relaxed) as u64
    );
    assert_eq!(exit.consumer.queued_frames(), 0);
    assert_eq!(
        exit.file_bytes,
        exit.destination.bytes.get_ref().len() as u64
    );
    assert!(exit.written_bytes > exit.file_bytes);
    assert_eq!(counts.flushes.load(Ordering::Relaxed), 1);
    let ids = counts.threads.lock().unwrap();
    assert!(!ids.is_empty());
    assert!(
        ids.iter()
            .all(|id| *id == ids[0] && *id != thread::current().id())
    );
    drop(ids);
    #[cfg(feature = "compressed")]
    {
        let decoded =
            ravet_audio_file::decode_compressed(exit.destination.bytes.get_ref(), 8193).unwrap();
        assert_eq!(
            (decoded.frames(), decoded.channels(), decoded.sample_rate()),
            (8193, 2, 48000)
        );
        let energy: f64 = expected.iter().map(|x| f64::from(*x).powi(2)).sum();
        let noise: f64 = expected
            .iter()
            .zip(decoded.samples())
            .map(|(a, b)| (f64::from(*a) - f64::from(*b)).powi(2))
            .sum();
        assert!(10. * (energy / noise).log10() > 45.);
    }
    drop(expected);
}
#[test]
fn acknowledged_pause_preserves_queue_and_failed_proofs_then_resumes_and_drains_tail() {
    let (mut producer, consumer) = audio_ring(2, 4).unwrap();
    let (sink, _) = Sink::new();
    let mut worker = Mp3RecordingWorker::new(sink, format(), consumer, 1, 4096, 65536).unwrap();
    worker.start().unwrap();
    wait(|| worker.status() == S::Running);
    worker.pause().unwrap();
    assert_eq!(worker.status(), S::Paused);
    for _ in 0..32 {
        worker.resume().unwrap();
        worker.pause().unwrap();
        assert_eq!(worker.status(), S::Paused);
    }
    let before = worker.dequeued_frames();
    assert_eq!(
        producer
            .push(&[0.25, -0.25, 0.5, -0.5, 0., 0., -0.25, 0.25, 0.1, -0.1])
            .unwrap(),
        4
    );
    assert_eq!(producer.push(&[0.1, -0.1]).unwrap(), 0);
    assert_eq!(worker.dequeued_frames(), before);
    let (mut foreign, _) = audio_ring(2, 4).unwrap();
    assert!(matches!(
        worker.finish_stopped(&mut foreign),
        Err(E::InvalidSamples)
    ));
    assert!(matches!(
        worker.finish_after_producer_retired(),
        Err(E::InvalidRecord)
    ));
    assert_eq!(worker.status(), S::Paused);
    worker.resume().unwrap();
    wait(|| worker.dequeued_frames() == 4);
    worker.pause().unwrap();
    assert_eq!(producer.push(&[0.1, -0.1, 0.2, -0.2]).unwrap(), 2);
    let exit = worker.finish_stopped(&mut producer).unwrap();
    assert!(matches!(exit.completion(), C::Finalized));
    assert_eq!((exit.accepted_frames, exit.dequeued_frames), (6, 6));
    #[cfg(feature = "compressed")]
    assert_eq!(
        ravet_audio_file::decode_compressed(exit.destination.bytes.get_ref(), 6)
            .unwrap()
            .frames(),
        6
    );
}
#[test]
fn never_started_retired_tail_stays_bounded_and_starts_codec_on_worker() {
    let (mut producer, consumer) = audio_ring(2, 4).unwrap();
    producer
        .push(&[0.25, -0.25, 0.5, -0.5, 0., 0., -0.25, 0.25])
        .unwrap();
    drop(producer);
    let (sink, counts) = Sink::new();
    let mut worker = Mp3RecordingWorker::new(sink, format(), consumer, 3, 4, 65536).unwrap();
    let exit = worker.finish_after_producer_retired().unwrap();
    assert!(matches!(exit.completion(), C::Finalized));
    assert_eq!((exit.accepted_frames, exit.dequeued_frames), (4, 4));
    #[cfg(feature = "compressed")]
    assert_eq!(
        ravet_audio_file::decode_compressed(exit.destination.bytes.get_ref(), 4)
            .unwrap()
            .frames(),
        4
    );
    assert!(
        counts
            .threads
            .lock()
            .unwrap()
            .iter()
            .all(|id| *id != thread::current().id())
    );
}
#[test]
fn startup_seek_error_reclaims_untouched_source_without_retry() {
    let (mut producer, consumer) = audio_ring(2, 4).unwrap();
    producer.push(&[0.25, -0.25]).unwrap();
    let (sink, counts) = Sink::new();
    counts.seek_fail.store(true, Ordering::Release);
    let mut worker = Mp3RecordingWorker::new(sink, format(), consumer, 1, 4, 65536).unwrap();
    worker.start().unwrap();
    wait(|| worker.status() == S::Failed);
    let seeks = counts.seeks.load(Ordering::Relaxed);
    let exit = worker.finish_stopped(&mut producer).unwrap();
    assert!(
        matches!(exit.completion(), C::Failed(E::Io(error)) if error.to_string()=="original startup seek error")
    );
    assert_eq!(exit.consumer.queued_frames(), 1);
    assert_eq!((exit.accepted_frames, exit.dequeued_frames), (0, 0));
    assert_eq!(counts.seeks.load(Ordering::Relaxed), seeks);
    assert_eq!(counts.writes.load(Ordering::Relaxed), 0);
    assert_eq!(counts.flushes.load(Ordering::Relaxed), 0);
}
#[test]
fn invalid_dequeued_batch_stops_pumping_and_returns_original_error_with_queued_suffix() {
    let (mut producer, consumer) = audio_ring(2, 4).unwrap();
    producer
        .push(&[f32::NAN, 0.25, 0.1, -0.1, 0.2, -0.2])
        .unwrap();
    let (sink, counts) = Sink::new();
    let mut worker = Mp3RecordingWorker::new(sink, format(), consumer, 1, 4096, 65536).unwrap();
    worker.start().unwrap();
    wait(|| worker.status() == S::Failed);
    assert_eq!((worker.accepted_frames(), worker.dequeued_frames()), (0, 1));
    let writes = counts.writes.load(Ordering::Relaxed);
    let exit = worker.finish_stopped(&mut producer).unwrap();
    assert!(matches!(exit.completion(), C::Failed(E::InvalidSamples)));
    assert_eq!(exit.consumer.queued_frames(), 2);
    assert_eq!((exit.accepted_frames, exit.dequeued_frames), (0, 1));
    assert_eq!(counts.writes.load(Ordering::Relaxed), writes);
    assert_eq!(counts.flushes.load(Ordering::Relaxed), 0);
}
#[test]
fn flush_failure_is_failed_outcome_and_abort_drop_never_finalize() {
    let (mut producer, consumer) = audio_ring(2, 4).unwrap();
    producer.push(&[0.25, -0.25]).unwrap();
    let (sink, counts) = Sink::new();
    counts.flush_fail.store(true, Ordering::Release);
    let mut worker = Mp3RecordingWorker::new(sink, format(), consumer, 1, 4, 65536).unwrap();
    let exit = worker.finish_stopped(&mut producer).unwrap();
    assert!(
        matches!(exit.completion(),C::Failed(E::Io(error)) if error.to_string()=="original worker flush error")
    );
    assert_eq!(counts.flushes.load(Ordering::Relaxed), 1);
    assert_eq!(
        exit.written_bytes,
        counts.physical.load(Ordering::Relaxed) as u64
    );
    assert_eq!(worker.status(), S::Failed);
    for drop_only in [false, true] {
        let (mut producer, consumer) = audio_ring(2, 4).unwrap();
        let (sink, counts) = Sink::new();
        let mut worker = Mp3RecordingWorker::new(sink, format(), consumer, 1, 4, 65536).unwrap();
        worker.start().unwrap();
        wait(|| worker.status() == S::Running);
        worker.pause().unwrap();
        assert_eq!(producer.push(&[0.25, -0.25, 0.5, -0.5]).unwrap(), 2);
        assert_eq!(worker.dequeued_frames(), 0);
        if drop_only {
            drop(worker);
        } else {
            let exit = worker.abort().unwrap();
            assert!(matches!(exit.completion(), C::Aborted));
            assert_eq!((exit.accepted_frames, exit.dequeued_frames), (0, 0));
            assert_eq!(exit.consumer.queued_frames(), 2);
        }
        assert_eq!(counts.flushes.load(Ordering::Relaxed), 0);
        let writes = counts.writes.load(Ordering::Relaxed);
        thread::yield_now();
        assert_eq!(counts.writes.load(Ordering::Relaxed), writes);
    }
}
#[test]
fn panic_loses_thread_local_owner_and_reports_failure_instead_of_recovery_or_completion() {
    let (mut producer, consumer) = audio_ring(2, 4096).unwrap();
    producer.push(&signal(4096)).unwrap();
    let (sink, counts) = Sink::new();
    counts.panic.store(true, Ordering::Release);
    let mut worker = Mp3RecordingWorker::new(sink, format(), consumer, 4096, 4096, 65536).unwrap();
    worker.start().unwrap();
    wait(|| worker.status() == S::Failed);
    assert!(matches!(
        worker.finish_stopped(&mut producer),
        Err(E::Poisoned)
    ));
    assert_eq!(worker.status(), S::Failed);
    assert_eq!(counts.flushes.load(Ordering::Relaxed), 0);
}

#[test]
fn post_acceptance_partial_output_capacity_and_rewrite_errors_preserve_original() {
    for mode in 0..3 {
        let (mut producer, consumer) = audio_ring(2, 32).unwrap();
        producer.push(&signal(17)).unwrap();
        let (sink, counts) = Sink::new();
        let mut worker = Mp3RecordingWorker::new(
            sink,
            format(),
            consumer,
            17,
            17,
            if mode == 1 { 1 } else { 65536 },
        )
        .unwrap();
        worker.start().unwrap();
        wait(|| worker.dequeued_frames() == 17 || worker.status() == S::Failed);
        if worker.status() != S::Failed {
            // Dequeue is diagnostic: a capacity fault can publish after the
            // count observation but before the pause acknowledgement.
            match worker.pause() {
                Ok(()) => {}
                Err(E::Poisoned | E::InvalidRecord) => assert_eq!(worker.status(), S::Failed),
                Err(error) => panic!("unexpected pause error: {error:?}"),
            }
        }
        if mode == 0 {
            *counts.byte_limit.lock().unwrap() = Some(worker.written_bytes() as usize + 5);
        }
        if mode == 2 {
            counts.rewrite_fail.store(true, Ordering::Release);
        }
        let exit = worker.finish_stopped(&mut producer).unwrap();
        match exit.completion() {
            C::Failed(E::Capacity) if mode == 1 => {}
            C::Failed(E::Io(error)) if mode == 0 => {
                assert_original(error, &counts);
                assert_eq!(error.to_string(), "original partial worker sink error")
            }
            C::Failed(E::Io(error)) if mode == 2 => {
                assert_original(error, &counts);
                assert_eq!(error.to_string(), "original metadata seek error")
            }
            _ => panic!("wrong original outcome"),
        }
        assert_eq!((exit.accepted_frames, exit.dequeued_frames), (17, 17));
        assert_eq!(
            exit.written_bytes,
            counts.physical.load(Ordering::Relaxed) as u64
        );
        assert_eq!(counts.flushes.load(Ordering::Relaxed), 0);
        let actions = (
            counts.writes.load(Ordering::Relaxed),
            counts.seeks.load(Ordering::Relaxed),
        );
        drop(worker);
        assert_eq!(
            actions,
            (
                counts.writes.load(Ordering::Relaxed),
                counts.seeks.load(Ordering::Relaxed)
            )
        );
    }
}
#[test]
fn total_capacity_failure_distinguishes_dequeued_and_accepted_frames_and_preserves_suffix() {
    let (mut producer, consumer) = audio_ring(2, 4).unwrap();
    producer.push(&[0.25, -0.25, 0.1, -0.1, 0.2, -0.2]).unwrap();
    let (sink, _) = Sink::new();
    let mut worker = Mp3RecordingWorker::new(sink, format(), consumer, 1, 1, 65536).unwrap();
    worker.start().unwrap();
    wait(|| worker.status() == S::Failed);
    let exit = worker.finish_stopped(&mut producer).unwrap();
    assert!(matches!(exit.completion(), C::Failed(E::Capacity)));
    assert_eq!((exit.accepted_frames, exit.dequeued_frames), (1, 2));
    assert_eq!(exit.consumer.queued_frames(), 1);
}

#[test]
fn empty_explicit_finish_returns_failed_untouched_sink_without_flush() {
    let (producer, consumer) = audio_ring(2, 4).unwrap();
    drop(producer);
    let (sink, counts) = Sink::new();
    let mut worker = Mp3RecordingWorker::new(sink, format(), consumer, 1, 4, 65536).unwrap();
    let exit = worker.finish_after_producer_retired().unwrap();
    assert!(matches!(exit.completion(), C::Failed(E::InvalidSamples)));
    assert_eq!((exit.accepted_frames, exit.dequeued_frames), (0, 0));
    assert_eq!(counts.flushes.load(Ordering::Relaxed), 0);
    assert_eq!(
        exit.written_bytes,
        counts.physical.load(Ordering::Relaxed) as u64
    );
    assert!(exit.destination.bytes.get_ref().is_empty());
    assert_eq!(worker.status(), S::Failed);
}

#[test]
fn final_metadata_rewrite_budget_counts_physical_bytes_and_retains_file_region() {
    let source = signal(17);
    let complete = ravet_audio_file::encode_mp3(format(), &source).unwrap();
    let (mut producer, consumer) = audio_ring(2, 32).unwrap();
    producer.push(&source).unwrap();
    drop(producer);
    let (sink, counts) = Sink::new();
    let mut worker =
        Mp3RecordingWorker::new(sink, format(), consumer, 17, 17, complete.len() as u64).unwrap();
    let exit = worker.finish_after_producer_retired().unwrap();
    assert!(matches!(exit.completion(), C::Failed(E::Capacity)));
    assert_eq!((exit.accepted_frames, exit.dequeued_frames), (17, 17));
    assert_eq!(exit.file_bytes, complete.len() as u64);
    assert_eq!(exit.written_bytes, exit.file_bytes);
    assert_eq!(counts.flushes.load(Ordering::Relaxed), 0);
}

#[test]
fn prefix_region_is_preserved_and_file_end_restored_by_codec_owning_worker() {
    let (mut producer, consumer) = audio_ring(2, 4).unwrap();
    producer.push(&signal(4)).unwrap();
    drop(producer);
    let (mut sink, counts) = Sink::new();
    let prefix = b"caller prefix";
    sink.bytes = Cursor::new(prefix.to_vec());
    sink.bytes.set_position(prefix.len() as u64);
    let mut worker = Mp3RecordingWorker::new(sink, format(), consumer, 3, 4, 65536).unwrap();
    let exit = worker.finish_after_producer_retired().unwrap();
    assert!(matches!(exit.completion(), C::Finalized));
    assert_eq!(&exit.destination.bytes.get_ref()[..prefix.len()], prefix);
    assert_eq!(
        exit.destination.bytes.position(),
        exit.destination.bytes.get_ref().len() as u64
    );
    assert_eq!(
        exit.file_bytes,
        exit.destination.bytes.get_ref().len() as u64 - prefix.len() as u64
    );
    assert_eq!(
        exit.written_bytes,
        counts.physical.load(Ordering::Relaxed) as u64
    );
    #[cfg(feature = "compressed")]
    assert_eq!(
        ravet_audio_file::decode_compressed(&exit.destination.bytes.get_ref()[prefix.len()..], 4)
            .unwrap()
            .frames(),
        4
    );
}
