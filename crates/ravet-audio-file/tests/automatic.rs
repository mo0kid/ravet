use ravet_audio_file::{
    AudioConsumer, AudioFileError as E, BufferedPlayback, ReadAheadProgress, ReadAheadPump,
    ReadAheadStatus as S, ReadAheadWorker, WaveEncoding, WaveFormat, WaveReadAhead, WaveReader,
    WaveWriter, audio_ring,
};
use std::{
    io::Cursor,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
type Pump = WaveReadAhead<Cursor<Vec<u8>>>;
fn fixture(frames: usize) -> (Pump, AudioConsumer, Vec<f32>) {
    let samples: Vec<f32> = (0..frames)
        .flat_map(|i| [i as f32 / 1000., -(i as f32) / 1000.])
        .collect();
    let mut writer = WaveWriter::new(
        Cursor::new(Vec::new()),
        WaveFormat {
            channels: 2,
            sample_rate: 48000,
            encoding: WaveEncoding::Float32,
        },
        65536,
    )
    .unwrap();
    writer.write_frames(&samples).unwrap();
    let bytes = writer.finish().unwrap();
    let reader = WaveReader::new(bytes, 11).unwrap();
    let (producer, consumer) = audio_ring(2, 4).unwrap();
    (
        WaveReadAhead::new(reader, producer, 11).unwrap(),
        consumer,
        samples,
    )
}
fn wait_for(mut condition: impl FnMut() -> bool) {
    let limit = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < limit, "worker progress deadline");
        std::thread::yield_now();
    }
}
fn drain(playback: &mut BufferedPlayback) -> Vec<f32> {
    let mut observed = Vec::new();
    let mut output = [99.; 6];
    let limit = Instant::now() + Duration::from_secs(10);
    loop {
        let progress = playback.render(&mut output).unwrap();
        observed.extend_from_slice(&output[..progress.source_frames * 2]);
        assert!(
            output[progress.source_frames * 2..]
                .iter()
                .all(|sample| *sample == 0.)
        );
        if progress.ended {
            break;
        }
        assert!(Instant::now() < limit, "bounded EOF deadline");
        std::thread::yield_now();
    }
    observed
}
#[test]
fn automatic_backpressure_pause_resume_eof_and_callback_heap_policy() {
    let (pump, consumer, expected) = fixture(127);
    let (mut worker, mut playback) = ReadAheadWorker::new(pump, consumer, 7).unwrap();
    assert_eq!(playback.worker_status(), S::Stopped);
    assert_eq!((playback.sample_rate(), playback.channels()), (48000, 2));
    worker.start().unwrap();
    wait_for(|| playback.queued_frames() == 4);
    assert!(matches!(worker.start(), Err(E::InvalidRecord)));
    let mut output = [99.; 6];
    let allocations = allocation_counter::measure(|| {
        for _ in 0..1000 {
            let progress = playback.render(&mut output).unwrap();
            assert_eq!(progress.source_frames, 0);
            assert_eq!(progress.underflow_frames, 0);
            assert_eq!(output, [0.; 6]);
        }
    });
    assert_eq!((allocations.count_total, allocations.count_current), (0, 0));
    assert_eq!(
        (playback.position_frames(), playback.queued_frames()),
        (0, 4)
    );
    playback.play();
    let allocations = allocation_counter::measure(|| {
        let progress = playback.render(&mut output).unwrap();
        assert_eq!(progress.source_frames, 3);
    });
    assert_eq!((allocations.count_total, allocations.count_current), (0, 0));
    assert_eq!(&output, &expected[..6]);
    playback.pause();
    let position = playback.position_frames();
    wait_for(|| playback.queued_frames() == 4);
    let mut invalid = [99.; 3];
    assert!(playback.render(&mut invalid).is_err());
    assert_eq!(invalid, [0.; 3]);
    assert!(playback.render(&mut [99.; 16]).is_err());
    assert_eq!(playback.position_frames(), position);
    assert_eq!(playback.queued_frames(), 4);
    assert!(!playback.is_playing());
    playback.render(&mut []).unwrap();
    assert!(!playback.is_playing());
    playback.play();
    let rest = drain(&mut playback);
    assert_eq!(rest, &expected[6..]);
    assert_eq!(playback.position_frames(), 127);
    assert!(!playback.is_playing());
    assert_eq!(playback.worker_status(), S::EndOfFile);
    let exit = worker.shutdown().unwrap();
    assert!(exit.error.is_none());
    assert_eq!(exit.pump.reader_position(), 127);
}
struct GatePump {
    inner: Pump,
    open: Arc<AtomicBool>,
}
impl ReadAheadPump for GatePump {
    fn pump(&mut self) -> Result<ReadAheadProgress, E> {
        if !self.open.load(Ordering::Acquire) {
            return Ok(ReadAheadProgress::default());
        }
        self.inner.pump()
    }
    fn owns(&self, consumer: &AudioConsumer) -> bool {
        ReadAheadPump::owns(&self.inner, consumer)
    }
    fn pending_frames(&self) -> usize {
        ReadAheadPump::pending_frames(&self.inner)
    }
    fn reader_position(&self) -> u64 {
        self.inner.reader_position()
    }
    fn sample_rate(&self) -> u32 {
        self.inner.format().sample_rate
    }
    fn seek_stopped(&mut self, consumer: &mut AudioConsumer, frame: u64) -> Result<(), E> {
        self.inner.seek_stopped(consumer, frame)
    }
}
#[test]
fn true_running_underrun_stalls_source_clock_and_retains_continuation() {
    let (inner, consumer, expected) = fixture(17);
    let open = Arc::new(AtomicBool::new(false));
    let (mut worker, mut playback) = ReadAheadWorker::new(
        GatePump {
            inner,
            open: Arc::clone(&open),
        },
        consumer,
        7,
    )
    .unwrap();
    worker.start().unwrap();
    playback.play();
    let mut output = [99.; 14];
    let progress = playback.render(&mut output).unwrap();
    assert_eq!(
        (
            progress.source_frames,
            progress.underflow_frames,
            progress.worker_status
        ),
        (0, 7, S::Running)
    );
    assert_eq!(output, [0.; 14]);
    assert_eq!(playback.position_frames(), 0);
    assert!(playback.is_playing());
    open.store(true, Ordering::Release);
    wait_for(|| playback.queued_frames() == 4);
    assert_eq!(drain(&mut playback), expected);
    assert_eq!(playback.position_frames(), 17);
    assert!(worker.shutdown().unwrap().error.is_none());
}
#[test]
fn matching_stopped_seek_preserves_failed_generations_and_control_shutdown() {
    let (pump, consumer, expected) = fixture(127);
    let (mut worker, mut playback) = ReadAheadWorker::new(pump, consumer, 7).unwrap();
    worker.start().unwrap();
    wait_for(|| playback.queued_frames() == 4);
    let (foreign_pump, foreign_consumer, _) = fixture(7);
    let (foreign_owner, mut foreign) =
        ReadAheadWorker::new(foreign_pump, foreign_consumer, 7).unwrap();
    assert!(matches!(
        worker.seek_stopped(&mut foreign, 3),
        Err(E::InvalidSamples)
    ));
    assert_eq!(foreign.position_frames(), 0);
    assert_eq!(playback.queued_frames(), 4);
    assert!(worker.seek_stopped(&mut playback, 128).is_err());
    assert_eq!(playback.position_frames(), 0);
    assert!(!playback.is_playing());
    playback.play();
    let mut output = [99.; 6];
    assert_eq!(playback.render(&mut output).unwrap().source_frames, 3);
    assert_eq!(&output, &expected[..6]);
    playback.pause();
    worker.seek_stopped(&mut playback, 123).unwrap();
    assert_eq!(playback.position_frames(), 123);
    assert!(!playback.is_playing());
    playback.play();
    assert_eq!(drain(&mut playback), &expected[246..]);
    worker.seek_stopped(&mut playback, 127).unwrap();
    wait_for(|| playback.worker_status() == S::EndOfFile);
    assert!(playback.render(&mut output).unwrap().ended);
    playback.play();
    worker.stop_stopped(&mut playback).unwrap();
    assert_eq!(playback.position_frames(), 0);
    assert!(!playback.is_playing());
    worker.seek_stopped(&mut playback, 1).unwrap();
    wait_for(|| playback.queued_frames() == 4);
    let exit = worker.shutdown().unwrap();
    assert!(exit.error.is_none());
    assert_eq!(playback.worker_status(), S::Stopped);
    assert_eq!(playback.position_frames(), 1);
    playback.play();
    let count = playback.render(&mut output).unwrap().source_frames;
    assert_eq!(count, 3);
    assert_eq!(&output, &expected[2..8]);
    assert!(!playback.render(&mut output).unwrap().ended);
    assert!(!playback.is_playing());
    assert!(foreign_owner.shutdown().unwrap().error.is_none());
}
struct FaultPump {
    inner: Pump,
    fail: Arc<AtomicBool>,
    calls: usize,
    panic: bool,
}
impl ReadAheadPump for FaultPump {
    fn pump(&mut self) -> Result<ReadAheadProgress, E> {
        if self.panic {
            panic!("synthetic worker panic");
        }
        self.calls += 1;
        if self.calls > 1 && self.fail.load(Ordering::Acquire) {
            return Err(E::InvalidRecord);
        }
        self.inner.pump()
    }
    fn owns(&self, consumer: &AudioConsumer) -> bool {
        ReadAheadPump::owns(&self.inner, consumer)
    }
    fn pending_frames(&self) -> usize {
        ReadAheadPump::pending_frames(&self.inner)
    }
    fn reader_position(&self) -> u64 {
        self.inner.reader_position()
    }
    fn sample_rate(&self) -> u32 {
        self.inner.format().sample_rate
    }
    fn seek_stopped(&mut self, consumer: &mut AudioConsumer, frame: u64) -> Result<(), E> {
        self.inner.seek_stopped(consumer, frame)?;
        self.calls = 0;
        Ok(())
    }
}
#[test]
fn pump_fault_retains_prefix_and_source_ownership_panic_cannot_report_eof() {
    let (inner, consumer, expected) = fixture(17);
    let fail = Arc::new(AtomicBool::new(true));
    let (mut worker, mut playback) = ReadAheadWorker::new(
        FaultPump {
            inner,
            fail,
            calls: 0,
            panic: false,
        },
        consumer,
        7,
    )
    .unwrap();
    worker.start().unwrap();
    wait_for(|| playback.worker_status() == S::Failed);
    playback.play();
    let mut output = [99.; 14];
    let progress = playback.render(&mut output).unwrap();
    assert_eq!(progress.source_frames, 4);
    assert_eq!(&output[..8], &expected[..8]);
    assert_eq!(&output[8..], &[0.; 6]);
    assert_eq!(progress.worker_status, S::Failed);
    assert!(!progress.ended);
    assert!(!playback.is_playing());
    let exit = worker.shutdown().unwrap();
    assert!(matches!(exit.error, Some(E::InvalidRecord)));
    assert_eq!(exit.pump.inner.reader_position(), 11);
    assert_eq!(ReadAheadPump::pending_frames(&exit.pump), 7);
    let (inner, consumer, _) = fixture(17);
    let (mut worker, mut playback) = ReadAheadWorker::new(
        FaultPump {
            inner,
            fail: Arc::new(AtomicBool::new(false)),
            calls: 0,
            panic: true,
        },
        consumer,
        7,
    )
    .unwrap();
    worker.start().unwrap();
    wait_for(|| playback.worker_status() == S::Failed);
    assert!(!playback.render(&mut output).unwrap().ended);
    assert!(matches!(worker.shutdown(), Err(E::Poisoned)));
}
#[cfg(feature = "compressed")]
#[test]
fn indexed_compressed_automatic_playback_and_stopped_recall_are_exact() {
    use ravet_audio_file::{CompressedReadAhead, CompressedReader, decode_compressed};
    let encoded = include_bytes!("fixtures/sine.flac");
    let expected = decode_compressed(encoded, 4096).unwrap();
    let reader = CompressedReader::new_indexed_flac(encoded, 4096).unwrap();
    let (producer, consumer) = audio_ring(2, 4).unwrap();
    let pump = CompressedReadAhead::new(reader, producer, 11).unwrap();
    let (mut worker, mut playback) = ReadAheadWorker::new(pump, consumer, 7).unwrap();
    worker.start().unwrap();
    playback.play();
    assert_eq!(drain(&mut playback), expected.samples());
    worker.seek_stopped(&mut playback, 1234).unwrap();
    assert_eq!(playback.position_frames(), 1234);
    playback.play();
    assert_eq!(drain(&mut playback), &expected.samples()[2468..]);
    assert!(worker.shutdown().unwrap().error.is_none());
}

