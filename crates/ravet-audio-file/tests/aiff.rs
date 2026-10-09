use ravet_audio_file::{AiffEncoding, AiffFormat, AiffReader, AiffWriter, AudioFileError};
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
fn format(encoding: AiffEncoding, sample_rate: u32, channels: u16) -> AiffFormat {
    AiffFormat {
        encoding,
        sample_rate,
        channels,
    }
}
#[test]
fn partitions_seeks_endian_clipping_padding_and_integer_rate_edges() {
    for encoding in [
        AiffEncoding::Pcm16,
        AiffEncoding::Pcm24,
        AiffEncoding::Pcm32,
    ] {
        for rate in [1, 44100, 48000, 768000] {
            let f = format(encoding, rate, 2);
            let mut writer = AiffWriter::new(Cursor::new(Vec::new()), f, 4).unwrap();
            writer.write_frames(&[-2., 2., 0.25, -0.5]).unwrap();
            writer.write_frames(&[0., 0.125]).unwrap();
            assert_eq!(writer.frames(), 3);
            let source = writer.finish().unwrap();
            assert_eq!(
                u32::from_be_bytes(source.get_ref()[22..26].try_into().unwrap()),
                3
            );
            let mut reader = AiffReader::new(source, 4).unwrap();
            assert_eq!(reader.format(), f);
            assert_eq!(reader.frames(), 3);
            let mut output = [99.; 8];
            assert_eq!(reader.read_frames(&mut output).unwrap(), 3);
            assert_eq!(&output[6..], &[99.; 2]);
            let tolerance = match encoding {
                AiffEncoding::Pcm16 => 1. / 32768.,
                AiffEncoding::Pcm24 => 1. / 8388608.,
                AiffEncoding::Pcm32 => 1e-7,
            };
            for (actual, expected) in output[..6].iter().zip([-1., 1., 0.25, -0.5, 0., 0.125]) {
                assert!((actual - expected).abs() <= tolerance);
            }
            assert_eq!(reader.read_frames(&mut output).unwrap(), 0);
            reader.seek_frame(1).unwrap();
            let mut one = [99.; 2];
            assert_eq!(reader.read_frames(&mut one).unwrap(), 1);
            assert_eq!(one, [0.25, -0.5]);
            assert!(reader.seek_frame(4).is_err());
            assert_eq!(reader.position(), 2);
        }
    }
    let mut writer = AiffWriter::new(
        Cursor::new(Vec::new()),
        format(AiffEncoding::Pcm24, 44100, 1),
        1,
    )
    .unwrap();
    writer.write_frames(&[-0.5]).unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    assert_eq!(&bytes[28..38], &[0x40, 0x0e, 0xac, 0x44, 0, 0, 0, 0, 0, 0]);
    assert_eq!(&bytes[54..], &[0xc0, 0, 0, 0]);
    assert_eq!(bytes.len(), 58);
    assert_eq!(u32::from_be_bytes(bytes[42..46].try_into().unwrap()), 11);
}
fn fixture() -> Vec<u8> {
    // Independent reversed-chunk PCM16 stereo fixture: unknown odd padded chunk,
    // nonzero SSND offset, IEEE extended80 48000 and exact signed sample bytes.
    let mut bytes = b"FORM".to_vec();
    bytes.extend(62u32.to_be_bytes());
    bytes.extend(b"AIFFANNO");
    bytes.extend(1u32.to_be_bytes());
    bytes.extend([42, 0]);
    bytes.extend(b"SSND");
    bytes.extend(14u32.to_be_bytes());
    bytes.extend(2u32.to_be_bytes());
    bytes.extend(0u32.to_be_bytes());
    bytes.extend([0xff, 0xff, 0x40, 0, 0xc0, 0]);
    bytes.extend(b"COMM");
    bytes.extend(18u32.to_be_bytes());
    bytes.extend(2u16.to_be_bytes());
    bytes.extend(1u32.to_be_bytes());
    bytes.extend(16u16.to_be_bytes());
    bytes.extend([0x40, 0x0e, 0xbb, 0x80, 0, 0, 0, 0, 0, 0]);
    bytes
}
#[test]
fn independent_chunks_truncation_duplicate_and_malformed_metadata() {
    let bytes = fixture();
    assert_eq!(bytes.len(), 70);
    let mut reader = AiffReader::new(Cursor::new(bytes.clone()), 4).unwrap();
    let mut out = [99.; 4];
    assert_eq!(reader.read_frames(&mut out).unwrap(), 1);
    assert_eq!(out, [0.5, -0.5, 99., 99.]);
    for length in 0..bytes.len() {
        assert!(AiffReader::new(Cursor::new(bytes[..length].to_vec()), 4).is_err());
    }
    for (at, change) in [
        (26, u32::MAX.to_be_bytes().to_vec()),
        (30, 15u32.to_be_bytes().to_vec()),
        (34, 1u32.to_be_bytes().to_vec()),
        (52, 0u16.to_be_bytes().to_vec()),
        (54, 2u32.to_be_bytes().to_vec()),
        (58, 12u16.to_be_bytes().to_vec()),
        (60, 0xc00eu16.to_be_bytes().to_vec()),
        (69, vec![1]),
    ] {
        let mut bad = bytes.clone();
        bad[at..at + change.len()].copy_from_slice(&change);
        assert!(AiffReader::new(Cursor::new(bad), 4).is_err(), "offset {at}");
    }
    let mut duplicate = bytes.clone();
    duplicate.extend_from_slice(&bytes[44..]);
    let length = duplicate.len() as u32 - 8;
    duplicate[4..8].copy_from_slice(&length.to_be_bytes());
    assert!(AiffReader::new(Cursor::new(duplicate), 4).is_err());
    let mut empty = bytes[44..].to_vec();
    empty[10..14].fill(0);
    let mut container = b"FORM".to_vec();
    container.extend(30u32.to_be_bytes());
    container.extend(b"AIFF");
    container.extend(empty);
    let mut reader = AiffReader::new(Cursor::new(container), 4).unwrap();
    assert_eq!(reader.frames(), 0);
    assert_eq!(reader.read_frames(&mut out).unwrap(), 0);
    assert_eq!(out, [0.5, -0.5, 99., 99.]);
}
struct FaultRead {
    source: Cursor<Vec<u8>>,
    fail: Arc<AtomicBool>,
}
impl Read for FaultRead {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.fail.load(Ordering::Relaxed) && self.source.position() >= 54 {
            if self.source.position() >= 56 {
                return Err(io::Error::other("injected read"));
            }
            return self.source.read(&mut output[..2]);
        }
        self.source.read(output)
    }
}
impl Seek for FaultRead {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.source.seek(position)
    }
}
struct FaultWrite {
    source: Cursor<Vec<u8>>,
    remaining: usize,
}
impl Write for FaultWrite {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Err(io::Error::other("injected write"));
        }
        let count = bytes.len().min(self.remaining);
        self.remaining -= count;
        self.source.write(&bytes[..count])
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Seek for FaultWrite {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.source.seek(position)
    }
}
#[test]
fn validation_before_io_read_retry_and_poisoned_partial_write() {
    let f = format(AiffEncoding::Pcm16, 48000, 2);
    let mut writer = AiffWriter::new(Cursor::new(Vec::new()), f, 2).unwrap();
    for invalid in [&[f32::NAN, 0.][..], &[0.][..], &[0.; 6][..]] {
        assert!(writer.write_frames(invalid).is_err());
    }
    assert_eq!(writer.frames(), 0);
    writer.write_frames(&[0.5, -0.5]).unwrap();
    let fail = Arc::new(AtomicBool::new(false));
    let mut reader = AiffReader::new(
        FaultRead {
            source: writer.finish().unwrap(),
            fail: fail.clone(),
        },
        2,
    )
    .unwrap();
    let mut out = [99.; 2];
    fail.store(true, Ordering::Relaxed);
    assert!(reader.read_frames(&mut out).is_err());
    assert_eq!(out, [99.; 2]);
    assert_eq!(reader.position(), 0);
    fail.store(false, Ordering::Relaxed);
    assert_eq!(reader.read_frames(&mut out).unwrap(), 1);
    assert_eq!(out, [0.5, -0.5]);
    let mut writer = AiffWriter::new(
        FaultWrite {
            source: Cursor::new(Vec::new()),
            remaining: 56,
        },
        f,
        2,
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
