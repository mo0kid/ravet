//! Per-block cost of the prepared stereo FIR and oversampler against the
//! existing generic paths. Run in release mode:
//!
//! ```sh
//! cargo run --release --locked -p ravet-signal --example fir-oversampling-bench
//! ```
//!
//! Each figure is the median of seven timed runs, in nanoseconds per stereo
//! frame at the host rate (both channels). Results depend on the machine,
//! compiler and load; they are not a callback-deadline guarantee.
use ravet_signal::{
    FirConvolver, Oversampler, OversamplingFactor, PartitionedConvolver, StereoFir,
    StereoOversampler,
};
use std::hint::black_box;
use std::time::Instant;

const BLOCK: usize = 512;

fn signal(length: usize, seed: u32) -> Vec<f32> {
    let mut state = seed;
    (0..length)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 8) as f32 / (1u32 << 24) as f32 - 0.5
        })
        .collect()
}

/// Median nanoseconds per stereo frame of `run`, which processes one block.
fn measure(mut run: impl FnMut()) -> f64 {
    // Choose a block count giving roughly 50 ms per timed run.
    let start = Instant::now();
    let mut calibration = 0;
    while start.elapsed().as_millis() < 20 {
        run();
        calibration += 1;
    }
    let blocks = (calibration * 50 / 20).max(4);
    let mut samples: Vec<f64> = (0..7)
        .map(|_| {
            let start = Instant::now();
            for _ in 0..blocks {
                run();
            }
            start.elapsed().as_nanos() as f64 / (blocks * BLOCK) as f64
        })
        .collect();
    samples.sort_by(f64::total_cmp);
    samples[3]
}

/// A straightforward per-sample f32 ring-buffer FIR, as a typical baseline.
struct RingFir {
    taps: Vec<f32>,
    history: Vec<f32>,
    cursor: usize,
}
impl RingFir {
    fn process(&mut self, samples: &mut [f32]) {
        let length = self.history.len();
        for sample in samples {
            self.history[self.cursor] = *sample;
            let mut sum = 0.;
            let mut read = self.cursor;
            for tap in &self.taps {
                sum += tap * self.history[read];
                read = if read == 0 { length - 1 } else { read - 1 };
            }
            self.cursor = (self.cursor + 1) % length;
            *sample = sum;
        }
    }
}

fn main() {
    let input = [signal(BLOCK, 1), signal(BLOCK, 2)];
    println!("Block {BLOCK} frames; ns per stereo frame (median of 7).");
    println!();
    println!(
        "| Taps | StereoFir f32 | StereoFir f64 | ring f32 ×2 | FirConvolver f64 ×2 | Partitioned f64 ×2 |"
    );
    println!("|---:|---:|---:|---:|---:|---:|");
    for taps in [16usize, 64, 256, 1024] {
        let impulse = signal(taps, 3);
        let impulse64: Vec<f64> = impulse.iter().map(|&tap| f64::from(tap)).collect();
        let [mut left, mut right] = input.clone();
        let mut fir = StereoFir::new(&impulse, BLOCK).unwrap();
        let fir32 = measure(|| {
            left.copy_from_slice(&input[0]);
            right.copy_from_slice(&input[1]);
            fir.process(&mut left, &mut right).unwrap();
            black_box((&left, &right));
        });
        let mut left64: Vec<f64> = input[0].iter().map(|&s| f64::from(s)).collect();
        let mut right64: Vec<f64> = input[1].iter().map(|&s| f64::from(s)).collect();
        let source64 = (left64.clone(), right64.clone());
        let mut fir = StereoFir::new(&impulse64, BLOCK).unwrap();
        let fir64 = measure(|| {
            left64.copy_from_slice(&source64.0);
            right64.copy_from_slice(&source64.1);
            fir.process(&mut left64, &mut right64).unwrap();
            black_box((&left64, &right64));
        });
        let mut rings = [0, 1].map(|_| RingFir {
            taps: impulse.clone(),
            history: vec![0.; taps],
            cursor: 0,
        });
        let ring = measure(|| {
            left.copy_from_slice(&input[0]);
            right.copy_from_slice(&input[1]);
            rings[0].process(&mut left);
            rings[1].process(&mut right);
            black_box((&left, &right));
        });
        let mut generic = [0, 1].map(|_| FirConvolver::new(&impulse64).unwrap());
        let convolver = measure(|| {
            left64.copy_from_slice(&source64.0);
            right64.copy_from_slice(&source64.1);
            generic[0].process_in_place(&mut left64);
            generic[1].process_in_place(&mut right64);
            black_box((&left64, &right64));
        });
        let partition = 512.min(taps.next_power_of_two());
        let mut partitioned =
            [0, 1].map(|_| PartitionedConvolver::new(&impulse64, partition).unwrap());
        let fft = measure(|| {
            left64.copy_from_slice(&source64.0);
            right64.copy_from_slice(&source64.1);
            partitioned[0].process_in_place(&mut left64);
            partitioned[1].process_in_place(&mut right64);
            black_box((&left64, &right64));
        });
        println!("| {taps} | {fir32:.2} | {fir64:.2} | {ring:.2} | {convolver:.2} | {fft:.2} |");
    }
    println!();
    println!("| Factor | StereoOversampler | Oversampler ×2 (per sample) |");
    println!("|---:|---:|---:|");
    for factor in [
        OversamplingFactor::Two,
        OversamplingFactor::Four,
        OversamplingFactor::Eight,
    ] {
        let [mut left, mut right] = input.clone();
        let mut block = StereoOversampler::new(factor, BLOCK).unwrap();
        let stereo = measure(|| {
            left.copy_from_slice(&input[0]);
            right.copy_from_slice(&input[1]);
            block
                .process(&mut left, &mut right, |l, r| {
                    black_box((l, r));
                })
                .unwrap();
            black_box((&left, &right));
        });
        let mut channels = [Oversampler::new(factor), Oversampler::new(factor)];
        let per_sample = measure(|| {
            left.copy_from_slice(&input[0]);
            right.copy_from_slice(&input[1]);
            for (channel, samples) in channels.iter_mut().zip([&mut left, &mut right]) {
                for sample in samples.iter_mut() {
                    *sample = channel.process_sample(*sample, |s| s);
                }
            }
            black_box((&left, &right));
        });
        println!("| {}× | {stereo:.2} | {per_sample:.2} |", factor.factor());
    }
}
