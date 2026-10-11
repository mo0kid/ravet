//! Processing-thread allocator probes. All framework code stays safe Rust;
//! the dev-only counter supplies the global allocator in this test binary.
use allocation_counter::{AllocationInfo, measure};
use ravet::foundation::*;
use ravet_gain::{GAIN_ID, GainProcessor};
use std::hint::black_box;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn assert_no_heap_activity(info: AllocationInfo) {
    assert_eq!(info.count_total, 0, "callback allocated: {info:?}");
    // Since allocations are zero, a negative net count catches destruction
    // of storage that was allocated before entering the measured region.
    assert_eq!(info.count_current, 0, "callback deallocated: {info:?}");
    assert_eq!(info.bytes_total, 0);
    assert_eq!(info.bytes_current, 0);
}

#[test]
fn instrumentation_detects_both_allocation_and_free_of_existing_storage() {
    let allocation = measure(|| {
        drop(black_box(Box::new([0_u8; 256])));
    });
    assert_eq!(allocation.count_total, 1);
    assert_eq!(allocation.count_current, 0);
    let existing = black_box(Box::new([0_u8; 256]));
    let free = measure(|| {
        drop(existing);
    });
    assert_eq!(free.count_total, 0);
    assert_eq!(free.count_current, -1);
    assert_eq!(free.bytes_current, -256);
}

fn measured_processing(separate: bool) -> AllocationInfo {
    let mut processor = GainProcessor::default();
    processor
        .prepare(ProcessConfig::new(48_000.0, 64, 2, AudioLayout::STEREO).unwrap())
        .unwrap();
    let input_left = [0.5; 64];
    let input_right = [-0.25; 64];
    let inputs = [&input_left[..], &input_right[..]];
    let mut left = [0.0; 64];
    let mut right = [0.0; 64];
    let mut failure = None;
    let info = measure(|| {
        for iteration in 0..10_000 {
            let frames: usize = [0, 1, 3, 64][iteration % 4];
            let events = [
                Event {
                    frame_offset: 0,
                    data: EventData::ParameterValue {
                        id: GAIN_ID,
                        value: (iteration % 5) as f32,
                    },
                },
                Event {
                    frame_offset: frames.saturating_sub(1),
                    data: EventData::ParameterValue {
                        id: GAIN_ID,
                        value: 0.5,
                    },
                },
            ];
            left.fill(0.5);
            right.fill(-0.25);
            let mut outputs = [&mut left[..], &mut right[..]];
            let audio = if separate {
                AudioBuffer::separate(&inputs, &mut outputs, frames)
            } else {
                AudioBuffer::in_place(&mut outputs, frames)
            }
            .unwrap();
            let status =
                processor.process(&mut ProcessBlock::new(audio, &events, Transport::default()));
            if status != ProcessStatus::Continue {
                failure = Some(status);
            }
            if iteration % 37 == 0 {
                processor.reset();
            }
            black_box(&left);
            black_box(&right);
        }
    });
    assert_eq!(failure, None);
    info
}

#[test]
fn repeated_variable_blocks_events_smoothing_and_reset_have_no_heap_activity() {
    for separate in [false, true] {
        assert_no_heap_activity(measured_processing(separate));
    }
}

#[test]
fn fault_and_inactive_paths_have_no_heap_activity() {
    let mut processor = GainProcessor::default();
    processor
        .prepare(ProcessConfig::new(48_000.0, 64, 2, AudioLayout::MONO).unwrap())
        .unwrap();
    let mut samples = [1.0; 64];
    let events = [Event {
        frame_offset: 0,
        data: EventData::ParameterValue {
            id: GAIN_ID,
            value: f32::NAN,
        },
    }];
    let mut statuses = [ProcessStatus::Continue; 2];
    let info = measure(|| {
        for (index, status) in statuses.iter_mut().enumerate() {
            if index == 1 {
                processor.deactivate();
            }
            let mut channels = [&mut samples[..]];
            let audio = AudioBuffer::in_place(&mut channels, 64).unwrap();
            *status =
                processor.process(&mut ProcessBlock::new(audio, &events, Transport::default()));
        }
    });
    assert_no_heap_activity(info);
    assert_eq!(
        statuses,
        [
            ProcessStatus::Fault(ProcessError::Events(EventError::InvalidValue { index: 0 })),
            ProcessStatus::Fault(ProcessError::NotPrepared)
        ]
    );
    assert_eq!(samples, [0.0; 64]);
}

#[test]
fn concurrent_worker_allocations_are_not_attributed_to_processing() {
    let stop = Arc::new(AtomicBool::new(false));
    let started = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let worker_started = Arc::clone(&started);
    let worker = std::thread::spawn(move || {
        measure(|| {
            drop(black_box(Box::new([0_u8; 1024])));
            // Do not measure first-use blocking synchronisation: its retained
            // thread runtime storage is unrelated to this worker's Box loop.
            worker_started.store(true, Ordering::Release);
            while !worker_stop.load(Ordering::Relaxed) {
                drop(black_box(Box::new([0_u8; 1024])));
            }
        })
    });
    while !started.load(Ordering::Acquire) {
        std::thread::yield_now();
    }
    let info = measured_processing(false);
    stop.store(true, Ordering::Relaxed);
    let worker_info = worker.join().unwrap();
    assert!(worker_info.count_total > 0);
    assert_eq!(worker_info.count_current, 0);
    assert_no_heap_activity(info);
}
