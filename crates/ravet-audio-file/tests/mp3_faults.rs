#![cfg(feature = "mp3-encode")]
use ravet_audio_file::{
    AudioFileError as E, Mp3Bitrate, Mp3Format, Mp3WriteError as WE, Mp3Writer, encode_mp3,
};
use std::{
    io::{self, Cursor, Seek, SeekFrom, Write},
    sync::{Arc, Mutex},
};
#[derive(Debug)]
struct Marker(u32);
impl std::fmt::Display for Marker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "original marker{}", self.0)
    }
}
impl std::error::Error for Marker {}
#[derive(Default, Debug)]
struct Counts {
    write_behavior: u8,
    seeks: usize,
    writes: usize,
    flushes: usize,
    physical: usize,
    fail_seek: Option<usize>,
    bad_seek: Option<usize>,
    fail_write: Option<usize>,
    fail_flush: bool,
    physical_limit: Option<usize>,
}
#[derive(Debug)]
struct Sink {
    cursor: Cursor<Vec<u8>>,
    counts: Arc<Mutex<Counts>>,
}
impl Sink {
    fn new(prefix: Vec<u8>) -> (Self, Arc<Mutex<Counts>>) {
        let mut cursor = Cursor::new(prefix);
        cursor.set_position(cursor.get_ref().len() as u64);
        let counts = Arc::new(Mutex::new(Counts::default()));
        (
            Self {
                cursor,
                counts: Arc::clone(&counts),
            },
            counts,
        )
    }
}
impl Write for Sink {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        let mut c = self.counts.lock().unwrap();
        c.writes += 1;
        if c.write_behavior == 1 {
            return Ok(0);
        }
        if c.write_behavior == 2 {
            return Ok(b.len() + 1);
        }
        if c.fail_write == Some(c.writes) {
            return Err(io::Error::other(Marker(43)));
        }
        let n = c
            .physical_limit
            .map_or(b.len(), |l| b.len().min(l.saturating_sub(c.physical)));
        if n == 0 && !b.is_empty() {
            return Err(io::Error::other(Marker(44)));
        }
        let n = self.cursor.write(&b[..n])?;
        c.physical += n;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        let mut c = self.counts.lock().unwrap();
        c.flushes += 1;
        if c.fail_flush {
            return Err(io::Error::other(Marker(45)));
        }
        Ok(())
    }
}
impl Seek for Sink {
    fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
        let mut c = self.counts.lock().unwrap();
        c.seeks += 1;
        if c.fail_seek == Some(c.seeks) {
            self.cursor.set_position(19);
            return Err(io::Error::other(Marker(42)));
        }
        let actual = self.cursor.seek(p)?;
        Ok(if c.bad_seek == Some(c.seeks) {
            actual + 1
        } else {
            actual
        })
    }
}
fn format() -> Mp3Format {
    Mp3Format {
        channels: 2,
        sample_rate: 48000,
        bit_rate: Mp3Bitrate::Kbps192,
    }
}
fn source() -> Vec<f32> {
    (0..8193)
        .flat_map(|f| {
            [337., 977.].map(move |freq| {
                (0.55 * (std::f64::consts::TAU * freq * f as f64 / 48000.).sin()) as f32
            })
        })
        .collect()
}
fn fingerprint(e: &E) -> *const Marker {
    match e {
        E::Io(e) => e.get_ref().unwrap().downcast_ref::<Marker>().unwrap() as *const _,
        _ => panic!("unexpected{e:?}"),
    }
}
#[test]
fn constructor_rejection_and_original_seek_errors_recover_sink_without_retry() {
    for failure in [1, 2] {
        let (sink, c) = Sink::new(vec![]);
        c.lock().unwrap().fail_seek = Some(failure);
        let e = Mp3Writer::new(sink, format(), 8193, 65536).err().unwrap();
        assert!(matches!(e.error(), E::Io(_)));
        assert_eq!(c.lock().unwrap().seeks, failure);
        assert_eq!(e.written_bytes(), 0);
        let (sink, _) = e.into_parts();
        assert_eq!(sink.cursor.position(), 19);
        assert_eq!(c.lock().unwrap().writes, 0);
        drop(sink);
        assert_eq!(c.lock().unwrap().seeks, failure);
    }
    let (sink, c) = Sink::new(vec![]);
    let e = Mp3Writer::new(
        sink,
        Mp3Format {
            channels: 3,
            sample_rate: 48000,
            bit_rate: Mp3Bitrate::Kbps192,
        },
        8193,
        65536,
    )
    .err()
    .unwrap();
    assert!(matches!(e.error(), E::Unsupported));
    assert_eq!(c.lock().unwrap().seeks, 0);
    let (mut sink, c) = Sink::new(vec![1, 2, 3]);
    sink.cursor.set_position(1);
    let e = Mp3Writer::new(sink, format(), 8193, 65536).err().unwrap();
    assert!(matches!(e.error(), E::InvalidRecord));
    let (sink, _) = e.into_parts();
    assert_eq!(sink.cursor.position(), 1);
    assert_eq!(c.lock().unwrap().seeks, 3);
    let (mut sink, c) = Sink::new(vec![1, 2, 3]);
    sink.cursor.set_position(1);
    c.lock().unwrap().fail_seek = Some(3);
    let e = Mp3Writer::new(sink, format(), 8193, 65536).err().unwrap();
    assert!(matches!(e.error(), E::Io(_)));
    assert_eq!(c.lock().unwrap().seeks, 3);
}
#[test]
fn whole_call_preflight_does_not_mutate_codec_and_prefix_region_remains_exact() {
    let (sink, c) = Sink::new(vec![91, 92, 93]);
    let mut w = Mp3Writer::new(sink, format(), 8193, 65536).ok().unwrap();
    let src = source();
    let mut invalid = src.clone();
    *invalid.last_mut().unwrap() = f32::NAN;
    assert!(matches!(
        w.write_samples(&invalid),
        Err(WE::Rejected(E::InvalidSamples))
    ));
    assert_eq!(w.accepted_frames(), 0);
    assert_eq!(c.lock().unwrap().writes, 0);
    assert!(matches!(
        w.write_samples(&vec![0.; 65537 * 2]),
        Err(WE::Rejected(E::Capacity))
    ));
    assert_eq!(w.accepted_frames(), 0);
    w.write_samples(&src).unwrap();
    assert_eq!(w.accepted_frames(), 8193);
    let sink = w.finish().unwrap();
    assert_eq!(&sink.cursor.get_ref()[..3], &[91, 92, 93]);
    assert_eq!(
        &sink.cursor.get_ref()[3..],
        encode_mp3(format(), &src).unwrap()
    );
    assert_eq!(sink.cursor.position(), sink.cursor.get_ref().len() as u64);
    assert_eq!(c.lock().unwrap().flushes, 1);
}
#[test]
fn original_write_error_is_borrowed_then_recovered_and_poison_blocks_all_io() {
    let (sink, c) = Sink::new(vec![]);
    c.lock().unwrap().physical_limit = Some(101);
    let mut w = Mp3Writer::new(sink, format(), 8193, 65536).ok().unwrap();
    let pointer = match w.write_samples(&source()) {
        Err(WE::Failed(e)) => fingerprint(e),
        _ => panic!("expected original failure"),
    };
    let before = {
        let c = c.lock().unwrap();
        (c.writes, c.seeks)
    };
    assert!(matches!(w.write_samples(&[0., 0.]), Err(WE::Failed(_))));
    let e = w.finish().unwrap_err();
    assert_eq!(fingerprint(e.error()), pointer);
    assert_eq!((e.file_bytes(), e.written_bytes()), (101, 101));
    assert_eq!(c.lock().unwrap().writes, before.0);
    assert_eq!(c.lock().unwrap().seeks, before.1);
    assert_eq!(c.lock().unwrap().flushes, 0);
}
#[test]
fn finalize_seek_header_rewrite_restore_and_flush_faults_return_original_without_retry() {
    for failing in [3, 4] {
        let (sink, c) = Sink::new(vec![]);
        c.lock().unwrap().fail_seek = Some(failing);
        let mut w = Mp3Writer::new(sink, format(), 8193, 65536).ok().unwrap();
        w.write_samples(&source()).unwrap();
        let e = w.finish().unwrap_err();
        assert!(matches!(e.error(), E::Io(_)));
        assert_eq!(c.lock().unwrap().seeks, failing);
        assert_eq!(c.lock().unwrap().flushes, 0);
        let (sink, _) = e.into_parts();
        assert_eq!(sink.cursor.position(), 19);
    }
    for failing in [3, 4] {
        let (sink, c) = Sink::new(vec![]);
        c.lock().unwrap().bad_seek = Some(failing);
        let mut w = Mp3Writer::new(sink, format(), 8193, 65536).ok().unwrap();
        w.write_samples(&source()).unwrap();
        let e = w.finish().unwrap_err();
        assert!(matches!(e.error(), E::InvalidRecord));
        assert_eq!(c.lock().unwrap().seeks, failing);
        assert_eq!(c.lock().unwrap().flushes, 0);
    }
    let (sink, c) = Sink::new(vec![]);
    c.lock().unwrap().fail_flush = true;
    let mut w = Mp3Writer::new(sink, format(), 8193, 65536).ok().unwrap();
    w.write_samples(&source()).unwrap();
    let e = w.finish().unwrap_err();
    assert!(matches!(e.error(), E::Io(_)));
    assert_eq!(c.lock().unwrap().flushes, 1);
    assert_eq!(c.lock().unwrap().seeks, 4);
    let bytes = encode_mp3(format(), &source()).unwrap();
    let (sink, c) = Sink::new(vec![]);
    c.lock().unwrap().physical_limit = Some(bytes.len() + 19);
    let mut w = Mp3Writer::new(sink, format(), 8193, 65536).ok().unwrap();
    w.write_samples(&source()).unwrap();
    let e = w.finish().unwrap_err();
    assert!(matches!(e.error(), E::Io(_)));
    assert_eq!(e.file_bytes(), bytes.len() as u64);
    assert_eq!(e.written_bytes(), bytes.len() as u64 + 19);
    assert_eq!(c.lock().unwrap().seeks, 3);
    assert_eq!(c.lock().unwrap().flushes, 0);
}
#[test]
fn physical_budget_includes_rewrite_and_abort_drop_do_no_sink_io() {
    let bytes = encode_mp3(format(), &source()).unwrap();
    let frame = (144000 * 192 / 48000) as usize;
    for budget in [bytes.len(), bytes.len() + frame - 1] {
        let (sink, c) = Sink::new(vec![]);
        let mut w = Mp3Writer::new(sink, format(), 8193, budget as u64)
            .ok()
            .unwrap();
        w.write_samples(&source()).unwrap();
        let e = w.finish().unwrap_err();
        assert!(matches!(e.error(), E::Capacity));
        assert_eq!(
            (e.file_bytes(), e.written_bytes()),
            (bytes.len() as u64, bytes.len() as u64)
        );
        assert_eq!(c.lock().unwrap().seeks, 2);
        assert_eq!(c.lock().unwrap().flushes, 0);
    }
    for abort in [false, true] {
        let (sink, c) = Sink::new(vec![]);
        let mut w = Mp3Writer::new(sink, format(), 8193, 65536).ok().unwrap();
        w.write_samples(&source()).unwrap();
        let before = {
            let c = c.lock().unwrap();
            (c.seeks, c.writes, c.flushes)
        };
        if abort {
            drop(w.abort())
        } else {
            drop(w)
        }
        let c = c.lock().unwrap();
        assert_eq!((c.seeks, c.writes, c.flushes), before);
    }
    let (sink, c) = Sink::new(vec![]);
    let w = Mp3Writer::new(sink, format(), 8193, 65536).ok().unwrap();
    let e = w.finish().unwrap_err();
    assert!(matches!(e.error(), E::InvalidSamples));
    assert_eq!(c.lock().unwrap().writes, 0);
    assert_eq!(c.lock().unwrap().seeks, 2);
    assert_eq!(c.lock().unwrap().flushes, 0);
}

