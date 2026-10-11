#![cfg(feature = "resampled-playback")]
use ravet_audio_file::{
    FilePlaybackOptions, PlaybackConversion, PreparedFilePlayback, WaveEncoding, WaveFormat,
    WaveReader, WaveWriter, prepare_file_playback,
};
use ravet_device::{FilePlaybackOutput, PreparedOutput};
use ravet_foundation::{
    AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessStatus, Processor,
    Transport,
};
use std::io::Cursor;
fn prepared(conversion: PlaybackConversion) -> PreparedFilePlayback {
    let samples: Vec<f32> = (0..257)
        .flat_map(|n| [n as f32 / 512., -n as f32 / 512.])
        .collect();
    let mut writer = WaveWriter::new(
        Cursor::new(Vec::new()),
        WaveFormat {
            channels: 2,
            sample_rate: 32000,
            encoding: WaveEncoding::Float32,
        },
        257,
    )
    .unwrap();
    writer.write_frames(&samples).unwrap();
    let reader = WaveReader::new(Cursor::new(writer.finish().unwrap().into_inner()), 17).unwrap();
    prepare_file_playback(
        reader,
        FilePlaybackOptions {
            output_rate: 48000,
            conversion,
            maximum_source_frames: 257,
            maximum_output_frames: 4096,
            source_batch_frames: 113,
            maximum_render_frames: 113,
        },
    )
    .unwrap()
}
fn config(rate: f64, frames: usize, channels: usize) -> ProcessConfig {
    ProcessConfig::new(rate, frames, 0, AudioLayout::new(0, channels).unwrap()).unwrap()
}
fn process(output: &mut FilePlaybackOutput, left: &mut [f32], right: &mut [f32]) -> ProcessStatus {
    let frames = left.len();
    let mut views = [left, right];
    let audio = AudioBuffer::separate(&[], &mut views, frames).unwrap();
    output.process(&mut ProcessBlock::new(audio, &[], Transport::default()))
}
#[test]
fn actual_prepared_output_matches_all_converted_frames_tail_and_variable_callbacks_without_allocations()
 {
    for conversion in [
        PlaybackConversion::Sinc { taps: 16 },
        PlaybackConversion::IntegerSinc { taps: 16 },
    ] {
        let mut reference = prepared(conversion);
        let frames = reference.info().prepared_frames;
        reference.playback_mut().play();
        let mut expected = vec![0.; frames * 2];
        for chunk in expected.chunks_mut(113 * 2) {
            reference.playback_mut().render(chunk).unwrap();
        }
        for partitions in [&[1, 17, 113, 4096][..], &[7, 61, 257][..]] {
            let mut processor = FilePlaybackOutput::new(prepared(conversion));
            processor.playback_mut().play();
            let mut output = PreparedOutput::new(processor, 48000, 2, 113).unwrap();
            let mut actual = vec![0.; (frames + 257) * 2];
            let mut offset = 0;
            let mut part = 0;
            let allocation = allocation_counter::measure(|| {
                while offset < actual.len() {
                    let n = (partitions[part % partitions.len()] * 2).min(actual.len() - offset);
                    output.render(&mut actual[offset..offset + n]);
                    offset += n;
                    part += 1;
                }
            });
            assert_eq!(allocation.count_total, 0);
            assert_eq!(&actual[..expected.len()], &expected);
            assert!(actual[expected.len()..].iter().all(|v| *v == 0.));
            assert_eq!(output.statistics().processor_faults, 0);
        }
    }
}
#[test]
fn actual_preparation_rejects_mismatches_and_invalid_blocks_never_advance() {
    for conversion in [
        PlaybackConversion::Sinc { taps: 16 },
        PlaybackConversion::IntegerSinc { taps: 16 },
    ] {
        assert!(matches!(
            PreparedOutput::new(FilePlaybackOutput::new(prepared(conversion)), 44100, 2, 113),
            Err(PrepareError::UnsupportedLayout)
        ));
        assert!(matches!(
            PreparedOutput::new(FilePlaybackOutput::new(prepared(conversion)), 48000, 1, 113),
            Err(PrepareError::UnsupportedLayout)
        ));
        assert!(matches!(
            PreparedOutput::new(FilePlaybackOutput::new(prepared(conversion)), 48000, 2, 114),
            Err(PrepareError::InvalidMaximumFrames)
        ));
        let mut processor = FilePlaybackOutput::new(prepared(conversion));
        processor.playback_mut().play();
        processor.prepare(config(48000., 113, 2)).unwrap();
        let mut left = [1.; 114];
        let mut right = [1.; 114];
        assert!(matches!(
            process(&mut processor, &mut left, &mut right),
            ProcessStatus::Fault(_)
        ));
        assert_eq!(processor.playback().position(), 0);
        assert!(left.iter().chain(&right).all(|x| *x == 0.));
        assert_eq!(
            processor.prepare(config(44100., 113, 2)),
            Err(PrepareError::UnsupportedLayout)
        );
        let mut left = [1.; 7];
        let mut right = [1.; 7];
        assert!(matches!(
            process(&mut processor, &mut left, &mut right),
            ProcessStatus::Fault(_)
        ));
        assert_eq!(processor.playback().position(), 0);
        assert!(left.iter().chain(&right).all(|x| *x == 0.));
    }
}
#[test]
fn exclusive_pause_resume_seek_loop_reset_and_deactivation_preserve_cursor_contract() {
    for conversion in [
        PlaybackConversion::Sinc { taps: 16 },
        PlaybackConversion::IntegerSinc { taps: 16 },
    ] {
        let mut processor = FilePlaybackOutput::new(prepared(conversion));
        processor.prepare(config(48000., 113, 2)).unwrap();
        let mut left = [1.; 7];
        let mut right = [1.; 7];
        assert_eq!(
            process(&mut processor, &mut left, &mut right),
            ProcessStatus::Silence
        );
        assert_eq!(processor.playback().position(), 0);
        processor.playback_mut().play();
        assert_eq!(
            process(&mut processor, &mut left, &mut right),
            ProcessStatus::Continue
        );
        assert_eq!(processor.playback().position(), 7);
        processor.playback_mut().pause();
        assert_eq!(
            process(&mut processor, &mut left, &mut right),
            ProcessStatus::Silence
        );
        assert_eq!(processor.playback().position(), 7);
        processor.playback_mut().seek(17).unwrap();
        processor.playback_mut().play();
        process(&mut processor, &mut left, &mut right);
        assert_eq!(processor.playback().position(), 24);
        processor.playback_mut().set_loop(Some(31..34)).unwrap();
        process(&mut processor, &mut left, &mut right);
        assert_eq!(processor.playback().position(), 32);
        assert_eq!(left[0], left[3]);
        assert_eq!(left[1], left[4]);
        assert_eq!(right[0], right[3]);
        processor.reset();
        assert_eq!(processor.playback().position(), 31);
        assert!(!processor.playback().is_playing());
        processor.deactivate();
        assert!(matches!(
            process(&mut processor, &mut left, &mut right),
            ProcessStatus::Fault(_)
        ));
        let recovered = processor.into_prepared();
        assert_eq!(recovered.playback().position(), 31);
    }
}

