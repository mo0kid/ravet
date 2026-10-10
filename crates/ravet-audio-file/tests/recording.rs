use ravet_audio_file::{
    AudioFileError as E, RecordingPump, RecordingStatus as S, RecordingWorker, WaveEncoding,
    WaveFormat, WaveReader, WaveRecorder, audio_ring,
};
use std::{
    io::{self, Cursor, Seek, SeekFrom, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
fn format() -> WaveFormat {
    WaveFormat {
        channels: 1,
        sample_rate: 48000,
        encoding: WaveEncoding::Pcm16,
    }
}
fn wait_for(mut condition: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < end, "bounded worker diagnostic deadline");
        std::thread::yield_now();
    }
}
#[test]
fn concurrent_capture_prefixes_restart_stopped_drain_and_successful_finalization() {
    let samples: Vec<_> = (0..251).map(|i| (i % 33) as f32 / 64. - 0.25).collect();
    let (mut producer, consumer) = audio_ring(1, 8).unwrap();
    let recorder = WaveRecorder::new(Cursor::new(Vec::new()), format(), 3, consumer).unwrap();
    let mut worker = RecordingWorker::new(recorder, &producer).unwrap();
    assert_eq!(worker.status(), S::Stopped);
    // Before worker start, full-ring overload is the ordinary accepted prefix.
    assert_eq!(producer.push(&samples[..11]).unwrap(), 8);
    assert_eq!(producer.push(&samples[8..11]).unwrap(), 0);
    worker.start().unwrap();
    assert!(matches!(worker.start(), Err(E::InvalidRecord)));
    let mut offset = 8;
    let end = Instant::now() + Duration::from_secs(10);
    while offset < samples.len() - 3 {
        let last = (offset + 17).min(samples.len() - 3);
        let mut count = 0;
        let heap = allocation_counter::measure(|| {
            count = producer.push(&samples[offset..last]).unwrap();
        });
        assert_eq!((heap.count_total, heap.count_current), (0, 0));
        offset += count;
        assert!(Instant::now() < end);
        std::thread::yield_now();
    }
    worker.stop().unwrap();
    assert_eq!(worker.status(), S::Stopped);
    worker.start().unwrap();
    worker.stop().unwrap();
    // Resume until the last accepted prefix is captured, then reclaim for the
    // recorder's bounded final drain rather than dropping a capture suffix.
    worker.start().unwrap();
    let mut tail = samples.len() - 3;
    while tail < samples.len() {
        tail += producer.push(&samples[tail..]).unwrap();
        assert!(Instant::now() < end);
        std::thread::yield_now();
    }
    worker.stop().unwrap();
    let (mut foreign, _) = audio_ring(1, 8).unwrap();
    assert!(matches!(
        worker.finish_stopped(&mut foreign),
        Err(E::InvalidSamples)
    ));
    assert_eq!(worker.status(), S::Stopped);
    let file = worker.finish_stopped(&mut producer).unwrap();
    assert_eq!(worker.status(), S::Finalized);
    assert!(matches!(worker.start(), Err(E::Poisoned)));
    assert!(matches!(
        worker.finish_stopped(&mut producer),
        Err(E::Poisoned)
    ));
    let mut reader = WaveReader::new(file, 512).unwrap();
    assert_eq!(reader.frames(), samples.len() as u64);
    let mut output = vec![99.; 512];
    assert_eq!(reader.read_frames(&mut output).unwrap(), samples.len());
    assert_eq!(&output[..samples.len()], samples.as_slice());
    assert!(output[samples.len()..].iter().all(|sample| *sample == 99.));
}
#[derive(Debug)]
struct FaultSink {
    inner: Cursor<Vec<u8>>,
    budget: usize,
    flush_fails: Arc<AtomicBool>,
}
impl Write for FaultSink {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        if self.budget == 0 {
            return Err(io::Error::other("original recording write failure"));
        }
        let count = input.len().min(self.budget);
        self.budget -= count;
        self.inner.write(&input[..count])
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.flush_fails.load(Ordering::Acquire) {
            return Err(io::Error::other("original final flush failure"));
        }
        Ok(())
    }
}
impl Seek for FaultSink {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.inner.seek(from)
    }
}
#[test]
fn original_pump_errors_reclaim_owned_recorder_and_finalization_failure_is_terminal() {
    for report_with_finish in [false, true] {
        let (mut producer, consumer) = audio_ring(1, 4).unwrap();
        let recorder = WaveRecorder::new(
            FaultSink {
                inner: Cursor::new(Vec::new()),
                budget: 45,
                flush_fails: Arc::new(AtomicBool::new(false)),
            },
            format(),
            2,
            consumer,
        )
        .unwrap();
        let mut worker = RecordingWorker::new(recorder, &producer).unwrap();
        worker.start().unwrap();
        producer.push(&[0.25, -0.25]).unwrap();
        wait_for(|| worker.status() == S::Failed);
        if report_with_finish {
            let error = worker.finish_stopped(&mut producer).unwrap_err();
            assert!(
                matches!(error, E::Io(ref io) if io.to_string() == "original recording write failure")
            );
        }
        let exit = worker.shutdown().unwrap();
        assert_eq!(exit.recorder.frames(), 0);
        if report_with_finish {
            assert!(exit.error.is_none());
        } else {
            assert!(
                matches!(exit.error, Some(E::Io(ref io)) if io.to_string() == "original recording write failure")
            );
        }
        assert!(matches!(exit.recorder.finish(), Err(E::Poisoned)));
    }
    let (mut producer, consumer) = audio_ring(1, 4).unwrap();
    let flush_fails = Arc::new(AtomicBool::new(false));
    let recorder = WaveRecorder::new(
        FaultSink {
            inner: Cursor::new(Vec::new()),
            budget: 1024,
            flush_fails: Arc::clone(&flush_fails),
        },
        format(),
        2,
        consumer,
    )
    .unwrap();
    let mut worker = RecordingWorker::new(recorder, &producer).unwrap();
    producer.push(&[0.25]).unwrap();
    flush_fails.store(true, Ordering::Release);
    assert!(
        matches!(worker.finish_stopped(&mut producer), Err(E::Io(ref io)) if io.to_string() == "original final flush failure")
    );
    assert_eq!(worker.status(), S::Failed);
    assert!(matches!(worker.start(), Err(E::Poisoned)));
    assert!(matches!(worker.shutdown(), Err(E::Poisoned)));
}
struct PanicRecorder(WaveRecorder<Cursor<Vec<u8>>>);
impl RecordingPump for PanicRecorder {
    type Destination = Cursor<Vec<u8>>;
    fn consumer(&self) -> &ravet_audio_file::AudioConsumer {
        RecordingPump::consumer(&self.0)
    }
    fn owns(&self, producer: &ravet_audio_file::AudioProducer) -> bool {
        RecordingPump::owns(&self.0, producer)
    }
    fn pump(&mut self) -> Result<usize, E> {
        panic!("synthetic recorder worker panic");
    }
    fn frames(&self) -> u64 {
        self.0.frames()
    }
    fn finish(self) -> Result<Self::Destination, E> {
        self.0.finish()
    }
}
#[test]
fn matching_identity_and_panic_are_distinct_from_finalized_success() {
    let (mut producer, consumer) = audio_ring(1, 4).unwrap();
    let recorder = WaveRecorder::new(Cursor::new(Vec::new()), format(), 2, consumer).unwrap();
    let (foreign, _) = audio_ring(1, 4).unwrap();
    assert!(matches!(
        RecordingWorker::new(recorder, &foreign),
        Err(E::InvalidSamples)
    ));
    let (_, consumer) = audio_ring(1, 4).unwrap();
    let recorder = WaveRecorder::new(Cursor::new(Vec::new()), format(), 2, consumer).unwrap();
    // Its consumer is foreign to the surviving producer too.
    assert!(matches!(
        RecordingWorker::new(recorder, &producer),
        Err(E::InvalidSamples)
    ));
    let (producer2, consumer) = audio_ring(1, 4).unwrap();
    producer = producer2;
    let recorder = WaveRecorder::new(Cursor::new(Vec::new()), format(), 2, consumer).unwrap();
    let mut worker = RecordingWorker::new(PanicRecorder(recorder), &producer).unwrap();
    worker.start().unwrap();
    wait_for(|| worker.status() == S::Failed);
    assert!(matches!(
        worker.finish_stopped(&mut producer),
        Err(E::Poisoned)
    ));
    assert!(matches!(worker.shutdown(), Err(E::Poisoned)));
}
#[test]
fn aiff_automatic_recording_finalizes_odd_pcm24_tail_and_pcm_fault_is_original() {
    use ravet_audio_file::{AiffEncoding, AiffFormat, AiffReader, AiffRecorder};
    let (mut producer, consumer) = audio_ring(1, 8).unwrap();
    let format = AiffFormat {
        channels: 1,
        sample_rate: 48000,
        encoding: AiffEncoding::Pcm24,
    };
    let recorder = AiffRecorder::new(Cursor::new(Vec::new()), format, 2, consumer).unwrap();
    let mut worker = RecordingWorker::new(recorder, &producer).unwrap();
    worker.start().unwrap();
    producer.push(&[-0.5, 0., 0.5, 0.25, -0.25]).unwrap();
    let file = worker.finish_stopped(&mut producer).unwrap();
    assert_eq!(file.get_ref().len() % 2, 0);
    let mut reader = AiffReader::new(file, 8).unwrap();
    let mut output = [99.; 8];
    assert_eq!(reader.read_frames(&mut output).unwrap(), 5);
    assert_eq!(&output[..5], &[-0.5, 0., 0.5, 0.25, -0.25]);
    let (mut producer, consumer) = audio_ring(1, 4).unwrap();
    let recorder = AiffRecorder::new(Cursor::new(Vec::new()), format, 2, consumer).unwrap();
    let mut worker = RecordingWorker::new(recorder, &producer).unwrap();
    worker.start().unwrap();
    producer.push(&[f32::NAN]).unwrap();
    wait_for(|| worker.status() == S::Failed);
    let exit = worker.shutdown().unwrap();
    assert!(matches!(exit.error, Some(E::InvalidSamples)));
    assert_eq!(exit.recorder.frames(), 0);
    assert!(matches!(exit.recorder.finish(), Err(E::Poisoned)));
}
#[cfg(all(feature = "flac-encode", feature = "compressed"))]
#[test]
fn flac_capture_policy_total_budget_and_exact_automatic_finalization() {
    use ravet_audio_file::{
        FlacCapturePolicy as Policy, FlacFormat, FlacPcmEncoding, FlacRecorder, decode_compressed,
    };
    let format = FlacFormat {
        channels: 1,
        sample_rate: 48000,
        encoding: FlacPcmEncoding::Pcm16,
    };
    let (mut producer, consumer) = audio_ring(1, 8).unwrap();
    let recorder = FlacRecorder::new(
        Cursor::new(Vec::new()),
        format,
        2,
        16,
        65536,
        Policy::ClipOverload,
        consumer,
    )
    .unwrap();
    let mut worker = RecordingWorker::new(recorder, &producer).unwrap();
    worker.start().unwrap();
    producer.push(&[-2., -0.5, 0., 0.5, 2.]).unwrap();
    let bytes = worker.finish_stopped(&mut producer).unwrap().into_inner();
    assert_eq!(
        decode_compressed(&bytes, 16).unwrap().samples(),
        &[-1., -0.5, 0., 0.5, 32767. / 32768.]
    );
    for (policy, samples, total, expected_capacity) in [
        (Policy::RejectOverload, [1.01, 0.], 16, false),
        (Policy::ClipOverload, [f32::INFINITY, 0.], 16, false),
        (Policy::RejectOverload, [0.25, 0.5], 1, true),
    ] {
        let (mut producer, consumer) = audio_ring(1, 4).unwrap();
        let recorder = FlacRecorder::new(
            Cursor::new(Vec::new()),
            format,
            2,
            total,
            65536,
            policy,
            consumer,
        )
        .unwrap();
        let mut worker = RecordingWorker::new(recorder, &producer).unwrap();
        producer.push(&samples).unwrap();
        worker.start().unwrap();
        wait_for(|| worker.status() == S::Failed);
        let exit = worker.shutdown().unwrap();
        if expected_capacity {
            assert!(matches!(exit.error, Some(E::Capacity)));
        } else {
            assert!(matches!(exit.error, Some(E::InvalidSamples)));
        }
        assert_eq!(exit.recorder.frames(), 0);
        assert!(exit.recorder.is_poisoned());
    }
}

