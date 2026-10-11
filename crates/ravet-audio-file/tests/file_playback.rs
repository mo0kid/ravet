#![cfg(feature = "resampled-playback")]
use ravet_audio_file::{
    AudioFileError as Error, FilePlaybackOptions as Options, FilePlaybackProgress,
    PlaybackConversion as Conversion, PlaybackReader, WaveEncoding, WaveFormat, WaveReader,
    WaveWriter, prepare_file_playback,
};
use std::{
    io::Cursor,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
#[derive(Clone)]
struct Reader {
    samples: Vec<f32>,
    channels: usize,
    rate: u32,
    position: usize,
    declared: Option<u64>,
    reads: Arc<AtomicUsize>,
    fault: Option<usize>,
}
impl PlaybackReader for Reader {
    fn channels(&self) -> usize {
        self.channels
    }
    fn sample_rate(&self) -> u32 {
        self.rate
    }
    fn position_frames(&self) -> u64 {
        self.position as u64
    }
    fn declared_frames(&self) -> Option<u64> {
        self.declared
    }
    fn maximum_read_frames(&self) -> usize {
        4096
    }
    fn read_frames(&mut self, output: &mut [f32]) -> Result<usize, Error> {
        self.reads.fetch_add(1, Ordering::Relaxed);
        if self.fault == Some(self.position) {
            self.position += 1;
            return Err(Error::Io(std::io::Error::other(
                "original advanced reader error",
            )));
        }
        let count =
            (output.len() / self.channels).min(self.samples.len() / self.channels - self.position);
        output[..count * self.channels].copy_from_slice(
            &self.samples[self.position * self.channels..(self.position + count) * self.channels],
        );
        self.position += count;
        Ok(count)
    }
}
fn reader(samples: Vec<f32>, channels: usize, rate: u32) -> Reader {
    Reader {
        declared: Some((samples.len() / channels) as u64),
        samples,
        channels,
        rate,
        position: 0,
        reads: Arc::new(AtomicUsize::new(0)),
        fault: None,
    }
}
fn options(
    output_rate: u32,
    conversion: Conversion,
    maximum_source_frames: usize,
    batch: usize,
) -> Options {
    Options {
        output_rate,
        conversion,
        maximum_source_frames,
        maximum_output_frames: 2_000_000,
        source_batch_frames: batch,
        maximum_render_frames: 113,
    }
}
fn render(mut playback: ravet_audio_file::PreparedFilePlayback) -> Vec<f32> {
    let frames = playback.info().prepared_frames;
    let channels = playback.info().channels;
    let mut output = vec![0.; frames * channels];
    playback.playback_mut().play();
    for buffer in output.chunks_mut(113 * channels) {
        playback.playback_mut().render(buffer).unwrap();
    }
    assert_eq!(playback.playback().position(), frames);
    output
}
fn signal(frames: usize, rate: u32) -> Vec<f32> {
    (0..frames)
        .flat_map(|frame| {
            [337., 977.].map(move |f| {
                (0.5 * (std::f64::consts::TAU * f * frame as f64 / f64::from(rate)).sin()) as f32
            })
        })
        .collect()
}
#[test]
fn causal_impulse_fractional_delay_and_finite_support_follow_actual_kernel() {
    for (input, output, expected, nominal) in [
        (48000, 48000, 16, 1),
        (24000, 48000, 31, 2),
        (48000, 24000, 8, 1),
    ] {
        let prepared = prepare_file_playback(
            reader(vec![1.], 1, input),
            options(output, Conversion::Sinc { taps: 16 }, 1, 1),
        )
        .unwrap();
        let info = prepared.info();
        assert_eq!(
            (info.prepared_frames, info.nominal_output_frames),
            (expected, nominal)
        );
        assert_eq!(info.filter_delay_input_samples(), 7.5);
        assert_eq!(
            info.filter_delay_output_samples(),
            7.5 * f64::from(output) / f64::from(input)
        );
        assert_eq!(prepared.progress().source_frames_read, 1);
        assert_eq!(prepared.progress().source_frames_submitted, 1);
        assert!(prepared.progress().padding_frames_submitted <= 16);
        let result = render(prepared);
        if input == output {
            assert!((result.iter().map(|x| f64::from(*x)).sum::<f64>() - 1.).abs() < 1e-6);
            for (a, b) in result.iter().zip(result.iter().rev()) {
                assert!((*a - *b).abs() < 1e-6)
            }
        }
    }
}
#[test]
fn actual_wave_rate_duration_pitch_channel_order_and_arbitrary_partitions_agree() {
    for (input, output, source_frames, total) in [
        (44100, 48000, 44100, 48068),
        (48000, 44100, 48000, 44157),
        (96000, 48000, 96000, 48032),
        (32000, 48000, 32000, 48094),
    ] {
        let source = signal(source_frames, input);
        let format = WaveFormat {
            channels: 2,
            sample_rate: input,
            encoding: WaveEncoding::Float32,
        };
        let mut writer = WaveWriter::new(Cursor::new(Vec::new()), format, 4096).unwrap();
        for chunk in source.chunks(4096 * 2) {
            writer.write_frames(chunk).unwrap();
        }
        let bytes = writer.finish().unwrap().into_inner();
        let mut reference = None;
        for batch in [1, 17, 113, 4096] {
            let source = WaveReader::new(Cursor::new(bytes.clone()), batch).unwrap();
            let prepared = prepare_file_playback(
                source,
                options(output, Conversion::Sinc { taps: 64 }, source_frames, batch),
            )
            .unwrap();
            let info = prepared.info();
            assert_eq!(
                (
                    info.source_frames,
                    info.nominal_output_frames,
                    info.prepared_frames
                ),
                (source_frames as u64, output as usize, total)
            );
            let result = render(prepared);
            if let Some(reference) = &reference {
                assert_eq!(reference, &result)
            } else {
                reference = Some(result.clone())
            }
            for (channel, frequency) in [337., 977.].into_iter().enumerate() {
                let mut energy = 0.;
                let mut noise = 0.;
                for frame in 256..output as usize - 256 {
                    let expected = 0.5
                        * (std::f64::consts::TAU
                            * frequency
                            * (frame as f64 / f64::from(output) - 31.5 / f64::from(input)))
                        .sin();
                    energy += expected * expected;
                    noise += (f64::from(result[frame * 2 + channel]) - expected).powi(2);
                }
                assert!(
                    10. * (energy / noise).log10() > 45.,
                    "{input}->{output} channel{channel} snr{}",
                    10. * (energy / noise).log10()
                );
            }
        }
    }
}
#[test]
fn explicit_copy_is_bit_exact_and_unequal_rate_or_bounds_fail_before_source_read() {
    let samples = vec![0.25, -0.25, 0.5, -0.5];
    let source = reader(samples.clone(), 2, 48000);
    let prepared = prepare_file_playback(source, options(48000, Conversion::Copy, 2, 1)).unwrap();
    assert_eq!(prepared.info().tail_output_frames, 0);
    assert_eq!(prepared.info().filter_delay_output_samples(), 0.);
    assert_eq!(render(prepared), samples);
    for mode in 0..5 {
        let source = reader(signal(17, 48000), 2, 48000);
        let reads = Arc::clone(&source.reads);
        let mut config = options(48000, Conversion::Sinc { taps: 64 }, 17, 1);
        match mode {
            0 => config.maximum_output_frames = 17,
            1 => config.maximum_source_frames = 16,
            2 => config.conversion = Conversion::Sinc { taps: 15 },
            3 => {
                config.conversion = Conversion::Copy;
                config.output_rate = 44100
            }
            _ => config.source_batch_frames = 0,
        }
        let failure = match prepare_file_playback(source, config) {
            Ok(_) => panic!("invalid accepted"),
            Err(e) => e,
        };
        assert_eq!(reads.load(Ordering::Relaxed), 0);
        assert_eq!(failure.progress(), FilePlaybackProgress::default());
        assert_eq!(failure.reader().position, 0);
    }
}
#[test]
fn original_advanced_reader_error_and_partial_output_are_recovered_without_rollback() {
    let mut source = reader(vec![0.25; 8], 1, 48000);
    source.fault = Some(3);
    let reads = Arc::clone(&source.reads);
    let failure = match prepare_file_playback(source, options(48000, Conversion::Copy, 8, 1)) {
        Ok(_) => panic!("fault accepted"),
        Err(e) => e,
    };
    assert!(
        matches!(failure.error(),Error::Io(error) if error.to_string()=="original advanced reader error")
    );
    assert_eq!(
        failure.progress(),
        FilePlaybackProgress {
            source_frames_read: 4,
            source_frames_submitted: 3,
            padding_frames_submitted: 0,
            prepared_frames: 3
        }
    );
    assert_eq!(failure.partial_samples(), &[0.25; 3]);
    assert_eq!(reads.load(Ordering::Relaxed), 4);
    let (reader, error, _, partial) = failure.into_parts();
    assert_eq!(reader.position, 4);
    assert!(matches!(error, Error::Io(_)));
    assert_eq!(partial.len(), 3);
}

#[test]
fn rational_counts_and_partitions_cover_tiny_and_extreme_supported_ratios() {
    for n in [1, 2, 17, 31, 257] {
        for (input, output) in [(44100, 48000), (48000, 44100), (3000, 48000), (48000, 3000)] {
            for taps in [16, 64, 256] {
                let expected = ((n + taps - 2) * output as usize / input as usize) + 1;
                let nominal = (n * output as usize).div_ceil(input as usize);
                let mut reference = None;
                for batch in [1, 7, 113] {
                    let samples = (0..n).map(|x| x as f32 / n as f32).collect();
                    let prepared = prepare_file_playback(
                        reader(samples, 1, input),
                        options(output, Conversion::Sinc { taps }, n, batch),
                    )
                    .unwrap();
                    let info = prepared.info();
                    assert_eq!(
                        (info.prepared_frames, info.nominal_output_frames),
                        (expected, nominal),
                        "n{n} {input}->{output} t{taps}"
                    );
                    assert!(prepared.progress().padding_frames_submitted <= taps);
                    let samples = render(prepared);
                    if let Some(previous) = &reference {
                        assert_eq!(previous, &samples);
                    } else {
                        reference = Some(samples);
                    }
                }
            }
        }
    }
}

#[test]
fn unknown_duration_budget_metadata_and_whole_batch_nonfinite_rejection_are_explicit() {
    let mut source = reader(vec![0.25; 17], 1, 48000);
    source.declared = None;
    let prepared = prepare_file_playback(
        source,
        options(44100, Conversion::Sinc { taps: 16 }, 100, 7),
    )
    .unwrap();
    assert_eq!(prepared.info().source_frames, 17);
    assert_eq!(prepared.info().prepared_frames, (31 * 44100 / 48000) + 1);
    for declared in [Some(16), Some(18), None] {
        let mut source = reader(vec![0.25; 17], 1, 48000);
        source.declared = declared;
        let failure = prepare_file_playback(source, options(48000, Conversion::Copy, 16, 7))
            .err()
            .unwrap();
        if declared == Some(18) {
            assert_eq!(failure.progress().source_frames_read, 0);
        } else {
            assert_eq!(failure.progress().source_frames_read, 17);
            assert!(failure.progress().prepared_frames <= 14);
        }
    }
    let mut short = reader(vec![0.25; 17], 1, 48000);
    short.declared = Some(18);
    let failure = prepare_file_playback(short, options(48000, Conversion::Copy, 18, 7))
        .err()
        .unwrap();
    assert!(matches!(failure.error(), Error::InvalidRecord));
    assert_eq!(
        (
            failure.progress().source_frames_read,
            failure.progress().source_frames_submitted,
            failure.progress().prepared_frames
        ),
        (17, 17, 17)
    );
    let source = reader(vec![0.25, f32::NAN, 0.5], 1, 48000);
    let failure =
        prepare_file_playback(source, options(48000, Conversion::Sinc { taps: 16 }, 3, 3))
            .err()
            .unwrap();
    assert!(matches!(failure.error(), Error::InvalidSamples));
    assert_eq!(
        (
            failure.progress().source_frames_read,
            failure.progress().source_frames_submitted,
            failure.progress().prepared_frames
        ),
        (3, 0, 0)
    );
    assert!(failure.partial_samples().is_empty());
    let source = reader(vec![0.25; 64], 64, 48000);
    let reads = Arc::clone(&source.reads);
    let failure = prepare_file_playback(
        source,
        options(48000, Conversion::Sinc { taps: 256 }, 1, 4096),
    )
    .err()
    .unwrap();
    assert!(matches!(failure.error(), Error::Capacity));
    assert_eq!(reads.load(Ordering::Relaxed), 0);
}

#[test]
fn actual_aiff_pcm_reader_shares_rate_conversion_and_complete_frame_contract() {
    use ravet_audio_file::{AiffEncoding, AiffFormat, AiffReader, AiffWriter};
    let samples: Vec<f32> = (0..257)
        .flat_map(|n| [n as f32 / 1024., -n as f32 / 1024.])
        .collect();
    let format = AiffFormat {
        channels: 2,
        sample_rate: 32000,
        encoding: AiffEncoding::Pcm24,
    };
    let mut writer = AiffWriter::new(Cursor::new(Vec::new()), format, 4096).unwrap();
    writer.write_frames(&samples).unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    let actual = prepare_file_playback(
        AiffReader::new(Cursor::new(bytes.clone()), 7).unwrap(),
        options(48000, Conversion::Sinc { taps: 16 }, 257, 113),
    )
    .unwrap();
    let mut decoder = AiffReader::new(Cursor::new(bytes), 4096).unwrap();
    let mut pcm = vec![0.; 257 * 2];
    assert_eq!(decoder.read_frames(&mut pcm).unwrap(), 257);
    let expected = prepare_file_playback(
        reader(pcm, 2, 32000),
        options(48000, Conversion::Sinc { taps: 16 }, 257, 1),
    )
    .unwrap();
    assert_eq!(actual.info(), expected.info());
    assert_eq!(render(actual), render(expected));
}

#[cfg(all(
    feature = "compressed",
    feature = "flac-encode",
    feature = "vorbis-encode",
    feature = "mp3-encode"
))]
#[test]
fn actual_flac_vorbis_mp3_preserve_gapless_frames_before_conversion() {
    use ravet_audio_file::{
        CompressedReader, FlacFormat, FlacPcmEncoding, Mp3Bitrate, Mp3Format, VorbisFormat,
        encode_flac, encode_mp3, encode_vorbis,
    };
    for frames in [1, 17, 8193] {
        let samples = signal(frames, 44100);
        let integers: Vec<i32> = samples.iter().map(|s| (*s * 8388608.) as i32).collect();
        let flac = encode_flac(
            &integers,
            FlacFormat {
                channels: 2,
                sample_rate: 44100,
                encoding: FlacPcmEncoding::Pcm24,
            },
        )
        .unwrap();
        let vorbis = encode_vorbis(
            &samples,
            VorbisFormat {
                channels: 2,
                sample_rate: 44100,
                quality: 1.,
                stream_serial: 194,
            },
        )
        .unwrap();
        let mp3 = encode_mp3(
            Mp3Format {
                channels: 2,
                sample_rate: 44100,
                bit_rate: Mp3Bitrate::Kbps256,
            },
            &samples,
        )
        .unwrap();
        for encoded in [flac, vorbis, mp3] {
            let mut decoder = CompressedReader::new(&encoded, frames).unwrap();
            let mut pcm = vec![0.; frames * 2];
            assert_eq!(decoder.read_frames(&mut pcm).unwrap(), frames);
            let mut eof = [0.; 2];
            assert_eq!(decoder.read_frames(&mut eof).unwrap(), 0);
            assert!(decoder.end_of_file());
            let expected = prepare_file_playback(
                reader(pcm, 2, 44100),
                options(48000, Conversion::Sinc { taps: 16 }, frames, 7),
            )
            .unwrap();
            let source = CompressedReader::new(&encoded, frames).unwrap();
            let actual = prepare_file_playback(
                source,
                options(48000, Conversion::Sinc { taps: 16 }, frames, 113),
            )
            .unwrap();
            assert_eq!(actual.info().source_frames, frames as u64);
            assert_eq!(actual.info(), expected.info());
            assert_eq!(render(actual), render(expected));
        }
    }
}

