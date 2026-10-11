#![cfg(feature = "resampled-playback")]
use ravet_audio_file::{
    AudioFileError as Error, ConvertedReadAhead, ConvertedReadAheadOptions, FilePlaybackOptions,
    IndexedConvertedReadAhead, IndexedPlaybackReader, PlaybackConversion, PlaybackReader,
    audio_ring, prepare_file_playback,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
#[derive(Clone)]
struct Source {
    position: u64,
    mode: usize,
    reads: Arc<AtomicUsize>,
    seeks: Arc<AtomicUsize>,
}
impl PlaybackReader for Source {
    fn channels(&self) -> usize {
        1
    }
    fn sample_rate(&self) -> u32 {
        if self.mode == 6 { 44100 } else { 48000 }
    }
    fn position_frames(&self) -> u64 {
        self.position
    }
    fn declared_frames(&self) -> Option<u64> {
        Some(if self.mode == 7 { 0 } else { 64 })
    }
    fn maximum_read_frames(&self) -> usize {
        7
    }
    fn read_frames(&mut self, output: &mut [f32]) -> Result<usize, Error> {
        self.reads.fetch_add(1, Ordering::Relaxed);
        if (self.mode == 8 || self.mode == 9) && self.position >= 2 {
            self.position = if self.mode == 8 { self.position + 1 } else { 0 };
            return Err(Error::Io(std::io::Error::other(
                "original post-prime fault",
            )));
        }
        if self.mode == 2 || (self.mode == 5 && self.position == 64) {
            self.position += 1;
            return Err(Error::Io(std::io::Error::other(
                "original indexed read fault",
            )));
        }
        if self.mode == 4 {
            return Ok(0);
        }
        let n = (self.declared_frames().unwrap() - self.position).min(output.len() as u64) as usize;
        for (index, sample) in output[..n].iter_mut().enumerate() {
            *sample = if self.mode == 3 {
                f32::NAN
            } else {
                ((self.position + index as u64) as f32 * 0.11).sin() * 0.5
            };
        }
        self.position += n as u64;
        Ok(n)
    }
}
impl IndexedPlaybackReader for Source {
    fn seek_original_frame(&mut self, frame: u64) -> Result<(), Error> {
        self.seeks.fetch_add(1, Ordering::Relaxed);
        self.position = frame;
        if self.mode == 1 {
            self.position += 1;
            return Err(Error::Io(std::io::Error::other(
                "original indexed seek fault",
            )));
        }
        Ok(())
    }
}
fn options() -> ConvertedReadAheadOptions {
    ConvertedReadAheadOptions {
        output_rate: 48000,
        conversion: PlaybackConversion::IntegerSinc { taps: 16 },
        maximum_source_frames: 64,
        maximum_output_frames: 79,
        source_batch_frames: 7,
        conversion_batch_frames: 7,
    }
}
fn source(mode: usize) -> Source {
    Source {
        position: 0,
        mode,
        reads: Arc::new(AtomicUsize::new(0)),
        seeks: Arc::new(AtomicUsize::new(0)),
    }
}
#[test]
fn failed_candidates_recover_original_error_and_cursor_preserve_queue_phase_and_generation() {
    let flag = Arc::new(AtomicUsize::new(0));
    let calls = Arc::new(AtomicUsize::new(0));
    let factory_flag = Arc::clone(&flag);
    let factory_calls = Arc::clone(&calls);
    let reader = source(0);
    let (producer, mut consumer) = audio_ring(1, 4).unwrap();
    let mut pump = IndexedConvertedReadAhead::new(
        reader,
        move || {
            factory_calls.fetch_add(1, Ordering::Relaxed);
            Ok(source(factory_flag.load(Ordering::Relaxed)))
        },
        producer,
        &consumer,
        options(),
    )
    .unwrap();
    pump.pump().unwrap();
    let before = pump.counts();
    let pending = pump.pending_output_frames();
    let queued = consumer.queued_frames();
    assert_eq!(pending, 3);
    assert_eq!(queued, 4);
    let failure = pump.seek_indexed_stopped(&mut consumer, 80).unwrap_err();
    assert!(matches!(failure.error(), Error::Capacity));
    assert!(failure.candidate().is_none());
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    for mode in 1..=6 {
        flag.store(mode, Ordering::Relaxed);
        let failure = pump.seek_indexed_stopped(&mut consumer, 78).unwrap_err();
        let candidate = failure.candidate().unwrap();
        assert_eq!(candidate.mode, mode);
        match mode {
            1 => {
                assert_eq!(candidate.position, 64);
                assert!(
                    matches!(failure.error(),Error::Io(e)if e.to_string()=="original indexed seek fault")
                );
            }
            2 => {
                assert_eq!(candidate.position, 64);
                assert!(
                    matches!(failure.error(),Error::Io(e)if e.to_string()=="original indexed read fault")
                );
            }
            3 => {
                assert_eq!(candidate.position, 64);
                assert!(matches!(failure.error(), Error::InvalidSamples));
            }
            4 => {
                assert_eq!(candidate.position, 63);
                assert!(matches!(failure.error(), Error::InvalidRecord));
            }
            5 => {
                assert_eq!(candidate.position, 65);
                assert!(
                    matches!(failure.error(),Error::Io(e)if e.to_string()=="original indexed read fault")
                );
            }
            6 => {
                assert_eq!(candidate.position, 0);
                assert_eq!(candidate.seeks.load(Ordering::Relaxed), 0);
                assert_eq!(candidate.reads.load(Ordering::Relaxed), 0);
                assert!(matches!(failure.error(), Error::InvalidRecord));
            }
            _ => unreachable!(),
        }
        let (owner, _) = failure.into_parts();
        assert!(owner.is_some());
        assert_eq!(pump.counts(), before);
        assert_eq!(pump.pending_output_frames(), pending);
        assert_eq!(consumer.queued_frames(), queued);
        assert_eq!(pump.generation(), 0);
    }
    // Drain the old generation after all failures; its exact continuation proves phase retention.
    let mut reference = prepare_file_playback(
        source(0),
        FilePlaybackOptions {
            output_rate: 48000,
            conversion: PlaybackConversion::IntegerSinc { taps: 16 },
            maximum_source_frames: 64,
            maximum_output_frames: 79,
            source_batch_frames: 7,
            maximum_render_frames: 79,
        },
    )
    .unwrap();
    let mut expected = vec![0.0; 79];
    reference.playback_mut().play();
    reference.playback_mut().render(&mut expected).unwrap();
    let mut actual = Vec::new();
    for _ in 0..100 {
        let mut samples = [0.0; 3];
        let n = consumer.pop(&mut samples).unwrap();
        actual.extend_from_slice(&samples[..n]);
        let p = pump.pump().unwrap();
        if p.end_of_file && consumer.queued_frames() == 0 {
            break;
        }
    }
    assert_eq!(actual, expected);
    flag.store(0, Ordering::Relaxed);
    let receipt = pump.seek_indexed_stopped(&mut consumer, 17).unwrap();
    assert_eq!(receipt.generation, 1);
    assert_eq!(receipt.output_frame, 17);
    assert_eq!(pump.counts().original_frames_submitted_after_prime, 0);
    assert_eq!(pump.counts().output_frames_generated_after_prime, 0);
}
#[test]
fn new_indexed_empty_endpoint_verifies_eof_while_legacy_still_rejects_before_reads() {
    let original = source(7);
    let reads = Arc::clone(&original.reads);
    let (producer, consumer) = audio_ring(1, 4).unwrap();
    let failure =
        ConvertedReadAhead::new(original, || Ok(source(7)), producer, &consumer, options())
            .err()
            .unwrap();
    assert!(matches!(failure.error(), Error::InvalidSamples));
    assert_eq!(reads.load(Ordering::Relaxed), 0);
    let (reader, factory, producer, _) = failure.into_parts();
    let mut pump =
        IndexedConvertedReadAhead::new(reader, factory, producer, &consumer, options()).unwrap();
    assert_eq!(reads.load(Ordering::Relaxed), 0);
    let p = pump.pump().unwrap();
    assert!(p.end_of_file);
    assert_eq!(p.generated_output_frames, 0);
    assert_eq!(reads.load(Ordering::Relaxed), 1);
    assert_eq!(pump.counts().absolute_output_generated, 0);
}
#[cfg(target_pointer_width = "64")]
#[test]
fn indexed_tail_history_uses_u128_beyond_u64_source_endpoint_without_fake_read_prefix() {
    struct Far {
        position: u64,
    }
    impl PlaybackReader for Far {
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
            Some(u64::MAX)
        }
        fn maximum_read_frames(&self) -> usize {
            7
        }
        fn read_frames(&mut self, output: &mut [f32]) -> Result<usize, Error> {
            let n = (u64::MAX - self.position).min(output.len() as u64) as usize;
            output[..n].fill(1.0);
            self.position = self.position.checked_add(n as u64).unwrap();
            Ok(n)
        }
    }
    impl IndexedPlaybackReader for Far {
        fn seek_original_frame(&mut self, frame: u64) -> Result<(), Error> {
            self.position = frame;
            Ok(())
        }
    }
    let total = ((u128::from(u64::MAX) + 14) / 16 + 1) as u64;
    let (producer, mut consumer) = audio_ring(1, 4).unwrap();
    let mut pump = IndexedConvertedReadAhead::new(
        Far { position: 0 },
        || Ok(Far { position: 0 }),
        producer,
        &consumer,
        ConvertedReadAheadOptions {
            output_rate: 48000,
            conversion: PlaybackConversion::IntegerSinc { taps: 16 },
            maximum_source_frames: usize::MAX,
            maximum_output_frames: total,
            source_batch_frames: 7,
            conversion_batch_frames: 7,
        },
    )
    .unwrap();
    let receipt = pump.seek_indexed_stopped(&mut consumer, total - 1).unwrap();
    assert_eq!(
        receipt.filter_input_coordinate,
        Some(u128::from(u64::MAX) + 2)
    );
    assert_eq!(receipt.trailing_zero_history_frames, 2);
    assert_eq!(receipt.window_original_frames, 14);
    assert_eq!(receipt.reader_position, u64::MAX);
    assert_eq!(pump.counts().original_cursor_advance_after_prime, Some(0));
    assert_eq!(pump.counts().original_frames_submitted_after_prime, 0);
    let p = pump.pump().unwrap();
    assert!(p.end_of_file);
    let mut output = [0.0; 4];
    assert_eq!(consumer.pop(&mut output).unwrap(), 1);
    assert!(output[0].is_finite());
    assert_eq!(pump.counts().absolute_output_generated, total);
    assert_eq!(pump.counts().original_frames_submitted_after_prime, 0);
    assert_eq!(pump.counts().output_frames_generated_after_prime, 1);
}

