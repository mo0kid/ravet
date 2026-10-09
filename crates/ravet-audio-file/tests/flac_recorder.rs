#![cfg(all(feature = "flac-encode", feature = "compressed"))]
use ravet_audio_file::{
    AudioFileError as E, FlacCapturePolicy as Policy, FlacFormat, FlacPcmEncoding as P,
    FlacRecorder, audio_ring, decode_compressed,
};
use std::io::{self, Cursor, Seek, SeekFrom, Write};
fn format(encoding: P) -> FlacFormat {
    FlacFormat {
        channels: 1,
        sample_rate: 48000,
        encoding,
    }
}
#[test]
fn explicit_endpoint_tie_and_overload_quantization_with_stopped_tail_drain() {
    for precision in [P::Pcm16, P::Pcm24] {
        let scale = (1i32 << (precision.bits() - 1)) as f32;
        let (mut producer, consumer) = audio_ring(1, 16).unwrap();
        let mut recorder = FlacRecorder::new(
            Cursor::new(Vec::new()),
            format(precision),
            3,
            20,
            65536,
            Policy::RejectOverload,
            consumer,
        )
        .unwrap();
        assert_eq!(recorder.pump().unwrap(), 0);
        producer
            .push(&[-1., -0.5, -0.5 / scale, 0., 0.5 / scale, 0.5, 1.])
            .unwrap();
        assert_eq!(recorder.pump().unwrap(), 3);
        assert_eq!(recorder.frames(), 3);
        drop(producer);
        let bytes = recorder.finish().unwrap().into_inner();
        let decoded = decode_compressed(&bytes, 20).unwrap();
        assert_eq!(
            decoded.samples(),
            &[
                -1.,
                -0.5,
                -1. / scale,
                0.,
                1. / scale,
                0.5,
                (scale - 1.) / scale
            ]
        );
        let (mut producer, consumer) = audio_ring(1, 4).unwrap();
        let recorder = FlacRecorder::new(
            Cursor::new(Vec::new()),
            format(precision),
            2,
            20,
            65536,
            Policy::ClipOverload,
            consumer,
        )
        .unwrap();
        producer.push(&[-f32::MAX, -1.01, 1.01, f32::MAX]).unwrap();
        drop(producer);
        let bytes = recorder.finish().unwrap().into_inner();
        assert_eq!(
            decode_compressed(&bytes, 20).unwrap().samples(),
            &[-1., -1., (scale - 1.) / scale, (scale - 1.) / scale]
        );
    }
}
#[test]
fn dequeued_invalid_input_and_total_capacity_poison_without_later_dequeue() {
    for (policy, value) in [
        (Policy::RejectOverload, 1.01),
        (Policy::RejectOverload, f32::NAN),
        (Policy::ClipOverload, f32::INFINITY),
        (Policy::ClipOverload, f32::NEG_INFINITY),
    ] {
        let (mut producer, consumer) = audio_ring(1, 4).unwrap();
        let mut recorder = FlacRecorder::new(
            Cursor::new(Vec::new()),
            format(P::Pcm16),
            2,
            10,
            65536,
            policy,
            consumer,
        )
        .unwrap();
        producer.push(&[0., value, 0.25]).unwrap();
        assert!(matches!(recorder.pump(), Err(E::InvalidSamples)));
        assert!(recorder.is_poisoned());
        assert_eq!(recorder.frames(), 0);
        assert!(matches!(recorder.pump(), Err(E::Poisoned)));
        drop(producer);
        assert!(matches!(recorder.finish(), Err(E::Poisoned)));
    }
    let (mut producer, consumer) = audio_ring(1, 4).unwrap();
    let mut recorder = FlacRecorder::new(
        Cursor::new(Vec::new()),
        format(P::Pcm16),
        2,
        1,
        65536,
        Policy::RejectOverload,
        consumer,
    )
    .unwrap();
    producer.push(&[0.25, 0.5]).unwrap();
    assert!(matches!(recorder.pump(), Err(E::Capacity)));
    assert!(recorder.is_poisoned());
    let (_, consumer) = audio_ring(2, 4).unwrap();
    assert!(matches!(
        FlacRecorder::new(
            Cursor::new(Vec::new()),
            format(P::Pcm16),
            2,
            10,
            65536,
            Policy::RejectOverload,
            consumer
        ),
        Err(E::InvalidSamples)
    ));
    for maximum in [0, 65537] {
        let (_, consumer) = audio_ring(1, 4).unwrap();
        assert!(matches!(
            FlacRecorder::new(
                Cursor::new(Vec::new()),
                format(P::Pcm16),
                maximum,
                10,
                65536,
                Policy::RejectOverload,
                consumer
            ),
            Err(E::Capacity)
        ));
    }
}
struct FailingSink {
    inner: Cursor<Vec<u8>>,
    budget: usize,
}
impl Write for FailingSink {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.budget == 0 {
            return Err(io::Error::other("synthetic capture write failure"));
        }
        let n = b.len().min(self.budget);
        self.budget -= n;
        self.inner.write(&b[..n])
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Seek for FailingSink {
    fn seek(&mut self, s: SeekFrom) -> io::Result<u64> {
        self.inner.seek(s)
    }
}
#[test]
fn partial_encoded_output_poisons_capture() {
    let (mut producer, consumer) = audio_ring(1, 4096).unwrap();
    let sink = FailingSink {
        inner: Cursor::new(Vec::new()),
        budget: 43,
    };
    let mut recorder = FlacRecorder::new(
        sink,
        format(P::Pcm16),
        4096,
        8192,
        65536,
        Policy::RejectOverload,
        consumer,
    )
    .unwrap();
    producer.push(&[0.; 4096]).unwrap();
    assert!(matches!(recorder.pump(), Err(E::Io(_))));
    assert!(recorder.is_poisoned());
    assert_eq!(recorder.frames(), 4096);
    assert!(matches!(recorder.pump(), Err(E::Poisoned)));
    drop(producer);
    assert!(matches!(recorder.finish(), Err(E::Poisoned)));
}
#[test]
fn threaded_capture_preserves_exact_pcm_order_with_heap_free_producer_handoff() {
    const FRAMES: usize = 8193;
    let (mut producer, consumer) = audio_ring(1, 4).unwrap();
    let recorder = FlacRecorder::new(
        Cursor::new(Vec::new()),
        format(P::Pcm24),
        7,
        FRAMES as u64,
        1_048_576,
        Policy::RejectOverload,
        consumer,
    )
    .unwrap();
    let (stopped_tx, stopped_rx) = std::sync::mpsc::sync_channel(0);
    let worker = std::thread::spawn(move || {
        let mut recorder = recorder;
        while recorder.frames() < FRAMES as u64 {
            if recorder.pump().unwrap() == 0 {
                std::thread::yield_now();
            }
        }
        stopped_rx.recv().unwrap();
        recorder.finish().unwrap().into_inner()
    });
    let counts = allocation_counter::measure(|| {
        for frame in 0..FRAMES {
            let sample = ((frame as i32 % 1024) - 512) as f32 / 8388608.;
            while producer.push(&[sample]).unwrap() == 0 {
                assert!(!worker.is_finished(), "capture worker stopped prematurely");
                std::thread::yield_now();
            }
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    drop(producer);
    stopped_tx.send(()).unwrap();
    let bytes = worker.join().unwrap();
    let decoded = decode_compressed(&bytes, FRAMES).unwrap();
    assert_eq!(decoded.frames(), FRAMES);
    for (frame, sample) in decoded.samples().iter().enumerate() {
        assert_eq!(*sample, ((frame as i32 % 1024) - 512) as f32 / 8388608.);
    }
}