#[cfg(feature = "compressed")]
#[test]
fn codec_eof_integrity_failure_recovers_actual_committed_cursor_and_validated_prefix() {
    use ravet_audio_file::{CompressedReader, decode_compressed};
    let encoded = include_bytes!("fixtures/sine.flac");
    let expected = decode_compressed(encoded, 4096).unwrap();
    let mut corrupt = encoded.to_vec();
    // Alter only STREAMINFO MD5, preserving all coded PCM packets and duration.
    corrupt[26] ^= 0x80;
    let source = CompressedReader::new(&corrupt, 4096).unwrap();
    let failure = prepare_file_playback(source, options(48000, Conversion::Copy, 4096, 113))
        .err()
        .unwrap();
    assert!(matches!(failure.error(), Error::InvalidRecord));
    assert_eq!(failure.reader().position_frames(), 4096);
    assert!(!failure.reader().end_of_file());
    assert_eq!(
        failure.progress(),
        FilePlaybackProgress {
            source_frames_read: 4096,
            source_frames_submitted: 4068,
            padding_frames_submitted: 0,
            prepared_frames: 4068
        }
    );
    assert_eq!(failure.partial_samples(), &expected.samples()[..4068 * 2]);
    let (mut reader, original, progress, prefix) = failure.into_parts();
    assert!(matches!(original, Error::InvalidRecord));
    assert_eq!(progress.prepared_frames, prefix.len() / 2);
    assert!(matches!(
        reader.read_frames(&mut [1.; 2]),
        Err(Error::Poisoned)
    ));
    assert_eq!(reader.position_frames(), 4096);
}

#[test]
fn nonzero_source_origin_is_returned_without_reads_or_seek_rollback() {
    let mut source = reader(vec![0.25; 17], 1, 48000);
    source.position = 5;
    let reads = Arc::clone(&source.reads);
    let failure = prepare_file_playback(source, options(48000, Conversion::Copy, 17, 7))
        .err()
        .unwrap();
    assert!(matches!(failure.error(), Error::InvalidSamples));
    assert_eq!(failure.reader().position, 5);
    assert_eq!(reads.load(Ordering::Relaxed), 0);
    assert_eq!(failure.progress(), FilePlaybackProgress::default());
}