#[test]
fn stopped_seek_recovers_worker_fault_and_foreign_generation_stays_untouched() {
    let (inner, consumer, expected) = fixture(17);
    let fail = Arc::new(AtomicBool::new(true));
    let (mut worker, mut playback) = ReadAheadWorker::new(
        FaultPump {
            inner,
            fail: Arc::clone(&fail),
            calls: 0,
            panic: false,
        },
        consumer,
        7,
    )
    .unwrap();
    worker.start().unwrap();
    wait_for(|| playback.worker_status() == S::Failed);
    assert!(worker.seek_stopped(&mut playback, 18).is_err());
    wait_for(|| playback.worker_status() == S::Failed);
    assert_eq!(
        (playback.position_frames(), playback.queued_frames()),
        (0, 4)
    );
    fail.store(false, Ordering::Release);
    worker.seek_stopped(&mut playback, 5).unwrap();
    assert_eq!(playback.position_frames(), 5);
    playback.play();
    assert_eq!(drain(&mut playback), &expected[10..]);
    assert!(worker.shutdown().unwrap().error.is_none());

    let (pump, consumer, _) = fixture(17);
    assert!(matches!(
        ReadAheadWorker::new(pump, consumer, 0),
        Err(E::Capacity)
    ));
    let (mut pump, mut matching, expected) = fixture(17);
    pump.pump().unwrap();
    let (_, foreign) = audio_ring(2, 4).unwrap();
    assert!(matches!(
        ReadAheadWorker::new(pump, foreign, 7),
        Err(E::InvalidSamples)
    ));
    let mut output = [99.; 8];
    assert_eq!(matching.pop(&mut output).unwrap(), 4);
    assert_eq!(&output, &expected[..8]);
    let (mut pump, consumer, _) = fixture(17);
    pump.pump().unwrap();
    assert!(matches!(
        ReadAheadWorker::new(pump, consumer, 7),
        Err(E::InvalidSamples)
    ));
}
#[test]
fn source_float_bits_survive_automatic_transport_without_normalization() {
    let samples = [
        f32::from_bits(0x7fc1_2345),
        -0.,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::from_bits(1),
        -f32::from_bits(1),
    ];
    let mut writer = WaveWriter::new(
        Cursor::new(Vec::new()),
        WaveFormat {
            channels: 2,
            sample_rate: 48000,
            encoding: WaveEncoding::Float32,
        },
        7,
    )
    .unwrap();
    writer.write_frames(&samples).unwrap();
    let reader = WaveReader::new(writer.finish().unwrap(), 7).unwrap();
    let (producer, consumer) = audio_ring(2, 4).unwrap();
    let pump = WaveReadAhead::new(reader, producer, 7).unwrap();
    let (mut worker, mut playback) = ReadAheadWorker::new(pump, consumer, 7).unwrap();
    worker.start().unwrap();
    wait_for(|| playback.worker_status() == S::EndOfFile);
    playback.play();
    let mut output = [99.; 14];
    let progress = playback.render(&mut output).unwrap();
    assert_eq!(progress.source_frames, 3);
    assert!(progress.ended);
    assert_eq!(
        output[..6]
            .iter()
            .map(|sample| sample.to_bits())
            .collect::<Vec<_>>(),
        samples
            .iter()
            .map(|sample| sample.to_bits())
            .collect::<Vec<_>>()
    );
    assert_eq!(&output[6..], &[0.; 8]);
    assert!(worker.shutdown().unwrap().error.is_none());
}
