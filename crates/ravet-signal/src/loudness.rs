//! Prepared calibrated K-weighted loudness, with bounded CONTROL programme analysis.
use crate::{Biquad, BiquadCoefficients};
use std::fmt;
const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_RECORDS: usize = 1_000_000;
/// Whole-call refusal, preparation failure or invalid programme result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoudnessError {
    SampleRate,
    Coefficient,
    ChannelCount,
    MaximumFrames,
    RecordCapacity,
    BufferLength,
    TooManyFrames,
    FrameCount,
    RecordLimit,
    Finalized,
    TailInStandby,
    NumericFault,
    /// Defensive 32 MiB aggregate guard; current record caps request at most 24,000,000
    /// array bytes, so only a larger future cap or oversized actual capacity reaches it.
    ResourceLimit,
    Allocation,
}
impl fmt::Display for LoudnessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SampleRate => "loudness sample rate must be an integer 8000..768000 Hz",
            Self::Coefficient => "loudness coefficients must be finite and strictly stable",
            Self::ChannelCount => "loudness channels must match the prepared 1..64 roles",
            Self::MaximumFrames => "loudness maximum callback must be 1..65536 frames",
            Self::RecordCapacity => "loudness record capacity must be 1..1000000",
            Self::BufferLength => "loudness buffers must cover every complete prefix",
            Self::TooManyFrames => "loudness callback exceeds the prepared maximum",
            Self::FrameCount => {
                "loudness counts must retain complete optional end-silence headroom"
            }
            Self::RecordLimit => "loudness complete call exceeds prepared programme records",
            Self::Finalized => "loudness input or running control requires reset after finish",
            Self::TailInStandby => "requested loudness end silence requires a running programme",
            Self::NumericFault => "loudness results require reset after a filter numeric fault",
            Self::ResourceLimit => "loudness prepared arrays exceed 32 MiB",
            Self::Allocation => "loudness CONTROL allocation failed",
        })
    }
}
impl std::error::Error for LoudnessError {}
/// Explicit roles, without inferred channel ordering or advanced spatial metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoudnessChannelRole {
    Front,
    Surround,
    Excluded,
}
impl LoudnessChannelRole {
    pub const fn power_weight(self) -> f64 {
        match self {
            Self::Front => 1.,
            Self::Surround => 1.41,
            Self::Excluded => 0.,
        }
    }
}
/// Immutable CONTROL calibration. Native f32 audio deliberately widens into f64.
///
/// Two rational bilinear sections use the sample-rate-independent parameter facts
/// from Brecht De Man's 2018 loudness calibration (see guide/provenance). The RLB
/// numerator remains (1,-2,1); there is no hidden rate-dependent gain correction.
/// The actual rounded response therefore depends on the sample rate. Selected
/// oracle-row transfer facts relative to 48 kHz at the SAME physical frequency are
/// +0.215/+0.216/+0.014 dB at 37/137/997 Hz for 8 kHz, +0.086/+0.086/+0.040 dB for
/// 16 kHz and -0.041/-0.041/-0.035 dB for 768 kHz. These are selected-transfer facts
/// only, not a wideband supremum, programme error, profile accuracy or conformity
/// claim. Only the 48 kHz coefficients have an independent numeric anchor; other
/// rates are this bilinear derivation, checked by a Decimal oracle that shares the
/// same calibration facts (arithmetic fidelity, not independent certification).
/// Integer rates 8000..768000; strict existing biquad Jury validation. No default,
/// smoothing, standards certification or automatic spatial/object weighting.
#[derive(Clone, Copy, Debug)]
pub struct KWeightingDesign {
    rate: u32,
    coefficients: [BiquadCoefficients; 2],
    rows: [([f64; 3], [f64; 2]); 2],
}
impl KWeightingDesign {
    pub fn new(sample_rate: u32) -> Result<Self, LoudnessError> {
        if !(8000..=768000).contains(&sample_rate) {
            return Err(LoudnessError::SampleRate);
        }
        // Explicit published mathematical calibration facts, not filter tables.
        #[allow(clippy::excessive_precision)] // Retain the published decimal calibration fact.
        let shelf_frequency = 1681.9744509555319;
        let shelf_q = 0.7071752369554193;
        let gain = 3.99984385397;
        let exponent = 0.499666774155;
        let rlb_frequency = 38.13547087613982;
        let rlb_q = 0.5003270373253953;
        let polynomial = |c: [f64; 3], k: f64| {
            [
                c[0] + c[1] * k + c[2] * k * k,
                2. * (c[2] * k * k - c[0]),
                c[0] - c[1] * k + c[2] * k * k,
            ]
        };
        let k = (std::f64::consts::PI * shelf_frequency / sample_rate as f64).tan();
        let vh = 10_f64.powf(gain / 20.);
        let vb = vh.powf(exponent);
        let a = polynomial([1., 1. / shelf_q, 1.], k);
        let b = polynomial([vh, vb / shelf_q, 1.], k);
        let shelf = (b.map(|v| v / a[0]), [a[1] / a[0], a[2] / a[0]]);
        let k = (std::f64::consts::PI * rlb_frequency / sample_rate as f64).tan();
        let a = polynomial([1., 1. / rlb_q, 1.], k);
        let rlb = ([1., -2., 1.], [a[1] / a[0], a[2] / a[0]]);
        let coefficients = [
            BiquadCoefficients::new(shelf.0, shelf.1).map_err(|_| LoudnessError::Coefficient)?,
            BiquadCoefficients::new(rlb.0, rlb.1).map_err(|_| LoudnessError::Coefficient)?,
        ];
        Ok(Self {
            rate: sample_rate,
            coefficients,
            rows: [shelf, rlb],
        })
    }
    pub const fn sample_rate(self) -> u32 {
        self.rate
    }
    /// Supplied actual normalized row; denominator leading coefficient is one.
    pub fn section(self, index: usize) -> Option<([f64; 3], [f64; 2])> {
        self.rows.get(index).copied()
    }
    /// CONTROL response in linear amplitude, physical frequency 0..Nyquist.
    pub fn magnitude_at(self, frequency: f64) -> Result<f64, LoudnessError> {
        if !frequency.is_finite() || !(0. ..=self.rate as f64 / 2.).contains(&frequency) {
            return Err(LoudnessError::Coefficient);
        }
        let normalized = frequency / self.rate as f64;
        let mut result = 1.;
        for c in self.coefficients {
            result *= c
                .magnitude_at(normalized)
                .map_err(|_| LoudnessError::Coefficient)?;
        }
        if result.is_finite() {
            Ok(result)
        } else {
            Err(LoudnessError::Coefficient)
        }
    }
}
/// Independently bounded complete I/LRA records, each 1..1000000.
/// Prepared LRA report scratch follows the LRA limit. The aggregate 32 MiB check is a
/// defensive guard: current per-array caps imply at most 24,000,000 array bytes plus
/// the owner, so these requests cannot reach the pre-reserve refusal, and the check on
/// actual allocated capacity covers allocators that return more than requested.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoudnessRecordCapacities {
    pub integrated: usize,
    pub lra: usize,
}
/// Optional artificial analysis after stopped ownership, never original source.
/// The zero tail is this crate's own padding policy, not a standards requirement;
/// prefer `None` when only the original duration is analysed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoudnessTailPolicy {
    None,
    /// Exactly ceil(3*Fs/2) zeros; LRA only, not integrated or live audio windows.
    LraSilence1500ms,
}
/// First actual filter fault; coordinate is a zero-based source-plus-tail index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoudnessFault {
    pub coordinate: u64,
    pub artificial_tail: bool,
    pub channel: usize,
    pub section: usize,
}
/// Latest complete-grid windows, without implicit post-window smoothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoudnessLiveReading64 {
    pub momentary_lufs: Option<f64>,
    pub short_term_lufs: Option<f64>,
    pub maximum_momentary_lufs: Option<f64>,
    pub maximum_short_term_lufs: Option<f64>,
    pub momentary_endpoint_frame: Option<u64>,
    pub short_term_endpoint_frame: Option<u64>,
    pub source_frames: u64,
    pub programme_source_frames: u64,
    pub programme_interval_frames: u64,
    pub tail_frames: u64,
    pub integrated_records: usize,
    pub lra_records: usize,
    pub running: bool,
    pub finalized: bool,
    pub nonfinite_input: bool,
    pub numeric_fault: Option<LoudnessFault>,
}
/// Exclusive CONTROL analysis of all retained programme records.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoudnessProgrammeReport64 {
    pub integrated_lufs: Option<f64>,
    pub loudness_range_lu: Option<f64>,
    pub integrated_absolute_count: usize,
    pub integrated_relative_count: usize,
    pub lra_absolute_count: usize,
    pub lra_relative_count: usize,
    pub integrated_relative_gate_lufs: Option<f64>,
    pub lra_relative_gate_lufs: Option<f64>,
    pub lra_low_lufs: Option<f64>,
    pub lra_high_lufs: Option<f64>,
    pub integrated_endpoint_frame: Option<u64>,
    pub lra_endpoint_frame: Option<u64>,
    pub source_frames: u64,
    pub programme_source_frames: u64,
    pub tail_frames: u64,
    pub finalized: bool,
}
// Positive sums only: no sliding subtraction or physical large/small squares.
#[derive(Clone, Copy, Debug, Default)]
struct Energy {
    scale: f64,
    squares: f64,
    frames: u32,
}
impl Energy {
    fn add(&mut self, sample: f64, weight: f64) {
        let magnitude = sample.abs();
        if magnitude == 0. || weight == 0. {
            return;
        }
        if magnitude > self.scale {
            let ratio = self.scale / magnitude;
            self.squares = self.squares * ratio * ratio + weight;
            self.scale = magnitude;
        } else {
            let ratio = magnitude / self.scale;
            self.squares += weight * ratio * ratio;
        }
    }
    fn combine(&mut self, other: Self) {
        self.frames += other.frames;
        if other.scale == 0. {
            return;
        }
        if other.scale > self.scale {
            let ratio = self.scale / other.scale;
            self.squares = self.squares * ratio * ratio + other.squares;
            self.scale = other.scale;
        } else {
            let ratio = other.scale / self.scale;
            self.squares += other.squares * ratio * ratio;
        }
    }
    fn level(self) -> f64 {
        if self.scale == 0. {
            f64::NEG_INFINITY
        } else {
            -0.691 + 20. * self.scale.log10() + 10. * (self.squares / self.frames as f64).log10()
        }
    }
}
#[derive(Debug, Default)]
struct Window {
    blocks: [Energy; 30],
    next: usize,
    filled: usize,
}
impl Window {
    fn push(&mut self, energy: Energy) {
        self.blocks[self.next] = energy;
        self.next = (self.next + 1) % 30;
        self.filled = (self.filled + 1).min(30);
    }
    fn level(&self, count: usize) -> Option<f64> {
        (self.filled >= count).then(|| {
            let mut energy = Energy::default();
            for i in 0..count {
                energy.combine(self.blocks[(self.next + 29 - i) % 30]);
            }
            energy.level()
        })
    }
}
#[derive(Debug)]
struct Grid {
    rate: u32,
    remainder: u32,
    remaining: u32,
    energy: Energy,
    window: Window,
    frames: u64,
}
impl Grid {
    fn new(rate: u32) -> Self {
        Self {
            rate,
            remainder: rate % 10,
            remaining: rate / 10,
            energy: Energy::default(),
            window: Window::default(),
            frames: 0,
        }
    }
    fn finish_frame(&mut self) -> bool {
        self.frames += 1;
        self.energy.frames += 1;
        self.remaining -= 1;
        if self.remaining != 0 {
            return false;
        }
        self.window.push(self.energy);
        self.energy = Energy::default();
        let sum = self.remainder + self.rate % 10;
        self.remaining = self.rate / 10 + sum / 10;
        self.remainder = sum % 10;
        true
    }
}
fn completed(frames: u64, rate: u32) -> u64 {
    (((frames as u128 + 1) * 10 - 1) / rate as u128) as u64
}
fn new_records(old: u64, new: u64, warmup: u64, rate: u32) -> usize {
    (completed(new, rate).saturating_sub(warmup) - completed(old, rate).saturating_sub(warmup))
        as usize
}
fn reserve(count: usize) -> Result<Vec<f64>, LoudnessError> {
    let bytes = count.checked_mul(8).ok_or(LoudnessError::ResourceLimit)?;
    if bytes > MAX_BYTES {
        return Err(LoudnessError::ResourceLimit);
    }
    let mut result = Vec::new();
    result
        .try_reserve_exact(count)
        .map_err(|_| LoudnessError::Allocation)?;
    Ok(result)
}
fn mean_level(values: impl Iterator<Item = f64> + Clone) -> Option<(f64, usize)> {
    let mut count = 0;
    let mut maximum = f64::NEG_INFINITY;
    for value in values.clone() {
        maximum = maximum.max(value);
        count += 1;
    }
    if count == 0 {
        return None;
    }
    // Callers gate above a finite threshold, so maximum is finite.
    let scaled: f64 = values.map(|v| 10_f64.powf((v - maximum) / 10.)).sum();
    Some((maximum + 10. * (scaled / count as f64).log10(), count))
}
/// Prepared one-programme M/S/I/LRA owner, with independent weighted channels.
///
/// Exact nominal 10-Hz endpoints use floor(k*Fs/10), via bounded integer remainder.
/// Four-grid windows have declared sample quantization; thirty-grid windows are
/// exactly 3*Fs frames. Means divide by actual summed source-frame counts. Odd-rate
/// floor policy is not asserted as normative discrete rounding. Live windows run
/// during standby; programme pause discards partial windows and keeps records.
///
/// Whole-call shape/prefix/maximum/source/record admission precedes mutation.
/// Nonfinite input is measurement silence with flag, raw audio untouched. A genuine
/// finite-input filter overflow latches its exact first coordinate, invalidates
/// readings/reports until reset and stops further filter execution while accepted
/// counts continue. There is no false rollback or hidden sanitisation. After such a
/// fault `integrated_records()`/`lra_records()` remain readable as RAW diagnostic
/// history (complete pre-fault records, partially measured windows and later
/// unavailable `-inf` windows); they are not valid reports. Use `reading()` and
/// `analyse_programme()`, which refuse, to decide validity.
///
/// Excluded channels perform no filtering and keep no K history; they only flag
/// nonfinite source. Front/Surround channels keep K history across programme standby.
///
/// Observation/reset allocate nothing, lock nothing and do no I/O. Programme
/// analysis is bounded O(N)/O(N log N) CONTROL work using prepared scratch, never
/// callback work or automatic concurrent UI delivery. Explicit optional end silence
/// is stopped LRA-only work, not a finite IIR tail certificate or implicit Drop.
/// This calibrated profile has no standards, physical or deadline guarantee.
///
/// ```
/// use ravet_signal::{KWeightingDesign, LoudnessChannelRole as Role,
///     LoudnessRecordCapacities, LoudnessTailPolicy, PreparedLoudnessMeter64};
/// let design = KWeightingDesign::new(8001)?;
/// let mut meter = PreparedLoudnessMeter64::new(&design, &[Role::Front], 64,
///     LoudnessRecordCapacities { integrated: 64, lra: 32 }, true)?;
/// for block in 0..500 {
///     let samples: [f64; 64] = std::array::from_fn(|i|
///         0.1 * (std::f64::consts::TAU * 997. * (block*64+i) as f64 / 8001.).sin());
///     meter.observe_planar(&[&samples], 64)?;
/// }
/// assert!(meter.reading().short_term_lufs.is_some());
/// // After exclusive stopped ownership; no automatic finish on deactivation.
/// meter.finish(LoudnessTailPolicy::None)?;
/// let report = meter.analyse_programme()?;
/// assert_eq!(report.source_frames, 32000);
/// assert_eq!(report.tail_frames, 0);
/// assert!(report.integrated_lufs.is_some());
/// # Ok::<(), ravet_signal::LoudnessError>(())
/// ```
#[derive(Debug)]
pub struct PreparedLoudnessMeter64 {
    design: KWeightingDesign,
    roles: [LoudnessChannelRole; 64],
    channels: usize,
    maximum: usize,
    capacities: LoudnessRecordCapacities,
    filters: [[Biquad; 2]; 64],
    live: Grid,
    programme: Grid,
    source: u64,
    admitted: u64,
    tail: u64,
    running: bool,
    finalized: bool,
    nonfinite: bool,
    fault: Option<LoudnessFault>,
    momentary: Option<f64>,
    short: Option<f64>,
    max_momentary: Option<f64>,
    max_short: Option<f64>,
    momentary_end: Option<u64>,
    short_end: Option<u64>,
    integrated_end: Option<u64>,
    lra_end: Option<u64>,
    integrated: Vec<f64>,
    lra: Vec<f64>,
    scratch: Vec<f64>,
}
impl PreparedLoudnessMeter64 {
    /// Borrowed profile/roles survive any refusal; all reservation is CONTROL.
    pub fn new(
        design: &KWeightingDesign,
        roles: &[LoudnessChannelRole],
        maximum_frames: usize,
        capacities: LoudnessRecordCapacities,
        initial_running: bool,
    ) -> Result<Self, LoudnessError> {
        if !(1..=64).contains(&roles.len()) {
            return Err(LoudnessError::ChannelCount);
        }
        if !(1..=65536).contains(&maximum_frames) {
            return Err(LoudnessError::MaximumFrames);
        }
        if !(1..=MAX_RECORDS).contains(&capacities.integrated)
            || !(1..=MAX_RECORDS).contains(&capacities.lra)
        {
            return Err(LoudnessError::RecordCapacity);
        }
        // Defensive 32 MiB guard, distinct from the tighter 24,000,000-byte maximum that
        // the current record caps can request. It runs on requested sizes before any
        // allocation and again on the actual capacities that were reserved.
        let within_budget = |capacities: [usize; 3]| {
            capacities
                .into_iter()
                .try_fold(size_of::<Self>(), |sum, n| {
                    n.checked_mul(8)?.checked_add(sum)
                })
                .is_some_and(|bytes| bytes <= MAX_BYTES)
        };
        if !within_budget([capacities.integrated, capacities.lra, capacities.lra]) {
            return Err(LoudnessError::ResourceLimit);
        }
        let integrated = reserve(capacities.integrated)?;
        let lra = reserve(capacities.lra)?;
        let mut scratch = reserve(capacities.lra)?;
        if !within_budget([integrated.capacity(), lra.capacity(), scratch.capacity()]) {
            return Err(LoudnessError::ResourceLimit);
        }
        scratch.resize(capacities.lra, 0.);
        let mut owned_roles = [LoudnessChannelRole::Excluded; 64];
        owned_roles[..roles.len()].copy_from_slice(roles);
        Ok(Self {
            design: *design,
            roles: owned_roles,
            channels: roles.len(),
            maximum: maximum_frames,
            capacities,
            filters: std::array::from_fn(|_| design.coefficients.map(Biquad::new)),
            live: Grid::new(design.rate),
            programme: Grid::new(design.rate),
            source: 0,
            admitted: 0,
            tail: 0,
            running: initial_running,
            finalized: false,
            nonfinite: false,
            fault: None,
            momentary: None,
            short: None,
            max_momentary: None,
            max_short: None,
            momentary_end: None,
            short_end: None,
            integrated_end: None,
            lra_end: None,
            integrated,
            lra,
            scratch,
        })
    }
    pub const fn design(&self) -> KWeightingDesign {
        self.design
    }
    pub fn roles(&self) -> &[LoudnessChannelRole] {
        &self.roles[..self.channels]
    }
    pub const fn maximum_frames(&self) -> usize {
        self.maximum
    }
    pub const fn record_capacities(&self) -> LoudnessRecordCapacities {
        self.capacities
    }
    /// Actual complete-window LUFS records; readonly, without report sorting.
    /// After a numeric fault these are raw diagnostic history, not a valid result:
    /// check `reading().numeric_fault` before treating them as programme data.
    pub fn integrated_records(&self) -> &[f64] {
        &self.integrated
    }
    pub fn lra_records(&self) -> &[f64] {
        &self.lra
    }
    pub fn reading(&self) -> LoudnessLiveReading64 {
        let valid = self.fault.is_none();
        LoudnessLiveReading64 {
            momentary_lufs: valid.then_some(self.momentary).flatten(),
            short_term_lufs: valid.then_some(self.short).flatten(),
            maximum_momentary_lufs: valid.then_some(self.max_momentary).flatten(),
            maximum_short_term_lufs: valid.then_some(self.max_short).flatten(),
            momentary_endpoint_frame: self.momentary_end,
            short_term_endpoint_frame: self.short_end,
            source_frames: self.source,
            programme_source_frames: self.admitted,
            programme_interval_frames: self.programme.frames,
            tail_frames: self.tail,
            integrated_records: self.integrated.len(),
            lra_records: self.lra.len(),
            running: self.running,
            finalized: self.finalized,
            nonfinite_input: self.nonfinite,
            numeric_fault: self.fault,
        }
    }
    fn tail_bound(&self) -> u64 {
        (3 * self.design.rate as u64).div_ceil(2)
    }
    fn capacity_preflight(&self, frames: u64, tail_only: bool) -> Result<(), LoudnessError> {
        let new = self
            .programme
            .frames
            .checked_add(frames)
            .ok_or(LoudnessError::FrameCount)?;
        for (length, warmup, capacity) in [
            (self.integrated.len(), 3, self.capacities.integrated),
            (self.lra.len(), 29, self.capacities.lra),
        ] {
            if tail_only && warmup == 3 {
                continue;
            }
            let additional = new_records(self.programme.frames, new, warmup, self.design.rate);
            if length.checked_add(additional).is_none_or(|n| n > capacity) {
                return Err(LoudnessError::RecordLimit);
            }
        }
        Ok(())
    }
    fn preflight(&self, channels: usize, frames: usize) -> Result<(), LoudnessError> {
        if channels != self.channels {
            return Err(LoudnessError::ChannelCount);
        }
        if frames > self.maximum {
            return Err(LoudnessError::TooManyFrames);
        }
        if self.finalized {
            return Err(LoudnessError::Finalized);
        }
        if self
            .source
            .checked_add(frames as u64)
            .is_none_or(|n| n > u64::MAX - self.tail_bound())
        {
            return Err(LoudnessError::FrameCount);
        }
        if self.running {
            self.capacity_preflight(frames as u64, false)?;
        }
        Ok(())
    }
    fn step(&mut self, input: &[f64], artificial: bool) {
        for (channel, &raw) in input.iter().enumerate() {
            let sample = if raw.is_finite() {
                raw
            } else {
                self.nonfinite = true;
                0.
            };
            let weight = self.roles[channel].power_weight();
            if weight == 0. || self.fault.is_some() {
                continue;
            }
            let mut y = sample;
            for (section, filter) in self.filters[channel].iter_mut().enumerate() {
                y = filter.process_sample(y);
                if !y.is_finite() || filter.delay_state().iter().any(|v| !v.is_finite()) {
                    self.fault = Some(LoudnessFault {
                        coordinate: self.source + self.tail,
                        artificial_tail: artificial,
                        channel,
                        section,
                    });
                    break;
                }
            }
            if self.fault.is_none() {
                if !artificial {
                    self.live.energy.add(y, weight);
                }
                if self.running {
                    self.programme.energy.add(y, weight);
                }
            }
        }
        if artificial {
            self.tail += 1;
        } else {
            self.source += 1;
            if self.running {
                self.admitted += 1;
            }
            if self.live.finish_frame() {
                if let Some(level) = self.live.window.level(4) {
                    self.momentary = Some(level);
                    self.max_momentary =
                        Some(self.max_momentary.unwrap_or(f64::NEG_INFINITY).max(level));
                    self.momentary_end = Some(self.source);
                }
                if let Some(level) = self.live.window.level(30) {
                    self.short = Some(level);
                    self.max_short = Some(self.max_short.unwrap_or(f64::NEG_INFINITY).max(level));
                    self.short_end = Some(self.source);
                }
            }
        }
        if self.running && self.programme.finish_frame() {
            let endpoint = self.source + self.tail;
            if !artificial && let Some(level) = self.programme.window.level(4) {
                self.integrated.push(level);
                self.integrated_end = Some(endpoint);
            }
            if let Some(level) = self.programme.window.level(30) {
                self.lra.push(level);
                self.lra_end = Some(endpoint);
            }
        }
    }
    pub fn observe_frame(&mut self, input: &[f64]) -> Result<(), LoudnessError> {
        self.preflight(input.len(), 1)?;
        self.step(input, false);
        Ok(())
    }
    pub fn observe_frame_f32(&mut self, input: &[f32]) -> Result<(), LoudnessError> {
        self.preflight(input.len(), 1)?;
        let mut frame = [0.; 64];
        for (out, &sample) in frame.iter_mut().zip(input) {
            *out = sample as f64;
        }
        self.step(&frame[..self.channels], false);
        Ok(())
    }
    pub fn observe_planar(&mut self, input: &[&[f64]], frames: usize) -> Result<(), LoudnessError> {
        self.observe(input.iter().copied(), input.len(), frames, |v| v)
    }
    pub fn observe_planar_f32(
        &mut self,
        input: &[&[f32]],
        frames: usize,
    ) -> Result<(), LoudnessError> {
        self.observe(input.iter().copied(), input.len(), frames, |v| v as f64)
    }
    pub fn observe_in_place(
        &mut self,
        input: &[&mut [f64]],
        frames: usize,
    ) -> Result<(), LoudnessError> {
        self.observe(input.iter().map(|s| &**s), input.len(), frames, |v| v)
    }
    pub fn observe_in_place_f32(
        &mut self,
        input: &[&mut [f32]],
        frames: usize,
    ) -> Result<(), LoudnessError> {
        self.observe(input.iter().map(|s| &**s), input.len(), frames, |v| {
            v as f64
        })
    }
    fn observe<'a, S: Copy + 'a, I: Iterator<Item = &'a [S]> + Clone>(
        &mut self,
        input: I,
        channels: usize,
        frames: usize,
        convert: impl Fn(S) -> f64,
    ) -> Result<(), LoudnessError> {
        self.preflight(channels, frames)?;
        if input.clone().any(|s| s.len() < frames) {
            return Err(LoudnessError::BufferLength);
        }
        let mut frame = [0.; 64];
        for i in 0..frames {
            for (out, source) in frame.iter_mut().zip(input.clone()) {
                *out = convert(source[i]);
            }
            self.step(&frame[..self.channels], false);
        }
        Ok(())
    }
    /// Simultaneous I/LRA control; standby discards partial programme windows.
    /// K history/live audio windows continue, prior programme records remain.
    pub fn set_programme_running(&mut self, running: bool) -> Result<(), LoudnessError> {
        if self.finalized {
            return Err(LoudnessError::Finalized);
        }
        if self.running != running {
            if !running {
                self.programme = Grid::new(self.design.rate);
            }
            self.running = running;
        }
        Ok(())
    }
    /// Explicit exclusive stopped finalization; record refusal changes nothing.
    /// Repeated finish is idempotent even with a different requested tail policy:
    /// the first successful policy wins and later requests are not applied. With no
    /// admitted programme frame `LraSilence1500ms` succeeds without any tail; the
    /// actual `tail_frames` count is the only signal. The tail is own padding policy.
    pub fn finish(&mut self, policy: LoudnessTailPolicy) -> Result<(), LoudnessError> {
        if self.finalized {
            return Ok(());
        }
        if policy == LoudnessTailPolicy::LraSilence1500ms {
            if !self.running {
                return Err(LoudnessError::TailInStandby);
            }
            if self.admitted != 0 {
                let count = self.tail_bound();
                self.capacity_preflight(count, true)?;
                let zero = [0.; 64];
                for _ in 0..count {
                    self.step(&zero[..self.channels], true);
                }
            }
        }
        self.finalized = true;
        Ok(())
    }
    /// Clear all measurement history and counts, preserving profile/running choice.
    pub fn reset(&mut self) {
        for channel in &mut self.filters {
            for filter in channel {
                filter.reset();
            }
        }
        self.live = Grid::new(self.design.rate);
        self.programme = Grid::new(self.design.rate);
        self.source = 0;
        self.admitted = 0;
        self.tail = 0;
        self.finalized = false;
        self.nonfinite = false;
        self.fault = None;
        self.momentary = None;
        self.short = None;
        self.max_momentary = None;
        self.max_short = None;
        self.momentary_end = None;
        self.short_end = None;
        self.integrated_end = None;
        self.lra_end = None;
        self.integrated.clear();
        self.lra.clear();
        self.scratch.fill(0.);
    }
    /// Exclusive CONTROL O(N)/O(N log N) report, never callback work.
    /// Strict I gates; inclusive LRA gates and integer nearest-rank percentiles.
    pub fn analyse_programme(&mut self) -> Result<LoudnessProgrammeReport64, LoudnessError> {
        if self.fault.is_some() {
            return Err(LoudnessError::NumericFault);
        }
        let absolute = mean_level(self.integrated.iter().copied().filter(|v| *v > -70.));
        let gate = absolute.map(|(level, _)| level - 10.);
        let integrated = gate.and_then(|gate| {
            mean_level(
                self.integrated
                    .iter()
                    .copied()
                    .filter(|v| *v > -70. && *v > gate),
            )
        });
        let lra_absolute = mean_level(self.lra.iter().copied().filter(|v| *v >= -70.));
        let lra_gate = lra_absolute.map(|(level, _)| level - 20.);
        let mut count = 0;
        if let Some(gate) = lra_gate {
            for &value in &self.lra {
                if value >= -70. && value >= gate {
                    self.scratch[count] = value;
                    count += 1;
                }
            }
        }
        self.scratch[..count].sort_unstable_by(f64::total_cmp);
        let (low, high) = if count == 0 {
            (None, None)
        } else {
            let rank = |p: usize| ((count - 1) * p + 50) / 100;
            (Some(self.scratch[rank(10)]), Some(self.scratch[rank(95)]))
        };
        Ok(LoudnessProgrammeReport64 {
            integrated_lufs: integrated.map(|(level, _)| level),
            loudness_range_lu: low.zip(high).map(|(low, high)| high - low),
            integrated_absolute_count: absolute.map_or(0, |(_, n)| n),
            integrated_relative_count: integrated.map_or(0, |(_, n)| n),
            lra_absolute_count: lra_absolute.map_or(0, |(_, n)| n),
            lra_relative_count: count,
            integrated_relative_gate_lufs: gate,
            lra_relative_gate_lufs: lra_gate,
            lra_low_lufs: low,
            lra_high_lufs: high,
            integrated_endpoint_frame: self.integrated_end,
            lra_endpoint_frame: self.lra_end,
            source_frames: self.source,
            programme_source_frames: self.admitted,
            tail_frames: self.tail,
            finalized: self.finalized,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn owner(rate: u32) -> PreparedLoudnessMeter64 {
        PreparedLoudnessMeter64::new(
            &KWeightingDesign::new(rate).unwrap(),
            &[LoudnessChannelRole::Front],
            65536,
            LoudnessRecordCapacities {
                integrated: 64,
                lra: 64,
            },
            true,
        )
        .unwrap()
    }
    #[test]
    fn retained_records_energy_gates_integer_percentiles_and_boundary_policies() {
        let mut m = owner(8001);
        m.integrated
            .extend_from_slice(&[-70., -69., -35., -25., -95.]);
        m.lra
            .extend_from_slice(&[-70., -60., -50., -40., -30., -20., -10., 0.]);
        let r = m.analyse_programme().unwrap();
        assert_eq!(r.integrated_absolute_count, 3);
        assert_eq!(r.integrated_relative_count, 2);
        let expected = 10. * ((10_f64.powf(-3.5) + 10_f64.powf(-2.5)) / 2.).log10();
        assert!((r.integrated_lufs.unwrap() - expected).abs() < 1e-14);
        assert_eq!(r.lra_absolute_count, 8);
        assert_eq!(r.lra_relative_count, 3);
        assert_eq!(r.lra_low_lufs, Some(-20.));
        assert_eq!(r.lra_high_lufs, Some(0.));
        assert_eq!(r.loudness_range_lu, Some(20.));
        m.integrated.push(-5.);
        let r = m.analyse_programme().unwrap();
        assert_eq!(r.integrated_relative_count, 1);
        assert_eq!(r.integrated_lufs, Some(-5.));
        assert_eq!(
            &m.integrated[..5],
            &[-70., -69., -35., -25., -95.],
            "relative gate does not destroy quiet history"
        );
        m.reset();
        m.integrated.push(-70.);
        m.lra.push(-70.);
        let r = m.analyse_programme().unwrap();
        assert_eq!(r.integrated_lufs, None);
        assert_eq!(r.loudness_range_lu, Some(0.));
        assert_eq!(r.lra_absolute_count, 1);
        assert_eq!(r.lra_relative_count, 1);
        for n in 1..=64 {
            m.reset();
            m.lra.extend((0..n).map(|i| i as f64 / 100.));
            let r = m.analyse_programme().unwrap();
            let low = (((n - 1) as f64 * 0.1).round() as usize) as f64 / 100.;
            let high = (((n - 1) as f64 * 0.95).round() as usize) as f64 / 100.;
            assert_eq!(r.lra_low_lufs, Some(low));
            assert_eq!(r.lra_high_lufs, Some(high));
            assert_eq!(r.lra_relative_count, n);
        }
        m.reset();
        let r = m.analyse_programme().unwrap();
        assert_eq!(r.integrated_lufs, None);
        assert_eq!(r.loudness_range_lu, None);
        assert_eq!(mean_level([6000., 6000.].into_iter()), Some((6000., 2)));
        println!(
            "GATING retrospective original energy records, strict I/inclusive LRA -70 boundary, integer ranks N1..64/ties, scaled high-log means qualified"
        );
    }
    #[test]
    fn relative_gate_exact_equality_and_neighbours_use_declared_strict_inclusive_policy() {
        // Solve the two-value energy mean: one low E plus high199E makes mean100E.
        // This yields a -20 LU relative gate equal to the low value. Native
        // log arithmetic is first checked, not assumed to preserve the identity.
        let low = 0.;
        let high = 10. * 199_f64.log10();
        let (mean, _) = mean_level([low, high].into_iter()).unwrap();
        assert_eq!(
            mean - 20.,
            low,
            "selected native boundary must actually be exact"
        );
        let mut m = owner(8000);
        m.lra.extend_from_slice(&[low, high]);
        let r = m.analyse_programme().unwrap();
        assert_eq!(r.lra_relative_gate_lufs, Some(low));
        assert_eq!(r.lra_relative_count, 2);
        // Nine low plus high91E gives mean10E, so I gate equals the low value.
        let high = 10. * 91_f64.log10();
        m.reset();
        m.integrated.extend([low; 9]);
        m.integrated.push(high);
        let (mean, _) = mean_level(m.integrated.iter().copied()).unwrap();
        assert_eq!(mean - 10., low);
        let r = m.analyse_programme().unwrap();
        assert_eq!(r.integrated_relative_gate_lufs, Some(low));
        assert_eq!(r.integrated_relative_count, 1);
        for value in [low.next_down(), low, low.next_up()] {
            m.reset();
            m.integrated.extend([value; 9]);
            m.integrated.push(10. * 91_f64.log10());
            m.lra.extend_from_slice(&[value, 10. * 199_f64.log10()]);
            let actual = m.analyse_programme().unwrap();
            assert_eq!(actual.integrated_relative_gate_lufs, Some(low));
            assert_eq!(actual.lra_relative_gate_lufs, Some(low));
            assert_eq!(
                actual.integrated_relative_count,
                if value > low { 10 } else { 1 }
            );
            assert_eq!(actual.lra_relative_count, if value >= low { 2 } else { 1 });
        }
    }
    #[test]
    fn integer_schedule_long_coordinates_all_remainders_and_exact30_block_duration() {
        let endpoint = |k: u64, rate: u32| k as u128 * rate as u128 / 10;
        for rate in 8000..8010 {
            for k in [0, 1, 9, 10, 29, 30, 1 << 24, u64::MAX / 1000] {
                let start = endpoint(k, rate);
                let stop = endpoint(k + 30, rate);
                assert_eq!(stop - start, 3 * rate as u128);
                let mut frames = start;
                let mut remainder = ((k + 1) as u128 * rate as u128 % 10) as u32;
                let mut interval = (endpoint(k + 1, rate) - start) as u32;
                for step in 1..=31 {
                    frames += interval as u128;
                    assert_eq!(frames, endpoint(k + step, rate));
                    let sum = remainder + rate % 10;
                    interval = rate / 10 + sum / 10;
                    remainder = sum % 10;
                }
            }
            let mut grid = Grid::new(rate);
            let mut count = 0;
            for n in 1..=endpoint(31, rate) as u64 {
                if grid.finish_frame() {
                    count += 1;
                    assert_eq!(n as u128, endpoint(count, rate));
                }
                assert_eq!(completed(n, rate), count);
            }
        }
        println!(
            "GRID exact u128 schedule all remainders; >2^24 and near-u64 chunk coordinates without long audio allocation; every30 blocks exactly3Fs"
        );
    }
    #[test]
    fn scaled_energy_extremes_subnormals_weighted_channels_and_frame_denominator() {
        for sample in [f64::MAX, f64::MAX / 4., 1e300, f64::from_bits(1), 1e-300] {
            let mut energy = Energy::default();
            energy.add(sample, 1.);
            energy.add(-sample, 1.);
            energy.frames = 2;
            let expected = -0.691 + 20. * sample.log10();
            assert_eq!(energy.level(), expected);
            let mut weighted = Energy::default();
            weighted.add(sample, 1.);
            weighted.add(sample, 1.41);
            weighted.frames = 1;
            let expected = -0.691 + 20. * sample.log10() + 10. * 2.41_f64.log10();
            assert!((weighted.level() - expected).abs() < 2e-12);
        }
        assert!((f64::MAX * f64::MAX).is_infinite());
        assert_eq!(f64::from_bits(1).powi(2), 0.);
        let mut m = owner(8000);
        let finite = vec![f64::MAX / 16.; 3200];
        m.observe_planar(&[&finite], 3200).unwrap();
        let r = m.reading();
        assert!(r.numeric_fault.is_none());
        assert!(r.momentary_lufs.unwrap().is_finite());
        assert!(r.momentary_lufs.unwrap() > 5900.);
        m.reset();
        let tiny = vec![f64::from_bits(1); 3200];
        m.observe_planar(&[&tiny], 3200).unwrap();
        println!(
            "WIDE actual finite filter sourceMAX/16 mean report before reset finite; native minimum-subnormal profile observed LUFS={:?} (no automatic flush), source={}",
            m.reading().momentary_lufs,
            m.source
        );
    }
    #[test]
    fn synthetic_exact_source_headroom_boundary_retains_owners_and_full_tail() {
        for rate in [8000, 8001, 8009, 768000] {
            let mut m = owner(rate);
            let tail = m.tail_bound();
            let limit = u64::MAX - tail;
            // All preceding input is conceptually paused zero. Counter injection
            // proves finite endpoint admission, not actual-duration execution.
            m.source = limit - 1;
            m.live.frames = m.source;
            let k = completed(m.source, rate);
            let endpoint = |k: u64| (k as u128 * rate as u128 / 10) as u64;
            m.live.remaining = (endpoint(k + 1) - m.source) as u32;
            m.live.remainder = ((k as u128 + 1) * rate as u128 % 10) as u32;
            m.live.energy.frames = (m.source - endpoint(k)) as u32;
            let before = m.reading();
            let states: Vec<_> = m
                .filters
                .iter()
                .map(|f| f.each_ref().map(Biquad::delay_state))
                .collect();
            assert_eq!(
                m.observe_planar(&[&[0.; 2]], 2),
                Err(LoudnessError::FrameCount)
            );
            assert_eq!(m.reading(), before);
            assert_eq!(
                m.filters
                    .iter()
                    .map(|f| f.each_ref().map(Biquad::delay_state))
                    .collect::<Vec<_>>(),
                states
            );
            m.observe_frame(&[0.]).unwrap();
            assert_eq!(m.source, limit);
            assert_eq!(m.observe_frame(&[0.]), Err(LoudnessError::FrameCount));
            m.finish(LoudnessTailPolicy::LraSilence1500ms).unwrap();
            assert_eq!(m.source, limit);
            assert_eq!(m.tail, tail);
            assert_eq!(m.source + m.tail, u64::MAX);
            assert!(m.finalized);
        }
    }

    #[test]
    fn injected_strict_stable_filter_tail_fault_keeps_actual_coordinate_and_progress() {
        let mut m = owner(8001);
        // Private protocol injection, not a claim that the calibrated K sections
        // have this transfer. Rounded Jury margins are positive. The first tail
        // output is finite, but its next memory overflows: inspect both memories.
        m.filters[0][0] = Biquad::new(BiquadCoefficients::new([1., 0., 0.], [-1.9, 0.95]).unwrap());
        m.filters[0][1] = Biquad::new(BiquadCoefficients::new([1., 0., 0.], [0., 0.]).unwrap());
        m.observe_frame(&[f64::MAX / 2.]).unwrap();
        assert!(m.reading().numeric_fault.is_none());
        m.finish(LoudnessTailPolicy::LraSilence1500ms).unwrap();
        let r = m.reading();
        assert_eq!(
            r.numeric_fault,
            Some(LoudnessFault {
                coordinate: 1,
                artificial_tail: true,
                channel: 0,
                section: 0
            })
        );
        assert_eq!(r.source_frames, 1);
        assert_eq!(r.tail_frames, 12002);
        assert!(r.finalized);
        assert_eq!(m.analyse_programme(), Err(LoudnessError::NumericFault));
        m.finish(LoudnessTailPolicy::None).unwrap();
        assert_eq!(m.reading(), r);
    }

    fn owner_with(rate: u32, roles: &[LoudnessChannelRole]) -> PreparedLoudnessMeter64 {
        PreparedLoudnessMeter64::new(
            &KWeightingDesign::new(rate).unwrap(),
            roles,
            65536,
            LoudnessRecordCapacities {
                integrated: 64,
                lra: 64,
            },
            true,
        )
        .unwrap()
    }
    fn section(b: [f64; 3], a: [f64; 2]) -> Biquad {
        Biquad::new(BiquadCoefficients::new(b, a).unwrap())
    }
    // Bit patterns, so frozen non-finite (including NaN) memories compare reliably.
    fn memories(m: &PreparedLoudnessMeter64) -> Vec<[u64; 2]> {
        m.filters[..2]
            .iter()
            .flatten()
            .map(|f| f.delay_state().map(f64::to_bits))
            .collect()
    }

    #[test]
    fn injected_faults_distinguish_channel_section_output_memory_coordinate_and_raw_records() {
        // Private protocol injection into Surround (channel 1); the calibrated K sections do
        // not have these transfers, so the records below are compared only with an identically
        // injected meter that never overflows. Frame 4007 carries f64::MAX in a later call.
        let cases: [(&str, [f64; 3], [f64; 3], usize); 4] = [
            ("output of section 0", [2., 0., 0.], [1., 0., 0.], 0),
            ("memory of section 0", [1., 2., 0.], [1., 0., 0.], 0),
            ("output of section 1", [1., 0., 0.], [2., 0., 0.], 1),
            ("memory of section 1", [1., 0., 0.], [1., 2., 0.], 1),
        ];
        let roles = [LoudnessChannelRole::Front, LoudnessChannelRole::Surround];
        let (block5, total) = (4000_usize, 8000_usize);
        let fault_at = block5 + 7;
        let sine = |phase: f64| -> Vec<f64> {
            (0..total)
                .map(|i| 0.1 * (std::f64::consts::TAU * 997. * i as f64 / 8000. + phase).sin())
                .collect()
        };
        let (a, b) = (sine(0.), sine(0.5));
        let mut faulty = b.clone();
        faulty[fault_at] = f64::MAX;
        for (name, first, second, section_index) in cases {
            let inject = |m: &mut PreparedLoudnessMeter64| {
                m.filters[1][0] = section(first, [0., 0.]);
                m.filters[1][1] = section(second, [0., 0.]);
            };
            let mut control = owner_with(8000, &roles);
            inject(&mut control);
            control.observe_planar(&[&a, &b], total).unwrap();
            assert!(control.reading().numeric_fault.is_none(), "{name}");
            let mut m = owner_with(8000, &roles);
            inject(&mut m);
            m.observe_planar(&[&a[..block5], &faulty[..block5]], block5)
                .unwrap();
            assert!(m.reading().numeric_fault.is_none(), "{name}");
            m.observe_planar(&[&a[block5..], &faulty[block5..]], total - block5)
                .unwrap();
            let expected = LoudnessFault {
                coordinate: fault_at as u64,
                artificial_tail: false,
                channel: 1,
                section: section_index,
            };
            assert_eq!(m.reading().numeric_fault, Some(expected), "{name}");
            // Accepted progress and both grids continue after the latch.
            assert_eq!((m.source, m.admitted), (8000, 8000), "{name}");
            assert_eq!(m.integrated.len(), 7, "{name}");
            assert_eq!(m.integrated.len(), control.integrated.len(), "{name}");
            // Raw records are read-only diagnostic history, never a valid report: complete
            // records before the fault are unchanged, partially measured windows are finite
            // but lose the energy that was never accumulated, and windows wholly after the
            // latch are exactly unavailable (-inf).
            assert_eq!(m.integrated[..2], control.integrated[..2], "{name}");
            for (i, (faulted, valid)) in m
                .integrated
                .iter()
                .zip(&control.integrated)
                .enumerate()
                .skip(2)
                .take(4)
            {
                assert!(faulted.is_finite() && faulted < valid, "{name}: record {i}");
            }
            assert_eq!(m.integrated[6], f64::NEG_INFINITY, "{name}");
            assert!(control.integrated[6].is_finite(), "{name}");
            assert_eq!(m.analyse_programme(), Err(LoudnessError::NumericFault));
            assert_eq!(m.reading().momentary_lufs, None, "{name}");
            let frozen = memories(&m);
            m.observe_planar(&[&[0.1; 10], &[0.1; 10]], 10).unwrap();
            assert_eq!(memories(&m), frozen, "{name}: filters stop after the latch");
            assert_eq!((m.source, m.admitted), (8010, 8010), "{name}");
            assert_eq!(m.reading().numeric_fault, Some(expected), "{name}");
            m.reset();
            assert!(m.integrated.is_empty() && m.lra.is_empty(), "{name}");
            assert_eq!(m.reading().numeric_fault, None, "{name}");
            assert!(
                memories(&m).iter().flatten().all(|&bits| bits == 0),
                "{name}: reset clears non-finite recurrence memory"
            );
            m.observe_planar(&[&a[..100], &b[..100]], 100).unwrap();
            assert_eq!(m.reading().numeric_fault, None, "{name}");
        }
    }

    #[test]
    fn injected_surround_section_one_tail_fault_keeps_tail_coordinate_and_progress() {
        let mut m = owner_with(
            8001,
            &[LoudnessChannelRole::Front, LoudnessChannelRole::Surround],
        );
        // Same resonator arithmetic as the Front test above, but behind an identity section on
        // channel 1: the first tail output is finite, the next memory overflows in section 1.
        m.filters[1][0] = section([1., 0., 0.], [0., 0.]);
        m.filters[1][1] = section([1., 0., 0.], [-1.9, 0.95]);
        m.observe_frame(&[0.1, f64::MAX / 2.]).unwrap();
        assert!(m.reading().numeric_fault.is_none());
        m.finish(LoudnessTailPolicy::LraSilence1500ms).unwrap();
        let r = m.reading();
        assert_eq!(
            r.numeric_fault,
            Some(LoudnessFault {
                coordinate: 1,
                artificial_tail: true,
                channel: 1,
                section: 1
            })
        );
        assert_eq!((r.source_frames, r.tail_frames), (1, 12002));
        assert!(r.finalized);
        assert_eq!(m.analyse_programme(), Err(LoudnessError::NumericFault));
        m.finish(LoudnessTailPolicy::None).unwrap();
        assert_eq!(m.reading(), r);
    }

    #[test]
    fn defensive_budget_guard_is_distinct_from_the_current_record_caps() {
        // Today's largest legal request is three 1,000,000-element arrays: 24,000,000 bytes
        // plus the owner, below 32 MiB. The pre-reserve check is therefore defensive for
        // current caps; it still refuses a request above the budget before any allocation.
        assert_eq!(3 * MAX_RECORDS * 8, 24_000_000);
        assert!(3 * MAX_RECORDS * 8 + size_of::<PreparedLoudnessMeter64>() < MAX_BYTES);
        assert_eq!(
            reserve(MAX_BYTES / 8 + 1).unwrap_err(),
            LoudnessError::ResourceLimit
        );
        assert_eq!(
            reserve(usize::MAX).unwrap_err(),
            LoudnessError::ResourceLimit
        );
        // The actual-capacity check follows allocation because capacity may exceed the request.
        let largest = PreparedLoudnessMeter64::new(
            &KWeightingDesign::new(8000).unwrap(),
            &[LoudnessChannelRole::Front],
            1,
            LoudnessRecordCapacities {
                integrated: MAX_RECORDS,
                lra: MAX_RECORDS,
            },
            true,
        )
        .unwrap();
        assert!(largest.integrated.capacity() >= MAX_RECORDS);
        assert!(largest.lra.capacity() >= MAX_RECORDS);
        assert_eq!(largest.scratch.len(), MAX_RECORDS);
    }
}
