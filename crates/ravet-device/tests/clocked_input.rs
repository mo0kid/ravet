use ravet_audio_file::audio_ring;
use ravet_device::{ClockedDuplexError, ClockedDuplexOptions, PreparedClockedInput};
use ravet_signal::FifoClockConfig;
fn options(capacity: usize, target: usize, frames: usize) -> ClockedDuplexOptions {
    ClockedDuplexOptions {
        maximum_frames: frames,
        taps: 32,
        clock: FifoClockConfig {
            capacity_frames: capacity,
            target_frames: target,
            ..FifoClockConfig::default()
        },
    }
}
#[test]
fn priming_zeros_and_retained_suffixes_never_reconvert_or_consume_capture_twice() {
    let (mut capture, raw) = audio_ring(2, 128).unwrap();
    let (converted, mut graph_input) = audio_ring(2, 4).unwrap();
    let mut worker =
        PreparedClockedInput::new(raw, converted, 96000, 48000, options(128, 64, 8)).unwrap();
    assert_eq!(worker.statistics().callbacks, 0);
    let first = worker.pump();
    assert_eq!(
        (
            first.converted_frames,
            first.published_frames,
            first.pending_frames
        ),
        (8, 4, 4)
    );
    assert_eq!(worker.statistics().callbacks, 1);
    assert_eq!(worker.clock_statistics().priming_output_frames, 8);
    assert_eq!(capture.push(&[1.; 128]).unwrap(), 64);
    let full = worker.pump();
    assert_eq!(
        (
            full.converted_frames,
            full.published_frames,
            full.pending_frames
        ),
        (0, 0, 4)
    );
    assert_eq!(worker.statistics().callbacks, 1);
    assert_eq!(worker.clock_statistics().priming_output_frames, 8);
    let mut samples = [99.; 8];
    assert_eq!(graph_input.pop(&mut samples).unwrap(), 4);
    assert_eq!(samples, [0.; 8]);
    let pending = worker.pump();
    assert_eq!(
        (
            pending.converted_frames,
            pending.published_frames,
            pending.pending_frames
        ),
        (0, 4, 0)
    );
    assert_eq!(worker.statistics().callbacks, 1);
    assert_eq!(graph_input.pop(&mut samples).unwrap(), 4);
    assert_eq!(samples, [0.; 8]);
    let next = worker.pump();
    assert_eq!(next.converted_frames, 8);
    assert_eq!(worker.statistics().callbacks, 2);
    assert_eq!(worker.clock_statistics().priming_output_frames, 8);
}
#[test]
fn nominal_conversion_preserves_stereo_and_bounded_queue_progress_without_heap_actions() {
    let (mut capture, raw) = audio_ring(2, 128).unwrap();
    let (converted, mut graph_input) = audio_ring(2, 8).unwrap();
    let mut worker =
        PreparedClockedInput::new(raw, converted, 96000, 48000, options(128, 64, 16)).unwrap();
    let source: [f32; 256] =
        std::array::from_fn(|index| if index % 2 == 0 { 0.25 } else { -0.125 });
    capture.push(&source).unwrap();
    let mut output = [99.; 16];
    let mut last = [0.; 16];
    let allocations = allocation_counter::measure(|| {
        for _ in 0..1000 {
            capture.push(&source).unwrap();
            let first = worker.pump();
            assert_eq!(
                (
                    first.converted_frames,
                    first.published_frames,
                    first.pending_frames
                ),
                (16, 8, 8)
            );
            graph_input.pop(&mut output).unwrap();
            for pair in output.as_chunks::<2>().0 {
                assert!(pair[0].is_finite());
                assert_eq!(pair[1], -0.5 * pair[0]);
            }
            let callbacks = worker.statistics().callbacks;
            let pending = worker.pump();
            assert_eq!(
                (
                    pending.converted_frames,
                    pending.published_frames,
                    pending.pending_frames
                ),
                (0, 8, 0)
            );
            assert_eq!(worker.statistics().callbacks, callbacks);
            graph_input.pop(&mut last).unwrap();
        }
    });
    assert_eq!(allocations.count_total, 0);
    assert_eq!(allocations.count_current, 0);
    for pair in last.as_chunks::<2>().0 {
        assert!((pair[0] - 0.25).abs() < 0.001);
        assert_eq!(pair[1], -0.5 * pair[0]);
    }
    assert_eq!(worker.statistics().callbacks, 1000);
    assert_eq!(worker.statistics().frames, 16000);
    assert_eq!(worker.statistics().processor_faults, 0);
    assert_eq!(worker.clock_statistics().underflow_output_frames, 0);
}
#[test]
fn configuration_rejects_channel_rate_clock_and_frame_mismatches() {
    for (channels, input_rate, output_rate, options) in [
        (1, 96000, 48000, options(128, 64, 8)),
        (2, 7999, 48000, options(128, 64, 8)),
        (2, 96000, 192001, options(128, 64, 8)),
        (2, 96000, 48000, options(64, 32, 8)),
        (2, 96000, 48000, options(128, 128, 8)),
        (2, 96000, 48000, options(128, 64, 0)),
    ] {
        let (_capture, raw) = audio_ring(2, 128).unwrap();
        let (converted, _graph_input) = audio_ring(channels, 8).unwrap();
        assert!(matches!(
            PreparedClockedInput::new(raw, converted, input_rate, output_rate, options),
            Err(ClockedDuplexError::InvalidConfiguration) | Err(ClockedDuplexError::Clock(_))
        ));
    }
}
