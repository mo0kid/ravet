#![cfg(feature = "resampled-playback")]
use ravet_audio_file::{
    ConvertedReadAhead, ConvertedReadAheadOptions, PlaybackConversion, WaveReader, audio_ring,
};
use std::io::{self, Read, Seek, SeekFrom};
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
#[test]
fn actual_wave_reader_streams_past_old_ceiling_in_bounded_batches_through_verified_eof() {
    let n = 16_777_216u32 + 33;
    let bytes = SparseWave::new(n);
    let reader = WaveReader::new(bytes.clone(), 4096).unwrap();
    assert_eq!(reader.frames(), u64::from(n));
    let factory = move || WaveReader::new(bytes.clone(), 4096);
    let (producer, mut consumer) = audio_ring(1, 256).unwrap();
    let total = (u64::from(n) + 14) / 16 + 1;
    let mut pump = ConvertedReadAhead::new(
        reader,
        factory,
        producer,
        &consumer,
        ConvertedReadAheadOptions {
            output_rate: 48000,
            conversion: PlaybackConversion::IntegerSinc { taps: 16 },
            maximum_source_frames: n as usize,
            maximum_output_frames: total,
            source_batch_frames: 4096,
            conversion_batch_frames: 113,
        },
    )
    .unwrap();
    let mut delivered = 0u64;
    let mut peak = 0f32;
    let mut verified = false;
    for _ in 0..100_000 {
        let p = pump.pump().unwrap();
        let mut output = [0.0; 127];
        let count = consumer.pop(&mut output).unwrap();
        delivered += count as u64;
        peak = output[..count].iter().map(|v| v.abs()).fold(peak, f32::max);
        if p.end_of_file && consumer.queued_frames() == 0 {
            verified = true;
            break;
        }
    }
    assert!(verified);
    assert!(peak > 0.001);
    assert_eq!(delivered, total);
    assert_eq!(pump.counts().input_frames_read, u64::from(n));
    assert_eq!(pump.counts().input_frames_submitted, u64::from(n));
    assert_eq!(pump.output_generated_position(), total);
    let (reader, _, _, counts) = pump.into_parts();
    assert_eq!(reader.position(), u64::from(n));
    assert_eq!(counts.output_frames_published, total);
}
