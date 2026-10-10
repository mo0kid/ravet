//! Synthetic capture using the same hidden-producer ownership as open_input.
use ravet_audio_file::{
    AudioFileError as E, RecordingStatus as S, RecordingWorker, WaveEncoding, WaveFormat,
    WaveReader, WaveRecorder,
};
use ravet_device::PreparedCapture;
use std::{
    io::Cursor,
    time::{Duration, Instant},
};
#[test]
fn prepared_capture_hidden_producer_finalizes_automatic_recording_after_drop() {
    let (mut capture, consumer) = PreparedCapture::new(2, 8).unwrap();
    let format = WaveFormat {
        channels: 2,
        sample_rate: 48000,
        encoding: WaveEncoding::Float32,
    };
    let recorder = WaveRecorder::new(Cursor::new(Vec::new()), format, 3, consumer).unwrap();
    let mut worker = RecordingWorker::from_recorder(recorder);
    worker.start().unwrap();
    assert!(matches!(
        worker.finish_after_producer_retired(),
        Err(E::InvalidRecord)
    ));
    assert_eq!(worker.status(), S::Running);
    let samples: Vec<f32> = (0..251)
        .flat_map(|i| [i as f32 / 1024., -(i as f32) / 1024.])
        .collect();
    let end = Instant::now() + Duration::from_secs(10);
    let mut frame = 0;
    while frame < 251 {
        let before = capture.statistics().captured_frames;
        let heap = allocation_counter::measure(|| {
            capture.capture(&samples[frame * 2..(frame + 1) * 2]);
        });
        assert_eq!((heap.count_total, heap.count_current), (0, 0));
        // This test retries synthetic input for exact file comparison; the real
        // PreparedCapture callback explicitly counts/drops its full-ring suffix.
        if capture.statistics().captured_frames != before {
            frame += 1;
        }
        assert!(Instant::now() < end);
        std::thread::yield_now();
    }
    assert_eq!(capture.statistics().captured_frames, 251);
    drop(capture); // the inaccessible unique AudioProducer retires permanently
    let file = worker.finish_after_producer_retired().unwrap();
    assert_eq!(worker.status(), S::Finalized);
    let mut reader = WaveReader::new(file, 512).unwrap();
    let mut output = [99.; 1024];
    assert_eq!(reader.read_frames(&mut output).unwrap(), 251);
    assert_eq!(&output[..502], samples.as_slice());
    assert!(output[502..].iter().all(|sample| *sample == 99.));
}
#[test]
fn dropped_capture_retains_full_fixed_ring_for_final_stopped_tail_drain() {
    let (mut capture, consumer) = PreparedCapture::new(2, 4).unwrap();
    let format = WaveFormat {
        channels: 2,
        sample_rate: 48000,
        encoding: WaveEncoding::Float32,
    };
    let recorder = WaveRecorder::new(Cursor::new(Vec::new()), format, 2, consumer).unwrap();
    let mut worker = RecordingWorker::from_recorder(recorder);
    capture.capture(&[0.25, -0.5, 0., -0., 0.5, -0.25, -0.5, 0.25, 99., 99.]);
    assert_eq!(capture.statistics().captured_frames, 4);
    assert_eq!(capture.statistics().dropped_frames, 1);
    drop(capture);
    let file = worker.finish_after_producer_retired().unwrap();
    let mut reader = WaveReader::new(file, 8).unwrap();
    let mut output = [99.; 16];
    assert_eq!(reader.read_frames(&mut output).unwrap(), 4);
    assert_eq!(
        <[f32; 8]>::try_from(&output[..8])
            .unwrap()
            .map(f32::to_bits),
        [0.25_f32, -0.5, 0., -0., 0.5, -0.25, -0.5, 0.25].map(f32::to_bits)
    );
    assert_eq!(&output[8..], &[99.; 8]);
}
