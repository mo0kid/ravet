#![cfg(feature = "resampled-playback")]
//! File-backed impulse-response preparation matches the memory path exactly and
//! refuses invalid sources before reading.
use ravet_audio_file::{
    AiffEncoding, AiffFormat, AiffReader, AiffWriter, AudioFileError, ImpulseResponseFileErrorKind,
    ImpulseResponseFileOptions, PlaybackReader, WaveEncoding, WaveFormat, WaveReader, WaveWriter,
    prepare_impulse_response_file,
};
use ravet_signal::{
    ImpulseResponseError, ImpulseResponseRequest, ResponseChannelMap, ResponseDelay,
    ResponseNormalization, ResponseRateConversion, ResponseRateGain, ResponseTrim, TrimThreshold,
    prepare_impulse_response,
};
use std::io::Cursor;

const OPTIONS: ImpulseResponseFileOptions = ImpulseResponseFileOptions {
    maximum_source_frames: 4096,
    source_batch_frames: 100,
};

fn stereo() -> (Vec<f32>, Vec<f64>, Vec<f64>) {
    let frames = 700;
    let mut interleaved = Vec::new();
    let (mut l, mut r) = (Vec::new(), Vec::new());
    for i in 0..frames {
        let decay = (-(i as f32) / 90.).exp();
        let a = if !(20..500).contains(&i) {
            0.
        } else {
            decay * ((i % 13) as f32 / 13. - 0.5)
        };
        let b = if !(25..480).contains(&i) {
            0.
        } else {
            decay * ((i % 7) as f32 / 7. - 0.4)
        };
        interleaved.extend([a, b]);
        l.push(f64::from(a));
        r.push(f64::from(b));
    }
    (interleaved, l, r)
}

fn wave(samples: &[f32], channels: u16, rate: u32) -> WaveReader<Cursor<Vec<u8>>> {
    let format = WaveFormat {
        channels,
        sample_rate: rate,
        encoding: WaveEncoding::Float32,
    };
    let mut writer = WaveWriter::new(Cursor::new(Vec::new()), format, 4096).unwrap();
    writer.write_frames(samples).unwrap();
    WaveReader::new(Cursor::new(writer.finish().unwrap().into_inner()), 512).unwrap()
}

fn full_request(map: &[usize]) -> ImpulseResponseRequest<'_> {
    ImpulseResponseRequest {
        target_rate: 48_000,
        conversion: ResponseRateConversion::Sinc {
            taps: 64,
            gain: ResponseRateGain::PreserveResponseGain,
            delay: ResponseDelay::RemoveWholeTargetFrames,
        },
        channels: ResponseChannelMap::Select(map),
        trim: ResponseTrim {
            leading: Some(TrimThreshold::Absolute(1e-3)),
            trailing: Some(TrimThreshold::RelativeToPeak(1e-3)),
        },
        normalization: ResponseNormalization::Energy { target: 1.0 },
        maximum_taps: 262_144,
        maximum_working_bytes: 16 << 20,
    }
}

#[test]
fn wave_file_preparation_equals_memory_preparation_bitwise() {
    let (interleaved, l, r) = stereo();
    let map = [1usize, 0];
    let request = full_request(&map);
    let mut reader = wave(&interleaved, 2, 44_100);
    let from_file = prepare_impulse_response_file(&mut reader, OPTIONS, &request).unwrap();
    let from_memory = prepare_impulse_response(44_100, &[&l, &r], &request).unwrap();
    assert_eq!(from_file, from_memory);
    assert_eq!(reader.position_frames(), 700);
    let report = from_file.report();
    assert_eq!(report.source_frames, 700);
    assert!(report.leading_trimmed_frames > 0 && report.trailing_trimmed_frames > 0);
    assert!((report.maximum_channel_energy - 1.0).abs() < 1e-12);
}

