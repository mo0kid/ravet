use ravet_audio_file::{MemoryPlayback, PlaybackError};
#[test]
fn pause_seek_end_silence_and_stop_preserve_frame_clock() {
    let mut player = MemoryPlayback::new(&[1., -1., 2., -2., 3., -3.], 2, 48000, 4).unwrap();
    assert_eq!(
        (player.channels(), player.sample_rate(), player.frames()),
        (2, 48000, 3)
    );
    let mut output = [99.; 8];
    assert_eq!(player.render(&mut output).unwrap().source_frames, 0);
    assert_eq!(output, [0.; 8]);
    player.play();
    assert_eq!(player.render(&mut output[..2]).unwrap().source_frames, 1);
    assert_eq!(output[..2], [1., -1.]);
    player.pause();
    player.render(&mut output).unwrap();
    assert_eq!(player.position(), 1);
    player.play();
    let end = player.render(&mut output).unwrap();
    assert_eq!(end.source_frames, 2);
    assert!(end.ended);
    assert!(!player.is_playing());
    assert_eq!(output, [2., -2., 3., -3., 0., 0., 0., 0.]);
    player.play();
    assert!(!player.is_playing());
    player.seek(0).unwrap();
    assert!(!player.is_playing());
    player.play();
    player.render(&mut []).unwrap();
    assert_eq!(player.position(), 0);
    assert!(player.is_playing());
    player.stop();
    assert_eq!(player.position(), 0);
    assert!(!player.is_playing());
}
#[test]
fn half_open_loops_wrap_across_blocks_and_invalid_commands_preserve_state() {
    let mut player = MemoryPlayback::new(&[0., 1., 2., 3.], 1, 48000, 8).unwrap();
    player.seek(4).unwrap();
    player.set_loop(Some(1..3)).unwrap();
    assert_eq!(player.position(), 1);
    player.play();
    let mut output = [99.; 7];
    let progress = player.render(&mut output).unwrap();
    assert_eq!(output, [1., 2., 1., 2., 1., 2., 1.]);
    assert_eq!(progress.loop_wraps, 3);
    assert_eq!(player.position(), 2);
    assert!(!progress.ended);
    assert_eq!(player.set_loop(Some(3..3)), Err(PlaybackError::InvalidLoop));
    assert_eq!(player.set_loop(Some(1..5)), Err(PlaybackError::InvalidLoop));
    assert_eq!(player.seek(5), Err(PlaybackError::InvalidPosition));
    assert_eq!(player.position(), 2);
    assert_eq!(player.loop_range(), Some(1..3));
    assert!(player.is_playing());
    let mut oversized = [9.; 9];
    assert!(player.render(&mut oversized).is_err());
    assert_eq!(oversized, [0.; 9]);
    assert_eq!(player.position(), 2);
    player.stop();
    assert_eq!(player.position(), 1);
    player.set_loop(Some(2..3)).unwrap();
    player.play();
    assert_eq!(player.render(&mut output).unwrap().loop_wraps, 7);
    assert_eq!(output, [2.; 7]);
    player.set_loop(None).unwrap();
    assert_eq!(player.render(&mut output).unwrap().source_frames, 2);
    assert_eq!(output, [2., 3., 0., 0., 0., 0., 0.]);
}
#[test]
fn preparation_render_rejection_and_prepared_heap_contract() {
    for (samples, channels, rate, maximum) in [
        (&[][..], 1, 48000, 4),
        (&[f32::NAN][..], 1, 48000, 4),
        (&[0.][..], 2, 48000, 4),
        (&[0.][..], 0, 48000, 4),
        (&[0.][..], 1, 0, 4),
        (&[0.][..], 1, 48000, 0),
    ] {
        assert!(MemoryPlayback::new(samples, channels, rate, maximum).is_err());
    }
    let mut player = MemoryPlayback::new(&[1., 2.], 2, 48000, 8).unwrap();
    player.play();
    let mut malformed = [9.; 3];
    assert!(player.render(&mut malformed).is_err());
    assert_eq!(malformed, [0.; 3]);
    assert!(player.is_playing());
    assert_eq!(player.position(), 0);
    let mut output = [0.; 16];
    let counts = allocation_counter::measure(|| {
        for _ in 0..10_000 {
            player.set_loop(Some(0..1)).unwrap();
            player.play();
            assert_eq!(player.render(&mut output).unwrap().source_frames, 8);
            player.pause();
            player.seek(1).unwrap();
            player.stop();
            player.set_loop(None).unwrap();
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}
