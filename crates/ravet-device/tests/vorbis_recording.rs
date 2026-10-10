//! Synthetic prepared input uses the same hidden producer ownership as open_input.
#![cfg(feature = "vorbis-recording")]
use ravet_audio_file::{
    AudioFileError as E, VorbisFormat, VorbisRecordingCompletion as C, VorbisRecordingWorker,
};
use ravet_device::PreparedCapture;
use std::{
    thread,
    time::{Duration, Instant},
};
#[test]
fn hidden_prepared_capture_producer_records_automatically_and_finalizes_after_retirement() {
    let (mut capture, consumer) = PreparedCapture::new(2, 8).unwrap();
    let format = VorbisFormat {
        channels: 2,
        sample_rate: 48000,
        quality: 1.,
        stream_serial: 71,
    };
    let mut worker =
        VorbisRecordingWorker::new(Vec::new(), format, consumer, 3, 8193, 65536).unwrap();
    worker.start().unwrap();
    assert!(matches!(
        worker.finish_after_producer_retired(),
        Err(E::InvalidRecord)
    ));
    let source: Vec<f32> = (0..8193)
        .flat_map(|frame| {
            [337., 977.].map(move |frequency| {
                (0.5 * (std::f64::consts::TAU * frequency * frame as f64 / 48000.).sin()) as f32
            })
        })
        .collect();
    let end = Instant::now() + Duration::from_secs(10);
    let mut frame = 0;
    while frame < 8193 {
        let before = capture.statistics().captured_frames;
        let heap =
            allocation_counter::measure(|| capture.capture(&source[frame * 2..(frame + 1) * 2]));
        assert_eq!((heap.count_total, heap.count_current), (0, 0));
        // Synthetic source retries for deterministic accepted-frame evidence;
        // physical capture's full-ring suffix is explicitly counted/dropped.
        if capture.statistics().captured_frames != before {
            frame += 1;
        }
        assert!(Instant::now() < end);
        thread::yield_now();
    }
    assert_eq!(capture.statistics().captured_frames, 8193);
    drop(capture);
    let exit = worker.finish_after_producer_retired().unwrap();
    assert!(matches!(exit.completion(), C::Finalized));
    assert_eq!((exit.accepted_frames, exit.dequeued_frames), (8193, 8193));
    assert_eq!(exit.consumer.queued_frames(), 0);
    assert_eq!(exit.written_bytes, exit.destination.len() as u64);
    // Parse exact final Ogg granule without requiring the independent decoder
    // feature in the default-disabled device forwarding graph.
    let mut offset = 0;
    let mut final_frames = None;
    while offset < exit.destination.len() {
        let page = &exit.destination[offset..];
        assert_eq!(&page[..4], b"OggS");
        let segments = usize::from(page[26]);
        let size = 27
            + segments
            + page[27..27 + segments]
                .iter()
                .map(|v| usize::from(*v))
                .sum::<usize>();
        if page[5] & 4 != 0 {
            assert_eq!(offset + size, exit.destination.len());
            final_frames = Some(u64::from_le_bytes(page[6..14].try_into().unwrap()));
        }
        offset += size;
    }
    assert_eq!(final_frames, Some(8193));
}
#[test]
fn prepared_capture_retained_full_tail_finishes_on_worker_without_prior_start() {
    let (mut capture, consumer) = PreparedCapture::new(2, 4).unwrap();
    capture.capture(&[0.25, -0.25, 0.5, -0.5, 0., 0., -0.25, 0.25, 0.1, -0.1]);
    assert_eq!(
        (
            capture.statistics().captured_frames,
            capture.statistics().dropped_frames
        ),
        (4, 1)
    );
    drop(capture);
    let mut worker = VorbisRecordingWorker::new(
        Vec::new(),
        VorbisFormat {
            channels: 2,
            sample_rate: 48000,
            quality: 1.,
            stream_serial: 72,
        },
        consumer,
        3,
        4,
        65536,
    )
    .unwrap();
    let exit = worker.finish_after_producer_retired().unwrap();
    assert!(matches!(exit.completion(), C::Finalized));
    assert_eq!((exit.accepted_frames, exit.dequeued_frames), (4, 4));
    assert_eq!(exit.consumer.queued_frames(), 0);
}