#[test]
fn permanent_producer_retirement_allows_bounded_finish_and_preserves_live_worker() {
    let (mut producer, consumer) = audio_ring(1, 8).unwrap();
    assert!(!consumer.producer_retired());
    let recorder = WaveRecorder::new(Cursor::new(Vec::new()), format(), 2, consumer).unwrap();
    let mut worker = RecordingWorker::from_recorder(recorder);
    worker.start().unwrap();
    assert!(matches!(
        worker.finish_after_producer_retired(),
        Err(E::InvalidRecord)
    ));
    assert_eq!(worker.status(), S::Running);
    producer.push(&[0.25, -0.25, 0.5, -0.5, 0.]).unwrap();
    drop(producer);
    let file = worker.finish_after_producer_retired().unwrap();
    assert_eq!(worker.status(), S::Finalized);
    let mut reader = WaveReader::new(file, 8).unwrap();
    let mut output = [99.; 8];
    assert_eq!(reader.read_frames(&mut output).unwrap(), 5);
    assert_eq!(&output[..5], &[0.25, -0.25, 0.5, -0.5, 0.]);
    assert_eq!(&output[5..], &[99.; 3]);

    let (mut producer, mut consumer) = audio_ring(1, 4).unwrap();
    let thread = std::thread::spawn(move || {
        producer.push(&[0.25, -0.5]).unwrap();
    });
    thread.join().unwrap();
    assert!(consumer.producer_retired());
    let mut output = [99.; 4];
    assert_eq!(consumer.pop(&mut output).unwrap(), 2);
    assert_eq!(output, [0.25, -0.5, 99., 99.]);
    assert!(consumer.producer_retired());
}
