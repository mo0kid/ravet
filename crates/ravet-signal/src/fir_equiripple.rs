//! Bounded sampled weighted minimax FIR exchange, distinct from continuous certificates.
use crate::{FirBand, FirDesign, FirDesignError};
use std::f64::consts::{PI, TAU};
const MAX_GRID: usize = 32768;
const MAX_BYTES: usize = 4 * 1024 * 1024;
/// Flat amplitude bands and explicit unconstrained Hz transitions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EquirippleFirResponse {
    LowPass {
        pass_hz: f64,
        stop_hz: f64,
    },
    HighPass {
        stop_hz: f64,
        pass_hz: f64,
    },
    BandPass {
        low_stop_hz: f64,
        low_pass_hz: f64,
        high_pass_hz: f64,
        high_stop_hz: f64,
    },
    BandStop {
        low_pass_hz: f64,
        low_stop_hz: f64,
        high_stop_hz: f64,
        high_pass_hz: f64,
    },
}
/// Bounded exchange controls. Default density16,64 iterations, relative gap1e-8.
/// Density8..64,iterations1..128 and finite relative gap1e-12..1e-3 reject otherwise.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EquirippleOptions {
    pub grid_density: usize,
    pub max_iterations: usize,
    pub relative_tolerance: f64,
}
impl Default for EquirippleOptions {
    fn default() -> Self {
        Self {
            grid_density: 16,
            max_iterations: 64,
            relative_tolerance: 1e-8,
        }
    }
}
/// Preparation returns no partial design on numerical failure or stalled exchange.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EquirippleError {
    Design(FirDesignError),
    NonConvergence { iterations: usize },
}
impl From<FirDesignError> for EquirippleError {
    fn from(e: FirDesignError) -> Self {
        Self::Design(e)
    }
}
/// Actual supported weighted error, desired minus signed zero-phase amplitude.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EquirippleExtremum {
    pub frequency_hz: f64,
    pub weighted_error: f64,
    /// Same signed observation with weights normalized by original maximum.
    pub normalized_error: f64,
}
/// Immutable sampled exchange result. Odd3..511 TypeI/even2..512 TypeII;<=32768
/// observed grid points and4MiB aggregate scratch. CONTROL fallible allocation,
/// bounded design/Drop; AUDIO belongs to separately prepared existing convolvers.
/// Numeric allowance and actual grid/ripple observations are exposed. This is not
/// a certified continuous minimax bound, automatic tap/spec sizing or deadline.
///
/// ```
/// use ravet_signal::{EquirippleFirDesign,EquirippleFirResponse,EquirippleOptions,FirConvolver};
/// let d=EquirippleFirDesign::new(EquirippleFirResponse::LowPass{pass_hz:7000.,stop_hz:9500.},
///     48000.,31,1.,2.,EquirippleOptions::default()).unwrap();
/// assert!(d.normalized_grid_error() <= d.normalized_ripple()*(1.+1e-8)
///     + d.normalized_rounding_allowance());
/// let mut f=FirConvolver::new(d.design().coefficients()).unwrap();
/// assert!(f.process_sample(1.).is_finite());
/// ```
#[derive(Debug)]
pub struct EquirippleFirDesign {
    design: FirDesign,
    iterations: usize,
    grid_points: usize,
    density: usize,
    normalized_error: f64,
    normalized_ripple: f64,
    normalized_allowance: f64,
    weight_scale: f64,
    extrema: Vec<EquirippleExtremum>,
    rate: f64,
    halfband: bool,
    scratch_bytes: usize,
    forced_nyquist: bool,
}
impl EquirippleFirDesign {
    /// Flat0/1 bands; weights finite(0,1e6]. All transition edges strictly ordered
    /// inside Nyquist. HP/BS needs odd taps because nonzero Nyquist is impossible
    /// for symmetric TypeII. No hidden tap promotion or pass-gain normalization.
    pub fn new(
        response: EquirippleFirResponse,
        rate: f64,
        taps: usize,
        pass_weight: f64,
        stop_weight: f64,
        options: EquirippleOptions,
    ) -> Result<Self, EquirippleError> {
        let make = |low_hz, high_hz, gain, weight| FirBand {
            low_hz,
            high_hz,
            low_gain: gain,
            high_gain: gain,
            weight,
        };
        let mut bands = [make(0., 0., 0., stop_weight); 3];
        let count = match response {
            EquirippleFirResponse::LowPass { pass_hz, stop_hz } => {
                if pass_hz >= stop_hz {
                    return invalid();
                }
                bands[0] = make(0., pass_hz, 1., pass_weight);
                bands[1] = make(stop_hz, rate * 0.5, 0., stop_weight);
                2
            }
            EquirippleFirResponse::HighPass { stop_hz, pass_hz } => {
                if stop_hz >= pass_hz {
                    return invalid();
                }
                bands[0] = make(0., stop_hz, 0., stop_weight);
                bands[1] = make(pass_hz, rate * 0.5, 1., pass_weight);
                2
            }
            EquirippleFirResponse::BandPass {
                low_stop_hz,
                low_pass_hz,
                high_pass_hz,
                high_stop_hz,
            } => {
                if !(low_stop_hz < low_pass_hz
                    && low_pass_hz < high_pass_hz
                    && high_pass_hz < high_stop_hz)
                {
                    return invalid();
                }
                bands = [
                    make(0., low_stop_hz, 0., stop_weight),
                    make(low_pass_hz, high_pass_hz, 1., pass_weight),
                    make(high_stop_hz, rate * 0.5, 0., stop_weight),
                ];
                3
            }
            EquirippleFirResponse::BandStop {
                low_pass_hz,
                low_stop_hz,
                high_stop_hz,
                high_pass_hz,
            } => {
                if !(low_pass_hz < low_stop_hz
                    && low_stop_hz < high_stop_hz
                    && high_stop_hz < high_pass_hz)
                {
                    return invalid();
                }
                bands = [
                    make(0., low_pass_hz, 1., pass_weight),
                    make(low_stop_hz, high_stop_hz, 0., stop_weight),
                    make(high_pass_hz, rate * 0.5, 1., pass_weight),
                ];
                3
            }
        };
        Self::from_bands(rate, taps, &bands[..count], options)
    }
    ///1..32 strictly separated positive-width bands in0..Nyquist, finite desired
    /// endpoints[-16,16] interpolated linearly inHz, weights(0,1e6]. TypeII requires
    /// desiredNyquist0; that forced endpoint is observed but excluded from support.
    /// Rate must have an exactly available positive Nyquist; normalized endpoints
    /// must remain finite0..0.5 with preserved original widths/gaps. Rounded missing
    /// boundaries reject before allocation, never nudge or create a forced endpoint.
    /// Bounded sampled exchange may reject a valid but unresolved/stalled profile.
    pub fn from_bands(
        rate: f64,
        taps: usize,
        bands: &[FirBand],
        options: EquirippleOptions,
    ) -> Result<Self, EquirippleError> {
        let model = if taps.is_multiple_of(2) {
            Model::Even
        } else {
            Model::Odd
        };
        run(rate, taps, bands, options, model)
    }
    /// Genuine reduced halfband minimax: N4m+3,3..511, full normalized transition
    /// width0..0.5cycles. Exactcenter0.5/noncentral even-offset0/complement identity.
    /// Ripple remains atDC; normalizingDC would destroy this constrained solution.
    pub fn halfband(
        taps: usize,
        width: f64,
        options: EquirippleOptions,
    ) -> Result<Self, EquirippleError> {
        if !width.is_finite() || width <= 0. || width >= 0.5 || taps % 4 != 3 {
            return invalid();
        }
        let pass = 0.25 - width * 0.5;
        if pass <= 0. || pass >= 0.25 {
            return numeric();
        }
        let band = FirBand {
            low_hz: 0.,
            high_hz: pass,
            low_gain: 1.,
            high_gain: 1.,
            weight: 1.,
        };
        run(1., taps, &[band], options, Model::Half)
    }
    /// Borrow actual coefficients for CONTROL consumer preparation.
    pub fn design(&self) -> &FirDesign {
        &self.design
    }
    /// Move actual coefficients into a separately prepared existing convolver.
    pub fn into_design(self) -> FirDesign {
        self.design
    }
    /// Completed exchanges; exactzero target can finish in one exchange.
    pub fn iterations(&self) -> usize {
        self.iterations
    }
    /// All observed grid points, including forcedNyquist or reflected halfband.
    pub fn grid_points(&self) -> usize {
        self.grid_points
    }
    /// Selected preparation density.
    pub fn grid_density(&self) -> usize {
        self.density
    }
    /// Actual maximum weighted griderror in original units; f64 multiplication
    /// may underflow. Normalized observation+weight_scale retains its meaning.
    pub fn weighted_grid_error(&self) -> f64 {
        self.normalized_error * self.weight_scale
    }
    /// Actual maximum error with all weights divided by original maximum weight.
    pub fn normalized_grid_error(&self) -> f64 {
        self.normalized_error
    }
    /// Fitted supported ripple in maximum-weight-normalized units, before rounding.
    pub fn normalized_ripple(&self) -> f64 {
        self.normalized_ripple
    }
    /// Explicit numeric allowance in maximum-weight-normalized units.
    pub fn normalized_rounding_allowance(&self) -> f64 {
        self.normalized_allowance
    }
    /// Original maximumweight; global scaling does not alter the numerical solve.
    pub fn weight_scale(&self) -> f64 {
        self.weight_scale
    }
    /// Actual alternating supported errors in original units. Halfband support is
    /// in its lowband free basis; reflected stop errors are available separately.
    pub fn extrema(&self) -> &[EquirippleExtremum] {
        &self.extrema
    }
    /// Actual rounded-coefficient halfband stop errors at reflected support, in
    /// frequency order; ordinary results empty. CONTROL iteration is bounded O(N²)
    /// across all supports and allocates nothing; errors are recomputed, not inferred.
    pub fn reflected_halfband_extrema(&self) -> impl Iterator<Item = EquirippleExtremum> + '_ {
        self.extrema
            .iter()
            .rev()
            .filter(|_| self.halfband)
            .map(|e| {
                let frequency_hz = self.rate * 0.5 - e.frequency_hz;
                let normalized_error =
                    -response(self.design.coefficients(), frequency_hz / self.rate);
                EquirippleExtremum {
                    frequency_hz,
                    weighted_error: normalized_error * self.weight_scale,
                    normalized_error,
                }
            })
    }
    /// A TypeII band included its structurally forced zero Nyquist endpoint.
    /// It contributes one observed grid point and exact error0, never support.
    pub fn forced_nyquist(&self) -> bool {
        self.forced_nyquist
    }
    /// Aggregate capacity bytes of live scratch/result numeric buffers at preparation.
    pub fn scratch_capacity_bytes(&self) -> usize {
        self.scratch_bytes
    }
}
#[derive(Clone, Copy)]
enum Model {
    Odd,
    Even,
    Half,
}
impl Model {
    fn variables(self, taps: usize) -> usize {
        match self {
            Self::Odd => taps.div_ceil(2),
            Self::Even => taps / 2,
            Self::Half => (taps + 1) / 4,
        }
    }
    fn xq(self, f: f64) -> (f64, f64) {
        let w = TAU * f;
        match self {
            Self::Odd => (w.cos(), 1.),
            Self::Even => (w.cos(), (w * 0.5).cos()),
            Self::Half => ((2. * w).cos(), w.cos()),
        }
    }
    fn baseline(self) -> f64 {
        if matches!(self, Self::Half) { 0.5 } else { 0. }
    }
}
#[derive(Clone, Copy)]
struct Point {
    f: f64,
    x: f64,
    d: f64,
    w: f64,
    target: f64,
    weight: f64,
    q: f64,
    forced: bool,
}
fn invalid<T>() -> Result<T, EquirippleError> {
    Err(FirDesignError::InvalidSpecification.into())
}
fn numeric<T>() -> Result<T, EquirippleError> {
    Err(FirDesignError::Unrepresentable.into())
}
fn buffer<T>(capacity: usize) -> Result<Vec<T>, EquirippleError> {
    let mut v = Vec::new();
    v.try_reserve_exact(capacity)
        .map_err(|_| FirDesignError::Allocation)?;
    Ok(v)
}
fn numeric_bytes(capacities: [usize; 8]) -> Result<usize, EquirippleError> {
    let widths = [
        std::mem::size_of::<Point>(),
        std::mem::size_of::<usize>(),
        std::mem::size_of::<f64>(),
        std::mem::size_of::<usize>(),
        std::mem::size_of::<f64>(),
        std::mem::size_of::<usize>(),
        std::mem::size_of::<f64>(),
        std::mem::size_of::<EquirippleExtremum>(),
    ];
    let mut bytes = 3 * std::mem::size_of::<Support>() + 4 * 257 * std::mem::size_of::<usize>();
    for (count, width) in capacities.into_iter().zip(widths) {
        bytes = bytes
            .checked_add(count.checked_mul(width).ok_or(FirDesignError::Capacity)?)
            .ok_or(FirDesignError::Capacity)?;
    }
    if bytes > MAX_BYTES {
        return Err(FirDesignError::Capacity.into());
    }
    Ok(bytes)
}
fn run(
    rate: f64,
    taps: usize,
    bands: &[FirBand],
    o: EquirippleOptions,
    model: Model,
) -> Result<EquirippleFirDesign, EquirippleError> {
    if !rate.is_finite()
        || rate <= 0.
        || bands.is_empty()
        || bands.len() > 32
        || !(8..=64).contains(&o.grid_density)
        || !(1..=128).contains(&o.max_iterations)
        || !o.relative_tolerance.is_finite()
        || !(1e-12..=1e-3).contains(&o.relative_tolerance)
    {
        return invalid();
    }
    if !(2..=512).contains(&taps) || (matches!(model, Model::Odd | Model::Half) && taps > 511) {
        return invalid();
    }
    let nyquist = rate * 0.5;
    if !nyquist.is_finite() || nyquist <= 0. || nyquist / rate != 0.5 || nyquist * 2. != rate {
        return numeric();
    }
    let vars = model.variables(taps);
    let needed = vars + 1;
    let step = 1. / (2. * (taps + 1) as f64 * o.grid_density as f64);
    let mut sizes = [0usize; 32];
    let mut count = 0usize;
    let mut previous = -1.;
    let mut previous_normalized = -1.;
    let mut scale = 0f64;
    let mut target_scale = 1f64;
    for (i, b) in bands.iter().enumerate() {
        if !b.low_hz.is_finite()
            || !b.high_hz.is_finite()
            || b.low_hz < 0.
            || b.low_hz >= b.high_hz
            || b.high_hz > nyquist
            || b.low_hz <= previous
            || !b.low_gain.is_finite()
            || !b.high_gain.is_finite()
            || b.low_gain.abs() > 16.
            || b.high_gain.abs() > 16.
            || !b.weight.is_finite()
            || b.weight <= 0.
            || b.weight > 1e6
        {
            return invalid();
        }
        let low = b.low_hz / rate;
        let high = b.high_hz / rate;
        if !low.is_finite()
            || !high.is_finite()
            || !(0.0..=0.5).contains(&low)
            || !(0.0..=0.5).contains(&high)
            || low >= high
            || low <= previous_normalized
            || (b.low_hz > 0. && low == 0.)
            || (high == 0.5 && b.high_hz != nyquist)
            || (b.high_hz == nyquist && high != 0.5)
        {
            return numeric();
        }
        if matches!(model, Model::Even) && b.high_hz == nyquist && b.high_gain != 0. {
            return invalid();
        }
        previous = b.high_hz;
        previous_normalized = high;
        scale = scale.max(b.weight);
        target_scale = target_scale.max(b.low_gain.abs()).max(b.high_gain.abs());
        let points = (((b.high_hz - b.low_hz) / rate / step).ceil() + 1.) as usize;
        if !(2..=MAX_GRID).contains(&points) {
            return Err(FirDesignError::Capacity.into());
        }
        sizes[i] = points;
        count = count.checked_add(points).ok_or(FirDesignError::Capacity)?;
    }
    let reported = if matches!(model, Model::Half) {
        count.checked_mul(2).ok_or(FirDesignError::Capacity)?
    } else {
        count
    };
    if reported > MAX_GRID || count < needed {
        return Err(FirDesignError::Capacity.into());
    }
    numeric_bytes([count, count, count, count, count, count, taps, needed])?;
    let mut grid = buffer::<Point>(count)?;
    let mut active = buffer::<usize>(count)?;
    let mut errors = buffer::<f64>(count)?;
    let mut peaks = buffer::<usize>(count)?;
    let mut strengths = buffer::<f64>(count)?;
    let mut qualified = buffer::<usize>(count)?;
    let mut coefficients = buffer::<f64>(taps)?;
    let mut extrema = buffer::<EquirippleExtremum>(needed)?;
    let bytes = numeric_bytes([
        grid.capacity(),
        active.capacity(),
        errors.capacity(),
        peaks.capacity(),
        strengths.capacity(),
        qualified.capacity(),
        coefficients.capacity(),
        extrema.capacity(),
    ])?;
    let mut last_x = f64::INFINITY;
    for (i, b) in bands.iter().enumerate() {
        for j in 0..sizes[i] {
            let t = j as f64 / (sizes[i] - 1) as f64;
            let f = (1. - t) * (b.low_hz / rate) + t * (b.high_hz / rate);
            let target = (1. - t) * b.low_gain + t * b.high_gain;
            let weight = b.weight / scale;
            let (x, mut q) = model.xq(f);
            let forced = matches!(model, Model::Even)
                && j + 1 == sizes[i]
                && b.high_hz == nyquist
                && f == 0.5;
            if !f.is_finite()
                || !(0.0..=0.5).contains(&f)
                || (matches!(model, Model::Even) && f == 0.5 && !forced)
            {
                return numeric();
            }
            if forced {
                q = 0.;
            }
            let d = if forced {
                0.
            } else {
                (target - model.baseline()) / q
            };
            let w = weight * q;
            if !x.is_finite()
                || x >= last_x
                || !d.is_finite()
                || !w.is_finite()
                || (!forced && (q <= 0. || w <= 0.))
            {
                return numeric();
            }
            last_x = x;
            let index = grid.len();
            grid.push(Point {
                f,
                x,
                d,
                w,
                target,
                weight,
                q,
                forced,
            });
            if !forced {
                active.push(index);
            }
        }
    }
    if active.len() < needed {
        return Err(FirDesignError::Capacity.into());
    }
    errors.resize(count, 0.);
    coefficients.resize(taps, 0.);
    let mut indices = [0usize; 257];
    for (i, v) in indices[..needed].iter_mut().enumerate() {
        *v = active[i * (active.len() - 1) / (needed - 1)];
    }
    for iteration in 1..=o.max_iterations {
        let support = Support::new(&grid, &indices[..needed])?;
        let mut maximum = 0f64;
        for (i, p) in grid.iter().enumerate() {
            let value = if p.forced {
                0.
            } else {
                model.baseline() + p.q * support.evaluate(p.x)?
            };
            let e = p.weight * (p.target - value);
            if !e.is_finite() {
                return numeric();
            }
            errors[i] = e;
            maximum = maximum.max(e.abs());
        }
        recover(&support, model, &mut coefficients)?;
        let sum: f64 = coefficients.iter().map(|x| x.abs()).sum();
        let allowance = 128. * f64::EPSILON * needed as f64 * target_scale.max(sum);
        if !allowance.is_finite() || allowance > 1e-6 * target_scale {
            return numeric();
        }
        let ripple = support.delta.abs();
        let limit = ripple * (1. + o.relative_tolerance) + allowance;
        if maximum <= limit {
            let mut actual = 0f64;
            let mut valid = true;
            for p in &grid {
                let value = if p.forced {
                    0.
                } else {
                    response(&coefficients, p.f)
                };
                let e = p.weight * (p.target - value);
                if !e.is_finite() {
                    return numeric();
                }
                actual = actual.max(e.abs());
                if matches!(model, Model::Half) {
                    let stop_error = -response(&coefficients, 0.5 - p.f);
                    if !stop_error.is_finite() {
                        return numeric();
                    }
                    actual = actual.max(stop_error.abs());
                }
            }
            extrema.clear();
            let mut previous_sign = 0.;
            if actual > allowance {
                for &index in &indices[..needed] {
                    let p = grid[index];
                    let e = p.weight * (p.target - response(&coefficients, p.f));
                    if (e.abs() - ripple).abs() > ripple * o.relative_tolerance + allowance
                        || e == 0.
                        || e.signum() == previous_sign
                    {
                        valid = false;
                        break;
                    }
                    if matches!(model, Model::Half) {
                        let stop_error = -response(&coefficients, 0.5 - p.f);
                        if !stop_error.is_finite()
                            || (stop_error.abs() - ripple).abs()
                                > ripple * o.relative_tolerance + allowance
                            || stop_error == 0.
                            || stop_error.signum() == e.signum()
                        {
                            valid = false;
                            break;
                        }
                    }
                    previous_sign = e.signum();
                    extrema.push(EquirippleExtremum {
                        frequency_hz: p.f * rate,
                        weighted_error: e * scale,
                        normalized_error: e,
                    });
                }
            }
            if valid && actual <= limit {
                let design = FirDesign::from_symmetric_owned(coefficients)?;
                return Ok(EquirippleFirDesign {
                    design,
                    iterations: iteration,
                    grid_points: reported,
                    density: o.grid_density,
                    normalized_error: actual,
                    normalized_ripple: ripple,
                    normalized_allowance: allowance,
                    weight_scale: scale,
                    extrema,
                    rate,
                    halfband: matches!(model, Model::Half),
                    scratch_bytes: bytes,
                    forced_nyquist: grid.iter().any(|p| p.forced),
                });
            }
        }
        peaks.clear();
        for &i in &active {
            let e = errors[i];
            if e == 0. {
                continue;
            }
            if let Some(last) = peaks.last_mut()
                && errors[*last].signum() == e.signum()
            {
                if errors[*last].abs() < e.abs() {
                    *last = i;
                }
            } else {
                peaks.push(i);
            }
        }
        if peaks.len() < needed {
            return Err(EquirippleError::NonConvergence {
                iterations: iteration,
            });
        }
        strengths.clear();
        strengths.extend(peaks.iter().map(|&i| errors[i].abs()));
        strengths.sort_unstable_by(f64::total_cmp);
        let mut lo = 0;
        let mut hi = strengths.len();
        while lo + 1 < hi {
            let mid = (lo + hi) / 2;
            qualify(&peaks, &errors, strengths[mid], &mut qualified);
            if qualified.len() >= needed {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        qualify(&peaks, &errors, strengths[lo], &mut qualified);
        let mut best = 0;
        let mut value: f64 = qualified[..needed].iter().map(|&i| errors[i].abs()).sum();
        let mut window = value;
        for i in needed..qualified.len() {
            window += errors[qualified[i]].abs() - errors[qualified[i - needed]].abs();
            if window > value {
                value = window;
                best = i + 1 - needed;
            }
        }
        let next = &qualified[best..best + needed];
        if next == &indices[..needed] {
            return Err(EquirippleError::NonConvergence {
                iterations: iteration,
            });
        }
        indices[..needed].copy_from_slice(next);
    }
    Err(EquirippleError::NonConvergence {
        iterations: o.max_iterations,
    })
}
fn qualify(peaks: &[usize], errors: &[f64], threshold: f64, out: &mut Vec<usize>) {
    out.clear();
    for &i in peaks {
        if errors[i].abs() < threshold {
            continue;
        }
        if let Some(last) = out.last_mut()
            && errors[*last].signum() == errors[i].signum()
        {
            if errors[*last].abs() < errors[i].abs() {
                *last = i;
            }
        } else {
            out.push(i);
        }
    }
}
struct Support {
    xs: [f64; 257],
    ys: [f64; 257],
    weights: [f64; 257],
    len: usize,
    delta: f64,
}
impl Support {
    fn new(grid: &[Point], indices: &[usize]) -> Result<Self, EquirippleError> {
        let mut out = Self {
            xs: [0.; 257],
            ys: [0.; 257],
            weights: [0.; 257],
            len: indices.len(),
            delta: 0.,
        };
        let mut largest = f64::NEG_INFINITY;
        for (i, &idx) in indices.iter().enumerate() {
            let x = grid[idx].x;
            out.xs[i] = x;
            let mut log = 0.;
            for (j, &other) in indices.iter().enumerate() {
                if i != j {
                    let diff = (x - grid[other].x).abs();
                    if !diff.is_finite() || diff == 0. {
                        return numeric();
                    }
                    log -= diff.ln();
                }
            }
            if !log.is_finite() {
                return numeric();
            }
            out.weights[i] = log;
            largest = largest.max(log);
        }
        let (mut numerator, mut denominator) = (0., 0.);
        for (i, &idx) in indices.iter().enumerate() {
            let magnitude = (out.weights[i] - largest).exp();
            let sign = if i.is_multiple_of(2) { 1. } else { -1. };
            if magnitude == 0. {
                return numeric();
            }
            out.weights[i] = sign * magnitude;
            numerator += out.weights[i] * grid[idx].d;
            denominator += magnitude / grid[idx].w;
        }
        out.delta = numerator / denominator;
        if !out.delta.is_finite() || !denominator.is_finite() || denominator <= 0. {
            return numeric();
        }
        for (i, &idx) in indices.iter().enumerate() {
            out.ys[i] = grid[idx].d
                - if i.is_multiple_of(2) {
                    out.delta / grid[idx].w
                } else {
                    -out.delta / grid[idx].w
                };
            if !out.ys[i].is_finite() {
                return numeric();
            }
        }
        Ok(out)
    }
    fn evaluate(&self, x: f64) -> Result<f64, EquirippleError> {
        let mut max = 0f64;
        for i in 0..self.len {
            if x == self.xs[i] {
                return Ok(self.ys[i]);
            }
            max = max.max((self.weights[i] / (x - self.xs[i])).abs());
        }
        if !max.is_finite() || max == 0. {
            return numeric();
        }
        let (mut top, mut bottom) = (0., 0.);
        for i in 0..self.len {
            let w = self.weights[i] / (x - self.xs[i]) / max;
            top += w * self.ys[i];
            bottom += w;
        }
        let value = top / bottom;
        if !value.is_finite() {
            return numeric();
        }
        Ok(value)
    }
}
fn response(h: &[f64], f: f64) -> f64 {
    let center = (h.len() - 1) as f64 * 0.5;
    h.iter()
        .enumerate()
        .map(|(i, &v)| v * (TAU * f * (i as f64 - center)).cos())
        .sum()
}
fn recover(s: &Support, model: Model, h: &mut [f64]) -> Result<(), EquirippleError> {
    let vars = model.variables(h.len());
    let mut values = [0.; 256];
    for (j, v) in values[..vars].iter_mut().enumerate() {
        let omega = PI * (j as f64 + 0.5) / vars as f64;
        let (x, q) = if matches!(model, Model::Half) {
            (omega.cos(), (omega * 0.5).cos())
        } else {
            model.xq(omega / TAU)
        };
        *v = s.evaluate(x)? * q;
    }
    h.fill(0.);
    if matches!(model, Model::Half) {
        h[h.len() / 2] = 0.5;
    }
    for k in 0..vars {
        let mut sum = 0.;
        for (j, &v) in values[..vars].iter().enumerate() {
            let omega = PI * (j as f64 + 0.5) / vars as f64;
            let phase = if matches!(model, Model::Odd) {
                k as f64 * omega
            } else {
                (k as f64 + 0.5) * omega
            };
            sum += v * phase.cos();
        }
        let value = sum / vars as f64;
        if !value.is_finite() {
            return numeric();
        }
        let offset = if matches!(model, Model::Half) {
            2 * k + 1
        } else {
            k
        };
        let center = h.len() / 2;
        match model {
            Model::Odd => {
                h[center - offset] = value;
                h[center + offset] = value;
            }
            Model::Even => {
                h[center - 1 - offset] = value;
                h[center + offset] = value;
            }
            Model::Half => {
                h[center - offset] = value;
                h[center + offset] = value;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn declared_numeric_budget_is_checked_before_any_reserve() {
        use super::*;
        let measured = allocation_counter::measure(|| {
            assert!(
                numeric_bytes([32768, 32768, 32768, 32768, 32768, 32768, 512, 257]).unwrap()
                    <= MAX_BYTES
            );
            assert_eq!(
                numeric_bytes([41000, 41000, 41000, 41000, 41000, 41000, 512, 257]),
                Err(FirDesignError::Capacity.into())
            );
            assert_eq!(
                numeric_bytes([usize::MAX, 0, 0, 0, 0, 0, 0, 0]),
                Err(FirDesignError::Capacity.into())
            );
        });
        assert_eq!(measured.count_total, 0);
        assert_eq!(measured.count_current, 0);
    }
}