#[test]
fn repeated_prepare_deactivate_and_reset_retain_clip_and_transport_semantics() {
    for conversion in [
        PlaybackConversion::Sinc { taps: 16 },
        PlaybackConversion::IntegerSinc { taps: 16 },
    ] {
        let mut processor = FilePlaybackOutput::new(prepared(conversion));
        for _ in 0..3 {
            processor.prepare(config(48000., 113, 2)).unwrap();
            processor.playback_mut().play();
            let mut left = [0.; 17];
            let mut right = [0.; 17];
            assert_eq!(
                process(&mut processor, &mut left, &mut right),
                ProcessStatus::Continue
            );
            assert_eq!(processor.playback().position(), 17);
            processor.deactivate();
            assert!(matches!(
                process(&mut processor, &mut left, &mut right),
                ProcessStatus::Fault(_)
            ));
            assert_eq!(processor.playback().position(), 17);
            processor.prepare(config(48000., 7, 2)).unwrap();
            processor.reset();
            assert_eq!(processor.playback().position(), 0);
            assert!(!processor.playback().is_playing());
            let mut left = [1.; 7];
            let mut right = [1.; 7];
            assert_eq!(
                process(&mut processor, &mut left, &mut right),
                ProcessStatus::Silence
            );
            assert!(left.iter().chain(&right).all(|s| *s == 0.));
        }
    }
}

#[test]
fn malformed_native_partial_frame_is_silenced_before_clip_consumption() {
    for conversion in [
        PlaybackConversion::Sinc { taps: 16 },
        PlaybackConversion::IntegerSinc { taps: 16 },
    ] {
        let mut expected = prepared(conversion);
        expected.playback_mut().play();
        let mut reference = [0.; 14];
        expected.playback_mut().render(&mut reference).unwrap();
        let mut processor = FilePlaybackOutput::new(prepared(conversion));
        processor.playback_mut().play();
        let mut output = PreparedOutput::new(processor, 48000, 2, 113).unwrap();
        let mut malformed = [1.; 3];
        output.render(&mut malformed);
        assert_eq!(malformed, [0.; 3]);
        let mut valid = [0.; 14];
        output.render(&mut valid);
        assert_eq!(valid, reference);
        assert_eq!(output.statistics().processor_faults, 1);
    }
}
