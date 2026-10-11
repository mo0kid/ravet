#![cfg(feature = "resampled-playback")]
use ravet_audio_file::{
    AudioFileError as Error, ConvertedReadAhead, ConvertedReadAheadOptions as Options,
    FilePlaybackOptions, PlaybackConversion as Conversion, PlaybackReader, ReadAheadPump,
    audio_ring, prepare_file_playback,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
#[derive(Clone)]
struct Reader {
    samples: Vec<f32>,
    channels: usize,
    rate: u32,
    position: usize,
    reads: Arc<AtomicUsize>,
    fault: Option<usize>,
    declared: Option<u64>,
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
            return Err(Error::Io(std::io::Error::other("original candidate fault")));
        }
        let n =
            (output.len() / self.channels).min(self.samples.len() / self.channels - self.position);
        output[..n * self.channels].copy_from_slice(
            &self.samples[self.position * self.channels..(self.position + n) * self.channels],
        );
        self.position += n;
        Ok(n)
    }
}
fn reader(n: usize, rate: u32) -> Reader {
    Reader {
        samples: (0..n)
            .flat_map(|i| [i as f32 / 2048., -(i as f32) / 2048.])
            .collect(),
        channels: 2,
        rate,
        position: 0,
        reads: Arc::new(AtomicUsize::new(0)),
        fault: None,
        declared: Some(n as u64),
    }
}
fn options(output: u32, n: usize, input_batch: usize, output_batch: usize) -> Options {
    Options {
        output_rate: output,
        conversion: Conversion::Sinc { taps: 16 },
        maximum_source_frames: n,
        maximum_output_frames: 1_000_000,
        source_batch_frames: input_batch,
        conversion_batch_frames: output_batch,
    }
}
fn reference(source: Reader, options: Options) -> Vec<f32> {
    let mut p = prepare_file_playback(
        source,
        FilePlaybackOptions {
            output_rate: options.output_rate,
            conversion: options.conversion,
            maximum_source_frames: options.maximum_source_frames,
            maximum_output_frames: options.maximum_output_frames as usize,
            source_batch_frames: options.source_batch_frames,
            maximum_render_frames: 113,
        },
    )
    .unwrap();
    let mut samples = vec![0.; p.info().prepared_frames * 2];
    p.playback_mut().play();
    for chunk in samples.chunks_mut(226) {
        p.playback_mut().render(chunk).unwrap();
    }
    samples
}
#[test]
fn bounded_pump_matches_complete_preparation_under_input_output_ring_partitions() {
    for (input, output) in [
        (48000, 48000),
        (44100, 48000),
        (48000, 44100),
        (3000, 48000),
        (48000, 3000),
    ] {
        for n in [1, 17, 257] {
            for (input_batch, output_batch, ring) in [(1, 1, 1), (7, 17, 4), (113, 7, 32)] {
                let source = reader(n, input);
                let fresh = source.clone();
                let options = options(output, n, input_batch, output_batch);
                let expected = reference(source.clone(), options);
                let (producer, mut consumer) = audio_ring(2, ring).unwrap();
                let mut pump = ConvertedReadAhead::new(
                    source,
                    move || Ok(fresh.clone()),
                    producer,
                    &consumer,
                    options,
                )
                .unwrap();
                let mut actual = Vec::new();
                for _ in 0..100_000 {
                    let p = pump.pump().unwrap();
                    let mut buffer = [0.; 14];
                    let n = consumer.pop(&mut buffer).unwrap();
                    actual.extend_from_slice(&buffer[..n * 2]);
                    if p.end_of_file && consumer.queued_frames() == 0 {
                        break;
                    }
                }
                assert_eq!(
                    actual, expected,
                    "{input}->{output} n{n} {input_batch}/{output_batch}/{ring}"
                );
                let counts = pump.counts();
                assert_eq!(counts.input_frames_read, n as u64);
                assert_eq!(counts.input_frames_submitted, n as u64);
                assert_eq!(counts.output_frames_generated, (actual.len() / 2) as u64);
                assert_eq!(
                    counts.output_frames_published,
                    counts.output_frames_generated
                );
                assert!(counts.padding_frames_submitted <= 16);
            }
        }
    }
}
#[test]
fn full_ring_preserves_input_output_suffix_and_native_progress_default_is_unchanged() {
    let source = reader(17, 48000);
    let fresh = source.clone();
    let reads = Arc::clone(&source.reads);
    let (producer, consumer) = audio_ring(2, 1).unwrap();
    let mut pump = ConvertedReadAhead::new(
        source,
        move || Ok(fresh.clone()),
        producer,
        &consumer,
        options(3000, 17, 1, 1),
    )
    .unwrap();
    let p = ReadAheadPump::pump(&mut pump).unwrap();
    assert_eq!(p.queued_frames, 1);
    assert!(ReadAheadPump::worker_progressed(&pump, &p));
    let before = pump.counts();
    let reads_before = reads.load(Ordering::Relaxed);
    for _ in 0..3 {
        let p = ReadAheadPump::pump(&mut pump).unwrap();
        assert_eq!(p.queued_frames, 0);
        assert!(!ReadAheadPump::worker_progressed(&pump, &p));
        assert_eq!(pump.counts(), before);
        assert_eq!(reads.load(Ordering::Relaxed), reads_before);
    }
    drop(consumer);
}
#[test]
fn exact_candidate_seeks_preserve_old_generation_suffix_on_failure() {
    let source = reader(257, 44100);
    let fresh = source.clone();
    let factory_calls = Arc::new(AtomicUsize::new(0));
    let calls = Arc::clone(&factory_calls);
    let fail = Arc::new(AtomicUsize::new(0));
    let factory_fail = Arc::clone(&fail);
    let options = options(48000, 257, 7, 17);
    let expected = reference(source.clone(), options);
    let frames = expected.len() / 2;
    let (producer, mut consumer) = audio_ring(2, 4).unwrap();
    let mut pump = ConvertedReadAhead::new(
        source,
        move || {
            calls.fetch_add(1, Ordering::Relaxed);
            if factory_fail.load(Ordering::Relaxed) != 0 {
                return Err(Error::Io(std::io::Error::other("original factory fault")));
            }
            Ok(fresh.clone())
        },
        producer,
        &consumer,
        options,
    )
    .unwrap();
    pump.pump().unwrap();
    let before = (
        pump.counts(),
        pump.input_reader_position(),
        pump.pending_input_frames(),
        pump.pending_output_frames(),
        consumer.queued_frames(),
        pump.generation(),
    );
    fail.store(1, Ordering::Relaxed);
    assert!(pump.seek_stopped(&mut consumer, 17).is_err());
    assert_eq!(
        (
            pump.counts(),
            pump.input_reader_position(),
            pump.pending_input_frames(),
            pump.pending_output_frames(),
            consumer.queued_frames(),
            pump.generation()
        ),
        before
    );
    let count = factory_calls.load(Ordering::Relaxed);
    assert!(pump.seek_stopped(&mut consumer, frames as u64 + 1).is_err());
    assert_eq!(factory_calls.load(Ordering::Relaxed), count);
    fail.store(0, Ordering::Relaxed);
    for target in [0, 1, frames - 1, 17, frames] {
        pump.seek_stopped(&mut consumer, target as u64).unwrap();
        assert_eq!(consumer.queued_frames(), 0);
        assert_eq!(pump.output_origin(), target as u64);
        let mut observed = Vec::new();
        for _ in 0..10000 {
            let p = pump.pump().unwrap();
            let mut buffer = [0.; 14];
            let n = consumer.pop(&mut buffer).unwrap();
            observed.extend_from_slice(&buffer[..n * 2]);
            if p.end_of_file && consumer.queued_frames() == 0 {
                break;
            }
        }
        assert_eq!(observed, &expected[target * 2..]);
    }
    assert_eq!(pump.generation(), 5);
}
#[test]
fn converter_input_only_progress_prevents_worker_sleep_without_output() {
    let source = reader(257, 48000);
    let fresh = source.clone();
    let (producer, mut consumer) = audio_ring(2, 4).unwrap();
    let mut pump = ConvertedReadAhead::new(
        source,
        move || Ok(fresh.clone()),
        producer,
        &consumer,
        options(3000, 257, 1, 1),
    )
    .unwrap();
    let first = ReadAheadPump::pump(&mut pump).unwrap();
    assert_eq!(first.queued_frames, 1);
    consumer.pop(&mut [0.; 2]).unwrap();
    let before = pump.counts();
    let p = ReadAheadPump::pump(&mut pump).unwrap();
    assert_eq!(p.read_frames, 0);
    assert_eq!(p.queued_frames, 0);
    assert!(ReadAheadPump::worker_progressed(&pump, &p));
    assert_eq!(
        pump.counts().input_frames_submitted,
        before.input_frames_submitted + 1
    );
}

