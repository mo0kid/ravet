//! Worker-only validation of caller-selected raw mono little-endian f64 PCM.
//! No device access or implicit playback; maximum 16 MiB input, explicit profile.
use ravet_signal::{TruePeakDesign, TruePeakFactor, TruePeakMeter64};
use std::io::Read;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 6 {
        return Err("usage: true-peak-file RAW_F64LE RATE FACTOR HALF_WIDTH BETA".into());
    }
    let rate = args[2].parse()?;
    let factor = match args[3].as_str() {
        "4" => TruePeakFactor::Four,
        "8" => TruePeakFactor::Eight,
        "16" => TruePeakFactor::Sixteen,
        _ => return Err("factor must be 4/8/16".into()),
    };
    let half = args[4].parse()?;
    let beta = args[5].parse()?;
    let design = TruePeakDesign::new(rate, half, factor, beta)?;
    const MAX_BYTES: usize = 16 * 1024 * 1024;
    let file = std::fs::File::open(&args[1])?;
    let mut data = Vec::new();
    data.try_reserve_exact(MAX_BYTES + 1)?;
    file.take((MAX_BYTES + 1) as u64).read_to_end(&mut data)?;
    if data.len() > MAX_BYTES || !data.len().is_multiple_of(8) {
        return Err("raw f64 input must contain complete samples and at most 16 MiB".into());
    }
    let source: Vec<f64> = data
        .as_chunks::<8>()
        .0
        .iter()
        .map(|b| f64::from_le_bytes(*b))
        .collect();
    let mut meter = TruePeakMeter64::new(&design, 65536)?;
    for part in source.chunks(65536) {
        meter.observe(part)?;
    }
    meter.finish();
    let r = meter.reading();
    println!(
        "rate={rate} factor={} half={half} beta={beta} source_frames={} tail_frames={} phase_observations={} sample_peak={:.17e} interpolated_peak={:.17e} db_tp={:.17e} nonfinite={} overflow={}",
        factor.factor(),
        r.source_frames,
        r.tail_frames,
        r.phase_observations,
        r.sample_peak,
        r.interpolated_peak,
        r.db_tp(),
        r.nonfinite_input,
        r.amplitude_overflow
    );
    Ok(())
}
