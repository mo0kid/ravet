#![cfg(all(feature = "flac-encode", feature = "compressed"))]
use ravet_audio_file::{
    AudioFileError as E, FlacFormat, FlacPcmEncoding as P, FlacWriter, decode_compressed,
    encode_flac,
};
use std::io::{self, Cursor, Seek, SeekFrom, Write};
fn format(channels: u16, encoding: P) -> FlacFormat {
    FlacFormat {
        channels,
        sample_rate: 48000,
        encoding,
    }
}
#[test]
fn incremental_partitioning_all_channels_extrema_short_tails_match_complete_encoding() {
    for channels in 1..=8 {
        for precision in [P::Pcm16, P::Pcm24] {
            for frames in [1, 17, 4096, 4097, 8211] {
                let limit = 1i32 << (precision.bits() - 1);
                let samples: Vec<i32> = (0..frames * usize::from(channels))
                    .map(|i| match i % 5 {
                        0 => -limit,
                        1 => limit - 1,
                        2 => 0,
                        _ => (i as i32 * 13007) % limit,
                    })
                    .collect();
                let mut writer = FlacWriter::new(
                    Cursor::new(Vec::new()),
                    format(channels, precision),
                    frames as u64,
                    1_048_576,
                )
                .unwrap();
                for chunk in samples.chunks(usize::from(channels) * 113) {
                    writer.write_frames(chunk).unwrap_or_else(|error| {
                        panic!(
                            "{channels}/{precision:?}/{frames}: {error}, accepted {} written {}",
                            writer.accepted_frames(),
                            writer.written_bytes()
                        )
                    });
                }
                assert_eq!(writer.accepted_frames(), frames as u64);
                assert_eq!(writer.pending_frames(), frames % 4096);
                assert!(!writer.is_poisoned());
                let encoded = writer.finish().unwrap();
                assert_eq!(encoded.position() as usize, encoded.get_ref().len());
                let bytes = encoded.into_inner();
                let whole = encode_flac(&samples, format(channels, precision)).unwrap();
                assert_eq!(bytes, whole, "{channels}/{precision:?}/{frames}");
                let decoded = decode_compressed(&bytes, frames).unwrap();
                assert_eq!(decoded.frames(), frames);
                assert_eq!(
                    decoded.samples(),
                    samples
                        .iter()
                        .map(|v| *v as f32 / limit as f32)
                        .collect::<Vec<_>>()
                );
            }
        }
    }
}
#[test]
fn rejected_writes_preserve_pending_output_digest_and_valid_continuation() {
    let mut writer =
        FlacWriter::new(Cursor::new(Vec::new()), format(2, P::Pcm16), 10, 65536).unwrap();
    writer.write_frames(&[1, -1, 2, -2]).unwrap();
    for samples in [vec![1], vec![32768, 0], vec![0; 18], vec![0; 65537 * 2]] {
        assert!(writer.write_frames(&samples).is_err());
        assert_eq!(
            (
                writer.accepted_frames(),
                writer.pending_frames(),
                writer.written_bytes()
            ),
            (2, 2, 42)
        );
        assert!(!writer.is_poisoned());
    }
    writer.write_frames(&[]).unwrap();
    writer.write_frames(&[3, -3]).unwrap();
    assert_eq!(
        writer.finish().unwrap().into_inner(),
        encode_flac(&[1, -1, 2, -2, 3, -3], format(2, P::Pcm16)).unwrap()
    );
    assert!(matches!(
        FlacWriter::new(Cursor::new(vec![1]), format(2, P::Pcm16), 10, 100),
        Err(E::InvalidRecord)
    ));
    for (frames, bytes) in [(0, 100), (1u64 << 36, 100), (10, 41)] {
        assert!(matches!(
            FlacWriter::new(Cursor::new(Vec::new()), format(1, P::Pcm16), frames, bytes),
            Err(E::Capacity)
        ));
    }
    for f in [
        format(0, P::Pcm16),
        format(9, P::Pcm16),
        FlacFormat {
            sample_rate: 0,
            ..format(1, P::Pcm16)
        },
    ] {
        assert!(matches!(
            FlacWriter::new(Cursor::new(Vec::new()), f, 10, 100),
            Err(E::Unsupported)
        ));
    }
    let empty = FlacWriter::new(Cursor::new(Vec::new()), format(1, P::Pcm16), 10, 100).unwrap();
    assert!(matches!(empty.finish(), Err(E::InvalidSamples)));
    let mut limited =
        FlacWriter::new(Cursor::new(Vec::new()), format(1, P::Pcm16), 8192, 42).unwrap();
    assert!(matches!(limited.write_frames(&[0; 4096]), Err(E::Capacity)));
    assert!(limited.is_poisoned());
    assert_eq!(limited.accepted_frames(), 4096);
    assert!(matches!(limited.write_frames(&[]), Err(E::Poisoned)));
    assert!(matches!(limited.finish(), Err(E::Poisoned)));
}
struct FailSink {
    inner: Cursor<Vec<u8>>,
    remaining: usize,
    fail_seek: bool,
    fail_flush: bool,
}
impl Write for FailSink {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Err(io::Error::other("synthetic write failure"));
        }
        let n = b.len().min(self.remaining);
        self.remaining -= n;
        self.inner.write(&b[..n])
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.fail_flush {
            Err(io::Error::other("synthetic flush failure"))
        } else {
            Ok(())
        }
    }
}
impl Seek for FailSink {
    fn seek(&mut self, s: SeekFrom) -> io::Result<u64> {
        if self.fail_seek && self.inner.get_ref().len() > 42 {
            Err(io::Error::other("synthetic final seek failure"))
        } else {
            self.inner.seek(s)
        }
    }
}
#[test]
fn output_failures_poison_partial_writes_and_finalization_reports_seek_flush_errors() {
    let sink = FailSink {
        inner: Cursor::new(Vec::new()),
        remaining: 43,
        fail_seek: false,
        fail_flush: false,
    };
    let mut writer = FlacWriter::new(sink, format(1, P::Pcm16), 8192, 65536).unwrap();
    assert!(matches!(writer.write_frames(&[0; 4096]), Err(E::Io(_))));
    assert_eq!(
        (writer.accepted_frames(), writer.written_bytes()),
        (4096, 42)
    );
    assert!(writer.is_poisoned());
    assert!(matches!(writer.write_frames(&[1]), Err(E::Poisoned)));
    for (seek, flush, budget) in [
        (true, false, usize::MAX),
        (false, true, usize::MAX),
        (false, false, 44),
    ] {
        let sink = FailSink {
            inner: Cursor::new(Vec::new()),
            remaining: budget,
            fail_seek: seek,
            fail_flush: flush,
        };
        let mut writer = FlacWriter::new(sink, format(1, P::Pcm16), 8192, 65536).unwrap();
        writer.write_frames(&[1]).unwrap();
        assert!(matches!(writer.finish(), Err(E::Io(_))));
    }
}
