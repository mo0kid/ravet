#![cfg(feature = "resampled-playback")]
use ravet_audio_file::{
    AudioFileError as Error, ConvertedReadAhead, ConvertedReadAheadOptions, FilePlaybackOptions,
    PlaybackConversion, PlaybackReader, audio_ring, prepare_file_playback,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
#[derive(Clone)]
struct Generated {
    n: u64,
    position: u64,
    fault: Option<u64>,
}
impl PlaybackReader for Generated {
    fn channels(&self) -> usize {
        1
    }
    fn sample_rate(&self) -> u32 {
        768000
    }
    fn position_frames(&self) -> u64 {
        self.position
    }
    fn declared_frames(&self) -> Option<u64> {
        Some(self.n)
    }
    fn maximum_read_frames(&self) -> usize {
        4096
    }
    fn read_frames(&mut self, output: &mut [f32]) -> Result<usize, Error> {
        if self.fault.is_some_and(|at| self.position >= at) {
            self.position += 1;
            return Err(Error::Io(std::io::Error::other(
                "original late replay fault",
            )));
        }
        let count = (self.n - self.position).min(output.len() as u64) as usize;
        output[..count].fill(0.0);
        for (frame, sample) in output[..count].iter_mut().enumerate() {
            if self.position + frame as u64 == self.n - 8 {
                *sample = 1.0;
            }
        }
        self.position += count as u64;
        Ok(count)
    }
}
fn options(n: usize, mode: PlaybackConversion) -> ConvertedReadAheadOptions {
    ConvertedReadAheadOptions {
        output_rate: 48000,
        conversion: mode,
        maximum_source_frames: n,
        maximum_output_frames: (n as u64 + 14) / 16 + 1,
        source_batch_frames: 4096,
        conversion_batch_frames: 113,
    }
}
#[test]
fn actual_long_conversion_failed_candidate_and_stopped_replay_preserve_exact_tail() {
    let base = 16_777_216usize;
    let n = base + 33;
    let small = Generated {
        n: 33,
        position: 0,
        fault: None,
    };
    let mut reference = prepare_file_playback(
        small,
        FilePlaybackOptions {
            output_rate: 48000,
            conversion: PlaybackConversion::IntegerSinc { taps: 16 },
            maximum_source_frames: 33,
            maximum_output_frames: 16,
            source_batch_frames: 17,
            maximum_render_frames: 16,
        },
    )
    .unwrap();
    let retained = reference.info().prepared_frames;
    let mut tail = vec![0.0; retained];
    reference.playback_mut().play();
    reference.playback_mut().render(&mut tail).unwrap();
    assert!(tail.iter().any(|value| value.abs() > 0.0));
    let total = (n as u64 + 14) / 16 + 1;
    let target = base as u64 / 16;
    assert_eq!(total - target, retained as u64);
    let fault = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let factory_fault = Arc::clone(&fault);
    let factory_calls = Arc::clone(&calls);
    let reader = Generated {
        n: n as u64,
        position: 0,
        fault: None,
    };
    let fresh = reader.clone();
    let factory = move || {
        factory_calls.fetch_add(1, Ordering::Relaxed);
        let mut fresh = fresh.clone();
        fresh.fault = factory_fault
            .load(Ordering::Relaxed)
            .then_some(base as u64 - 4096);
        Ok(fresh)
    };
    let (producer, mut consumer) = audio_ring(1, 64).unwrap();
    let mut pump = ConvertedReadAhead::new(
        reader,
        factory,
        producer,
        &consumer,
        options(n, PlaybackConversion::IntegerSinc { taps: 16 }),
    )
    .unwrap();
    pump.pump().unwrap();
    let old = pump.counts();
    let old_pending = pump.pending_output_frames();
    let old_queue = consumer.queued_frames();
    assert!(old_queue > 0);
    assert!(matches!(
        pump.seek_stopped(&mut consumer, total + 1),
        Err(Error::Capacity)
    ));
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    fault.store(true, Ordering::Relaxed);
    let error = pump.seek_stopped(&mut consumer, target).unwrap_err();
    assert!(matches!(error,Error::Io(ref e) if e.to_string()=="original late replay fault"));
    assert_eq!(pump.counts(), old);
    assert_eq!(pump.pending_output_frames(), old_pending);
    assert_eq!(consumer.queued_frames(), old_queue);
    assert_eq!(pump.generation(), 0);
    fault.store(false, Ordering::Relaxed);
    pump.seek_stopped(&mut consumer, target).unwrap();
    assert_eq!(pump.generation(), 1);
    assert_eq!(pump.output_origin(), target);
    assert_eq!(consumer.queued_frames(), 0);
    assert!(pump.counts().input_frames_read > base as u64 - 32);
    let mut actual = Vec::new();
    for _ in 0..100 {
        let progress = pump.pump().unwrap();
        let mut output = [0.0; 7];
        let count = consumer.pop(&mut output).unwrap();
        actual.extend_from_slice(&output[..count]);
        if progress.end_of_file && consumer.queued_frames() == 0 {
            break;
        }
    }
    assert_eq!(actual, tail);
    assert_eq!(pump.counts().input_frames_read, n as u64);
    assert_eq!(pump.counts().input_frames_submitted, n as u64);
    assert_eq!(pump.output_generated_position(), total);
    assert_eq!(pump.counts().output_frames_published, retained as u64);
}
#[test]
fn legacy_ceiling_and_integer_large_duration_preflight_keep_original_owners() {
    let n = 16_777_216usize + 1;
    let reader = Generated {
        n: n as u64,
        position: 0,
        fault: None,
    };
    let (producer, consumer) = audio_ring(1, 4).unwrap();
    let failure = ConvertedReadAhead::new(
        reader.clone(),
        move || Ok(reader.clone()),
        producer,
        &consumer,
        options(n, PlaybackConversion::Sinc { taps: 16 }),
    )
    .err()
    .unwrap();
    assert!(matches!(failure.error(), Error::Capacity));
    assert_eq!(failure.reader().position_frames(), 0);
    let (reader, factory, producer, _) = failure.into_parts();
    let mut config = options(n, PlaybackConversion::IntegerSinc { taps: 16 });
    config.maximum_output_frames = 1;
    let failure = ConvertedReadAhead::new(reader, factory, producer, &consumer, config)
        .err()
        .unwrap();
    assert!(matches!(failure.error(), Error::Capacity));
    assert_eq!(failure.reader().position_frames(), 0);
}

#[cfg(target_pointer_width = "64")]
#[test]
fn exact_integer_terminal_render_headroom_boundary_preflights_without_io() {
    #[derive(Clone)]
    struct NeverRead {
        n: u64,
        reads: Arc<AtomicUsize>,
    }
    impl PlaybackReader for NeverRead {
        fn channels(&self) -> usize {
            1
        }
        fn sample_rate(&self) -> u32 {
            48000
        }
        fn position_frames(&self) -> u64 {
            0
        }
        fn declared_frames(&self) -> Option<u64> {
            Some(self.n)
        }
        fn maximum_read_frames(&self) -> usize {
            17
        }
        fn read_frames(&mut self, _: &mut [f32]) -> Result<usize, Error> {
            self.reads.fetch_add(1, Ordering::Relaxed);
            panic!("preflight must not read");
        }
    }
    let reads = Arc::new(AtomicUsize::new(0));
    let factories = Arc::new(AtomicUsize::new(0));
    let boundary = u64::MAX - 65_536;
    for extra in [0, 1] {
        let n = boundary - 15 + extra;
        let reader = NeverRead {
            n,
            reads: Arc::clone(&reads),
        };
        let fresh = reader.clone();
        let observed = Arc::clone(&factories);
        let factory = move || {
            observed.fetch_add(1, Ordering::Relaxed);
            Ok(fresh.clone())
        };
        let (producer, consumer) = audio_ring(1, 4).unwrap();
        let options = ConvertedReadAheadOptions {
            output_rate: 48000,
            conversion: PlaybackConversion::IntegerSinc { taps: 16 },
            maximum_source_frames: n as usize,
            maximum_output_frames: u64::MAX,
            source_batch_frames: 17,
            conversion_batch_frames: 7,
        };
        let result = ConvertedReadAhead::new(reader, factory, producer, &consumer, options);
        if extra == 0 {
            assert!(result.is_ok());
        } else {
            let failure = result.err().unwrap();
            assert!(matches!(failure.error(), Error::Capacity));
            assert_eq!(failure.reader().declared_frames(), Some(n));
            let (reader, _, _, _) = failure.into_parts();
            assert!(Arc::ptr_eq(&reader.reads, &reads));
        }
    }
    assert_eq!(reads.load(Ordering::Relaxed), 0);
    assert_eq!(factories.load(Ordering::Relaxed), 0);
}
