//! Headless selected-profile numerical/timing observation, not a deadline benchmark.
use ravet_native_dsp::{PreparedVectorOps32, PreparedVectorOps64, VectorBackend};
use std::{hint::black_box, time::Instant};
fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "65536".into())
        .parse()
        .expect("sample count");
    // Admit count/profile before CONTROL sample allocation.
    PreparedVectorOps32::new(VectorBackend::Scalar, n).expect("count1..1048576");
    let native = VectorBackend::native().expect("explicit native CPU profile");
    for backend in [VectorBackend::Scalar, native] {
        let ops = PreparedVectorOps32::new(backend, n).unwrap();
        let a: Vec<f32> = (0..n).map(|i| 0.5 * (i as f32 * 0.031).sin()).collect();
        let b: Vec<f32> = (0..n).map(|i| 0.3 * (i as f32 * 0.073).cos()).collect();
        let mut gained = vec![0.; n];
        let mut output = vec![0.; n];
        let mut timings = [0u128; 11];
        for time in &mut timings {
            let start = Instant::now();
            for _ in 0..32 {
                ops.gain(black_box(&a), 0.37, &mut gained, n).unwrap();
                ops.add_product(
                    black_box(&a),
                    black_box(&b),
                    &gained,
                    black_box(&mut output),
                    n,
                )
                .unwrap();
            }
            *time = start.elapsed().as_nanos();
        }
        for i in 0..n {
            let product = a[i] * b[i];
            let expected = product + a[i] * 0.37;
            assert_eq!(output[i].to_bits(), expected.to_bits());
        }
        timings.sort_unstable();
        println!(
            "f32 backend={backend:?} n={n} calls64 selected32-pair-loop ns min={} median={} max={} checksum={}",
            timings[0],
            timings[5],
            timings[10],
            ops.sum(&output, n).unwrap()
        );
    }
    for backend in [VectorBackend::Scalar, native] {
        let ops = PreparedVectorOps64::new(backend, n).unwrap();
        let a: Vec<f64> = (0..n).map(|i| 0.5 * (i as f64 * 0.031).sin()).collect();
        let b: Vec<f64> = (0..n).map(|i| 0.3 * (i as f64 * 0.073).cos()).collect();
        let mut gained = vec![0.; n];
        let mut output = vec![0.; n];
        let mut timings = [0u128; 11];
        for time in &mut timings {
            let start = Instant::now();
            for _ in 0..32 {
                ops.gain(black_box(&a), 0.37, &mut gained, n).unwrap();
                ops.add_product(
                    black_box(&a),
                    black_box(&b),
                    &gained,
                    black_box(&mut output),
                    n,
                )
                .unwrap();
            }
            *time = start.elapsed().as_nanos();
        }
        for i in 0..n {
            let product = a[i] * b[i];
            let expected = product + a[i] * 0.37;
            assert_eq!(output[i].to_bits(), expected.to_bits());
        }
        timings.sort_unstable();
        println!(
            "f64 backend={backend:?} n={n} calls64 selected32-pair-loop ns min={} median={} max={} checksum={}",
            timings[0],
            timings[5],
            timings[10],
            ops.sum(&output, n).unwrap()
        );
    }
}
