use ravet_audio_file::{WaveEncoding, WaveReader};
use std::{
    fs::File,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "ravet-examples-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn run(binary: &str, path: Option<&std::path::Path>) -> String {
    let mut command = Command::new(binary);
    if let Some(path) = path {
        command.arg("--render").arg(path);
    } else {
        command.arg("--offline");
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn read(path: &std::path::Path, frames: usize) -> Vec<f32> {
    let mut reader = WaveReader::new(File::open(path).unwrap(), 256).unwrap();
    assert_eq!(reader.format().channels, 2);
    assert_eq!(reader.format().sample_rate, 48_000);
    assert_eq!(reader.format().encoding, WaveEncoding::Pcm16);
    assert_eq!(reader.frames(), frames as u64);
    let mut audio = Vec::new();
    let mut block = [0.; 512];
    loop {
        let count = reader.read_frames(&mut block).unwrap();
        if count == 0 {
            break;
        }
        audio.extend_from_slice(&block[..2 * count]);
    }
    assert_eq!(audio.len(), frames * 2);
    audio
}

#[test]
fn synth_renders_timed_notes_pedal_hold_and_complete_release() {
    let scratch = Scratch::new();
    let path = scratch.0.join("nested/synth.wav");
    run(env!("CARGO_BIN_EXE_midi-synth"), Some(&path));
    let audio = read(&path, 72_000);
    assert!(
        audio
            .as_chunks::<2>()
            .0
            .iter()
            .all(|frame| frame[0] == frame[1])
    );
    assert!(audio[..2 * 2_400].iter().all(|v| *v == 0.));
    for (start, end) in [(3_000, 11_000), (17_000, 27_000), (47_000, 59_000)] {
        assert!(audio[2 * start..2 * end].iter().any(|v| v.abs() > 0.04));
    }
    for (start, end) in [(14_500, 16_000), (30_500, 32_000), (62_500, 72_000)] {
        assert!(audio[2 * start..2 * end].iter().all(|v| *v == 0.));
    }
    let original = std::fs::read(&path).unwrap();
    assert!(
        !Command::new(env!("CARGO_BIN_EXE_midi-synth"))
            .arg("--render")
            .arg(&path)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[test]
fn effect_renders_ping_pong_repeats_and_sample_timed_bypass() {
    let scratch = Scratch::new();
    let path = scratch.0.join("effect.wav");
    run(env!("CARGO_BIN_EXE_effect"), Some(&path));
    let audio = read(&path, 96_000);
    for (frame, channel, value) in [
        (12000, 0, 0.5),
        (14400, 1, 0.25),
        (24000, 1, 0.25),
        (26400, 0, 0.125),
        (36000, 0, 0.125),
        (38400, 1, 0.0625),
        (48000, 1, 0.0625),
    ] {
        assert!((audio[2 * frame + channel] - value).abs() < 0.00004);
        assert_eq!(audio[2 * frame + 1 - channel], 0.);
    }
    assert!(audio[2 * 72007..2 * 73600].iter().all(|s| *s == 0.));
    assert!((audio[2 * 73600] - 0.125).abs() < 0.00004);
    assert!(audio[2 * 73600 + 1..].iter().all(|s| *s == 0.));
}

#[test]
fn expression_walkthroughs_report_independent_mpe_and_lossless_midi2() {
    let mpe = run(env!("CARGO_BIN_EXE_mpe"), None);
    assert!(mpe.contains(
        "channel=1 key=60 Held: member_bend=12.0 master_bend=-2.0 pressure=0.787 timbre=0.504"
    ));
    assert!(mpe.contains(
        "channel=2 key=64 Held: member_bend=0.0 master_bend=-2.0 pressure=0.000 timbre=0.000"
    ));
    assert!(mpe.contains("channel=1 key=60 Released: member_bend=12.0 master_bend=0.0"));
    let midi2 = run(env!("CARGO_BIN_EXE_midi2"), None);
    assert!(midi2.contains("frame=0 group=1 channel=2 words=[41923c00, abcd0000]"));
    assert!(midi2.contains("frame=17 group=1 channel=2 words=[41623c00, 91234567]"));
    assert!(midi2.contains("frame=17 group=1 channel=2 words=[41a23c00, deadbeef]"));
    assert!(midi2.contains("frame=48 group=1 channel=2 words=[41823c00, 12340000]"));
}
