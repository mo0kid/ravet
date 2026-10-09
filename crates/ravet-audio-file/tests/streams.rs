use ravet_audio_file::{
    AudioFileError, RingError, WaveEncoding, WaveFormat, WaveReader, WaveWriter, audio_ring,
};
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
fn format(encoding: WaveEncoding, channels: u16) -> WaveFormat {
    WaveFormat {
        encoding,
        channels,
        sample_rate: 48000,
    }
}
#[test]
fn wav_roundtrips_partitions_seek_padding_and_tails() {
    for encoding in [
        WaveEncoding::Pcm16,
        WaveEncoding::Pcm24,
        WaveEncoding::Pcm32,
        WaveEncoding::Float32,
    ] {
        let mut writer = WaveWriter::new(Cursor::new(Vec::new()), format(encoding, 2), 4).unwrap();
        writer.write_frames(&[-1., 1., 0.25, -0.5]).unwrap();
        writer.write_frames(&[0., 0.125]).unwrap();
        assert_eq!(writer.frames(), 3);
        let source = writer.finish().unwrap();
        let bytes = source.get_ref();
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8,
            bytes.len()
        );
        if encoding == WaveEncoding::Float32 {
            assert_eq!(&bytes[38..42], b"fact");
            assert_eq!(u32::from_le_bytes(bytes[46..50].try_into().unwrap()), 3);
        }
        let mut reader = WaveReader::new(source, 4).unwrap();
        assert_eq!(reader.frames(), 3);
        assert_eq!(reader.format(), format(encoding, 2));
        let mut output = [99.; 8];
        assert_eq!(reader.read_frames(&mut output).unwrap(), 3);
        assert_eq!(&output[6..], &[99.; 2]);
        let tolerance = match encoding {
            WaveEncoding::Pcm16 => 1. / 32768.,
            WaveEncoding::Pcm24 => 1. / 8388608.,
            WaveEncoding::Pcm32 => 1e-7,
            WaveEncoding::Float32 => 0.,
        };
        for (actual, expected) in output[..6].iter().zip([-1., 1., 0.25, -0.5, 0., 0.125]) {
            assert!(
                (actual - expected).abs() <= tolerance,
                "{encoding:?}: {actual} vs {expected}"
            );
        }
        assert_eq!(reader.read_frames(&mut output).unwrap(), 0);
        reader.seek_frame(1).unwrap();
        let mut one = [99.; 2];
        assert_eq!(reader.read_frames(&mut one).unwrap(), 1);
        assert_eq!(one, [0.25, -0.5]);
        assert!(reader.seek_frame(4).is_err());
        assert_eq!(reader.position(), 2);
    }
    let mut writer =
        WaveWriter::new(Cursor::new(Vec::new()), format(WaveEncoding::Pcm24, 1), 1).unwrap();
    writer.write_frames(&[-0.5]).unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    assert_eq!(bytes.len(), 48);
    assert_eq!(&bytes[44..], &[0, 0, 192, 0]);
    assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 3);
}
fn fixture() -> Vec<u8> {
    // Independent stereo PCM16 fixture with an odd-sized unknown padded chunk.
    let mut bytes = b"RIFF".to_vec();
    bytes.extend(50u32.to_le_bytes());
    bytes.extend(b"WAVEJUNK");
    bytes.extend(1u32.to_le_bytes());
    bytes.extend([42, 0]);
    bytes.extend(b"fmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(2u16.to_le_bytes());
    bytes.extend(48000u32.to_le_bytes());
    bytes.extend(192000u32.to_le_bytes());
    bytes.extend(4u16.to_le_bytes());
    bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(4u32.to_le_bytes());
    bytes.extend([0, 64, 0, 192]);
    bytes
}
#[test]
fn independent_chunk_fixture_truncation_and_malformed_headers() {
    let bytes = fixture();
    let mut reader = WaveReader::new(Cursor::new(bytes.clone()), 4).unwrap();
    let mut out = [99.; 4];
    assert_eq!(reader.read_frames(&mut out).unwrap(), 1);
    assert_eq!(out, [0.5, -0.5, 99., 99.]);
    for length in 0..bytes.len() {
        assert!(WaveReader::new(Cursor::new(bytes[..length].to_vec()), 4).is_err());
    }
    let mut bad = bytes.clone();
    bad[38..42].copy_from_slice(&1u32.to_le_bytes());
    assert!(WaveReader::new(Cursor::new(bad), 4).is_err());
    let mut bad = bytes.clone();
    bad[50..54].copy_from_slice(&3u32.to_le_bytes());
    assert!(WaveReader::new(Cursor::new(bad), 4).is_err());
    let mut bad = bytes;
    bad[26..30].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(WaveReader::new(Cursor::new(bad), 4).is_err());
}
struct FaultRead {
    data: Cursor<Vec<u8>>,
    failed: Arc<AtomicBool>,
}
impl Read for FaultRead {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.failed.load(Ordering::Relaxed) && self.data.position() >= 44 {
            if self.data.position() >= 46 {
                return Err(io::Error::other("injected read"));
            }
            let count = out.len().min(2);
            return self.data.read(&mut out[..count]);
        }
        self.data.read(out)
    }
}
impl Seek for FaultRead {
    fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
        self.data.seek(p)
    }
}
struct FaultWrite {
    data: Cursor<Vec<u8>>,
    remaining: usize,
}
impl Write for FaultWrite {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Err(io::Error::other("injected write"));
        }
        let count = bytes.len().min(self.remaining);
        self.remaining -= count;
        self.data.write(&bytes[..count])
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Seek for FaultWrite {
    fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
        self.data.seek(p)
    }
}
#[test]
fn io_failure_preserves_read_destination_and_poisons_writes() {
    let mut writer =
        WaveWriter::new(Cursor::new(Vec::new()), format(WaveEncoding::Pcm16, 2), 4).unwrap();
    assert!(matches!(
        writer.write_frames(&[f32::NAN, 0.]),
        Err(AudioFileError::InvalidSamples)
    ));
    assert_eq!(writer.frames(), 0);
    writer.write_frames(&[0.5, -0.5]).unwrap();
    let data = writer.finish().unwrap();
    let flag = Arc::new(AtomicBool::new(false));
    let mut reader = WaveReader::new(
        FaultRead {
            data,
            failed: Arc::clone(&flag),
        },
        4,
    )
    .unwrap();
    flag.store(true, Ordering::Relaxed);
    let mut out = [99.; 2];
    assert!(reader.read_frames(&mut out).is_err());
    assert_eq!(out, [99.; 2]);
    assert_eq!(reader.position(), 0);
    flag.store(false, Ordering::Relaxed);
    assert_eq!(reader.read_frames(&mut out).unwrap(), 1);
    assert_eq!(out, [0.5, -0.5]);
    let mut writer = WaveWriter::new(
        FaultWrite {
            data: Cursor::new(Vec::new()),
            remaining: 46,
        },
        format(WaveEncoding::Pcm16, 2),
        4,
    )
    .unwrap();
    assert!(matches!(
        writer.write_frames(&[0.5, -0.5]),
        Err(AudioFileError::Io(_))
    ));
    assert!(matches!(
        writer.write_frames(&[0., 0.]),
        Err(AudioFileError::Poisoned)
    ));
    assert!(matches!(writer.finish(), Err(AudioFileError::Poisoned)));
}
#[test]
fn ring_backpressure_order_tails_partial_frames_and_heap() {
    let (mut producer, mut consumer) = audio_ring(2, 4).unwrap();
    assert_eq!(
        producer
            .push(&[1., -1., 2., -2., 3., -3., 4., -4., 5., -5.])
            .unwrap(),
        4
    );
    let mut out = [99.; 4];
    assert_eq!(consumer.pop(&mut out).unwrap(), 2);
    assert_eq!(out, [1., -1., 2., -2.]);
    assert_eq!(producer.push(&[5., -5., 6., -6.]).unwrap(), 2);
    let mut rest = [99.; 10];
    assert_eq!(consumer.pop(&mut rest).unwrap(), 4);
    assert_eq!(rest, [3., -3., 4., -4., 5., -5., 6., -6., 99., 99.]);
    assert_eq!(consumer.pop(&mut rest).unwrap(), 0);
    assert_eq!(producer.push(&[1.]), Err(RingError::PartialFrame));
    let counts = allocation_counter::measure(|| {
        for _ in 0..10000 {
            producer.push(&[0.5, -0.5]).unwrap();
            consumer.pop(&mut out).unwrap();
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    assert!(audio_ring(2, 3).is_err());
}
#[test]
fn ring_two_thread_ordered_frame_handoff() {
    let (mut producer, mut consumer) = audio_ring(2, 128).unwrap();
    let writer = std::thread::spawn(move || {
        for frame in 0..100000 {
            let pair = [frame as f32, -(frame as f32)];
            while producer.push(&pair).unwrap() == 0 {
                std::thread::yield_now();
            }
        }
    });
    let mut next = 0;
    let mut buffer = [0.; 14];
    while next < 100000 {
        let count = consumer.pop(&mut buffer).unwrap();
        for frame in 0..count {
            assert_eq!(
                &buffer[frame * 2..frame * 2 + 2],
                &[next as f32, -(next as f32)]
            );
            next += 1;
        }
        if count == 0 {
            std::thread::yield_now();
        }
    }
    writer.join().unwrap();
}

#[test]
fn float_wave_preserves_nonfinite_payload_bits() {
    let input = [
        f32::from_bits(0x7fc01234),
        f32::INFINITY,
        f32::NEG_INFINITY,
        -0.0,
    ];
    let mut writer =
        WaveWriter::new(Cursor::new(Vec::new()), format(WaveEncoding::Float32, 1), 4).unwrap();
    writer.write_frames(&input).unwrap();
    let mut reader = WaveReader::new(writer.finish().unwrap(), 4).unwrap();
    let mut output = [0.; 4];
    assert_eq!(reader.read_frames(&mut output).unwrap(), 4);
    assert_eq!(input.map(f32::to_bits), output.map(f32::to_bits));
}
