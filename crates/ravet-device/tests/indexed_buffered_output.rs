#![cfg(feature = "resampled-playback")]
use ravet_audio_file::{
    AudioFileError as Error, BufferedPlayback, ConvertedReadAheadOptions, FilePlaybackOptions,
    IndexedConvertedReadAhead, PlaybackConversion, ReadAheadStatus, ReadAheadWorker, WaveEncoding,
    WaveFormat, WaveReader, WaveWriter, audio_ring, prepare_file_playback,
};
use ravet_device::{BufferedPlaybackOutput, PreparedOutput};
use ravet_foundation::PrepareError;
use std::{
    io::Cursor,
    time::{Duration, Instant},
};
type Reader = WaveReader<Cursor<Vec<u8>>>;
type Factory = Box<dyn FnMut() -> Result<Reader, Error> + Send>;
type Worker = ReadAheadWorker<IndexedConvertedReadAhead<Reader, Factory>>;
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
    let pump = IndexedConvertedReadAhead::new(
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

fn filled(playback: &BufferedPlayback) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while playback.queued_frames() < 32 {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
#[test]
fn indexed_worker_actual_prepared_output_drains_exact_pcm_and_tail_without_callback_heap() {
    let (mut worker, mut playback, expected) =
        prepared(PlaybackConversion::IntegerSinc { taps: 16 });
    worker.start().unwrap();
    playback.play();
    let processor = BufferedPlaybackOutput::new(playback);
    let monitor = processor.monitor();
    let mut output = PreparedOutput::new(processor, 48000, 2, 113).unwrap();
    let mut actual = Vec::new();
    let mut buffer = [0.0; 226];
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut step = 0;
    loop {
        let before = monitor.statistics().consumed_output_frames;
        let requested = [1, 7, 17, 32, 113][step % 5];
        step += 1;
        let allocation =
            allocation_counter::measure(|| output.render(&mut buffer[..requested * 2]));
        assert_eq!(allocation.count_total, 0);
        let after = monitor.statistics();
        let n = (after.consumed_output_frames - before) as usize;
        actual.extend_from_slice(&buffer[..n * 2]);
        if after.ended {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(actual, expected);
    assert_eq!(
        monitor.statistics().worker_status,
        ReadAheadStatus::EndOfFile
    );
    assert_eq!(output.statistics().processor_faults, 0);
    let processor = output.into_processor();
    assert_eq!(
        processor.playback().position_frames(),
        (expected.len() / 2) as u64
    );
    drop(processor);
    let exit = worker.shutdown().unwrap();
    assert!(exit.error.is_none());
    assert_eq!(
        exit.pump.counts().output_frames_published,
        (expected.len() / 2) as u64
    );
}
#[test]
fn reclaimed_actual_renderer_failed_successful_indexed_seek_and_reprepare_preserve_owners() {
    let (mut worker, mut playback, expected) =
        prepared(PlaybackConversion::IntegerSinc { taps: 16 });
    worker.start().unwrap();
    filled(&playback);
    playback.play();
    let processor = BufferedPlaybackOutput::new(playback);
    let monitor = processor.monitor();
    let mut output = PreparedOutput::new(processor, 48000, 2, 7).unwrap();
    let mut prefix = [0.0; 14];
    output.render(&mut prefix);
    assert_eq!(prefix, &expected[..14]);
    let mut processor = output.into_processor();
    assert_eq!(processor.playback().position_frames(), 7);
    filled(processor.playback());
    let queued = processor.playback().queued_frames();
    assert!(matches!(
        worker.seek_stopped(processor.playback_mut(), 10001),
        Err(Error::Capacity)
    ));
    assert_eq!(processor.playback().position_frames(), 7);
    assert_eq!(processor.playback().queued_frames(), queued);
    worker.seek_stopped(processor.playback_mut(), 17).unwrap();
    assert_eq!(processor.playback().position_frames(), 17);
    assert!(processor.playback().is_playing());
    filled(processor.playback());
    let failure = PreparedOutput::new_owned(processor, 44100, 2, 7)
        .err()
        .unwrap();
    assert_eq!(failure.error(), &PrepareError::UnsupportedLayout);
    let (processor, _) = failure.into_parts();
    assert_eq!(processor.playback().position_frames(), 17);
    let mut output = PreparedOutput::new_owned(processor, 48000, 2, 7).unwrap();
    let mut resumed = [0.0; 14];
    assert_eq!(
        allocation_counter::measure(|| output.render(&mut resumed)).count_total,
        0
    );
    assert_eq!(resumed, &expected[34..48]);
    assert_eq!(monitor.statistics().output_source_position, 24);
    assert_eq!(monitor.statistics().consumed_output_frames, 14);
    let mut processor = output.into_processor();
    processor.playback_mut().pause();
    let queued = processor.playback().queued_frames();
    let mut output = PreparedOutput::new_owned(processor, 48000, 2, 7).unwrap();
    let mut paused = [1.0; 14];
    output.render(&mut paused);
    assert_eq!(paused, [0.0; 14]);
    let processor = output.into_processor();
    assert_eq!(processor.playback().position_frames(), 24);
    assert!(processor.playback().queued_frames() >= queued);
    drop(processor);
    let exit = worker.shutdown().unwrap();
    assert!(exit.error.is_none());
    assert_eq!(exit.pump.generation(), 1);
    assert_eq!(exit.pump.output_origin(), 17);
}
