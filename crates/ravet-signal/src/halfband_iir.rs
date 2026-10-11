//! Configured elliptic halfband allpasses and explicit precision/conversion profiles.
use crate::{FilterError, OversamplingFactor};
use std::f64::consts::PI;
const MAX_ORDER: usize = 63;
const MAX_COEFFICIENTS: usize = 31;
const STOP_POINTS: usize = 4097;
const MIN_ATTENUATION: f64 = 3.010_299_956_639_812;

/// Actual rounded-coefficient stopband observation on 4097 uniform closed points.
/// Frequency is normalized HIGH-rate cycles/sample. This is a sampled observation,
/// distinct from the sizing stop-edge criterion and continuous certification.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HalfBandStopPeak {
    pub normalized_frequency: f64,
    pub magnitude: f64,
}

/// Immutable f64 elliptic allpass halfband profile. Odd order 1..63 and finite full
/// HIGH-rate transition width in (0, 0.5); fixed 31 coefficient storage, no allocation.
/// CONTROL bounded AGM/Jacobi/degree solve and 4097-point measurement. Unresolved
/// edges/moduli/stability or derived nominal attenuation above 300 dB reject, no repairs.
/// Low-rate A_b(q)=product((a+q^-1)/(1+a*q^-1)); full-rate
/// Hlow(z)=0.5*(A0(z²)+z^-1*A1(z²)), Hhigh uses minus. Allpass phase gives
/// frequency-dependent delay; nominal ripple is not a continuous bound after rounding.
///
/// ```
/// use ravet_signal::{HalfBandIirDesign, ConfiguredOversampler, OversamplingFactor};
/// let design = HalfBandIirDesign::from_attenuation(0.05, 80.).unwrap();
/// assert!(design.stop_edge_magnitude().unwrap() <= 1e-4);
/// let rounded = design.quantize_f32().unwrap();
/// assert_eq!(rounded.order(), design.order());
/// let mut wet = ConfiguredOversampler::new(OversamplingFactor::Four, design, design);
/// assert_eq!(wet.process_sample(0., |x| x.tanh()), 0.);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HalfBandIirDesign {
    coefficients: [f64; MAX_COEFFICIENTS],
    count: usize,
    direct: usize,
    order: usize,
    width: f64,
    ripple: f64,
    attenuation: f64,
    peak: HalfBandStopPeak,
}
impl HalfBandIirDesign {
    /// Selected odd order and full normalized transition width, not a sample-rate
    /// duration. Pass=0.25-width/2; stop=0.25+width/2. Rounded missing edges reject.
    pub fn new(order: usize, width: f64) -> Result<Self, FilterError> {
        let pair = width_pair(width)?;
        if !(1..=MAX_ORDER).contains(&order) || order.is_multiple_of(2) {
            return Err(FilterError);
        }
        let k1 = if order == 1 {
            pair
        } else {
            pair_for_ratio(order as f64 * ratio(pair)?)?
        };
        let attenuation = 10. / std::f64::consts::LN_10 * (k1.k.ln_1p() - k1.k.ln());
        let ripple = 10. / std::f64::consts::LN_10 * k1.k.ln_1p();
        if !attenuation.is_finite()
            || attenuation > 300.
            || attenuation <= MIN_ATTENUATION
            || !ripple.is_finite()
            || ripple <= 0.
        {
            return Err(FilterError);
        }
        let count = (order - 1) / 2;
        let direct = count.div_ceil(2);
        let mut result = Self {
            coefficients: [0.; MAX_COEFFICIENTS],
            count,
            direct,
            order,
            width,
            ripple,
            attenuation,
            peak: HalfBandStopPeak {
                normalized_frequency: 0.5,
                magnitude: 0.,
            },
        };
        let complete = pair.integral()?;
        let mut previous = 0.;
        for j in 1..=count {
            let (sn, cn, dn) = jacobi(2. * j as f64 * complete / order as f64, pair)?;
            // Rationalized squared imaginary digital pole radius, without projection.
            let r = (1. + pair.k) * sn / (1. + pair.k * sn * sn + cn * dn);
            let a = r * r;
            if !a.is_finite() || a <= previous || a >= 1. {
                return Err(FilterError);
            }
            previous = a;
            result.coefficients[(j - 1) / 2 + if j.is_multiple_of(2) { direct } else { 0 }] = a;
        }
        result.peak = stop_peak(
            result.direct_coefficients(),
            result.delayed_coefficients(),
            result.stop_edge(),
        )?;
        Ok(result)
    }
    /// Size from the analytic odd-order minimum, then retry odd orders through 63.
    /// Finite requested attenuation above 10log10(2) and at most 300 dB; only actual rounded STOP EDGE amplitude
    /// must meet 10^(-attenuation/20), without relaxed bounds. Sampled stopband peak
    /// is exposed independently; no continuous/full-stopband certificate implied.
    pub fn from_attenuation(width: f64, attenuation_db: f64) -> Result<Self, FilterError> {
        let (first, delta) = minimum_order(width, attenuation_db)?;
        for order in (first..=MAX_ORDER).step_by(2) {
            if let Ok(d) = Self::new(order, width)
                && let Ok(edge) = d.stop_edge_magnitude()
                && edge <= delta
            {
                return Ok(d);
            }
        }
        Err(FilterError)
    }
    pub fn order(&self) -> usize {
        self.order
    }
    pub fn transition_width(&self) -> f64 {
        self.width
    }
    pub fn pass_edge(&self) -> f64 {
        0.25 - self.width * 0.5
    }
    pub fn stop_edge(&self) -> f64 {
        0.25 + self.width * 0.5
    }
    /// Coupled ideal-prototype nominal ripple, before coefficient rounding.
    pub fn nominal_ripple_db(&self) -> f64 {
        self.ripple
    }
    /// Coupled ideal-prototype nominal stop attenuation, before rounding.
    pub fn nominal_attenuation_db(&self) -> f64 {
        self.attenuation
    }
    pub fn direct_coefficients(&self) -> &[f64] {
        &self.coefficients[..self.direct]
    }
    pub fn delayed_coefficients(&self) -> &[f64] {
        &self.coefficients[self.direct..self.count]
    }
    /// Actual rounded lowpass amplitude; finite HIGH-rate normalized 0..0.5 only.
    pub fn magnitude_at(&self, f: f64) -> Result<f64, FilterError> {
        magnitude(self.direct_coefficients(), self.delayed_coefficients(), f)
    }
    pub fn pass_edge_magnitude(&self) -> Result<f64, FilterError> {
        self.magnitude_at(self.pass_edge())
    }
    pub fn stop_edge_magnitude(&self) -> Result<f64, FilterError> {
        self.magnitude_at(self.stop_edge())
    }
    pub fn sampled_stopband_peak(&self) -> HalfBandStopPeak {
        self.peak
    }
    pub const fn stopband_observation_points(&self) -> usize {
        STOP_POINTS
    }
    /// Explicit precision conversion: reject a rounded 0/1/nonfinite/duplicate or
    /// reordered coefficient. Re-measure actual f32-coefficient spectrum; nominal
    /// f64 attenuation never becomes an implicit f32 attenuation guarantee.
    pub fn quantize_f32(&self) -> Result<HalfBandIirDesign32, FilterError> {
        let mut coefficients = [0f32; MAX_COEFFICIENTS];
        let mut previous = 0f32;
        for j in 1..=self.count {
            let index = (j - 1) / 2 + if j.is_multiple_of(2) { self.direct } else { 0 };
            let a = self.coefficients[index] as f32;
            if !a.is_finite() || a <= previous || a >= 1. {
                return Err(FilterError);
            }
            previous = a;
            coefficients[index] = a;
        }
        let mut d = HalfBandIirDesign32 {
            prototype: *self,
            coefficients,
            peak: HalfBandStopPeak {
                normalized_frequency: 0.5,
                magnitude: 0.,
            },
        };
        d.peak = d.measure_peak()?;
        Ok(d)
    }
}
/// Explicit f32 rounded coefficients with separately measured spectrum. Prototype
/// nominal metadata describes the originating f64 design. Fixed storage only.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HalfBandIirDesign32 {
    prototype: HalfBandIirDesign,
    coefficients: [f32; MAX_COEFFICIENTS],
    peak: HalfBandStopPeak,
}
impl HalfBandIirDesign32 {
    /// Independently size/quantize/retry and strictly admit actual f32 stop EDGE.
    pub fn from_attenuation(width: f64, attenuation_db: f64) -> Result<Self, FilterError> {
        let (first, delta) = minimum_order(width, attenuation_db)?;
        for order in (first..=MAX_ORDER).step_by(2) {
            if let Ok(p) = HalfBandIirDesign::new(order, width)
                && let Ok(d) = p.quantize_f32()
                && let Ok(edge) = d.stop_edge_magnitude()
                && edge <= delta
            {
                return Ok(d);
            }
        }
        Err(FilterError)
    }
    pub fn order(&self) -> usize {
        self.prototype.order()
    }
    pub fn transition_width(&self) -> f64 {
        self.prototype.transition_width()
    }
    pub fn pass_edge(&self) -> f64 {
        self.prototype.pass_edge()
    }
    pub fn stop_edge(&self) -> f64 {
        self.prototype.stop_edge()
    }
    pub fn nominal_ripple_db(&self) -> f64 {
        self.prototype.nominal_ripple_db()
    }
    pub fn nominal_attenuation_db(&self) -> f64 {
        self.prototype.nominal_attenuation_db()
    }
    pub fn direct_coefficients(&self) -> &[f32] {
        &self.coefficients[..self.prototype.direct]
    }
    pub fn delayed_coefficients(&self) -> &[f32] {
        &self.coefficients[self.prototype.direct..self.prototype.count]
    }
    pub fn magnitude_at(&self, f: f64) -> Result<f64, FilterError> {
        let coefficients = self.coefficients.map(f64::from);
        magnitude(
            &coefficients[..self.prototype.direct],
            &coefficients[self.prototype.direct..self.prototype.count],
            f,
        )
    }
    pub fn pass_edge_magnitude(&self) -> Result<f64, FilterError> {
        self.magnitude_at(self.pass_edge())
    }
    pub fn stop_edge_magnitude(&self) -> Result<f64, FilterError> {
        self.magnitude_at(self.stop_edge())
    }
    pub fn sampled_stopband_peak(&self) -> HalfBandStopPeak {
        self.peak
    }
    pub const fn stopband_observation_points(&self) -> usize {
        STOP_POINTS
    }
    fn measure_peak(&self) -> Result<HalfBandStopPeak, FilterError> {
        let coefficients = self.coefficients.map(f64::from);
        stop_peak(
            &coefficients[..self.prototype.direct],
            &coefficients[self.prototype.direct..self.prototype.count],
            self.stop_edge(),
        )
    }
}
fn width_pair(width: f64) -> Result<Pair, FilterError> {
    if !width.is_finite() || width <= 0. || width >= 0.5 {
        return Err(FilterError);
    }
    let pass = 0.25 - width * 0.5;
    let stop = 0.25 + width * 0.5;
    if pass <= 0. || pass >= 0.25 || stop <= 0.25 || stop >= 0.5 {
        return Err(FilterError);
    }
    let t = (PI * pass).tan();
    Pair::from_k(t * t)
}
fn ratio(p: Pair) -> Result<f64, FilterError> {
    Ok(p.swapped().integral()? / p.integral()?)
}
fn minimum_order(width: f64, attenuation: f64) -> Result<(usize, f64), FilterError> {
    let p = width_pair(width)?;
    if !attenuation.is_finite() || attenuation <= MIN_ATTENUATION || attenuation > 300. {
        return Err(FilterError);
    }
    let delta = 10f64.powf(-attenuation / 20.);
    let square = delta * delta;
    let k1 = Pair::from_k(square / (1. - square))?;
    let n = (ratio(k1)? / ratio(p)?).ceil();
    if !n.is_finite() || n < 1. || n > MAX_ORDER as f64 {
        return Err(FilterError);
    }
    let n = n as usize;
    Ok((if n.is_multiple_of(2) { n + 1 } else { n }, delta))
}
fn stop_peak(direct: &[f64], delayed: &[f64], stop: f64) -> Result<HalfBandStopPeak, FilterError> {
    let mut peak = HalfBandStopPeak {
        normalized_frequency: stop,
        magnitude: 0.,
    };
    for j in 0..STOP_POINTS {
        let t = j as f64 / (STOP_POINTS - 1) as f64;
        let f = (1. - t) * stop + t * 0.5;
        let a = magnitude(direct, delayed, f)?;
        if a > peak.magnitude {
            peak = HalfBandStopPeak {
                normalized_frequency: f,
                magnitude: a,
            };
        }
    }
    Ok(peak)
}
#[derive(Clone, Copy)]
struct C {
    re: f64,
    im: f64,
}
impl C {
    fn mul(self, b: Self) -> Self {
        Self {
            re: self.re * b.re - self.im * b.im,
            im: self.re * b.im + self.im * b.re,
        }
    }
    fn div(self, b: Self) -> Self {
        let norm = b.re * b.re + b.im * b.im;
        Self {
            re: (self.re * b.re + self.im * b.im) / norm,
            im: (self.im * b.re - self.re * b.im) / norm,
        }
    }
}
fn magnitude(direct: &[f64], delayed: &[f64], f: f64) -> Result<f64, FilterError> {
    if !f.is_finite() || !(0.0..=0.5).contains(&f) {
        return Err(FilterError);
    }
    if f == 0. {
        return Ok(1.);
    }
    if f == 0.5 {
        return Ok(0.);
    }
    let (s, c) = (-std::f64::consts::TAU * f).sin_cos();
    let z = C { re: c, im: s };
    let q = z.mul(z);
    let branch = |a: &[f64]| {
        a.iter().fold(C { re: 1., im: 0. }, |v, &a| {
            v.mul(
                C {
                    re: a + q.re,
                    im: q.im,
                }
                .div(C {
                    re: 1. + a * q.re,
                    im: a * q.im,
                }),
            )
        })
    };
    let a = branch(direct);
    let b = z.mul(branch(delayed));
    let value = (a.re + b.re).hypot(a.im + b.im) * 0.5;
    if !value.is_finite() {
        return Err(FilterError);
    }
    Ok(value)
}

