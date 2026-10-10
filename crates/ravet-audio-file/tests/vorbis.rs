#![cfg(feature = "vorbis-encode")]
use ravet_audio_file::{AudioFileError as E, VorbisFormat, VorbisWriter, encode_vorbis};
use std::{
    cell::RefCell,
    io::{self, Write},
    rc::Rc,
};
fn format(channels: u16, sample_rate: u32) -> VorbisFormat {
    VorbisFormat {
        channels,
        sample_rate,
        quality: 1.,
        stream_serial: 0x12345678,
    }
}
fn signal(channels: u16, rate: u32, frames: usize) -> Vec<f32> {
    (0..frames)
        .flat_map(|frame| {
            (0..channels).map(move |channel| {
                let t = frame as f64 / f64::from(rate);
                let frequency = if channel == 0 { 337. } else { 977. };
                (0.55 * (std::f64::consts::TAU * frequency * t).sin()
                    + 0.1 * (std::f64::consts::TAU * 103. * t).sin()) as f32
            })
        })
        .collect()
}
fn ogg_timing(bytes: &[u8], expected: VorbisFormat, frames: usize) {
    let mut offset = 0;
    let mut sequence = 0;
    let mut last_granule = 0;
    while offset < bytes.len() {
        let page = &bytes[offset..];
        assert!(page.len() >= 27);
        assert_eq!(&page[..4], b"OggS");
        assert_eq!(page[4], 0);
        let flags = page[5];
        let granule = u64::from_le_bytes(page[6..14].try_into().unwrap());
        assert_eq!(
            i32::from_le_bytes(page[14..18].try_into().unwrap()),
            expected.stream_serial
        );
        assert_eq!(
            u32::from_le_bytes(page[18..22].try_into().unwrap()),
            sequence
        );
        let segments = usize::from(page[26]);
        let payload: usize = page[27..27 + segments]
            .iter()
            .map(|n| usize::from(*n))
            .sum();
        let size = 27 + segments + payload;
        assert!(size <= page.len());
        let declared_crc = u32::from_le_bytes(page[22..26].try_into().unwrap());
        let mut crc = 0_u32;
        for (index, byte) in page[..size].iter().enumerate() {
            let byte = if (22..26).contains(&index) { 0 } else { *byte };
            crc ^= u32::from(byte) << 24;
            for _ in 0..8 {
                crc = if crc & 0x80000000 != 0 {
                    (crc << 1) ^ 0x04c11db7
                } else {
                    crc << 1
                };
            }
        }
        assert_eq!(crc, declared_crc);
        if sequence == 0 {
            assert_eq!(flags & 2, 2);
            let header = &page[27 + segments..size];
            assert_eq!(&header[..7], b"\x01vorbis");
            assert_eq!(u16::from(header[11]), expected.channels);
            assert_eq!(
                u32::from_le_bytes(header[12..16].try_into().unwrap()),
                expected.sample_rate
            );
        } else {
            assert_eq!(flags & 2, 0);
        }
        if flags & 4 != 0 {
            assert_eq!(offset + size, bytes.len());
            assert_eq!(granule, frames as u64);
        }
        if granule != u64::MAX {
            assert!(granule >= last_granule);
            last_granule = granule;
        }
        offset += size;
        sequence += 1;
        if offset == bytes.len() {
            assert_eq!(flags & 4, 4);
        }
    }
    assert!(sequence >= 3);
    assert_eq!(last_granule, frames as u64);
}
fn snr(reference: &[f32], decoded: &[f32]) -> f64 {
    assert_eq!(reference.len(), decoded.len());
    let energy: f64 = reference.iter().map(|n| f64::from(*n).powi(2)).sum();
    let error: f64 = reference
        .iter()
        .zip(decoded)
        .map(|(a, b)| (f64::from(*a) - f64::from(*b)).powi(2))
        .sum();
    10. * (energy / error).log10()
}
#[cfg(feature = "compressed")]
#[test]
fn independent_decoder_rate_channels_timing_short_tails_and_lossy_pcm_agree() {
    for channels in [1, 2] {
        for rate in [8000, 16000, 22050, 44100, 48000, 96000] {
            if channels == 2 && rate < 16000 {
                continue;
            }
            for frames in [1, 17, 4096, 4097, 8193] {
                let format = format(channels, rate);
                let samples = signal(channels, rate, frames);
                let encoded = encode_vorbis(&samples, format).unwrap();
                ogg_timing(&encoded, format, frames);
                let decoded = ravet_audio_file::decode_compressed(&encoded, frames).unwrap();
                assert_eq!(
                    (decoded.channels(), decoded.sample_rate(), decoded.frames()),
                    (channels, rate, frames)
                );
                if frames >= 4096 {
                    assert!(
                        snr(&samples, decoded.samples()) > 25.,
                        "{}ch/{rate}Hz/{frames}: {}dB",
                        channels,
                        snr(&samples, decoded.samples())
                    );
                }
            }
        }
    }
}
#[cfg(feature = "compressed")]
#[test]
fn incremental_chunk_partitions_preflight_rejection_and_explicit_finish_preserve_signal() {
    let format = format(2, 48000);
    let samples = signal(2, 48000, 8193);
    let mut writer = VorbisWriter::new(Vec::new(), format, 8193, 65536).unwrap();
    let initial = writer.written_bytes();
    for malformed in [
        vec![0.],
        vec![f32::NAN, 0.],
        vec![f32::INFINITY, 0.],
        vec![1.0001, 0.],
        vec![-1.0001, 0.],
    ] {
        assert!(matches!(
            writer.write_frames(&malformed),
            Err(E::InvalidSamples)
        ));
        assert_eq!(
            (
                writer.accepted_frames(),
                writer.written_bytes(),
                writer.is_poisoned()
            ),
            (0, initial, false)
        );
    }
    assert!(matches!(
        writer.write_frames(&vec![0.; 8194 * 2]),
        Err(E::Capacity)
    ));
    writer.write_frames(&[]).unwrap();
    for chunk in samples.chunks(194) {
        writer.write_frames(chunk).unwrap();
    }
    assert_eq!(writer.accepted_frames(), 8193);
    assert!(matches!(writer.write_frames(&[0., 0.]), Err(E::Capacity)));
    let bytes = writer.finish().unwrap();
    ogg_timing(&bytes, format, 8193);
    let decoded = ravet_audio_file::decode_compressed(&bytes, 8193).unwrap();
    assert!(snr(&samples, decoded.samples()) > 25.);
}
#[derive(Default)]
struct Counts {
    bytes: Vec<u8>,
    writes: usize,
    flushes: usize,
    budget: Option<usize>,
    flush_failure: bool,
    zero_write: bool,
}
#[derive(Clone)]
struct Sink(Rc<RefCell<Counts>>);
impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut s = self.0.borrow_mut();
        s.writes += 1;
        if s.zero_write {
            return Ok(0);
        }
        let count = s.budget.map_or(bytes.len(), |budget| {
            budget.saturating_sub(s.bytes.len()).min(bytes.len())
        });
        if count == 0 && !bytes.is_empty() {
            return Err(io::Error::other("original selected sink fault"));
        }
        s.bytes.extend_from_slice(&bytes[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        let mut s = self.0.borrow_mut();
        s.flushes += 1;
        if s.flush_failure {
            Err(io::Error::other("original flush fault"))
        } else {
            Ok(())
        }
    }
}
#[test]
fn ordinary_empty_and_poisoned_drop_never_write_or_flush_a_sink() {
    for submitted in [false, true] {
        let counts = Rc::new(RefCell::new(Counts::default()));
        let mut writer =
            VorbisWriter::new(Sink(Rc::clone(&counts)), format(2, 48000), 32768, 65536).unwrap();
        if submitted {
            writer.write_frames(&signal(2, 48000, 13)).unwrap();
        }
        let before = (
            counts.borrow().bytes.len(),
            counts.borrow().writes,
            counts.borrow().flushes,
        );
        drop(writer);
        let s = counts.borrow();
        assert_eq!((s.bytes.len(), s.writes, s.flushes), before);
    }
    let counts = Rc::new(RefCell::new(Counts::default()));
    let mut writer =
        VorbisWriter::new(Sink(Rc::clone(&counts)), format(2, 48000), 32768, 65536).unwrap();
    let header = counts.borrow().bytes.len();
    counts.borrow_mut().budget = Some(header + 5);
    let error = writer.write_frames(&signal(2, 48000, 32768)).unwrap_err();
    match error {
        E::Io(error) => assert_eq!(error.to_string(), "original selected sink fault"),
        other => panic!("{other}"),
    }
    assert!(writer.is_poisoned());
    assert!(matches!(writer.write_frames(&[0., 0.]), Err(E::Poisoned)));
    let before = (
        counts.borrow().bytes.len(),
        counts.borrow().writes,
        counts.borrow().flushes,
    );
    assert_eq!(before.0, header + 5);
    drop(writer);
    let s = counts.borrow();
    assert_eq!((s.bytes.len(), s.writes, s.flushes), before);
    drop(s);
    let counts = Rc::new(RefCell::new(Counts::default()));
    let mut writer =
        VorbisWriter::new(Sink(Rc::clone(&counts)), format(1, 48000), 32768, 65536).unwrap();
    counts.borrow_mut().zero_write = true;
    match writer.write_frames(&signal(1, 48000, 32768)) {
        Err(E::Io(error)) => assert_eq!(error.kind(), io::ErrorKind::WriteZero),
        _ => panic!("expected zero-write fault"),
    }
    let before = counts.borrow().writes;
    drop(writer);
    assert_eq!(counts.borrow().writes, before);
    assert_eq!(counts.borrow().flushes, 0);
}
#[test]
fn output_capacity_poison_and_consuming_final_flush_errors_do_not_claim_complete_files() {
    let header = VorbisWriter::new(Vec::new(), format(2, 48000), 32768, 65536)
        .unwrap()
        .written_bytes();
    let counts = Rc::new(RefCell::new(Counts::default()));
    let mut writer = VorbisWriter::new(
        Sink(Rc::clone(&counts)),
        format(2, 48000),
        32768,
        header + 1,
    )
    .unwrap();
    assert!(matches!(
        writer.write_frames(&signal(2, 48000, 32768)),
        Err(E::Capacity)
    ));
    assert!(writer.is_poisoned());
    assert!(writer.written_bytes() <= header + 1);
    let before = counts.borrow().writes;
    assert!(matches!(writer.finish(), Err(E::Poisoned)));
    assert_eq!(counts.borrow().writes, before);
    assert_eq!(counts.borrow().flushes, 0);
    let counts = Rc::new(RefCell::new(Counts::default()));
    counts.borrow_mut().flush_failure = true;
    let mut writer =
        VorbisWriter::new(Sink(Rc::clone(&counts)), format(1, 48000), 17, 65536).unwrap();
    writer.write_frames(&[0.; 17]).unwrap();
    match writer.finish() {
        Err(E::Io(error)) => assert_eq!(error.to_string(), "original flush fault"),
        _ => panic!("expected terminal flush error"),
    }
    assert_eq!(counts.borrow().flushes, 1);
}
#[test]
fn explicit_format_count_and_source_limits_reject_before_header_output() {
    let valid = format(2, 48000);
    for format in [
        VorbisFormat {
            channels: 0,
            ..valid
        },
        VorbisFormat {
            channels: 3,
            ..valid
        },
        VorbisFormat {
            sample_rate: 7999,
            ..valid
        },
        VorbisFormat {
            sample_rate: 15999,
            ..valid
        },
        VorbisFormat {
            sample_rate: 96001,
            ..valid
        },
        VorbisFormat {
            quality: f32::NAN,
            ..valid
        },
        VorbisFormat {
            quality: -0.2001,
            ..valid
        },
        VorbisFormat {
            quality: 1.0001,
            ..valid
        },
    ] {
        let counts = Rc::new(RefCell::new(Counts::default()));
        assert!(matches!(
            VorbisWriter::new(Sink(Rc::clone(&counts)), format, 1, 65536),
            Err(E::Unsupported)
        ));
        assert_eq!(counts.borrow().writes, 0);
    }
    for (frames, bytes) in [(0, 65536), (16_777_217, 65536), (1, 0), (1, 67_108_865)] {
        let counts = Rc::new(RefCell::new(Counts::default()));
        assert!(matches!(
            VorbisWriter::new(Sink(Rc::clone(&counts)), valid, frames, bytes),
            Err(E::Capacity)
        ));
        assert_eq!(counts.borrow().writes, 0);
    }
    assert!(matches!(encode_vorbis(&[], valid), Err(E::Capacity)));
    assert!(matches!(
        encode_vorbis(&[0.], valid),
        Err(E::InvalidSamples)
    ));
    assert!(matches!(
        VorbisWriter::new(Vec::new(), valid, 1, 1),
        Err(E::Capacity)
    ));
    assert!(matches!(
        VorbisWriter::new(Vec::new(), valid, 1, 65536)
            .unwrap()
            .finish(),
        Err(E::InvalidSamples)
    ));
    let mut writer = VorbisWriter::new(Vec::new(), format(1, 48000), 65537, 65536).unwrap();
    assert!(matches!(
        writer.write_frames(&vec![0.; 65537]),
        Err(E::Capacity)
    ));
    assert_eq!(writer.accepted_frames(), 0);
    writer.write_frames(&[-1., 1., -0., 0.]).unwrap();
    let bytes = writer.finish().unwrap();
    ogg_timing(&bytes, format(1, 48000), 4);
    for quality in [-0.2, 1.] {
        for serial in [i32::MIN, 0, i32::MAX] {
            let format = VorbisFormat {
                quality,
                stream_serial: serial,
                ..format(1, 8000)
            };
            let bytes = encode_vorbis(&[0.; 17], format).unwrap();
            ogg_timing(&bytes, format, 17);
        }
    }
}
#[test]
#[ignore = "requires caller-selected independent FFmpeg native Vorbis decoder"]
fn ffmpeg_native_decoder_independently_agrees_on_pcm_timing_and_channel_order() {
    use std::process::{Command, Stdio};
    let executable = std::env::var_os("RAVET_TEST_FFMPEG").unwrap_or_else(|| "ffmpeg".into());
    for channels in [1, 2] {
        for rate in [8000, 16000, 22050, 44100, 48000, 96000] {
            if channels == 2 && rate < 16000 {
                continue;
            }
            let format = format(channels, rate);
            let source = signal(channels, rate, 8193);
            let encoded = encode_vorbis(&source, format).unwrap();
            ogg_timing(&encoded, format, 8193);
            let mut child = Command::new(&executable)
                .args([
                    "-v",
                    "error",
                    "-c:a",
                    "vorbis",
                    "-f",
                    "ogg",
                    "-i",
                    "pipe:0",
                    "-f",
                    "f32le",
                    "-c:a",
                    "pcm_f32le",
                    "pipe:1",
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let mut input = child.stdin.take().unwrap();
            let feed = std::thread::spawn(move || input.write_all(&encoded).map(|()| encoded));
            let result = child.wait_with_output().unwrap();
            let encoded = feed.join().unwrap().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(result.stdout.len(), source.len() * 4);
            let decoded: Vec<f32> = result
                .stdout
                .as_chunks::<4>()
                .0
                .iter()
                .map(|bytes| f32::from_le_bytes(*bytes))
                .collect();
            assert!(decoded.iter().all(|v| v.is_finite()));
            let quality = snr(&source, &decoded);
            println!("Native FFmpeg {channels}ch/{rate}Hz: {quality}dB");
            #[cfg(feature = "compressed")]
            {
                let independent = ravet_audio_file::decode_compressed(&encoded, 8193).unwrap();
                assert!(
                    decoded
                        .iter()
                        .zip(independent.samples())
                        .all(|(a, b)| (*a - *b).abs() < 0.0001)
                );
            }
            drop(encoded);
            assert!(quality > 25., "Native {channels}ch/{rate}Hz: {quality}dB");
        }
    }
}

#[test]
fn constructor_header_faults_preserve_original_error_without_retry_io() {
    let counts = Rc::new(RefCell::new(Counts::default()));
    counts.borrow_mut().budget = Some(5);
    match VorbisWriter::new(Sink(Rc::clone(&counts)), format(1, 48000), 17, 65536) {
        Err(E::Io(error)) => assert_eq!(error.to_string(), "original selected sink fault"),
        _ => panic!("expected constructor header error"),
    }
    let state = counts.borrow();
    assert_eq!((state.bytes.len(), state.writes, state.flushes), (5, 2, 0));
    drop(state);
    let counts = Rc::new(RefCell::new(Counts::default()));
    counts.borrow_mut().zero_write = true;
    match VorbisWriter::new(Sink(Rc::clone(&counts)), format(1, 48000), 17, 65536) {
        Err(E::Io(error)) => assert_eq!(error.kind(), io::ErrorKind::WriteZero),
        _ => panic!("expected constructor zero-write error"),
    }
    let state = counts.borrow();
    assert_eq!((state.bytes.len(), state.writes, state.flushes), (0, 1, 0));
    drop(state);
    let counts = Rc::new(RefCell::new(Counts::default()));
    assert!(matches!(
        VorbisWriter::new(Sink(Rc::clone(&counts)), format(1, 48000), 17, 1),
        Err(E::Capacity)
    ));
    let state = counts.borrow();
    assert_eq!((state.bytes.len(), state.writes, state.flushes), (0, 0, 0));
}

fn packets(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut result = Vec::new();
    let mut packet = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        let page = &bytes[offset..];
        assert_eq!(&page[..4], b"OggS");
        assert_eq!(page[5] & 1 != 0, !packet.is_empty());
        let segments = usize::from(page[26]);
        let mut body = 27 + segments;
        for lace in &page[27..27 + segments] {
            let length = usize::from(*lace);
            packet.extend_from_slice(&page[body..body + length]);
            body += length;
            if *lace < 255 {
                result.push(std::mem::take(&mut packet));
            }
        }
        offset += body;
    }
    assert!(packet.is_empty());
    result
}
#[test]
fn reframing_preserves_exact_backend_packets_serial_lacing_and_final_granules() {
    use std::num::{NonZeroU8, NonZeroU32};
    use vorbis_rs::{VorbisBitrateManagementStrategy as B, VorbisEncoderBuilder};
    for channels in [1, 2] {
        for frames in [1, 17, 4097, 8193] {
            let format = format(channels, 48000);
            let source = signal(channels, 48000, frames);
            let mut builder = VorbisEncoderBuilder::new_with_serial(
                NonZeroU32::new(format.sample_rate).unwrap(),
                NonZeroU8::new(channels as u8).unwrap(),
                Vec::new(),
                format.stream_serial,
            );
            builder.bitrate_management_strategy(B::QualityVbr {
                target_quality: format.quality,
            });
            builder.minimum_page_data_size(Some(0));
            let mut backend = builder.build().unwrap();
            for chunk in source.chunks(4096 * usize::from(channels)) {
                let planar: Vec<Vec<f32>> = (0..usize::from(channels))
                    .map(|channel| {
                        chunk
                            .chunks_exact(usize::from(channels))
                            .map(|frame| frame[channel])
                            .collect()
                    })
                    .collect();
                backend.encode_audio_block(&planar).unwrap();
            }
            let original = backend.finish().unwrap();
            let reframed = encode_vorbis(&source, format).unwrap();
            assert_eq!(packets(&original), packets(&reframed));
            ogg_timing(&reframed, format, frames);
            assert_eq!(reframed.len(), original.len() + 27);
        }
    }
}

fn direct_backend(samples: &[f32], format: VorbisFormat) -> Vec<u8> {
    use std::num::{NonZeroU8, NonZeroU32};
    use vorbis_rs::{VorbisBitrateManagementStrategy as B, VorbisEncoderBuilder};
    let channels = usize::from(format.channels);
    let mut builder = VorbisEncoderBuilder::new_with_serial(
        NonZeroU32::new(format.sample_rate).unwrap(),
        NonZeroU8::new(format.channels as u8).unwrap(),
        Vec::new(),
        format.stream_serial,
    );
    builder.bitrate_management_strategy(B::QualityVbr {
        target_quality: format.quality,
    });
    builder.minimum_page_data_size(Some(0));
    let mut encoder = builder.build().unwrap();
    for chunk in samples.chunks(4096 * channels) {
        let planar: Vec<Vec<f32>> = (0..channels)
            .map(|c| chunk.chunks_exact(channels).map(|frame| frame[c]).collect())
            .collect();
        encoder.encode_audio_block(&planar).unwrap();
    }
    encoder.finish().unwrap()
}
fn native_pcm(encoded: Vec<u8>) -> Vec<f32> {
    use std::process::{Command, Stdio};
    let executable = std::env::var_os("RAVET_TEST_FFMPEG").unwrap_or_else(|| "ffmpeg".into());
    let mut child = Command::new(executable)
        .args([
            "-v",
            "error",
            "-c:a",
            "vorbis",
            "-f",
            "ogg",
            "-i",
            "pipe:0",
            "-f",
            "f32le",
            "-c:a",
            "pcm_f32le",
            "pipe:1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let feed = std::thread::spawn(move || input.write_all(&encoded));
    let result = child.wait_with_output().unwrap();
    feed.join().unwrap().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    result
        .stdout
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| f32::from_le_bytes(*bytes))
        .collect()
}
#[test]
#[ignore = "requires independent FFmpeg for encoder/decoder alignment diagnosis"]
fn diagnose_low_rate_stereo_backend_framing_and_pcm_alignment() {
    for rate in [8000, 16000, 22050, 48000] {
        let format = format(2, rate);
        let source = signal(2, rate, 8193);
        let raw = direct_backend(&source, format);
        let header = packets(&raw);
        let long_window = 1_usize << (header[0][28] >> 4);
        let boundary = long_window / 2;
        let mut variants = vec![("direct", raw)];
        if rate >= 16000 {
            variants.push(("reframed", encode_vorbis(&source, format).unwrap()));
        } else {
            assert!(matches!(
                encode_vorbis(&source, format),
                Err(E::Unsupported)
            ));
        }
        for (kind, bytes) in variants {
            let decoded = native_pcm(bytes);
            println!(
                "{kind}/{rate}Hz: decoded{}frames; Vorbis long window{long_window}, edge analysis{boundary}",
                decoded.len() / 2
            );
            for channel in [0, 1] {
                let reference: Vec<f64> = source
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|f| f64::from(f[channel]))
                    .collect();
                let actual: Vec<f64> = decoded
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|f| f64::from(f[channel]))
                    .collect();
                let measure = |lag: isize, start: usize, end: usize| {
                    let mut reference_energy = 0.;
                    let mut actual_energy = 0.;
                    let mut cross = 0.;
                    let mut error = 0.;
                    for (frame, reference_sample) in
                        reference.iter().enumerate().take(end).skip(start)
                    {
                        let target = frame as isize + lag;
                        if target < 0 || target >= actual.len() as isize {
                            continue;
                        }
                        let r = *reference_sample;
                        let d = actual[target as usize];
                        reference_energy += r * r;
                        actual_energy += d * d;
                        cross += r * d;
                        error += (r - d).powi(2);
                    }
                    let gain = cross / reference_energy;
                    let correlation = cross / (reference_energy * actual_energy).sqrt();
                    let snr = 10. * (reference_energy / error).log10();
                    let fitted_error = actual_energy - cross * cross / reference_energy;
                    let fitted_snr =
                        10. * ((gain * gain * reference_energy) / fitted_error).log10();
                    (correlation, gain, snr, fitted_snr)
                };
                let whole = measure(0, 0, reference.len());
                let interior = measure(0, boundary, reference.len() - boundary);
                let (lag, best) = (-256_isize..=256)
                    .map(|lag| (lag, measure(lag, boundary, reference.len() - boundary)))
                    .max_by(|a, b| a.1.0.total_cmp(&b.1.0))
                    .unwrap();
                println!(
                    "channel{channel}: wholeSNR{:.5}dB interiorSNR{:.5}dB bestlag{lag} corr{:.9} gain{:.9} fittedSNR{:.5}dB",
                    whole.2, interior.2, best.0, best.1, best.3
                );
                for quarter in 0..4 {
                    let from = quarter * reference.len() / 4;
                    let to = (quarter + 1) * reference.len() / 4;
                    println!(
                        "channel{channel}quarter{quarter}: SNR{:.5}dB",
                        measure(0, from, to).2
                    );
                }
            }
        }
    }
}
