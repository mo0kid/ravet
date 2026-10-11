#![cfg(feature = "resampled-playback")]
use ravet_audio_file::{
    ConvertedReadAheadOptions, FilePlaybackOptions, IndexedConvertedReadAhead, PlaybackConversion,
    WaveReader, audio_ring, prepare_file_playback,
};
use std::{
    io::{self, Read, Seek, SeekFrom},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
/// A real RIFF byte layout supplied on demand; no giant PCM allocation or file.
#[derive(Clone)]
struct SparseWave {
    header: [u8; 44],
    length: u64,
    position: u64,
    impulse: u64,
}
impl SparseWave {
    fn new(frames: u32) -> Self {
        let mut header = [0u8; 44];
        header[0..4].copy_from_slice(b"RIFF");
        header[4..8].copy_from_slice(&(frames * 4 + 36).to_le_bytes());
        header[8..12].copy_from_slice(b"WAVE");
        header[12..16].copy_from_slice(b"fmt ");
        header[16..20].copy_from_slice(&16u32.to_le_bytes());
        header[20..22].copy_from_slice(&3u16.to_le_bytes());
        header[22..24].copy_from_slice(&1u16.to_le_bytes());
        header[24..28].copy_from_slice(&768000u32.to_le_bytes());
        header[28..32].copy_from_slice(&(768000u32 * 4).to_le_bytes());
        header[32..34].copy_from_slice(&4u16.to_le_bytes());
        header[34..36].copy_from_slice(&32u16.to_le_bytes());
        header[36..40].copy_from_slice(b"data");
        header[40..44].copy_from_slice(&(frames * 4).to_le_bytes());
        Self {
            header,
            length: 44 + u64::from(frames) * 4,
            position: 0,
            impulse: 44 + u64::from(frames - 8) * 4,
        }
    }
}
impl Read for SparseWave {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let n = (self.length.saturating_sub(self.position)).min(output.len() as u64) as usize;
        output[..n].fill(0);
        let end = self.position + n as u64;
        if self.position < 44 {
            let count = end.min(44) - self.position;
            output[..count as usize].copy_from_slice(
                &self.header[self.position as usize..(self.position + count) as usize],
            );
        }
        let impulse = 1f32.to_le_bytes();
        for (offset, byte) in impulse.into_iter().enumerate() {
            let at = self.impulse + offset as u64;
            if self.position <= at && at < end {
                output[(at - self.position) as usize] = byte;
            }
        }
        self.position = end;
        Ok(n)
    }
}
impl Seek for SparseWave {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let position = match from {
            SeekFrom::Start(n) => i128::from(n),
            SeekFrom::Current(n) => i128::from(self.position) + i128::from(n),
            SeekFrom::End(n) => i128::from(self.length) + i128::from(n),
        };
        self.position = u64::try_from(position)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "sparse WAV seek"))?;
        Ok(self.position)
    }
}

#[derive(Default)]
struct Work {
    bytes: AtomicUsize,
    seeks: AtomicUsize,
}
struct Counted {
    source: SparseWave,
    work: Arc<Work>,
}
impl Read for Counted {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let n = self.source.read(output)?;
        self.work.bytes.fetch_add(n, Ordering::Relaxed);
        Ok(n)
    }
}
impl Seek for Counted {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.work.seeks.fetch_add(1, Ordering::Relaxed);
        self.source.seek(from)
    }
}
#[test]
fn actual_long_wave_indexed_window_work_is_target_independent_and_terminal_impulse_matches_replay()
{
    let base = 16_777_216u64;
    let n = base + 33;
    let total = (n + 14) / 16 + 1;
    let mut reference = prepare_file_playback(
        WaveReader::new(SparseWave::new(33), 17).unwrap(),
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
    let mut tail = vec![0.0; reference.info().prepared_frames];
    reference.playback_mut().play();
    reference.playback_mut().render(&mut tail).unwrap();
    assert_eq!(tail.len(), 3);
    assert!(tail.iter().any(|v| v.abs() > 0.001));
    let work = Arc::new(Work::default());
    let factory_work = Arc::clone(&work);
    let factory = move || {
        let reader = WaveReader::new(
            Counted {
                source: SparseWave::new(n as u32),
                work: Arc::clone(&factory_work),
            },
            17,
        )?;
        factory_work.bytes.store(0, Ordering::Relaxed);
        factory_work.seeks.store(0, Ordering::Relaxed);
        Ok(reader)
    };
    let reader = factory().unwrap();
    let (producer, mut consumer) = audio_ring(1, 4).unwrap();
    let mut pump = IndexedConvertedReadAhead::new(
        reader,
        factory,
        producer,
        &consumer,
        ConvertedReadAheadOptions {
            output_rate: 48000,
            conversion: PlaybackConversion::IntegerSinc { taps: 16 },
            maximum_source_frames: n as usize,
            maximum_output_frames: total,
            source_batch_frames: 7,
            conversion_batch_frames: 7,
        },
    )
    .unwrap();
    let first = pump.seek_indexed_stopped(&mut consumer, 0).unwrap();
    assert_eq!(first.window_original_frames, 1);
    assert_eq!(work.bytes.load(Ordering::Relaxed), 4);
    for j in [base / 16, total - 1, total] {
        let receipt = pump.seek_indexed_stopped(&mut consumer, j).unwrap();
        assert!(receipt.window_original_start > base - 32);
        assert!(receipt.window_original_frames <= 16);
        assert!(work.bytes.load(Ordering::Relaxed) <= 64);
        assert!(work.seeks.load(Ordering::Relaxed) <= 3);
        let mut actual = Vec::new();
        let mut eof = false;
        for _ in 0..100 {
            let p = pump.pump().unwrap();
            let mut samples = [0.0; 3];
            let frames = consumer.pop(&mut samples).unwrap();
            actual.extend_from_slice(&samples[..frames]);
            if p.end_of_file && consumer.queued_frames() == 0 {
                eof = true;
                break;
            }
        }
        assert!(eof);
        assert_eq!(actual, tail[(j - base / 16) as usize..]);
        let counts = pump.counts();
        assert_eq!(counts.original_reader_position, n);
        assert_eq!(counts.output_frames_generated_after_prime, total - j);
        assert_eq!(counts.output_frames_published, total - j);
        assert_eq!(
            counts.original_cursor_advance_after_prime,
            Some(n - receipt.reader_position)
        );
        assert_eq!(
            counts.original_frames_submitted_after_prime,
            n - receipt.reader_position
        );
    }
}
