#![cfg(feature = "resampled-playback")]
use ravet_audio_file::{
    AiffEncoding, AiffFormat, AiffReader, AiffWriter, AudioFileError as Error,
    ConvertedReadAheadOptions, FilePlaybackOptions, IndexedConvertedReadAhead,
    IndexedPlaybackReader, PlaybackConversion, WaveEncoding, WaveFormat, WaveReader, WaveWriter,
    audio_ring, prepare_file_playback,
};
use std::{
    io::{self, Cursor, Read, Seek, SeekFrom},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
#[derive(Default)]
struct Work {
    bytes: AtomicUsize,
    reads: AtomicUsize,
    seeks: AtomicUsize,
}
impl Work {
    fn reset(&self) {
        self.bytes.store(0, Ordering::Relaxed);
        self.reads.store(0, Ordering::Relaxed);
        self.seeks.store(0, Ordering::Relaxed);
    }
}
struct Tracked<R> {
    source: R,
    work: Arc<Work>,
}
impl<R: Read> Read for Tracked<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.work.reads.fetch_add(1, Ordering::Relaxed);
        let n = self.source.read(output)?;
        self.work.bytes.fetch_add(n, Ordering::Relaxed);
        Ok(n)
    }
}
impl<R: Seek> Seek for Tracked<R> {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.work.seeks.fetch_add(1, Ordering::Relaxed);
        self.source.seek(from)
    }
}
fn exercise<R: IndexedPlaybackReader, F: FnMut() -> Result<R, Error>>(
    mut factory: F,
    work: &Work,
    encoding_bytes: usize,
    n: usize,
    input_rate: u32,
    output_rate: u32,
    taps: usize,
) {
    let conversion = PlaybackConversion::IntegerSinc { taps };
    let (full, total) = if n == 0 {
        (Vec::new(), 0)
    } else {
        let mut reference = prepare_file_playback(
            factory().unwrap(),
            FilePlaybackOptions {
                output_rate,
                conversion,
                maximum_source_frames: n.max(1),
                maximum_output_frames: 200000,
                source_batch_frames: 13,
                maximum_render_frames: 127,
            },
        )
        .unwrap();
        let total = reference.info().prepared_frames;
        let mut full = vec![0.0; total * 2];
        reference.playback_mut().play();
        for chunk in full.chunks_mut(254) {
            reference.playback_mut().render(chunk).unwrap();
        }
        (full, total)
    };
    assert_eq!(
        total,
        if n == 0 {
            0
        } else {
            ((n as u128 + taps as u128 - 2) * output_rate as u128 / input_rate as u128 + 1) as usize
        }
    );
    let reader = factory().unwrap();
    let (producer, mut consumer) = audio_ring(2, 32).unwrap();
    let mut pump = IndexedConvertedReadAhead::new(
        reader,
        factory,
        producer,
        &consumer,
        ConvertedReadAheadOptions {
            output_rate,
            conversion,
            maximum_source_frames: n.max(1),
            maximum_output_frames: 200000,
            source_batch_frames: 7,
            conversion_batch_frames: 11,
        },
    )
    .unwrap();
    for j in [0, total.min(1), total / 2, total.saturating_sub(1), total] {
        let receipt = pump.seek_indexed_stopped(&mut consumer, j as u64).unwrap();
        assert!(receipt.window_original_frames <= taps);
        assert_eq!(receipt.verified_end_point, j == total);
        assert!(work.bytes.load(Ordering::Relaxed) <= taps * 2 * encoding_bytes);
        assert!(work.reads.load(Ordering::Relaxed) <= taps.div_ceil(7));
        assert!(work.seeks.load(Ordering::Relaxed) <= taps.div_ceil(7));
        assert_eq!(pump.counts().original_frames_submitted_after_prime, 0);
        assert_eq!(pump.counts().output_frames_generated_after_prime, 0);
        let mut suffix = Vec::new();
        let mut ended = false;
        for _ in 0..30000 {
            let p = pump.pump().unwrap();
            let mut samples = [0.0; 34];
            let n = consumer.pop(&mut samples).unwrap();
            suffix.extend_from_slice(&samples[..n * 2]);
            if p.end_of_file && consumer.queued_frames() == 0 {
                ended = true;
                break;
            }
        }
        assert!(ended);
        assert_eq!(
            suffix,
            full[j * 2..],
            "indexed {input_rate}/{output_rate}, n={n}, j={j}"
        );
        assert_eq!(pump.counts().absolute_output_generated, total as u64);
        assert_eq!(pump.counts().output_frames_published, (total - j) as u64);
    }
}
#[test]
fn actual_wave_and_aiff_suffixes_and_payload_work_bound_cover_first_tail_and_eof() {
    for n in [0, 1, 17, 257, 8193] {
        let samples: Vec<f32> = (0..n)
            .flat_map(|v| [(v as f32 * 0.13).sin() * 0.5, (v as f32 * 0.07).cos() * 0.5])
            .collect();
        for (i, o) in [(44100, 48000), (48000, 44100), (48000, 3000), (3000, 48000)] {
            let mut wave = WaveWriter::new(
                Cursor::new(Vec::new()),
                WaveFormat {
                    channels: 2,
                    sample_rate: i,
                    encoding: WaveEncoding::Float32,
                },
                8193,
            )
            .unwrap();
            wave.write_frames(&samples).unwrap();
            let bytes = wave.finish().unwrap().into_inner();
            let work = Arc::new(Work::default());
            let reader_work = Arc::clone(&work);
            let factory = move || {
                let r = WaveReader::new(
                    Tracked {
                        source: Cursor::new(bytes.clone()),
                        work: Arc::clone(&reader_work),
                    },
                    17,
                )?;
                reader_work.reset();
                Ok(r)
            };
            exercise(factory, &work, 4, n, i, o, 64);
        }
        let mut aiff = AiffWriter::new(
            Cursor::new(Vec::new()),
            AiffFormat {
                channels: 2,
                sample_rate: 44100,
                encoding: AiffEncoding::Pcm16,
            },
            8193,
        )
        .unwrap();
        aiff.write_frames(&samples).unwrap();
        let bytes = aiff.finish().unwrap().into_inner();
        let work = Arc::new(Work::default());
        let reader_work = Arc::clone(&work);
        let factory = move || {
            let r = AiffReader::new(
                Tracked {
                    source: Cursor::new(bytes.clone()),
                    work: Arc::clone(&reader_work),
                },
                17,
            )?;
            reader_work.reset();
            Ok(r)
        };
        exercise(factory, &work, 2, n, 44100, 48000, 64);
    }
}