// Bounded own math follows the separately qualified elliptic design mechanics.
#[derive(Clone, Copy)]
struct Pair {
    k: f64,
    kp: f64,
}
impl Pair {
    fn from_k(k: f64) -> Result<Self, FilterError> {
        if !k.is_finite() || k <= 0. || k >= 1. {
            return Err(FilterError);
        }
        Ok(Self {
            k,
            kp: ((1. - k) * (1. + k)).sqrt(),
        })
    }
    fn swapped(self) -> Self {
        Self {
            k: self.kp,
            kp: self.k,
        }
    }
    fn integral(self) -> Result<f64, FilterError> {
        if !self.k.is_finite()
            || !self.kp.is_finite()
            || self.k <= 0.
            || self.k > 1.
            || self.kp <= 0.
            || self.kp > 1.
        {
            return Err(FilterError);
        }
        let mut a = 1.;
        let mut b = self.kp;
        for _ in 0..32 {
            let next = (a + b) * 0.5;
            let geometric = (a * b).sqrt();
            a = next;
            b = geometric;
            if !a.is_finite() || b <= 0. {
                return Err(FilterError);
            }
            if (a - b).abs() <= 4. * f64::EPSILON * a {
                return Ok(PI / (2. * a));
            }
        }
        Err(FilterError)
    }
}
fn pair_for_ratio(target: f64) -> Result<Pair, FilterError> {
    let max = Pair::from_k(f64::from_bits(1f64.to_bits() - 1))?;
    let min = Pair::from_k(f64::MIN_POSITIVE)?;
    let mut lo = f64::MIN_POSITIVE.ln();
    let mut hi = (max.k / max.kp).ln();
    let ratio =
        |p: Pair| -> Result<f64, FilterError> { Ok(p.swapped().integral()? / p.integral()?) };
    if target < ratio(max)? || target > ratio(min)? {
        return Err(FilterError);
    }
    let mut best = min;
    let mut residual = (ratio(min)? / target - 1.).abs();
    let upper_error = (ratio(max)? / target - 1.).abs();
    if upper_error < residual {
        best = max;
        residual = upper_error;
    }
    for _ in 0..128 {
        let x = (lo + hi) * 0.5;
        let t = (-x.abs()).exp();
        let norm = 1f64.hypot(t);
        let candidate = Pair::from_k(if x >= 0. { 1. / norm } else { t / norm })?;
        let r = ratio(candidate)?;
        let error = (r / target - 1.).abs();
        if error < residual {
            best = candidate;
            residual = error;
        }
        if r > target {
            lo = x;
        } else {
            hi = x;
        }
    }
    if residual > 1e-12 {
        return Err(FilterError);
    }
    Ok(best)
}
fn jacobi(u: f64, p: Pair) -> Result<(f64, f64, f64), FilterError> {
    let complete = p.integral()?;
    if !u.is_finite() || u < 0. || u > complete {
        return Err(FilterError);
    }
    if u == 0. {
        return Ok((0., 1., 1.));
    }
    if u == complete {
        return Ok((1., 0., p.kp));
    }
    let mut a = 1.;
    let mut b = p.kp;
    let mut stages = [(0., 0.); 32];
    let mut count = 0;
    let mut converged = false;
    for stage in &mut stages {
        let next = (a + b) * 0.5;
        let c = (a - b) * 0.5;
        let geometric = (a * b).sqrt();
        *stage = (c, next);
        count += 1;
        a = next;
        b = geometric;
        if c.abs() <= 4. * f64::EPSILON * a {
            converged = true;
            break;
        }
    }
    if !converged {
        return Err(FilterError);
    }
    let mut phi = 2f64.powi(count as i32) * a * u;
    for (c, a) in stages[..count].iter().rev() {
        phi = (phi + (c / a * phi.sin()).asin()) * 0.5;
    }
    let (sn, cn) = phi.sin_cos();
    let dn = cn.hypot(p.kp * sn);
    if !sn.is_finite() || !cn.is_finite() || !dn.is_finite() || sn < 0. || cn <= 0. || dn <= 0. {
        return Err(FilterError);
    }
    Ok((sn, cn, dn))
}