#[test]
fn zero_and_overreport_writes_poison_without_trusting_progress_or_retrying() {
    for behavior in [1, 2] {
        let (sink, c) = Sink::new(vec![]);
        c.lock().unwrap().write_behavior = behavior;
        let mut writer = Mp3Writer::new(sink, format(), 8193, 65536).ok().unwrap();
        assert!(matches!(
            writer.write_samples(&source()),
            Err(WE::Failed(_))
        ));
        assert!(writer.accepted_frames() > 0);
        assert_eq!((writer.file_bytes(), writer.written_bytes()), (0, 0));
        let before = {
            let c = c.lock().unwrap();
            (c.seeks, c.writes, c.flushes)
        };
        let failure = writer.finish().unwrap_err();
        assert!(if behavior == 1 {
            matches!(failure.error(),E::Io(e) if e.kind()==io::ErrorKind::WriteZero)
        } else {
            matches!(failure.error(), E::InvalidRecord)
        });
        let c = c.lock().unwrap();
        assert_eq!((c.seeks, c.writes, c.flushes), before);
    }
}

#[test]
fn all_constructor_format_and_budget_limits_reject_before_any_sink_action() {
    let good = format();
    for (profile, frames, bytes) in [
        (
            Mp3Format {
                sample_rate: 22050,
                ..good
            },
            1,
            65536,
        ),
        (
            Mp3Format {
                channels: 0,
                ..good
            },
            1,
            65536,
        ),
        (good, 0, 65536),
        (good, 16_777_217, 65536),
        (good, 1, 0),
        (good, 1, 67_108_865),
    ] {
        let (sink, c) = Sink::new(vec![]);
        let failure = Mp3Writer::new(sink, profile, frames, bytes).err().unwrap();
        assert!(matches!(failure.error(), E::Capacity | E::Unsupported));
        let c = c.lock().unwrap();
        assert_eq!((c.seeks, c.writes, c.flushes), (0, 0, 0));
    }
}
