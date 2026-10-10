//! Synthetic prepared input uses the same hidden producer ownership as open_input.
#![cfg(feature = "mp3-recording")]
use ravet_audio_file::{
    AudioFileError as E, Mp3Format, Mp3RecordingCompletion as C, Mp3RecordingWorker,
};
use ravet_device::PreparedCapture;
use std::{
    thread,
    time::{Duration, Instant},
};
#[test]
fn hidden_prepared_capture_producer_records_automatically_and_finalizes_after_retirement() {
    let (mut capture, consumer) = PreparedCapture::new(2, 8).unwrap();
    let format = Mp3Format {
        channels: 2,
        sample_rate: 48000,
        bit_rate: ravet_audio_file::Mp3Bitrate::Kbps256,
    };
    let mut worker = Mp3RecordingWorker::new(
        std::io::Cursor::new(Vec::new()),
        format,
        consumer,
        3,
        8193,
        65536,
    )
    .unwrap();
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
    assert_eq!(exit.file_bytes, exit.destination.get_ref().len() as u64);
    assert!(exit.written_bytes > exit.file_bytes); // rewritten first metadata frame
    assert_eq!(finalized_frames(exit.destination.get_ref()), 8193);
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
    let mut worker = Mp3RecordingWorker::new(
        std::io::Cursor::new(Vec::new()),
        Mp3Format {
            channels: 2,
            sample_rate: 48000,
            bit_rate: ravet_audio_file::Mp3Bitrate::Kbps256,
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
    assert_eq!(finalized_frames(exit.destination.get_ref()), 4);
}

// Parse the MPEG-1 stereo Info/LAME relation independently of the encoder's
// validation and without enabling the optional decoder dependency graph.
fn finalized_frames(bytes: &[u8]) -> u64 {
    let header = u32::from_be_bytes(bytes[..4].try_into().unwrap());
    assert_eq!(header >> 21, 0x7ff);
    assert_eq!((header >> 19) & 3, 3);
    assert_eq!((header >> 17) & 3, 1);
    assert_eq!((header >> 16) & 1, 1);
    assert_ne!((header >> 6) & 3, 3);
    let info = 4 + 32;
    assert_eq!(&bytes[info..info + 4], b"Info");
    let flags = u32::from_be_bytes(bytes[info + 4..info + 8].try_into().unwrap());
    assert_eq!(flags & 3, 3);
    let frames = u32::from_be_bytes(bytes[info + 8..info + 12].try_into().unwrap());
    let file = u32::from_be_bytes(bytes[info + 12..info + 16].try_into().unwrap());
    assert_eq!(file as usize, bytes.len());
    let mut lame = info + 16;
    if flags & 4 != 0 {
        lame += 100;
    }
    if flags & 8 != 0 {
        lame += 4;
    }
    assert_eq!(&bytes[lame..lame + 9], b"LAME3.100");
    let delay = (u16::from(bytes[lame + 21]) << 4) | u16::from(bytes[lame + 22] >> 4);
    let padding = (u16::from(bytes[lame + 22] & 15) << 8) | u16::from(bytes[lame + 23]);
    assert_eq!(delay, 576);
    u64::from(frames) * 1152 - u64::from(delay) - u64::from(padding)
}