#[test]
fn aiff_pcm_preparation_copies_decoded_samples_exactly() {
    let samples: Vec<f32> = (0..300)
        .map(|i| ((i * 37) % 64) as f32 / 64. - 0.5)
        .collect();
    let format = AiffFormat {
        channels: 1,
        sample_rate: 96_000,
        encoding: AiffEncoding::Pcm24,
    };
    let mut writer = AiffWriter::new(Cursor::new(Vec::new()), format, 512).unwrap();
    writer.write_frames(&samples).unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    let mut decoded = vec![0f32; 300];
    let mut check = AiffReader::new(Cursor::new(bytes.clone()), 512).unwrap();
    assert_eq!(check.read_frames(&mut decoded).unwrap(), 300);
    let mut reader = AiffReader::new(Cursor::new(bytes), 512).unwrap();
    let request = ImpulseResponseRequest {
        target_rate: 96_000,
        conversion: ResponseRateConversion::RequireEqualRates,
        channels: ResponseChannelMap::Select(&[0, 0]),
        trim: ResponseTrim::default(),
        normalization: ResponseNormalization::None,
        maximum_taps: 300,
        maximum_working_bytes: 1 << 20,
    };
    let prepared = prepare_impulse_response_file(&mut reader, OPTIONS, &request).unwrap();
    let widened: Vec<f64> = decoded.iter().map(|&x| f64::from(x)).collect();
    assert_eq!(prepared.channel(0).unwrap(), &widened[..]);
    assert_eq!(prepared.channel(1).unwrap(), &widened[..]);
}

struct Mock {
    samples: Vec<f32>,
    channels: usize,
    position: usize,
    declared: Option<u64>,
    fail_at: Option<usize>,
    reads: usize,
}
impl PlaybackReader for Mock {
    fn channels(&self) -> usize {
        self.channels
    }
    fn sample_rate(&self) -> u32 {
        48_000
    }
    fn position_frames(&self) -> u64 {
        self.position as u64
    }
    fn declared_frames(&self) -> Option<u64> {
        self.declared
    }
    fn maximum_read_frames(&self) -> usize {
        64
    }
    fn read_frames(&mut self, output: &mut [f32]) -> Result<usize, AudioFileError> {
        self.reads += 1;
        let total = self.samples.len() / self.channels;
        let frames = (output.len() / self.channels).min(total - self.position);
        if self.fail_at.is_some_and(|f| self.position + frames > f) {
            self.position += 1; // a reader may commit progress before failing
            return Err(AudioFileError::InvalidRecord);
        }
        let start = self.position * self.channels;
        output[..frames * self.channels]
            .copy_from_slice(&self.samples[start..start + frames * self.channels]);
        self.position += frames;
        Ok(frames)
    }
}
fn mock(frames: usize) -> Mock {
    Mock {
        samples: (0..frames)
            .map(|i| (i as f32 + 1.) / frames as f32)
            .collect(),
        channels: 1,
        position: 0,
        declared: Some(frames as u64),
        fail_at: None,
        reads: 0,
    }
}
fn copy_request() -> ImpulseResponseRequest<'static> {
    ImpulseResponseRequest {
        target_rate: 48_000,
        conversion: ResponseRateConversion::RequireEqualRates,
        channels: ResponseChannelMap::Identity,
        trim: ResponseTrim::default(),
        normalization: ResponseNormalization::None,
        maximum_taps: 262_144,
        maximum_working_bytes: 1 << 20,
    }
}

