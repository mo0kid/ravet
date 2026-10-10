use ravet_audio_file::{
    AiffEncoding as P, AiffFormat, AiffReadAhead, AiffReader, AiffRecorder, AiffWriter,
    AudioFileError as E, ReadAheadWorker, audio_ring,
};
use std::{
    io::{self, Cursor, Read, Seek, SeekFrom, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
fn format(channels: u16, encoding: P) -> AiffFormat {
    AiffFormat {
        channels,
        sample_rate: 48000,
        encoding,
    }
}
fn source(channels: usize, encoding: P) -> (Vec<u8>, Vec<f32>) {
    let samples: Vec<_> = (0..37 * channels)
        .map(|i| (i % 17) as f32 / 32. - 0.25)
        .collect();
    let mut writer = AiffWriter::new(
        Cursor::new(Vec::new()),
        format(channels as u16, encoding),
        64,
    )
    .unwrap();
    writer.write_frames(&samples).unwrap();
    (writer.finish().unwrap().into_inner(), samples)
}
#[test]
fn aiff_backpressure_failed_seek_generations_and_all_pcm_profiles_are_exact() {
    for channels in [1, 2, 8] {
        for encoding in [P::Pcm16, P::Pcm24, P::Pcm32] {
            let (bytes, expected) = source(channels, encoding);
            let reader = AiffReader::new(Cursor::new(bytes), 5).unwrap();
            let (producer, mut consumer) = audio_ring(channels, 2).unwrap();
            let mut pump = AiffReadAhead::new(reader, producer, 5).unwrap();
            assert_eq!(pump.format(), format(channels as u16, encoding));
            let first = pump.pump().unwrap();
            assert_eq!(
                (first.read_frames, first.queued_frames, first.pending_frames),
                (5, 2, 3)
            );
            assert_eq!(pump.pump().unwrap().pending_frames, 3);
            assert_eq!(pump.reader_position(), 5);
            let (_, mut foreign) = audio_ring(channels, 2).unwrap();
            assert!(matches!(
                pump.seek_stopped(&mut foreign, 10),
                Err(E::InvalidSamples)
            ));
            assert!(pump.seek_stopped(&mut consumer, 38).is_err());
            assert_eq!((pump.reader_position(), consumer.queued_frames()), (5, 2));
            let mut output = vec![99.; channels];
            assert_eq!(consumer.pop(&mut output).unwrap(), 1);
            assert_eq!(&output, &expected[..channels]);
            pump.seek_stopped(&mut consumer, 10).unwrap();
            assert_eq!(consumer.queued_frames(), 0);
            assert_eq!(pump.reader_position(), 10);
            let mut observed = Vec::new();
            loop {
                let progress = pump.pump().unwrap();
                let mut count = 0;
                let heap = allocation_counter::measure(|| {
                    count = consumer.pop(&mut output).unwrap();
                });
                assert_eq!((heap.count_total, heap.count_current), (0, 0));
                observed.extend_from_slice(&output[..count * channels]);
                if progress.end_of_file
                    && progress.pending_frames == 0
                    && consumer.queued_frames() == 0
                {
                    break;
                }
            }
            assert_eq!(observed, &expected[10 * channels..]);
            pump.seek_stopped(&mut consumer, 37).unwrap();
            assert!(pump.pump().unwrap().end_of_file);
            assert_eq!(consumer.queued_frames(), 0);
        }
    }
    let (bytes, _) = source(2, P::Pcm24);
    let (producer, _) = audio_ring(1, 2).unwrap();
    assert!(matches!(
        AiffReadAhead::new(
            AiffReader::new(Cursor::new(bytes.clone()), 5).unwrap(),
            producer,
            5
        ),
        Err(E::InvalidSamples)
    ));
    for capacity in [0, 6] {
        let (producer, _) = audio_ring(2, 2).unwrap();
        assert!(matches!(
            AiffReadAhead::new(
                AiffReader::new(Cursor::new(bytes.clone()), 5).unwrap(),
                producer,
                capacity
            ),
            Err(E::Capacity)
        ));
    }
}
struct FailRead {
    inner: Cursor<Vec<u8>>,
    fail: Arc<AtomicBool>,
}
impl Read for FailRead {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.fail.load(Ordering::Acquire) {
            return Err(io::Error::other("synthetic read fault"));
        }
        self.inner.read(output)
    }
}
impl Seek for FailRead {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.inner.seek(from)
    }
}
#[test]
fn aiff_reader_io_failure_preserves_generation_and_explicit_retry() {
    let (bytes, expected) = source(2, P::Pcm24);
    let fail = Arc::new(AtomicBool::new(false));
    let reader = AiffReader::new(
        FailRead {
            inner: Cursor::new(bytes),
            fail: Arc::clone(&fail),
        },
        5,
    )
    .unwrap();
    let (producer, mut consumer) = audio_ring(2, 2).unwrap();
    let mut pump = AiffReadAhead::new(reader, producer, 5).unwrap();
    fail.store(true, Ordering::Release);
    assert!(matches!(pump.pump(), Err(E::Io(_))));
    assert_eq!((pump.reader_position(), consumer.queued_frames()), (0, 0));
    fail.store(false, Ordering::Release);
    assert_eq!(pump.pump().unwrap().read_frames, 5);
    let mut output = [99.; 4];
    assert_eq!(consumer.pop(&mut output).unwrap(), 2);
    assert_eq!(&output, &expected[..4]);
}
#[test]
fn aiff_recorder_bounded_final_drain_preserves_integer_samples_and_odd_padding() {
    for channels in [1, 2, 8] {
        for encoding in [P::Pcm16, P::Pcm24, P::Pcm32] {
            let (mut producer, consumer) = audio_ring(channels, 8).unwrap();
            let mut recorder = AiffRecorder::new(
                Cursor::new(Vec::new()),
                format(channels as u16, encoding),
                2,
                consumer,
            )
            .unwrap();
            assert_eq!(recorder.pump().unwrap(), 0);
            let samples: Vec<_> = (0..5 * channels).map(|i| i as f32 / 64. - 0.25).collect();
            assert_eq!(producer.push(&samples).unwrap(), 5);
            assert_eq!(recorder.pump().unwrap(), 2);
            assert_eq!(recorder.frames(), 2);
            drop(producer);
            let file = recorder.finish().unwrap();
            let bytes = file.into_inner();
            assert_eq!(bytes.len() % 2, 0); // mono PCM24 has an odd data payload
            let mut reader = AiffReader::new(Cursor::new(bytes), 8).unwrap();
            assert_eq!(reader.frames(), 5);
            let mut output = vec![99.; channels * 8];
            assert_eq!(reader.read_frames(&mut output).unwrap(), 5);
            assert_eq!(&output[..channels * 5], samples.as_slice());
            assert!(output[channels * 5..].iter().all(|sample| *sample == 99.));
        }
    }
}
struct FailWrite {
    inner: Cursor<Vec<u8>>,
    budget: usize,
}
impl Write for FailWrite {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        if self.budget == 0 {
            return Err(io::Error::other("synthetic partial write fault"));
        }
        let count = input.len().min(self.budget);
        self.budget -= count;
        self.inner.write(&input[..count])
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Seek for FailWrite {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.inner.seek(from)
    }
}
#[test]
fn aiff_dequeued_pcm_and_partial_writer_failures_poison_future_pumps_and_finish() {
    let (mut producer, consumer) = audio_ring(2, 4).unwrap();
    let mut recorder = AiffRecorder::new(
        FailWrite {
            inner: Cursor::new(Vec::new()),
            budget: 56,
        },
        format(2, P::Pcm24),
        2,
        consumer,
    )
    .unwrap();
    producer.push(&[0.25, -0.25, 0.5, -0.5]).unwrap();
    assert!(matches!(recorder.pump(), Err(E::Io(_))));
    assert_eq!(recorder.frames(), 0);
    assert!(matches!(recorder.pump(), Err(E::Poisoned)));
    drop(producer);
    assert!(matches!(recorder.finish(), Err(E::Poisoned)));
    let (mut producer, consumer) = audio_ring(2, 4).unwrap();
    let mut recorder =
        AiffRecorder::new(Cursor::new(Vec::new()), format(2, P::Pcm24), 2, consumer).unwrap();
    producer.push(&[f32::NAN, 0.]).unwrap();
    assert!(matches!(recorder.pump(), Err(E::InvalidSamples)));
    assert!(matches!(recorder.pump(), Err(E::Poisoned)));
    drop(producer);
    assert!(matches!(recorder.finish(), Err(E::Poisoned)));
    let (_, consumer) = audio_ring(1, 4).unwrap();
    assert!(matches!(
        AiffRecorder::new(Cursor::new(Vec::new()), format(2, P::Pcm24), 2, consumer),
        Err(E::InvalidSamples)
    ));
}
#[test]
fn automatic_aiff_playback_and_stopped_recall_use_shared_transport_contract() {
    let (bytes, expected) = source(2, P::Pcm24);
    let reader = AiffReader::new(Cursor::new(bytes), 5).unwrap();
    let (producer, consumer) = audio_ring(2, 4).unwrap();
    let pump = AiffReadAhead::new(reader, producer, 5).unwrap();
    let (mut worker, mut playback) = ReadAheadWorker::new(pump, consumer, 3).unwrap();
    worker.start().unwrap();
    playback.play();
    let limit = Instant::now() + Duration::from_secs(10);
    let mut observed = Vec::new();
    let mut output = [99.; 6];
    loop {
        let progress = playback.render(&mut output).unwrap();
        observed.extend_from_slice(&output[..progress.source_frames * 2]);
        if progress.ended {
            break;
        }
        assert!(Instant::now() < limit);
        std::thread::yield_now();
    }
    assert_eq!(observed, expected);
    worker.seek_stopped(&mut playback, 31).unwrap();
    assert_eq!(playback.position_frames(), 31);
    playback.play();
    observed.clear();
    loop {
        let progress = playback.render(&mut output).unwrap();
        observed.extend_from_slice(&output[..progress.source_frames * 2]);
        if progress.ended {
            break;
        }
        assert!(Instant::now() < limit);
        std::thread::yield_now();
    }
    assert_eq!(observed, &expected[62..]);
    assert!(worker.shutdown().unwrap().error.is_none());
}
