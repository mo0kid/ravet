//! CONTROL coefficient design for finite symmetric f64 FIR responses.
use crate::WindowKind;
use std::f64::consts::{PI, TAU};

/// Ideal window-method cutoff/edge frequencies in Hz (not IIR -3dB points).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FirResponse {
    LowPass { cutoff_hz: f64 },
    HighPass { cutoff_hz: f64 },
    BandPass { low_hz: f64, high_hz: f64 },
    BandStop { low_hz: f64, high_hz: f64 },
}
/// Symmetric window selection without changing existing [`WindowKind`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FirWindow {
    Cosine(WindowKind),
    /// Finite beta 0..=32. Beta zero is rectangular.
    Kaiser {
        beta: f64,
    },
}
/// Preparation failures; no partially prepared response is returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FirDesignError {
    InvalidSpecification,
    Capacity,
    Allocation,
    Unrepresentable,
}
/// One closed frequency band for weighted coefficient design. Desired
/// gain is linear between finite endpoints -16..=16; weight is finite (0,1e6].
/// Sorted nonoverlapping positive-width bands may leave unconstrained gaps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FirBand {
    pub low_hz: f64,
    pub high_hz: f64,
    pub low_gain: f64,
    pub high_gain: f64,
    pub weight: f64,
}
/// Transparent empirical Kaiser sizing, not verified pass/stop enforcement.
/// Inspect actual coefficients/response for the consumer's requirements.
#[derive(Debug)]
pub struct KaiserFirDesign {
    design: FirDesign,
    beta: f64,
    /// Empirical order before promotion to an even order/odd tap count.
    estimated_order: usize,
}
impl KaiserFirDesign {
    /// Borrow the actual generated response, without editable sizing metadata.
    pub fn design(&self) -> &FirDesign {
        &self.design
    }
    /// Move the actual coefficients into the consumer's CONTROL preparation.
    pub fn into_design(self) -> FirDesign {
        self.design
    }
    /// Selected empirical beta.
    pub fn beta(&self) -> f64 {
        self.beta
    }
    /// Empirical order before promotion to an even order/odd tap count.
    pub fn estimated_order(&self) -> usize {
        self.estimated_order
    }
}
/// Owned symmetric finite coefficients, at most65,536 f64 taps (512KiB).
/// CONTROL fallible allocation/design/Drop; processing belongs to existing
/// convolver owners. Symmetry gives delay(N-1)/2, possibly a half sample. Factories
/// state their numerical profile; no minimum-phase, continuous transition-bound
/// certificate or callback-deadline guarantee.
///
/// ```
/// use ravet_signal::{FirDesign, FirResponse, FirWindow, WindowKind, FirConvolver};
/// let design = FirDesign::windowed(FirResponse::LowPass { cutoff_hz: 4000. },
///     48000., 63, FirWindow::Cosine(WindowKind::Hamming)).unwrap();
/// let mut convolver = FirConvolver::new(design.coefficients()).unwrap();
/// assert_eq!(design.group_delay_samples(), 31.);
/// assert!(convolver.process_sample(1.).is_finite());
/// ```
#[derive(Debug)]
pub struct FirDesign {
    coefficients: Vec<f64>,
}
impl FirDesign {
    /// Centered ideal band integral times a symmetric cosine/Kaiser window.
    /// Rate finite/positive, frequencies strict inside Nyquist. LP/BP accepts
    /// 2..=65536 taps (Type I/II); HP/BS requires odd 3..=65535 because an even
    /// symmetric filter has zero Nyquist gain. Normalize actual pass response
    /// at LP/BS DC, HP Nyquist or BP arithmetic Hz center. Extreme zero/rounded
    /// reference gain rejects. Finite response cutoff need not be exactly -6dB.
    pub fn windowed(
        response: FirResponse,
        rate: f64,
        taps: usize,
        window: FirWindow,
    ) -> Result<Self, FirDesignError> {
        count(taps)?;
        let (lo, hi) = edges(response, rate)?;
        let includes_nyquist = matches!(
            response,
            FirResponse::HighPass { .. } | FirResponse::BandStop { .. }
        );
        if includes_nyquist && taps.is_multiple_of(2) {
            return Err(FirDesignError::InvalidSpecification);
        }
        let prepared = PreparedWindow::new(window)?;
        let mut design = Self::generate(taps, |t, index| {
            let l = 2. * lo * sinc(2. * lo * t);
            let h = 2. * hi * sinc(2. * hi * t);
            let ideal = match response {
                FirResponse::LowPass { .. } => l,
                FirResponse::HighPass { .. } => sinc(t) - l,
                FirResponse::BandPass { .. } => h - l,
                FirResponse::BandStop { .. } => sinc(t) - (h - l),
            };
            ideal * prepared.at(index, taps)
        })?;
        let reference = match response {
            FirResponse::LowPass { .. } | FirResponse::BandStop { .. } => 0.,
            FirResponse::HighPass { .. } => 0.5,
            FirResponse::BandPass { .. } => (lo + hi) * 0.5,
        };
        design.normalize(reference)?;
        Ok(design)
    }
    /// Empirical Kaiser LP sizing: finite 8..=120dB, 0<pass<stop<Nyquist.
    /// Order estimate ceil((A-8)/(2.285*2pi*transition)), promoted to even order;
    /// at least3 taps. Midpoint cutoff, standard empirical beta. The first
    /// sidelobe can miss the requested attenuation: this is expressly an
    /// estimate, not a continuous-band specification verifier or retry search.
    pub fn kaiser_lowpass(
        rate: f64,
        pass_hz: f64,
        stop_hz: f64,
        attenuation_db: f64,
    ) -> Result<KaiserFirDesign, FirDesignError> {
        if !attenuation_db.is_finite()
            || !(8.0..=120.0).contains(&attenuation_db)
            || pass_hz >= stop_hz
        {
            return Err(FirDesignError::InvalidSpecification);
        }
        let pass = frequency(rate, pass_hz)?;
        let stop = frequency(rate, stop_hz)?;
        let width = stop - pass;
        if width <= 0. {
            return Err(FirDesignError::Unrepresentable);
        }
        let estimate = ((attenuation_db - 8.) / (2.285 * TAU * width)).ceil();
        if !estimate.is_finite() || estimate > 65534. {
            return Err(FirDesignError::Capacity);
        }
        let estimated_order = estimate as usize;
        let order = estimated_order.max(2).div_ceil(2) * 2;
        let beta = if attenuation_db > 50. {
            0.1102 * (attenuation_db - 8.7)
        } else if attenuation_db >= 21. {
            0.5842 * (attenuation_db - 21.).powf(0.4) + 0.07886 * (attenuation_db - 21.)
        } else {
            0.
        };
        let cutoff_hz = (pass * 0.5 + stop * 0.5) * rate;
        let design = Self::windowed(
            FirResponse::LowPass { cutoff_hz },
            rate,
            order + 1,
            FirWindow::Kaiser { beta },
        )?;
        Ok(KaiserFirDesign {
            design,
            beta,
            estimated_order,
        })
    }
    /// Truncate an analytically shaped low-pass transition. Degree K=1..4
    /// means K convolutions with unit-area frequency boxes, each width Δ/K:
    /// their integrated edge is degree K and has total width Δ. Infinite taps
    /// are 2fc*sinc(2fc*t)*sinc((Δ/K)*t)^K. Positive width and the whole transition
    /// must lie strictly inside DC/Nyquist. 2..=65536 taps, actual DC normalized;
    /// finite truncation is not a verified bound on the infinite target.
    pub fn transition_lowpass(
        rate: f64,
        cutoff_hz: f64,
        width_hz: f64,
        taps: usize,
        degree: usize,
    ) -> Result<Self, FirDesignError> {
        count(taps)?;
        let cutoff = frequency(rate, cutoff_hz)?;
        if !width_hz.is_finite() || width_hz <= 0. || !(1..=4).contains(&degree) {
            return Err(FirDesignError::InvalidSpecification);
        }
        let width = width_hz / rate;
        if !width.is_finite()
            || width <= 0.
            || cutoff - width * 0.5 <= 0.
            || cutoff + width * 0.5 >= 0.5
        {
            return Err(FirDesignError::InvalidSpecification);
        }
        let mut design = Self::generate(taps, |t, _| {
            2. * cutoff
                * sinc(2. * cutoff * t)
                * sinc(width / degree as f64 * t).powi(degree as i32)
        })?;
        design.normalize(0.)?;
        Ok(design)
    }
    /// Weighted integrated least-squares Type I FIR. Odd3..=511 taps,1..=32
    /// bands inside closed DC..Nyquist. Gain endpoints -16..=16, weight(0,1e6].
    /// Gaps are unconstrained and may overshoot. Preserve the solved gain (no
    /// post-normalization). Exact analytic cosine integrals form normal equations.
    /// CONTROL O(V³+bands*V²), V=(N+1)/2<=256, matrix<=512KiB plus vectors.
    /// Normalize weights by their maximum; underflowed positive weight/width
    /// rejects. Unregularized Cholesky rejects pivot<=64*eps*V*largest diagonal;
    /// normalized infinity residual must be<=1024*eps*V. These finite f64 checks
    /// are representability policy, not exact-arithmetic optimality/minimax or
    /// requested continuous-band loss/attenuation certification.
    pub fn least_squares(
        rate: f64,
        taps: usize,
        bands: &[FirBand],
    ) -> Result<Self, FirDesignError> {
        if !(3..=511).contains(&taps)
            || taps.is_multiple_of(2)
            || bands.is_empty()
            || bands.len() > 32
        {
            return Err(FirDesignError::Capacity);
        }
        if !rate.is_finite() || rate <= 0. {
            return Err(FirDesignError::InvalidSpecification);
        }
        let mut maximum_weight: f64 = 0.;
        let mut last = 0.;
        for band in bands {
            if !band.low_hz.is_finite()
                || !band.high_hz.is_finite()
                || band.low_hz < last
                || band.low_hz < 0.
                || band.high_hz <= band.low_hz
                || band.high_hz > rate * 0.5
                || !band.low_gain.is_finite()
                || !band.high_gain.is_finite()
                || band.low_gain.abs() > 16.
                || band.high_gain.abs() > 16.
                || !band.weight.is_finite()
                || band.weight <= 0.
                || band.weight > 1e6
            {
                return Err(FirDesignError::InvalidSpecification);
            }
            maximum_weight = maximum_weight.max(band.weight);
            last = band.high_hz;
        }
        let mut normalized = [NormalBand {
            lo: 0.,
            hi: 0.,
            low_gain: 0.,
            high_gain: 0.,
            weight: 0.,
        }; 32];
        for (i, b) in bands.iter().enumerate() {
            let lo = b.low_hz / rate;
            let hi = b.high_hz / rate;
            let weight = b.weight / maximum_weight;
            if hi <= lo || weight <= 0. {
                return Err(FirDesignError::Unrepresentable);
            }
            normalized[i] = NormalBand {
                lo,
                hi,
                low_gain: b.low_gain,
                high_gain: b.high_gain,
                weight,
            };
        }
        let normalized = &normalized[..bands.len()];
        let v = taps.div_ceil(2);
        let mut matrix = zeros(v * v)?;
        let mut rhs = zeros(v)?;
        let mut scale: f64 = 0.;
        for m in 0..v {
            rhs[m] = linear_integral(normalized, m);
            for n in 0..=m {
                matrix[m * v + n] = gram(normalized, m, n);
            }
            scale = scale.max(matrix[m * v + m]);
        }
        if !scale.is_finite() || scale <= 0. {
            return Err(FirDesignError::Unrepresentable);
        }
        let threshold = 64. * f64::EPSILON * v as f64 * scale;
        for i in 0..v {
            for j in 0..=i {
                let mut sum = matrix[i * v + j];
                for k in 0..j {
                    sum -= matrix[i * v + k] * matrix[j * v + k];
                }
                if !sum.is_finite() {
                    return Err(FirDesignError::Unrepresentable);
                }
                if i == j {
                    if sum <= threshold {
                        return Err(FirDesignError::Unrepresentable);
                    }
                    matrix[i * v + j] = sum.sqrt();
                } else {
                    matrix[i * v + j] = sum / matrix[j * v + j];
                }
            }
        }
        let mut solution = zeros(v)?;
        for i in 0..v {
            let mut sum = rhs[i];
            for j in 0..i {
                sum -= matrix[i * v + j] * solution[j];
            }
            solution[i] = sum / matrix[i * v + i];
        }
        for i in (0..v).rev() {
            let mut sum = solution[i];
            for j in i + 1..v {
                sum -= matrix[j * v + i] * solution[j];
            }
            solution[i] = sum / matrix[i * v + i];
        }
        let mut residual: f64 = 0.;
        let mut matrix_norm: f64 = 0.;
        let mut rhs_norm: f64 = 0.;
        let mut solution_norm: f64 = 0.;
        for &a in &solution {
            if !a.is_finite() {
                return Err(FirDesignError::Unrepresentable);
            }
            solution_norm = solution_norm.max(a.abs());
        }
        for (m, &rhs_value) in rhs.iter().enumerate() {
            let mut product = 0.;
            let mut row = 0.;
            for (n, &a) in solution.iter().enumerate() {
                let g = gram(normalized, m, n);
                product += g * a;
                row += g.abs();
            }
            residual = residual.max((product - rhs_value).abs());
            matrix_norm = matrix_norm.max(row);
            rhs_norm = rhs_norm.max(rhs_value.abs());
        }
        let denominator = matrix_norm * solution_norm + rhs_norm;
        if !denominator.is_finite()
            || !residual.is_finite()
            || residual > 1024. * f64::EPSILON * v as f64 * denominator
        {
            return Err(FirDesignError::Unrepresentable);
        }
        let middle = taps / 2;
        let mut coefficients = zeros(taps)?;
        coefficients[middle] = solution[0];
        for k in 1..v {
            coefficients[middle - k] = solution[k] * 0.5;
            coefficients[middle + k] = solution[k] * 0.5;
        }
        Ok(Self { coefficients })
    }
    /// Windowed quarter-cycle halfband with exact central0.5 and noncentral
    /// even-offset zero taps. Length4m+3<=65535. Scale only paired odd taps for
    /// DC unity/Nyquist zero, retaining complementary centered response within
    /// rounding. This is not equiripple/minimax or an automatic resampler.
    pub fn halfband_windowed(taps: usize, window: FirWindow) -> Result<Self, FirDesignError> {
        if !(3..=65535).contains(&taps) || taps % 4 != 3 {
            return Err(FirDesignError::Capacity);
        }
        let prepared = PreparedWindow::new(window)?;
        let middle = taps / 2;
        let mut design = Self::generate(taps, |t, index| {
            if index == middle {
                0.5
            } else if index.abs_diff(middle).is_multiple_of(2) {
                0.
            } else {
                0.5 * sinc(0.5 * t) * prepared.at(index, taps)
            }
        })?;
        let mut sum = 0.;
        for (i, &tap) in design.coefficients.iter().enumerate() {
            if i != middle {
                sum += tap;
            }
        }
        if !sum.is_finite() || sum <= 0. {
            return Err(FirDesignError::Unrepresentable);
        }
        for (i, tap) in design.coefficients.iter_mut().enumerate() {
            if i != middle {
                *tap *= 0.5 / sum;
                if !tap.is_finite() {
                    return Err(FirDesignError::Unrepresentable);
                }
            }
        }
        Ok(design)
    }
    pub(crate) fn from_symmetric_owned(coefficients: Vec<f64>) -> Result<Self, FirDesignError> {
        if !(2..=65536).contains(&coefficients.len()) {
            return Err(FirDesignError::Capacity);
        }
        if coefficients.iter().any(|v| !v.is_finite())
            || coefficients
                .iter()
                .zip(coefficients.iter().rev())
                .any(|(a, b)| a != b)
        {
            return Err(FirDesignError::Unrepresentable);
        }
        Ok(Self { coefficients })
    }
    fn generate(
        taps: usize,
        mut function: impl FnMut(f64, usize) -> f64,
    ) -> Result<Self, FirDesignError> {
        let mut coefficients = zeros(taps)?;
        let middle = (taps - 1) as f64 * 0.5;
        for i in 0..taps.div_ceil(2) {
            let value = function(i as f64 - middle, i);
            if !value.is_finite() {
                return Err(FirDesignError::Unrepresentable);
            }
            coefficients[i] = value;
            coefficients[taps - 1 - i] = value;
        }
        Ok(Self { coefficients })
    }
    fn normalize(&mut self, frequency: f64) -> Result<(), FirDesignError> {
        let gain = self.centered_response(frequency);
        if !gain.is_finite() || gain <= 0. {
            return Err(FirDesignError::Unrepresentable);
        }
        for tap in &mut self.coefficients {
            *tap /= gain;
            if !tap.is_finite() {
                return Err(FirDesignError::Unrepresentable);
            }
        }
        Ok(())
    }
    fn centered_response(&self, frequency: f64) -> f64 {
        let middle = (self.coefficients.len() - 1) as f64 * 0.5;
        self.coefficients
            .iter()
            .enumerate()
            .map(|(j, &h)| h * (TAU * frequency * (j as f64 - middle)).cos())
            .sum()
    }
    /// Borrow coefficients for a separately prepared convolver on CONTROL.
    pub fn coefficients(&self) -> &[f64] {
        &self.coefficients
    }
    /// Number of delays, one fewer than tap count.
    pub fn order(&self) -> usize {
        self.coefficients.len() - 1
    }
    /// Linear-phase delay, including Type II half-sample delay. Consumer host
    /// latency conventions are separate from this exact coefficient symmetry.
    pub fn group_delay_samples(&self) -> f64 {
        self.order() as f64 * 0.5
    }
    /// FIR tail after last actual input, excluding consumer block latency.
    pub fn tail_samples(&self) -> usize {
        self.order()
    }
    /// CONTROL actual linear amplitude at normalized frequency0..=0.5.
    pub fn magnitude_at(&self, frequency: f64) -> Result<f64, FirDesignError> {
        if !frequency.is_finite() || !(0.0..=0.5).contains(&frequency) {
            return Err(FirDesignError::InvalidSpecification);
        }
        let amplitude = self.centered_response(frequency).abs();
        if !amplitude.is_finite() {
            return Err(FirDesignError::Unrepresentable);
        }
        Ok(amplitude)
    }
}
fn count(taps: usize) -> Result<(), FirDesignError> {
    if !(2..=65536).contains(&taps) {
        Err(FirDesignError::Capacity)
    } else {
        Ok(())
    }
}
fn frequency(rate: f64, hz: f64) -> Result<f64, FirDesignError> {
    if !rate.is_finite() || rate <= 0. || !hz.is_finite() || hz <= 0. || hz >= rate * 0.5 {
        return Err(FirDesignError::InvalidSpecification);
    }
    let f = hz / rate;
    if f <= 0. || f >= 0.5 {
        return Err(FirDesignError::Unrepresentable);
    }
    Ok(f)
}
fn edges(response: FirResponse, rate: f64) -> Result<(f64, f64), FirDesignError> {
    match response {
        FirResponse::LowPass { cutoff_hz } | FirResponse::HighPass { cutoff_hz } => {
            Ok((frequency(rate, cutoff_hz)?, 0.))
        }
        FirResponse::BandPass { low_hz, high_hz } | FirResponse::BandStop { low_hz, high_hz } => {
            let lo = frequency(rate, low_hz)?;
            let hi = frequency(rate, high_hz)?;
            if lo >= hi {
                return Err(FirDesignError::InvalidSpecification);
            }
            Ok((lo, hi))
        }
    }
}
fn zeros(length: usize) -> Result<Vec<f64>, FirDesignError> {
    let mut v = Vec::new();
    v.try_reserve_exact(length)
        .map_err(|_| FirDesignError::Allocation)?;
    v.resize(length, 0.);
    Ok(v)
}
fn sinc(x: f64) -> f64 {
    if x == 0. {
        1.
    } else {
        (PI * x).sin() / (PI * x)
    }
}
struct PreparedWindow {
    kind: FirWindow,
    i0: f64,
}
impl PreparedWindow {
    fn new(kind: FirWindow) -> Result<Self, FirDesignError> {
        let i0 = match kind {
            FirWindow::Cosine(_) => 1.,
            FirWindow::Kaiser { beta } => {
                if !beta.is_finite() || !(0.0..=32.0).contains(&beta) {
                    return Err(FirDesignError::InvalidSpecification);
                }
                bessel_i0(beta)
            }
        };
        Ok(Self { kind, i0 })
    }
    fn at(&self, index: usize, taps: usize) -> f64 {
        match self.kind {
            FirWindow::Kaiser { beta } => {
                let position = 2. * index as f64 / (taps - 1) as f64 - 1.;
                bessel_i0(beta * (1. - position * position).max(0.).sqrt()) / self.i0
            }
            FirWindow::Cosine(kind) => {
                let a = match kind {
                    WindowKind::Rectangular => [1., 0., 0., 0.],
                    WindowKind::Hann => [0.5, -0.5, 0., 0.],
                    WindowKind::Hamming => [0.54, -0.46, 0., 0.],
                    WindowKind::Blackman => [0.42, -0.5, 0.08, 0.],
                    WindowKind::BlackmanHarris => [0.35875, -0.48829, 0.14128, -0.01168],
                };
                let phase = TAU * index as f64 / (taps - 1) as f64;
                a.iter()
                    .enumerate()
                    .map(|(k, &a)| a * (k as f64 * phase).cos())
                    .sum()
            }
        }
    }
}
fn bessel_i0(x: f64) -> f64 {
    let mut term = 1.;
    let mut sum = 1.;
    for k in 1..=128 {
        term *= x * x / (4. * (k * k) as f64);
        let next = sum + term;
        if next == sum {
            return sum;
        }
        sum = next;
    }
    sum
}
#[derive(Clone, Copy)]
struct NormalBand {
    lo: f64,
    hi: f64,
    low_gain: f64,
    high_gain: f64,
    weight: f64,
}
fn cos_integral(b: NormalBand, k: usize) -> f64 {
    if k == 0 {
        b.hi - b.lo
    } else {
        let w = TAU * k as f64;
        ((w * b.hi).sin() - (w * b.lo).sin()) / w
    }
}
fn gram(bands: &[NormalBand], m: usize, n: usize) -> f64 {
    bands
        .iter()
        .map(|&b| b.weight * 0.5 * (cos_integral(b, m.abs_diff(n)) + cos_integral(b, m + n)))
        .sum()
}
fn linear_integral(bands: &[NormalBand], k: usize) -> f64 {
    bands
        .iter()
        .map(|&b| {
            if k == 0 {
                return b.weight * (b.hi - b.lo) * (b.low_gain * 0.5 + b.high_gain * 0.5);
            }
            let w = TAU * k as f64;
            let width = b.hi - b.lo;
            let slope = (b.high_gain - b.low_gain) / width;
            b.weight
                * ((b.high_gain * (w * b.hi).sin() - b.low_gain * (w * b.lo).sin()) / w
                    + slope * ((w * b.hi).cos() - (w * b.lo).cos()) / (w * w))
        })
        .sum()
}

impl std::fmt::Display for FirDesignError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidSpecification => "invalid FIR frequency, parity, window or band specification",
            Self::Capacity => "FIR tap, band or matrix count exceeds the selected design profile",
            Self::Allocation => "FIR CONTROL storage reservation failed",
            Self::Unrepresentable => "FIR gain, arithmetic or unregularized least-squares conditioning is unrepresentable",
        })
    }
}
impl std::error::Error for FirDesignError {}