#[test]
fn post_prime_error_cursor_commits_are_not_labelled_submitted_pcm_and_poison_has_no_io() {
    for mode in [8, 9] {
        let (producer, mut consumer) = audio_ring(1, 4).unwrap();
        let mut pump = IndexedConvertedReadAhead::new(
            source(0),
            || Ok(source(mode)),
            producer,
            &consumer,
            options(),
        )
        .unwrap();
        let receipt = pump.seek_indexed_stopped(&mut consumer, 1).unwrap();
        assert_eq!(receipt.window_original_frames, 2);
        assert_eq!(receipt.reader_position, 2);
        let error = pump.pump().unwrap_err();
        assert!(matches!(error, Error::Io(e) if e.to_string() == "original post-prime fault"));
        let counts = pump.counts();
        assert_eq!(
            counts.original_reader_position,
            if mode == 8 { 3 } else { 0 }
        );
        assert_eq!(
            counts.original_cursor_advance_after_prime,
            if mode == 8 { Some(1) } else { None }
        );
        assert_eq!(counts.original_frames_submitted_after_prime, 0);
        assert_eq!(counts.padding_frames_submitted_after_prime, 0);
        assert_eq!(counts.output_frames_generated_after_prime, 0);
        assert_eq!(counts.output_frames_published, 0);
        assert_eq!(consumer.queued_frames(), 0);
        let (reader, _, _, recovered) = pump.into_parts();
        assert_eq!(recovered, counts);
        let calls = reader.reads.load(Ordering::Relaxed);
        // A second identical pump proves the poisoned adapter performs no read.
        let (producer, mut consumer) = audio_ring(1, 4).unwrap();
        let mut pump = IndexedConvertedReadAhead::new(
            source(0),
            || Ok(source(mode)),
            producer,
            &consumer,
            options(),
        )
        .unwrap();
        pump.seek_indexed_stopped(&mut consumer, 1).unwrap();
        pump.pump().unwrap_err();
        pump.pump().unwrap_err();
        assert_eq!(pump.counts(), counts);
        let (reader, _, _, _) = pump.into_parts();
        assert_eq!(reader.reads.load(Ordering::Relaxed), calls);
    }
}
