#![cfg(feature = "vorbis-encode")]
use ravet_audio_file::{
    AudioFileError as E, VorbisFormat, VorbisRecordingCompletion as C, VorbisRecordingStatus as S,
    VorbisRecordingWorker, audio_ring,
};
use std::{
    io::{self, Write},
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
}
struct Sink {
    bytes: Vec<u8>,
    counts: Arc<Counts>,
}
impl Sink {
    fn new() -> (Self, Arc<Counts>) {
        let counts = Arc::new(Counts::default());
        (
            Self {
                bytes: Vec::new(),
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
            return Err(io::Error::other("original worker sink error"));
        }
        let limit = *self.counts.byte_limit.lock().unwrap();
        let count = limit.map_or(bytes.len(), |limit| {
            bytes.len().min(limit.saturating_sub(self.bytes.len()))
        });
        if count == 0 && !bytes.is_empty() {
            return Err(io::Error::other("original partial worker sink error"));
        }
        self.bytes.extend_from_slice(&bytes[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.counts.flushes.fetch_add(1, Ordering::Relaxed);
        self.counts
            .threads
            .lock()
            .unwrap()
            .push(thread::current().id());
        if self.counts.flush_fail.load(Ordering::Acquire) {
            return Err(io::Error::other("original worker flush error"));
        }
        Ok(())
    }
}
fn format() -> VorbisFormat {
    VorbisFormat {
        channels: 2,
        sample_rate: 48000,
        quality: 1.,
        stream_serial: 731,
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
fn final_granule(bytes: &[u8]) -> u64 {
    let mut offset = 0;
    let mut last = 0;
    while offset < bytes.len() {
        let page = &bytes[offset..];
        assert_eq!(&page[..4], b"OggS");
        let segments = usize::from(page[26]);
        let size = 27
            + segments
            + page[27..27 + segments]
                .iter()
                .map(|x| usize::from(*x))
                .sum::<usize>();
        if page[5] & 4 != 0 {
            assert_eq!(offset + size, bytes.len());
            last = u64::from_le_bytes(page[6..14].try_into().unwrap());
        }
        offset += size;
    }
    last
}
#[test]
fn prepared_abort_shape_rejection_and_send_controller_do_not_create_codec_output() {
    fn assert_send<T: Send>() {}
    assert_send::<VorbisRecordingWorker<Sink>>();
    let (mut producer, consumer) = audio_ring(2, 4).unwrap();
    let (sink, counts) = Sink::new();
    producer.push(&[0.25, -0.25]).unwrap();
    let worker = VorbisRecordingWorker::new(sink, format(), consumer, 2, 4096, 65536).unwrap();
    assert_eq!(worker.status(), S::Prepared);
    let exit = worker.abort().unwrap();
    assert!(matches!(exit.completion(), C::Aborted));
    assert_eq!(exit.consumer.queued_frames(), 1);
    assert!(exit.destination.bytes.is_empty());
    assert_eq!(counts.writes.load(Ordering::Relaxed), 0);
    let (_, consumer) = audio_ring(1, 4).unwrap();
    let (sink, counts) = Sink::new();
    assert!(matches!(
        VorbisRecordingWorker::new(sink, format(), consumer, 2, 4096, 65536),
        Err(E::InvalidSamples)
    ));
    assert_eq!(counts.writes.load(Ordering::Relaxed), 0);
}
#[test]
fn concurrent_bounded_capture_uses_only_codec_owning_worker_and_finalizes_exact_timing() {
    let (mut producer, consumer) = audio_ring(2, 64).unwrap();
    let (sink, counts) = Sink::new();
    let mut worker = VorbisRecordingWorker::new(sink, format(), consumer, 17, 8193, 65536).unwrap();
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
    assert_eq!(exit.written_bytes, exit.destination.bytes.len() as u64);
    assert_eq!(exit.consumer.queued_frames(), 0);
    assert_eq!(final_granule(&exit.destination.bytes), 8193);
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
        let decoded = ravet_audio_file::decode_compressed(&exit.destination.bytes, 8193).unwrap();
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
        assert!(10. * (energy / noise).log10() > 25.);
    }
    drop(expected);
}
#[test]
fn acknowledged_pause_preserves_queue_and_failed_proofs_then_resumes_and_drains_tail() {
    let (mut producer, consumer) = audio_ring(2, 4).unwrap();
    let (sink, _) = Sink::new();
    let mut worker = VorbisRecordingWorker::new(sink, format(), consumer, 1, 4096, 65536).unwrap();
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
    assert_eq!(final_granule(&exit.destination.bytes), 6);
}
#[test]
fn never_started_retired_tail_stays_bounded_and_starts_codec_on_worker() {
    let (mut producer, consumer) = audio_ring(2, 4).unwrap();
    producer
        .push(&[0.25, -0.25, 0.5, -0.5, 0., 0., -0.25, 0.25])
        .unwrap();
    drop(producer);
    let (sink, counts) = Sink::new();
    let mut worker = VorbisRecordingWorker::new(sink, format(), consumer, 3, 4, 65536).unwrap();
    let exit = worker.finish_after_producer_retired().unwrap();
    assert!(matches!(exit.completion(), C::Finalized));
    assert_eq!((exit.accepted_frames, exit.dequeued_frames), (4, 4));
    assert_eq!(final_granule(&exit.destination.bytes), 4);
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
fn startup_sink_error_and_capacity_recover_original_sink_and_queued_source() {
    for capacity in [false, true] {
        let (mut producer, consumer) = audio_ring(2, 4).unwrap();
        producer.push(&[0.25, -0.25]).unwrap();
        let (sink, counts) = Sink::new();
        counts.fail.store(!capacity, Ordering::Release);
        let mut worker = VorbisRecordingWorker::new(
            sink,
            format(),
            consumer,
            2,
            4096,
            if capacity { 1 } else { 65536 },
        )
        .unwrap();
        worker.start().unwrap();
        wait(|| worker.status() == S::Failed);
        let writes = counts.writes.load(Ordering::Relaxed);
        let exit = worker.finish_stopped(&mut producer).unwrap();
        match exit.completion() {
            C::Failed(E::Io(error)) if !capacity => {
                assert_eq!(error.to_string(), "original worker sink error")
            }
            C::Failed(E::Capacity) if capacity => {}
            _ => panic!("wrong original startup outcome"),
        };
        assert_eq!(exit.consumer.queued_frames(), 1);
        assert_eq!((exit.accepted_frames, exit.dequeued_frames), (0, 0));
        assert_eq!(counts.writes.load(Ordering::Relaxed), writes);
        assert_eq!(counts.flushes.load(Ordering::Relaxed), 0);
        assert_eq!(exit.written_bytes, exit.destination.bytes.len() as u64);
        assert!(matches!(
            worker.finish_stopped(&mut producer),
            Err(E::Poisoned)
        ));
    }
}
#[test]
fn invalid_dequeued_batch_stops_pumping_and_returns_original_error_with_queued_suffix() {
    let (mut producer, consumer) = audio_ring(2, 4).unwrap();
    producer
        .push(&[f32::NAN, 0.25, 0.1, -0.1, 0.2, -0.2])
        .unwrap();
    let (sink, counts) = Sink::new();
    let mut worker = VorbisRecordingWorker::new(sink, format(), consumer, 1, 4096, 65536).unwrap();
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
    let mut worker = VorbisRecordingWorker::new(sink, format(), consumer, 1, 4, 65536).unwrap();
    let exit = worker.finish_stopped(&mut producer).unwrap();
    assert!(
        matches!(exit.completion(),C::Failed(E::Io(error)) if error.to_string()=="original worker flush error")
    );
    assert_eq!(counts.flushes.load(Ordering::Relaxed), 1);
    assert_eq!(exit.written_bytes, exit.destination.bytes.len() as u64);
    assert_eq!(worker.status(), S::Failed);
    for drop_only in [false, true] {
        let (mut producer, consumer) = audio_ring(2, 4).unwrap();
        let (sink, counts) = Sink::new();
        let mut worker = VorbisRecordingWorker::new(sink, format(), consumer, 1, 4, 65536).unwrap();
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
    let (mut producer, consumer) = audio_ring(2, 4).unwrap();
    let (sink, counts) = Sink::new();
    counts.panic.store(true, Ordering::Release);
    let mut worker = VorbisRecordingWorker::new(sink, format(), consumer, 1, 4, 65536).unwrap();
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
fn partial_header_and_post_acceptance_io_or_output_capacity_recover_exact_original_prefix() {
    let (mut producer, consumer) = audio_ring(2, 4).unwrap();
    producer.push(&[0.25, -0.25]).unwrap();
    let (sink, counts) = Sink::new();
    *counts.byte_limit.lock().unwrap() = Some(5);
    let mut worker = VorbisRecordingWorker::new(sink, format(), consumer, 1, 4, 65536).unwrap();
    worker.start().unwrap();
    wait(|| worker.status() == S::Failed);
    let exit = worker.finish_stopped(&mut producer).unwrap();
    assert!(
        matches!(exit.completion(),C::Failed(E::Io(error)) if error.to_string()=="original partial worker sink error")
    );
    assert_eq!(exit.written_bytes, 5);
    assert_eq!(exit.destination.bytes.len(), 5);
    assert_eq!(exit.consumer.queued_frames(), 1);
    assert_eq!(counts.flushes.load(Ordering::Relaxed), 0);
    assert_eq!(counts.writes.load(Ordering::Relaxed), 2);
    for capacity in [false, true] {
        let header_bytes = ravet_audio_file::VorbisWriter::new(Vec::new(), format(), 17, 65536)
            .unwrap()
            .written_bytes();
        let (mut producer, consumer) = audio_ring(2, 32).unwrap();
        producer.push(&signal(17)).unwrap();
        let (sink, counts) = Sink::new();
        let mut worker = VorbisRecordingWorker::new(
            sink,
            format(),
            consumer,
            17,
            17,
            if capacity { header_bytes + 1 } else { 65536 },
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
        if !capacity {
            *counts.byte_limit.lock().unwrap() = Some(worker.written_bytes() as usize + 5);
        }
        let exit = worker.finish_stopped(&mut producer).unwrap();
        match exit.completion() {
            C::Failed(E::Capacity) if capacity => {}
            C::Failed(E::Io(error)) if !capacity => {
                assert_eq!(error.to_string(), "original partial worker sink error")
            }
            _ => panic!("wrong post-acceptance failure"),
        };
        assert_eq!(exit.accepted_frames, 17);
        assert_eq!(exit.dequeued_frames, 17);
        assert_eq!(exit.written_bytes, exit.destination.bytes.len() as u64);
        assert_eq!(counts.flushes.load(Ordering::Relaxed), 0);
        let writes = counts.writes.load(Ordering::Relaxed);
        drop(worker);
        assert_eq!(counts.writes.load(Ordering::Relaxed), writes);
    }
}
#[test]
fn total_capacity_failure_distinguishes_dequeued_and_accepted_frames_and_preserves_suffix() {
    let (mut producer, consumer) = audio_ring(2, 4).unwrap();
    producer.push(&[0.25, -0.25, 0.1, -0.1, 0.2, -0.2]).unwrap();
    let (sink, _) = Sink::new();
    let mut worker = VorbisRecordingWorker::new(sink, format(), consumer, 1, 1, 65536).unwrap();
    worker.start().unwrap();
    wait(|| worker.status() == S::Failed);
    let exit = worker.finish_stopped(&mut producer).unwrap();
    assert!(matches!(exit.completion(), C::Failed(E::Capacity)));
    assert_eq!((exit.accepted_frames, exit.dequeued_frames), (1, 2));
    assert_eq!(exit.consumer.queued_frames(), 1);
}

#[test]
fn empty_explicit_finish_returns_failed_owned_prefix_without_eos_or_flush() {
    let (producer, consumer) = audio_ring(2, 4).unwrap();
    drop(producer);
    let (sink, counts) = Sink::new();
    let mut worker = VorbisRecordingWorker::new(sink, format(), consumer, 1, 4, 65536).unwrap();
    let exit = worker.finish_after_producer_retired().unwrap();
    assert!(matches!(exit.completion(), C::Failed(E::InvalidSamples)));
    assert_eq!((exit.accepted_frames, exit.dequeued_frames), (0, 0));
    assert_eq!(counts.flushes.load(Ordering::Relaxed), 0);
    assert_eq!(exit.written_bytes, exit.destination.bytes.len() as u64);
    assert!(!exit.destination.bytes.is_empty());
    assert_eq!(worker.status(), S::Failed);
}