#[test]
fn refusals_before_reads_leave_the_reader_untouched() {
    let check = |mut m: Mock, options, request: ImpulseResponseRequest<'_>| {
        let error = prepare_impulse_response_file(&mut m, options, &request).unwrap_err();
        assert_eq!((m.reads, m.position, error.source_frames_read), (0, 0, 0));
        error.kind
    };
    // Declared duration over bound.
    assert!(matches!(
        check(mock(5000), OPTIONS, copy_request()),
        ImpulseResponseFileErrorKind::Capacity
    ));
    // Options out of range.
    let bad = ImpulseResponseFileOptions {
        maximum_source_frames: 0,
        ..OPTIONS
    };
    assert!(matches!(
        check(mock(10), bad, copy_request()),
        ImpulseResponseFileErrorKind::Capacity
    ));
    let bad = ImpulseResponseFileOptions {
        source_batch_frames: 65_537,
        ..OPTIONS
    };
    assert!(matches!(
        check(mock(10), bad, copy_request()),
        ImpulseResponseFileErrorKind::Capacity
    ));
    // Policy refusal from the signal plan.
    let mut request = copy_request();
    request.target_rate = 44_100;
    assert!(matches!(
        check(mock(10), OPTIONS, request),
        ImpulseResponseFileErrorKind::Preparation(ImpulseResponseError::InvalidConversion)
    ));
    // Source storage plus declared bytes exceed the working budget.
    let mut request = copy_request();
    request.maximum_working_bytes = 4000 * 8 + 4000 * 8;
    assert!(matches!(
        check(mock(4000), OPTIONS, request),
        ImpulseResponseFileErrorKind::Capacity
    ));
    // Not at frame zero, or zero declared frames.
    let mut m = mock(10);
    m.position = 1;
    let error = prepare_impulse_response_file(&mut m, OPTIONS, &copy_request()).unwrap_err();
    assert!(matches!(error.kind, ImpulseResponseFileErrorKind::Format));
    assert_eq!(m.reads, 0);
    let mut m = mock(10);
    m.declared = Some(0);
    assert!(matches!(
        check(m, OPTIONS, copy_request()),
        ImpulseResponseFileErrorKind::Record
    ));
}

#[test]
fn read_time_refusals_report_committed_frames_without_partial_output() {
    let mut m = mock(200);
    m.fail_at = Some(150);
    let e = prepare_impulse_response_file(&mut m, OPTIONS, &copy_request()).unwrap_err();
    assert!(matches!(
        e.kind,
        ImpulseResponseFileErrorKind::Reader(AudioFileError::InvalidRecord)
    ));
    assert_eq!(e.source_frames_read, 129);
    let mut m = mock(200);
    m.samples[77] = f32::NAN;
    let e = prepare_impulse_response_file(&mut m, OPTIONS, &copy_request()).unwrap_err();
    assert!(matches!(e.kind, ImpulseResponseFileErrorKind::NonFinite));
    assert_eq!(e.source_frames_read, 128);
    let mut m = mock(200);
    m.declared = Some(201);
    let e = prepare_impulse_response_file(&mut m, OPTIONS, &copy_request()).unwrap_err();
    assert!(matches!(e.kind, ImpulseResponseFileErrorKind::Record));
    assert_eq!(e.source_frames_read, 200);
    // Unknown duration uses the caller bound; longer streams are refused.
    let mut m = mock(200);
    m.declared = None;
    let options = ImpulseResponseFileOptions {
        maximum_source_frames: 150,
        ..OPTIONS
    };
    let e = prepare_impulse_response_file(&mut m, options, &copy_request()).unwrap_err();
    assert!(matches!(e.kind, ImpulseResponseFileErrorKind::Capacity));
    let mut m = mock(200);
    m.declared = None;
    let prepared = prepare_impulse_response_file(&mut m, OPTIONS, &copy_request()).unwrap();
    assert_eq!(prepared.frames(), 200);
    // Data-dependent preparation refusal after a complete read: a requested trim
    // defers the length decision until the samples are known.
    let mut m = mock(200);
    let mut request = copy_request();
    request.maximum_taps = 199;
    request.trim.leading = Some(TrimThreshold::Absolute(0.));
    let e = prepare_impulse_response_file(&mut m, OPTIONS, &request).unwrap_err();
    assert!(matches!(
        e.kind,
        ImpulseResponseFileErrorKind::Preparation(ImpulseResponseError::TooLong)
    ));
    assert_eq!(e.source_frames_read, 200);
}