macro_rules! runtime_profile {
    ($sample:ty,$design:ident,$filter:ident,$two:ident,$oversampler:ident) => {
        /// Full-rate complementary LP/HP from validated immutable allpass coefficients.
        /// Fixed histories, one update/input; replacement resets, unsmoothed. LP+HP is
        /// the direct allpass, not dry input. Infinite recursive tail and frequency-
        /// dependent phase remain explicit. Nonfinite audio/headroom/denormals propagate.
        #[derive(Clone, Debug)]
        pub struct $filter {
            design: $design,
            states: [[$sample; 2]; MAX_COEFFICIENTS],
            parity: usize,
            delayed: $sample,
        }
        impl $filter {
            pub fn new(design: $design) -> Self {
                Self {
                    design,
                    states: [[0.; 2]; MAX_COEFFICIENTS],
                    parity: 0,
                    delayed: 0.,
                }
            }
            pub fn design(&self) -> &$design {
                &self.design
            }
            pub fn reset(&mut self) {
                self.states.fill([0.; 2]);
                self.parity = 0;
                self.delayed = 0.;
            }
            pub fn replace_design(&mut self, design: $design) {
                self.design = design;
                self.reset();
            }
            pub fn process_sample(&mut self, input: $sample) -> [$sample; 2] {
                let direct = self.design.direct_coefficients();
                let delayed = self.design.delayed_coefficients();
                let mut a = input;
                let mut b = input;
                for (&coefficient, state) in direct.iter().zip(&mut self.states[..direct.len()]) {
                    let y = coefficient * a + state[self.parity];
                    state[self.parity] = a - coefficient * y;
                    a = y;
                }
                for (&coefficient, state) in delayed
                    .iter()
                    .zip(&mut self.states[direct.len()..direct.len() + delayed.len()])
                {
                    let y = coefficient * b + state[self.parity];
                    state[self.parity] = b - coefficient * y;
                    b = y;
                }
                let result = [(a + self.delayed) * 0.5, (a - self.delayed) * 0.5];
                self.delayed = b;
                self.parity ^= 1;
                result
            }
            pub fn process_in_place_lowpass(&mut self, samples: &mut [$sample]) {
                for x in samples {
                    *x = self.process_sample(*x)[0];
                }
            }
            /// Equal lengths required before state/output mutation; empty is a no-op.
            pub fn process_block_lowpass(
                &mut self,
                input: &[$sample],
                output: &mut [$sample],
            ) -> Result<(), FilterError> {
                if input.len() != output.len() {
                    return Err(FilterError);
                }
                for (&x, y) in input.iter().zip(output) {
                    *y = self.process_sample(x)[0];
                }
                Ok(())
            }
        }
        /// Independent prepared interpolation/decimation coefficient profiles and states.
        /// Low-rate allpasses return even/odd interpolation samples in time order, with
        /// unity DC sample gain (gain 2 relative to high-rate zero stuffing). Decimation
        /// retains even phase and previous odd branch. No hidden phase/tail trimming.
        #[derive(Clone, Debug)]
        pub struct $two {
            up: $design,
            down: $design,
            up_states: [$sample; MAX_COEFFICIENTS],
            down_states: [$sample; MAX_COEFFICIENTS],
            delayed: $sample,
        }
        impl $two {
            pub fn new(up: $design, down: $design) -> Self {
                Self {
                    up,
                    down,
                    up_states: [0.; MAX_COEFFICIENTS],
                    down_states: [0.; MAX_COEFFICIENTS],
                    delayed: 0.,
                }
            }
            pub fn up_design(&self) -> &$design {
                &self.up
            }
            pub fn down_design(&self) -> &$design {
                &self.down
            }
            pub fn reset(&mut self) {
                self.up_states.fill(0.);
                self.down_states.fill(0.);
                self.delayed = 0.;
            }
            fn cascade(
                mut x: $sample,
                coefficients: &[$sample],
                states: &mut [$sample],
            ) -> $sample {
                for (&a, s) in coefficients.iter().zip(states) {
                    let y = a * x + *s;
                    *s = x - a * y;
                    x = y;
                }
                x
            }
            pub fn upsample(&mut self, input: $sample) -> [$sample; 2] {
                let a = self.up.direct_coefficients();
                let b = self.up.delayed_coefficients();
                [
                    Self::cascade(input, a, &mut self.up_states[..a.len()]),
                    Self::cascade(input, b, &mut self.up_states[a.len()..a.len() + b.len()]),
                ]
            }
            pub fn downsample(&mut self, input: [$sample; 2]) -> $sample {
                let a = self.down.direct_coefficients();
                let b = self.down.delayed_coefficients();
                let direct = Self::cascade(input[0], a, &mut self.down_states[..a.len()]);
                let delayed = Self::cascade(
                    input[1],
                    b,
                    &mut self.down_states[a.len()..a.len() + b.len()],
                );
                let result = (direct + self.delayed) * 0.5;
                self.delayed = delayed;
                result
            }
        }
        /// Configured 1/2/4/8 cascades; fixed 3 stages/stack 8 samples, exact closure call
        /// count/time order. CONTROL construction; heap/lock/I/O-free sample/block/reset.
        /// Caller closure obligations remain separate. FactorOne exact pass-through;
        /// no dry-phase alignment, integer host latency or modulation/crossfade implied.
        #[derive(Clone, Debug)]
        pub struct $oversampler {
            factor: OversamplingFactor,
            stages: [$two; 3],
        }
        impl $oversampler {
            pub fn new(factor: OversamplingFactor, up: $design, down: $design) -> Self {
                Self::from_stages(factor, [(up, down); 3])
            }
            pub fn from_stages(
                factor: OversamplingFactor,
                profiles: [($design, $design); 3],
            ) -> Self {
                Self {
                    factor,
                    stages: profiles.map(|(up, down)| $two::new(up, down)),
                }
            }
            pub fn factor(&self) -> OversamplingFactor {
                self.factor
            }
            pub fn reset(&mut self) {
                for stage in &mut self.stages {
                    stage.reset();
                }
            }
            fn stage_count(&self) -> usize {
                match self.factor {
                    OversamplingFactor::One => 0,
                    OversamplingFactor::Two => 1,
                    OversamplingFactor::Four => 2,
                    OversamplingFactor::Eight => 3,
                }
            }
            pub fn process_sample(
                &mut self,
                input: $sample,
                mut process: impl FnMut($sample) -> $sample,
            ) -> $sample {
                let mut samples = [0.; 8];
                samples[0] = input;
                let mut count = 1;
                let active = self.stage_count();
                for stage in &mut self.stages[..active] {
                    let mut next = [0.; 8];
                    for (i, &x) in samples[..count].iter().enumerate() {
                        next[2 * i..2 * i + 2].copy_from_slice(&stage.upsample(x));
                    }
                    samples = next;
                    count *= 2;
                }
                for x in &mut samples[..count] {
                    *x = process(*x);
                }
                for stage in self.stages[..active].iter_mut().rev() {
                    let mut next = [0.; 8];
                    for (i, pair) in samples[..count].as_chunks::<2>().0.iter().enumerate() {
                        next[i] = stage.downsample(*pair);
                    }
                    samples = next;
                    count /= 2;
                }
                samples[0]
            }
            pub fn process_in_place(
                &mut self,
                samples: &mut [$sample],
                mut process: impl FnMut($sample) -> $sample,
            ) {
                for x in samples {
                    *x = self.process_sample(*x, &mut process);
                }
            }
            /// Mismatched lengths preserve output/history and never call child DSP.
            pub fn process_block(
                &mut self,
                input: &[$sample],
                output: &mut [$sample],
                mut process: impl FnMut($sample) -> $sample,
            ) -> Result<(), FilterError> {
                if input.len() != output.len() {
                    return Err(FilterError);
                }
                for (&x, y) in input.iter().zip(output) {
                    *y = self.process_sample(x, &mut process);
                }
                Ok(())
            }
        }
    };
}
runtime_profile!(
    f64,
    HalfBandIirDesign,
    HalfBandIirFilter,
    ConfiguredHalfBand2x,
    ConfiguredOversampler
);
runtime_profile!(
    f32,
    HalfBandIirDesign32,
    HalfBandIirFilter32,
    ConfiguredHalfBand2x32,
    ConfiguredOversampler32
);