#[test]
fn failed_candidate_decode_metadata_and_eof_leave_exact_old_suffix_and_phase() {
    let source = reader(257, 44100);
    let fresh = source.clone();
    let mode = Arc::new(AtomicUsize::new(0));
    let factory_mode = Arc::clone(&mode);
    let options = options(48000, 257, 7, 17);
    let expected = reference(source.clone(), options);
    let (producer, mut consumer) = audio_ring(2, 4).unwrap();
    let mut pump = ConvertedReadAhead::new(
        source,
        move || {
            let mut candidate = fresh.clone();
            match factory_mode.load(Ordering::Relaxed) {
                1 => candidate.fault = Some(7),
                2 => candidate.rate = 44101,
                3 => candidate.samples.truncate(17 * 2),
                _ => {}
            }
            Ok(candidate)
        },
        producer,
        &consumer,
        options,
    )
    .unwrap();
    pump.pump().unwrap();
    let before = (
        pump.counts(),
        pump.input_reader_position(),
        pump.pending_input_frames(),
        pump.pending_output_frames(),
        consumer.queued_frames(),
        pump.generation(),
    );
    for kind in [1, 2, 3] {
        mode.store(kind, Ordering::Relaxed);
        let error = pump.seek_stopped(&mut consumer, 100).err().unwrap();
        if kind == 1 {
            assert!(matches!(&error,Error::Io(io) if io.to_string()=="original candidate fault"));
        }
        assert_eq!(
            (
                pump.counts(),
                pump.input_reader_position(),
                pump.pending_input_frames(),
                pump.pending_output_frames(),
                consumer.queued_frames(),
                pump.generation()
            ),
            before
        );
    }
    mode.store(0, Ordering::Relaxed);
    let mut actual = Vec::new();
    for _ in 0..10000 {
        let mut samples = [0.; 14];
        let count = consumer.pop(&mut samples).unwrap();
        actual.extend_from_slice(&samples[..count * 2]);
        let p = pump.pump().unwrap();
        if p.end_of_file && consumer.queued_frames() == 0 {
            break;
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn constructor_preflight_recovers_all_owners_without_read_or_factory() {
    for mode in 0..5 {
        let source = reader(17, 48000);
        let reads = Arc::clone(&source.reads);
        let fresh = source.clone();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&calls);
        let (producer, consumer) = audio_ring(if mode == 0 { 1 } else { 2 }, 4).unwrap();
        let mut options = options(48000, 17, 7, 7);
        match mode {
            0 => {}
            1 => options.maximum_output_frames = 17,
            2 => options.maximum_source_frames = 16,
            3 => options.source_batch_frames = 0,
            _ => options.maximum_source_frames = 16_777_217,
        };
        let failure = ConvertedReadAhead::new(
            source,
            move || {
                calls.fetch_add(1, Ordering::Relaxed);
                Ok(fresh.clone())
            },
            producer,
            &consumer,
            options,
        )
        .err()
        .unwrap();
        assert_eq!(failure.reader().position, 0);
        assert_eq!(reads.load(Ordering::Relaxed), 0);
        assert_eq!(observed.load(Ordering::Relaxed), 0);
        let (reader, _factory, mut producer, _error) = failure.into_parts();
        assert_eq!(reader.position, 0);
        assert_eq!(
            producer.push(&[0.25; 2]).unwrap(),
            if mode == 0 { 2 } else { 1 }
        );
    }
}

#[test]
fn native_pump_default_progress_remains_new_publication_only() {
    use ravet_audio_file::{WaveEncoding, WaveFormat, WaveReadAhead, WaveReader, WaveWriter};
    use std::io::Cursor;
    let mut writer = WaveWriter::new(
        Cursor::new(Vec::new()),
        WaveFormat {
            channels: 1,
            sample_rate: 48000,
            encoding: WaveEncoding::Float32,
        },
        17,
    )
    .unwrap();
    writer.write_frames(&[0.25; 17]).unwrap();
    let source = WaveReader::new(Cursor::new(writer.finish().unwrap().into_inner()), 17).unwrap();
    let (producer, _consumer) = audio_ring(1, 1).unwrap();
    let mut pump = WaveReadAhead::new(source, producer, 17).unwrap();
    let first = ReadAheadPump::pump(&mut pump).unwrap();
    assert!(ReadAheadPump::worker_progressed(&pump, &first));
    let full = ReadAheadPump::pump(&mut pump).unwrap();
    assert_eq!(full.queued_frames, 0);
    assert!(!ReadAheadPump::worker_progressed(&pump, &full));
}

#[test]
fn automatic_worker_pause_full_backpressure_resume_drain_and_stopped_seek_are_exact() {
    use ravet_audio_file::{ReadAheadStatus, ReadAheadWorker};
    use std::time::{Duration, Instant};
    let source = reader(257, 48000);
    let fresh = source.clone();
    let reads = Arc::clone(&source.reads);
    let options = options(3000, 257, 1, 1);
    let expected = reference(source.clone(), options);
    let (producer, consumer) = audio_ring(2, 4).unwrap();
    let pump = ConvertedReadAhead::new(
        source,
        move || Ok(fresh.clone()),
        producer,
        &consumer,
        options,
    )
    .unwrap();
    let (mut worker, mut playback) = ReadAheadWorker::new(pump, consumer, 7).unwrap();
    worker.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while playback.queued_frames() < 4 {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    let read_count = reads.load(Ordering::Relaxed);
    let mut paused = [1.; 14];
    let p = playback.render(&mut paused).unwrap();
    assert_eq!(p.source_frames, 0);
    assert_eq!(playback.position_frames(), 0);
    assert_eq!(playback.queued_frames(), 4);
    assert_eq!(paused, [0.; 14]);
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(reads.load(Ordering::Relaxed), read_count);
    playback.play();
    let mut actual = Vec::new();
    loop {
        let mut samples = [0.; 14];
        let p = playback.render(&mut samples).unwrap();
        actual.extend_from_slice(&samples[..p.source_frames * 2]);
        if p.ended {
            break;
        }
        assert_ne!(p.worker_status, ReadAheadStatus::Failed);
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(actual, expected);
    assert_eq!(playback.position_frames(), (expected.len() / 2) as u64);
    worker.seek_stopped(&mut playback, 1).unwrap();
    assert_eq!(playback.position_frames(), 1);
    playback.play();
    let mut actual = Vec::new();
    loop {
        let mut samples = [0.; 14];
        let p = playback.render(&mut samples).unwrap();
        actual.extend_from_slice(&samples[..p.source_frames * 2]);
        if p.ended {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(actual, &expected[2..]);
    worker.stop_stopped(&mut playback).unwrap();
    assert_eq!(playback.position_frames(), 0);
    assert!(!playback.is_playing());
    let exit = worker.shutdown().unwrap();
    assert!(exit.error.is_none());
}

#[test]
fn original_reader_error_committed_counts_and_queue_survive_poison_and_owner_recovery() {
    let mut source = reader(17, 48000);
    source.fault = Some(7);
    let fresh = source.clone();
    let mut config = options(48000, 17, 7, 7);
    config.conversion = Conversion::Copy;
    let (producer, mut consumer) = audio_ring(2, 32).unwrap();
    let mut pump = ConvertedReadAhead::new(
        source,
        move || Ok(fresh.clone()),
        producer,
        &consumer,
        config,
    )
    .unwrap();
    assert_eq!(pump.pump().unwrap().published_output_frames, 7);
    let error = pump.pump().unwrap_err();
    assert!(matches!(&error, Error::Io(e) if e.to_string() == "original candidate fault"));
    assert_eq!(pump.input_reader_position(), 8);
    assert_eq!(pump.counts().input_frames_read, 8);
    assert_eq!(pump.counts().input_frames_submitted, 7);
    assert_eq!(pump.counts().output_frames_generated, 7);
    assert_eq!(consumer.queued_frames(), 7);
    let before = pump.counts();
    assert!(matches!(pump.pump(), Err(Error::Poisoned)));
    assert_eq!(pump.counts(), before);
    let (source, _factory, _producer, counts) = pump.into_parts();
    assert_eq!(source.position, 8);
    assert_eq!(counts, before);
    let mut prefix = [0.; 14];
    assert_eq!(consumer.pop(&mut prefix).unwrap(), 7);
    assert_eq!(prefix.as_slice(), &reader(17, 48000).samples[..14]);
}

#[cfg(feature = "compressed")]
#[test]
fn actual_codec_eof_fault_preserves_published_prefix_and_failed_candidate_generation() {
    use ravet_audio_file::{CompressedReader, decode_compressed};
    let valid = include_bytes!("fixtures/sine.flac").to_vec();
    let decoded = decode_compressed(&valid, 4096).unwrap();
    let mut corrupt = valid.clone();
    corrupt[26] ^= 0x80;
    let factory_bytes = corrupt.clone();
    let mut config = options(48000, 4096, 113, 113);
    config.conversion = Conversion::Copy;
    let (producer, mut consumer) = audio_ring(2, 4096).unwrap();
    let source = CompressedReader::new(&valid, 4096).unwrap();
    let mut pump = ConvertedReadAhead::new(
        source,
        move || CompressedReader::new(&factory_bytes, 4096),
        producer,
        &consumer,
        config,
    )
    .unwrap();
    pump.pump().unwrap();
    let before = (
        pump.counts(),
        pump.input_reader_position(),
        consumer.queued_frames(),
        pump.generation(),
    );
    assert!(matches!(
        pump.seek_stopped(&mut consumer, 4096),
        Err(Error::InvalidRecord)
    ));
    assert_eq!(
        (
            pump.counts(),
            pump.input_reader_position(),
            consumer.queued_frames(),
            pump.generation()
        ),
        before
    );
    let mut prefix = vec![0.; 226];
    assert_eq!(consumer.pop(&mut prefix).unwrap(), 113);
    assert_eq!(prefix, &decoded.samples()[..226]);
    let (producer, mut consumer) = audio_ring(2, 4096).unwrap();
    let source = CompressedReader::new(&corrupt, 4096).unwrap();
    let mut pump = ConvertedReadAhead::new(
        source,
        move || CompressedReader::new(&corrupt, 4096),
        producer,
        &consumer,
        config,
    )
    .unwrap();
    loop {
        match pump.pump() {
            Ok(p) => assert!(!p.end_of_file),
            Err(e) => {
                assert!(matches!(e, Error::InvalidRecord));
                break;
            }
        }
    }
    assert_eq!(pump.counts().input_frames_read, 4096);
    assert_eq!(pump.counts().input_frames_submitted, 4068);
    assert_eq!(pump.counts().output_frames_published, 4068);
    assert_eq!(pump.counts().padding_frames_submitted, 0);
    assert!(matches!(pump.pump(), Err(Error::Poisoned)));
    let (mut original, _factory, _producer, _counts) = pump.into_parts();
    assert_eq!(original.position_frames(), 4096);
    assert!(!original.end_of_file());
    assert!(matches!(
        original.read_frames(&mut [0.; 2]),
        Err(Error::Poisoned)
    ));
    let mut prefix = vec![0.; 8136];
    assert_eq!(consumer.pop(&mut prefix).unwrap(), 4068);
    assert_eq!(prefix, &decoded.samples()[..8136]);
}

#[cfg(all(
    feature = "compressed",
    feature = "flac-encode",
    feature = "vorbis-encode",
    feature = "mp3-encode"
))]
#[test]
fn actual_gapless_codecs_retain_exact_source_length_before_streaming_conversion_and_seek() {
    use ravet_audio_file::{
        CompressedReader, FlacFormat, FlacPcmEncoding, Mp3Bitrate, Mp3Format, VorbisFormat,
        encode_flac, encode_mp3, encode_vorbis,
    };
    for frames in [1, 17, 8193] {
        let input = reader(frames, 44100);
        let pcm: Vec<f32> = input.samples.iter().map(|s| s * 0.0625).collect();
        let integer: Vec<i32> = pcm.iter().map(|x| (x * 8388608.) as i32).collect();
        let flac = encode_flac(
            &integer,
            FlacFormat {
                channels: 2,
                sample_rate: 44100,
                encoding: FlacPcmEncoding::Pcm24,
            },
        )
        .unwrap();
        let vorbis = encode_vorbis(
            &pcm,
            VorbisFormat {
                channels: 2,
                sample_rate: 44100,
                quality: 1.,
                stream_serial: 199,
            },
        )
        .unwrap();
        let mp3 = encode_mp3(
            Mp3Format {
                channels: 2,
                sample_rate: 44100,
                bit_rate: Mp3Bitrate::Kbps256,
            },
            &pcm,
        )
        .unwrap();
        for encoded in [flac, vorbis, mp3] {
            let mut decoded = CompressedReader::new(&encoded, frames).unwrap();
            let mut exact = vec![0.; frames * 2];
            assert_eq!(decoded.read_frames(&mut exact).unwrap(), frames);
            assert_eq!(decoded.read_frames(&mut [0.; 2]).unwrap(), 0);
            let mut oracle_reader = input.clone();
            oracle_reader.samples = exact;
            let config = options(48000, frames, 7, 17);
            let expected = reference(oracle_reader, config);
            let source = CompressedReader::new(&encoded, frames).unwrap();
            let (producer, mut consumer) = audio_ring(2, 4).unwrap();
            let mut pump = ConvertedReadAhead::new(
                source,
                move || CompressedReader::new(&encoded, frames),
                producer,
                &consumer,
                config,
            )
            .unwrap();
            for target in [0, 1, expected.len() / 2] {
                if target != 0 {
                    pump.seek_stopped(&mut consumer, target as u64).unwrap();
                }
                let mut actual = Vec::new();
                loop {
                    let p = pump.pump().unwrap();
                    let mut out = [0.; 14];
                    let n = consumer.pop(&mut out).unwrap();
                    actual.extend_from_slice(&out[..n * 2]);
                    if p.end_of_file && consumer.queued_frames() == 0 {
                        break;
                    }
                }
                assert_eq!(actual, &expected[target * 2..]);
                assert_eq!(pump.counts().input_frames_read, frames as u64);
                assert_eq!(pump.counts().input_frames_submitted, frames as u64);
            }
        }
    }
}

#[test]
fn changed_unknown_duration_candidate_cannot_stall_or_change_verified_input_length_at_eof() {
    for (frames, longer, conversion, rate) in [
        (17, 33, Conversion::Copy, 48000),
        (18, 19, Conversion::Sinc { taps: 16 }, 3000),
    ] {
        let mut source = reader(frames, 48000);
        source.declared = None;
        let original = source.clone();
        let mode = Arc::new(AtomicUsize::new(0));
        let factory_mode = Arc::clone(&mode);
        let (producer, mut consumer) = audio_ring(2, 32).unwrap();
        let mut config = options(rate, 64, 7, 7);
        config.conversion = conversion;
        let expected = reference(original.clone(), config);
        let mut pump = ConvertedReadAhead::new(
            source,
            move || {
                let mut r = reader(
                    match factory_mode.load(Ordering::Relaxed) {
                        1 => longer,
                        2 => frames - 1,
                        _ => frames,
                    },
                    48000,
                );
                r.declared = None;
                Ok(r)
            },
            producer,
            &consumer,
            config,
        )
        .unwrap();
        while !pump.pump().unwrap().end_of_file {}
        let endpoint = pump.output_generated_position();
        assert_eq!(consumer.queued_frames(), expected.len() / 2);
        let before = (
            pump.counts(),
            pump.input_reader_position(),
            pump.generation(),
            consumer.queued_frames(),
        );
        for changed in [1, 2] {
            mode.store(changed, Ordering::Relaxed);
            assert!(pump.seek_stopped(&mut consumer, endpoint).is_err());
            assert_eq!(
                (
                    pump.counts(),
                    pump.input_reader_position(),
                    pump.generation(),
                    consumer.queued_frames()
                ),
                before
            );
        }
        let mut exact = vec![0.; expected.len()];
        assert_eq!(consumer.pop(&mut exact).unwrap(), expected.len() / 2);
        assert_eq!(exact, expected);
        mode.store(0, Ordering::Relaxed);
        pump.seek_stopped(&mut consumer, endpoint).unwrap();
        assert!(pump.pump().unwrap().end_of_file);
        assert_eq!(consumer.queued_frames(), 0);
    }
}

#[test]
fn unknown_duration_overlimit_and_nonfinite_batches_preserve_only_validated_prefix() {
    for invalid in [false, true] {
        let mut source = reader(if invalid { 17 } else { 9 }, 48000);
        source.declared = None;
        if invalid {
            source.samples[8 * 2] = f32::NAN;
        }
        let fresh = source.clone();
        let (producer, mut consumer) = audio_ring(2, 32).unwrap();
        let mut config = options(48000, if invalid { 17 } else { 8 }, 7, 7);
        config.conversion = Conversion::Copy;
        let mut pump = ConvertedReadAhead::new(
            source,
            move || Ok(fresh.clone()),
            producer,
            &consumer,
            config,
        )
        .unwrap();
        assert_eq!(pump.pump().unwrap().published_output_frames, 7);
        let error = pump.pump().unwrap_err();
        if invalid {
            assert!(matches!(error, Error::InvalidSamples));
        } else {
            assert!(matches!(error, Error::Capacity));
        }
        assert_eq!(
            pump.counts().input_frames_read,
            if invalid { 14 } else { 9 }
        );
        assert_eq!(pump.counts().input_frames_submitted, 7);
        assert_eq!(pump.counts().output_frames_generated, 7);
        assert_eq!(consumer.queued_frames(), 7);
        let mut prefix = [0.; 14];
        consumer.pop(&mut prefix).unwrap();
        assert_eq!(prefix.as_slice(), &reader(17, 48000).samples[..14]);
        assert!(matches!(pump.pump(), Err(Error::Poisoned)));
    }
    let mut empty = reader(0, 48000);
    empty.declared = None;
    let fresh = empty.clone();
    let (producer, consumer) = audio_ring(2, 4).unwrap();
    let mut config = options(48000, 8, 7, 7);
    config.conversion = Conversion::Copy;
    let mut pump = ConvertedReadAhead::new(
        empty,
        move || Ok(fresh.clone()),
        producer,
        &consumer,
        config,
    )
    .unwrap();
    assert!(matches!(pump.pump(), Err(Error::InvalidSamples)));
    assert_eq!(consumer.queued_frames(), 0);
}
