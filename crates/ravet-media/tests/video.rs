#![cfg(target_os = "macos")]
use ravet_media::{VideoError, VideoReader};
use std::{ffi::OsString, fs, os::unix::ffi::OsStringExt, path::PathBuf};
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/red-blue.mp4")
}
#[test]
fn actual_h264_frames_have_independent_color_time_orientation_and_seek_order() {
    let mut video = VideoReader::open(fixture()).unwrap();
    let info = video.info();
    assert_eq!((info.width, info.height), (64, 48));
    assert!((info.duration_seconds - 2.).abs() < 0.000001);
    for time in [1.2, 0.2, 1.8, 0., 0.96, 1.] {
        let frame = video.frame_at(time).unwrap();
        assert_eq!((frame.info.width, frame.info.height), (64, 48));
        assert_eq!(frame.pixels.len(), 64 * 48 * 4);
        assert!((frame.actual_seconds - time).abs() < 0.000001);
        let expected = if time < 1. {
            [255_i16, 0, 0, 255]
        } else {
            [0, 0, 255, 255]
        };
        for (index, pixel) in frame.pixels.as_chunks::<4>().0.iter().enumerate() {
            let row = index / 64;
            if (6..12).contains(&row) {
                continue;
            }
            let expected = if row < 6 {
                [255_i16, 255, 255, 255]
            } else {
                expected
            };
            assert!(
                pixel
                    .iter()
                    .zip(expected)
                    .all(|(&actual, want)| (i16::from(actual) - want).abs() <= 50),
                "pixel {pixel:?}, time {time}"
            );
        }
    }
    for time in [f64::NAN, f64::INFINITY, -0.1, 2., 3.] {
        assert!(matches!(
            video.frame_at(time),
            Err(VideoError::InvalidInput)
        ));
    }
    assert!(video.frame_at(0.4).is_ok());
}
#[test]
fn invalid_paths_files_and_sparse_capacity_reject_without_opening_a_window() {
    let directory = std::env::temp_dir().join(format!("ravet-video-test-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    assert!(matches!(
        VideoReader::open(&directory),
        Err(VideoError::InvalidInput)
    ));
    assert!(matches!(
        VideoReader::open(directory.join("missing")),
        Err(VideoError::Io(_))
    ));
    assert!(matches!(
        VideoReader::open("bad\0path"),
        Err(VideoError::InvalidInput)
    ));
    assert!(matches!(
        VideoReader::open("x".repeat(4097)),
        Err(VideoError::InvalidInput)
    ));
    assert!(matches!(
        VideoReader::open(OsString::from_vec(vec![0xff])),
        Err(VideoError::InvalidInput)
    ));
    let empty = directory.join("empty.mp4");
    fs::write(&empty, []).unwrap();
    assert!(matches!(
        VideoReader::open(&empty),
        Err(VideoError::Capacity)
    ));
    let bad = directory.join("invalid.mp4");
    fs::write(&bad, b"public malformed media").unwrap();
    assert!(matches!(VideoReader::open(&bad), Err(VideoError::Decode)));
    let big = directory.join("large.mp4");
    fs::File::create(&big)
        .unwrap()
        .set_len(2 * 1024 * 1024 * 1024 + 1)
        .unwrap();
    assert!(matches!(VideoReader::open(&big), Err(VideoError::Capacity)));
    fs::remove_dir_all(directory).unwrap();
}
#[test]
fn owner_created_on_worker_supports_repeated_independent_lifecycle() {
    std::thread::spawn(|| {
        for _ in 0..12 {
            let mut video = VideoReader::open(fixture()).unwrap();
            assert_eq!(video.frame_at(0.08).unwrap().pixels.len(), 64 * 48 * 4);
        }
    })
    .join()
    .unwrap();
}
