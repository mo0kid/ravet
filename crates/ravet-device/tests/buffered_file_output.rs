#![cfg(feature = "resampled-playback")]
use ravet_audio_file::{
    AudioFileError as Error, BufferedPlayback, ConvertedReadAhead, ConvertedReadAheadOptions,
    FilePlaybackOptions, PlaybackConversion, ReadAheadStatus, ReadAheadWorker, WaveEncoding,
    WaveFormat, WaveReader, WaveWriter, audio_ring, prepare_file_playback,
};
use ravet_device::{BufferedPlaybackOutput, BufferedPlaybackOutputStatistics, PreparedOutput};
use ravet_foundation::{
    AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessStatus, Processor,
    Transport,
};
use std::{
    io::Cursor,
    time::{Duration, Instant},
};
type Reader = WaveReader<Cursor<Vec<u8>>>;
type Factory = Box<dyn FnMut() -> Result<Reader, Error> + Send>;
type Worker = ReadAheadWorker<ConvertedReadAhead<Reader, Factory>>;
fn prepared(conversion: PlaybackConversion) -> (Worker, BufferedPlayback, Vec<f32>) {
    let samples: Vec<f32> = (0..8193)
        .flat_map(|n| [0.5 * (n as f32 * 0.11).sin(), 0.5 * (n as f32 * 0.07).sin()])
        .collect();
    let mut writer = WaveWriter::new(
        Cursor::new(Vec::new()),
        WaveFormat {
            channels: 2,
            sample_rate: 44100,
            encoding: WaveEncoding::Float32,
        },
        4096,
    )
    .unwrap();
    for chunk in samples.chunks(8192) {
        writer.write_frames(chunk).unwrap();
    }
    let bytes = writer.finish().unwrap().into_inner();
    let mut reference = prepare_file_playback(
        WaveReader::new(Cursor::new(bytes.clone()), 113).unwrap(),
        FilePlaybackOptions {
            output_rate: 48000,
            conversion,
            maximum_source_frames: 8193,
            maximum_output_frames: 10000,
            source_batch_frames: 113,
            maximum_render_frames: 113,
        },
    )
    .unwrap();
    let mut expected = vec![0.; reference.info().prepared_frames * 2];
    reference.playback_mut().play();
    for chunk in expected.chunks_mut(226) {
        reference.playback_mut().render(chunk).unwrap();
    }
    let reader = WaveReader::new(Cursor::new(bytes.clone()), 17).unwrap();
    let factory: Factory = Box::new(move || WaveReader::new(Cursor::new(bytes.clone()), 17));
    let (producer, consumer) = audio_ring(2, 32).unwrap();
    let pump = ConvertedReadAhead::new(
        reader,
        factory,
        producer,
        &consumer,
        ConvertedReadAheadOptions {
            output_rate: 48000,
            conversion,
            maximum_source_frames: 8193,
            maximum_output_frames: 10000,
            source_batch_frames: 17,
            conversion_batch_frames: 7,
        },
    )
    .unwrap();
    let (worker, playback) = ReadAheadWorker::new(pump, consumer, 113).unwrap();
    (worker, playback, expected)
}
fn config(rate: f64, max: usize, channels: usize) -> ProcessConfig {
    ProcessConfig::new(rate, max, 0, AudioLayout::new(0, channels).unwrap()).unwrap()
}
fn process(
    processor: &mut BufferedPlaybackOutput,
    left: &mut [f32],
    right: &mut [f32],
) -> ProcessStatus {
    let frames = left.len();
    let mut views = [left, right];
    let audio = AudioBuffer::separate(&[], &mut views, frames).unwrap();
    processor.process(&mut ProcessBlock::new(audio, &[], Transport::default()))
}
#[test]
fn actual_prepared_output_streams_all_converted_pcm_tail_and_eof_without_callback_heap() {
    for conversion in [
        PlaybackConversion::Sinc { taps: 16 },
        PlaybackConversion::IntegerSinc { taps: 16 },
    ] {
        let (mut worker, mut playback, expected) = prepared(conversion);
        worker.start().unwrap();
        playback.play();
        let processor = BufferedPlaybackOutput::new(playback);
        let monitor = processor.monitor();
        let mut output = PreparedOutput::new(processor, 48000, 2, 113).unwrap();
        let mut actual = Vec::new();
        let mut buffer = [0.; 226];
        let partitions = [1, 7, 17, 32, 113];
        let mut callback = 0;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let before = monitor.statistics().consumed_output_frames;
            let requested = partitions[callback % partitions.len()];
            callback += 1;
            let allocation =
                allocation_counter::measure(|| output.render(&mut buffer[..requested * 2]));
            assert_eq!(allocation.count_total, 0);
            let after = monitor.statistics();
            let count = (after.consumed_output_frames - before) as usize;
            assert!(count <= requested);
            actual.extend_from_slice(&buffer[..count * 2]);
            if after.ended {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(actual, expected);
        assert_eq!(
            monitor.statistics().output_source_position,
            (expected.len() / 2) as u64
        );
        assert_eq!(output.statistics().processor_faults, 0);
        assert_eq!(
            monitor.statistics().worker_status,
            ReadAheadStatus::EndOfFile
        );
        drop(output);
        let exit = worker.shutdown().unwrap();
        assert!(exit.error.is_none());
    }
}
#[test]
fn exclusive_bridge_pause_reset_reclaimed_seek_lifecycle_and_mismatches() {
    for conversion in [
        PlaybackConversion::Sinc { taps: 16 },
        PlaybackConversion::IntegerSinc { taps: 16 },
    ] {
        let (mut worker, playback, _) = prepared(conversion);
        let mut processor = BufferedPlaybackOutput::new(playback);
        assert_eq!(
            processor.statistics(),
            BufferedPlaybackOutputStatistics::default()
        );
        assert_eq!(
            processor.statistics().worker_status,
            ReadAheadStatus::Stopped
        );
        assert_eq!(
            processor.prepare(config(44100., 113, 2)),
            Err(PrepareError::UnsupportedLayout)
        );
        assert_eq!(
            processor.prepare(config(48000., 114, 2)),
            Err(PrepareError::InvalidMaximumFrames)
        );
        assert_eq!(
            processor.prepare(config(48000., 113, 1)),
            Err(PrepareError::UnsupportedLayout)
        );
        processor.prepare(config(48000., 113, 2)).unwrap();
        worker.start().unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while processor.playback().queued_frames() < 32 {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        let mut left = [1.; 7];
        let mut right = [1.; 7];
        assert_eq!(
            process(&mut processor, &mut left, &mut right),
            ProcessStatus::Silence
        );
        assert_eq!(processor.playback().position_frames(), 0);
        assert_eq!(processor.playback().queued_frames(), 32);
        assert_eq!(
            processor.statistics().worker_status,
            ReadAheadStatus::Running
        );
        processor.playback_mut().play();
        assert_eq!(
            process(&mut processor, &mut left, &mut right),
            ProcessStatus::Continue
        );
        assert_eq!(processor.playback().position_frames(), 7);
        processor.reset();
        assert_eq!(
            process(&mut processor, &mut left, &mut right),
            ProcessStatus::Silence
        );
        assert_eq!(processor.playback().position_frames(), 7);
        processor.deactivate();
        assert!(matches!(
            process(&mut processor, &mut left, &mut right),
            ProcessStatus::Fault(_)
        ));
        processor.prepare(config(48000., 7, 2)).unwrap();
        worker.seek_stopped(processor.playback_mut(), 17).unwrap();
        assert_eq!(processor.playback().position_frames(), 17);
        assert!(!processor.playback().is_playing());
        for _ in 0..3 {
            processor.deactivate();
            processor.prepare(config(48000., 7, 2)).unwrap();
            processor.reset();
            assert_eq!(
                process(&mut processor, &mut left, &mut right),
                ProcessStatus::Silence
            );
            assert_eq!(processor.playback().position_frames(), 17);
        }
        let exit = worker.shutdown().unwrap();
        assert!(exit.error.is_none());
        assert_eq!(
            process(&mut processor, &mut left, &mut right),
            ProcessStatus::Silence
        );
        assert_eq!(
            processor.statistics().worker_status,
            ReadAheadStatus::Stopped
        );
        assert!(!processor.statistics().ended);
        processor.deactivate();
        let playback = processor.into_playback();
        assert_eq!(playback.position_frames(), 17);
        drop(playback);
    }
}

#[test]
fn failed_worker_retains_exact_queued_prefix_through_actual_output_and_returns_original_error() {
    use ravet_audio_file::PlaybackReader;
    struct FaultReader {
        position: u64,
    }
    impl PlaybackReader for FaultReader {
        fn channels(&self) -> usize {
            2
        }
        fn sample_rate(&self) -> u32 {
            48000
        }
        fn position_frames(&self) -> u64 {
            self.position
        }
        fn declared_frames(&self) -> Option<u64> {
            Some(17)
        }
        fn maximum_read_frames(&self) -> usize {
            7
        }
        fn read_frames(&mut self, output: &mut [f32]) -> Result<usize, Error> {
            if self.position != 0 {
                self.position += 1;
                return Err(Error::Io(std::io::Error::other(
                    "original bridge reader fault",
                )));
            }
            for frame in 0..7 {
                output[frame * 2] = frame as f32 / 16.;
                output[frame * 2 + 1] = -(frame as f32) / 16.;
            }
            self.position = 7;
            Ok(7)
        }
    }
    let (producer, consumer) = audio_ring(2, 32).unwrap();
    let pump = ConvertedReadAhead::new(
        FaultReader { position: 0 },
        || Ok(FaultReader { position: 0 }),
        producer,
        &consumer,
        ConvertedReadAheadOptions {
            output_rate: 48000,
            conversion: PlaybackConversion::Copy,
            maximum_source_frames: 17,
            maximum_output_frames: 17,
            source_batch_frames: 7,
            conversion_batch_frames: 7,
        },
    )
    .unwrap();
    let (mut worker, mut playback) = ReadAheadWorker::new(pump, consumer, 7).unwrap();
    worker.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while playback.worker_status() != ReadAheadStatus::Failed {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(playback.queued_frames(), 7);
    playback.play();
    let processor = BufferedPlaybackOutput::new(playback);
    let monitor = processor.monitor();
    let mut output = PreparedOutput::new(processor, 48000, 2, 7).unwrap();
    let mut first = [1.; 6];
    assert_eq!(
        allocation_counter::measure(|| output.render(&mut first)).count_total,
        0
    );
    assert_eq!(first, [0., 0., 1. / 16., -1. / 16., 2. / 16., -2. / 16.]);
    let mut last = [1.; 14];
    assert_eq!(
        allocation_counter::measure(|| output.render(&mut last)).count_total,
        0
    );
    assert_eq!(
        last,
        [
            3. / 16.,
            -3. / 16.,
            4. / 16.,
            -4. / 16.,
            5. / 16.,
            -5. / 16.,
            6. / 16.,
            -6. / 16.,
            0.,
            0.,
            0.,
            0.,
            0.,
            0.
        ]
    );
    assert_eq!(monitor.statistics().consumed_output_frames, 7);
    assert_eq!(monitor.statistics().output_source_position, 7);
    assert_eq!(monitor.statistics().underflow_output_frames, 0);
    assert!(!monitor.statistics().ended);
    assert_eq!(monitor.statistics().worker_status, ReadAheadStatus::Failed);
    assert_eq!(output.statistics().processor_faults, 0);
    drop(output);
    let exit = worker.shutdown().unwrap();
    assert!(
        matches!(exit.error,Some(Error::Io(e)) if e.to_string()=="original bridge reader fault")
    );
    assert_eq!(exit.pump.input_reader_position(), 8);
    assert_eq!(exit.pump.counts().input_frames_submitted, 7);
}
