//! CONTROL-only caller-selected raw interleaved little-endian f64 PCM analysis.
//! At most 16 MiB input; explicit roles/rate/tail. No device or playback.
use ravet_signal::{
    KWeightingDesign, LoudnessChannelRole as Role, LoudnessRecordCapacities, LoudnessTailPolicy,
    PreparedLoudnessMeter64,
};
use std::io::Read;
fn number(value: Option<f64>) -> String {
    value.map_or_else(|| "none".into(), |v| format!("{v:.17e}"))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err("usage: loudness-file RAW_F64LE RATE ROLES(f,s,x) TAIL(none,lra1500)".into());
    }
    let rate = args[2].parse()?;
    let design = KWeightingDesign::new(rate)?;
    let roles: Vec<_> = args[3]
        .split(',')
        .map(|r| match r {
            "f" => Ok(Role::Front),
            "s" => Ok(Role::Surround),
            "x" => Ok(Role::Excluded),
            _ => Err("roles must be comma-separated f/s/x"),
        })
        .collect::<Result<_, _>>()?;
    let tail = match args[4].as_str() {
        "none" => LoudnessTailPolicy::None,
        "lra1500" => LoudnessTailPolicy::LraSilence1500ms,
        _ => return Err("tail must be none/lra1500".into()),
    };
    let mut meter = PreparedLoudnessMeter64::new(
        &design,
        &roles,
        256,
        LoudnessRecordCapacities {
            integrated: 32768,
            lra: 32768,
        },
        true,
    )?;
    const MAX_BYTES: usize = 16 * 1024 * 1024;
    let file = std::fs::File::open(&args[1])?;
    let mut data = Vec::new();
    data.try_reserve_exact(MAX_BYTES + 1)?;
    file.take((MAX_BYTES + 1) as u64).read_to_end(&mut data)?;
    let frame_bytes = 8 * roles.len();
    if data.len() > MAX_BYTES || !data.len().is_multiple_of(frame_bytes) {
        return Err(
            "raw interleaved f64 input must contain complete frames and at most 16 MiB".into(),
        );
    }
    let mut planar = [[0_f64; 256]; 64];
    for chunk in data.chunks(256 * frame_bytes) {
        let frames = chunk.len() / frame_bytes;
        for (i, frame) in chunk.chunks_exact(frame_bytes).enumerate() {
            for (ch, sample) in frame.as_chunks::<8>().0.iter().enumerate() {
                planar[ch][i] = f64::from_le_bytes(*sample);
            }
        }
        let refs = planar.each_ref().map(|x| &x[..frames]);
        meter.observe_planar(&refs[..roles.len()], frames)?;
    }
    let live = meter.reading();
    meter.finish(tail)?;
    let r = meter.analyse_programme()?;
    println!(
        "rate={rate} channels={} source_frames={} tail_frames={} M={} S={} I={} LRA={} I_absolute={} I_relative={} LRA_absolute={} LRA_relative={} nonfinite={} finalized={}",
        roles.len(),
        r.source_frames,
        r.tail_frames,
        number(live.momentary_lufs),
        number(live.short_term_lufs),
        number(r.integrated_lufs),
        number(r.loudness_range_lu),
        r.integrated_absolute_count,
        r.integrated_relative_count,
        r.lra_absolute_count,
        r.lra_relative_count,
        live.nonfinite_input,
        r.finalized
    );
    Ok(())
}
