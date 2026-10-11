//! Selected-order Chebyshev I/II designs with explicit pass/stop edge semantics.
use crate::{Biquad, BiquadCoefficients, FilterError};
use std::f64::consts::PI;

/// Prototype family with finite positive ripple/attenuation. Type I supports
/// ripple<=120dB; Type II attenuation<=300dB. Unrepresentable extremes reject.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChebyshevKind {
    Type1 { ripple_db: f64 },
    Type2 { attenuation_db: f64 },
}
/// Family selection for automatic pass/stop specification methods.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChebyshevType {
    Type1,
    Type2,
}
/// Hz edge(s), strictly inside Nyquist. Type I edges bound its ripple passband;
/// Type II edges bound its ripple stopband. Neither is a -3dB cutoff. Band
/// order means the resulting digital order, twice the prototype degree.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChebyshevResponse {
    LowPass { edge_hz: f64 },
    HighPass { edge_hz: f64 },
    BandPass { low_hz: f64, high_hz: f64 },
    BandStop { low_hz: f64, high_hz: f64 },
}
/// Fixed16-section f64 design, LP/HP order1..32 or band total even order2..32.
/// CONTROL allocation-free design checks finite arithmetic/gains and strict Jury
/// stability; there are no coefficient, frequency or ripple repairs. Extreme
/// unrepresentable margins reject. Actual response inspection is CONTROL work.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChebyshevDesign {
    sections: [BiquadCoefficients; 16],
    count: usize,
    order: usize,
    kind: ChebyshevKind,
    response: ChebyshevResponse,
}
impl ChebyshevDesign {
    /// Prewarp the edge(s), transform prototype poles and finite/infinite zeros,
    /// and bilinear-map into stable real sections. Type I even prototype degree
    /// retains DC gain10^(-ripple/20), with pass peaks1; odd degree has DC1.
    /// Type II prototype DC1 and finite stop notches; even far-stop limit is
    /// 10^(-attenuation/20), odd limit0. These parity rules apply after frequency
    /// transforms, so Type II endpoints must not be assumed Butterworth zeros.
    pub fn new(
        kind: ChebyshevKind,
        response: ChebyshevResponse,
        rate: f64,
        order: usize,
    ) -> Result<Self, FilterError> {
        let band = matches!(
            response,
            ChebyshevResponse::BandPass { .. } | ChebyshevResponse::BandStop { .. }
        );
        if !(1..=32).contains(&order) || (band && !order.is_multiple_of(2)) {
            return Err(FilterError);
        }
        let db = match kind {
            ChebyshevKind::Type1 { ripple_db } => {
                if !ripple_db.is_finite() || ripple_db <= 0. || ripple_db > 120. {
                    return Err(FilterError);
                }
                ripple_db
            }
            ChebyshevKind::Type2 { attenuation_db } => {
                if !attenuation_db.is_finite() || attenuation_db <= 0. || attenuation_db > 300. {
                    return Err(FilterError);
                }
                attenuation_db
            }
        };
        let squared = (db * std::f64::consts::LN_10 / 10.).exp_m1();
        if !squared.is_finite() || squared <= 0. {
            return Err(FilterError);
        }
        let degree = if band { order / 2 } else { order };
        let type1 = matches!(kind, ChebyshevKind::Type1 { .. });
        let mu = (if type1 {
            1. / squared.sqrt()
        } else {
            squared.sqrt()
        })
        .asinh()
            / degree as f64;
        let sh = mu.sinh();
        let ch = mu.cosh();
        if !sh.is_finite() || !ch.is_finite() || sh <= 0. {
            return Err(FilterError);
        }
        let (low, high) = match response {
            ChebyshevResponse::LowPass { edge_hz } | ChebyshevResponse::HighPass { edge_hz } => {
                (warp(rate, edge_hz)?, None)
            }
            ChebyshevResponse::BandPass { low_hz, high_hz }
            | ChebyshevResponse::BandStop { low_hz, high_hz } => {
                if low_hz >= high_hz {
                    return Err(FilterError);
                }
                (warp(rate, low_hz)?, Some(warp(rate, high_hz)?))
            }
        };
        let bandwidth = high.map_or(0., |h| h - low);
        let center = high.map_or(0., |h| low.sqrt() * h.sqrt());
        let product = center * center;
        if band
            && (!bandwidth.is_finite() || bandwidth <= 0. || !product.is_finite() || product <= 0.)
        {
            return Err(FilterError);
        }
        let reference = match response {
            ChebyshevResponse::LowPass { .. } | ChebyshevResponse::BandStop { .. } => 0.,
            ChebyshevResponse::HighPass { .. } => 0.5,
            ChebyshevResponse::BandPass { .. } => center.atan() / PI,
        };
        let target = if type1 && degree.is_multiple_of(2) {
            10f64.powf(-db / 20.)
        } else {
            1.
        };
        let infinite = match response {
            ChebyshevResponse::LowPass { .. } => [1., 2., 1.],
            ChebyshevResponse::HighPass { .. } => [1., -2., 1.],
            ChebyshevResponse::BandPass { .. } => [1., 0., -1.],
            ChebyshevResponse::BandStop { .. } => notch(center)?,
        };
        let mut result = Self {
            sections: [BiquadCoefficients::new([1., 0., 0.], [0., 0.])?; 16],
            count: 0,
            order,
            kind,
            response,
        };
        for j in 0..degree / 2 {
            let theta = (2 * j + 1) as f64 * PI / (2 * degree) as f64;
            let c = theta.cos();
            let base = C {
                re: -sh * theta.sin(),
                im: ch * c,
            };
            let pole = if type1 { base } else { C::real(1.).div(base) };
            if band {
                let q = if matches!(response, ChebyshevResponse::BandPass { .. }) {
                    pole.scale(bandwidth)
                } else {
                    C::real(bandwidth).div(pole)
                };
                let roots = quadratic(q, product)?;
                let numerators = if type1 {
                    [infinite; 2]
                } else {
                    let a = 1. / c;
                    let imaginary = if matches!(response, ChebyshevResponse::BandPass { .. }) {
                        bandwidth * a
                    } else {
                        -bandwidth / a
                    };
                    let zeros = imaginary_roots(imaginary, center)?;
                    [notch(zeros[0])?, notch(zeros[1])?]
                };
                for (s, b) in roots.into_iter().zip(numerators) {
                    let z = digital(s)?;
                    result.push(b, [-2. * z.re, z.norm_squared()], reference, target)?;
                }
            } else {
                let s = if matches!(response, ChebyshevResponse::LowPass { .. }) {
                    pole.scale(low)
                } else {
                    C::real(low).div(pole)
                };
                let z = digital(s)?;
                let b = if type1 {
                    infinite
                } else {
                    notch(if matches!(response, ChebyshevResponse::LowPass { .. }) {
                        low / c
                    } else {
                        -low * c
                    })?
                };
                result.push(b, [-2. * z.re, z.norm_squared()], reference, target)?;
            }
        }
        if !degree.is_multiple_of(2) {
            let pole = C::real(if type1 { -sh } else { -1. / sh });
            if band {
                let q = if matches!(response, ChebyshevResponse::BandPass { .. }) {
                    pole.scale(bandwidth)
                } else {
                    C::real(bandwidth).div(pole)
                };
                let roots = quadratic(q, product)?;
                let a = digital(roots[0])?;
                let b = digital(roots[1])?;
                result.push(infinite, [-(a.re + b.re), a.mul(b).re], reference, target)?;
            } else {
                let s = if matches!(response, ChebyshevResponse::LowPass { .. }) {
                    pole.scale(low)
                } else {
                    C::real(low).div(pole)
                };
                let z = digital(s)?;
                let sign = if matches!(response, ChebyshevResponse::LowPass { .. }) {
                    1.
                } else {
                    -1.
                };
                result.push([1., sign, 0.], [-z.re, 0.], reference, target)?;
            }
        }
        Ok(result)
    }
    fn push(
        &mut self,
        b: [f64; 3],
        a: [f64; 2],
        reference: f64,
        target: f64,
    ) -> Result<(), FilterError> {
        let raw = BiquadCoefficients::new(b, a)?;
        let magnitude = raw.magnitude_at(reference)?;
        if !magnitude.is_finite() || magnitude <= 0. {
            return Err(FilterError);
        }
        let gain = if self.count == 0 { target } else { 1. };
        let normalized = b.map(|v| v / magnitude * gain);
        if normalized[0] == 0. {
            return Err(FilterError);
        }
        self.sections[self.count] = BiquadCoefficients::new(normalized, a)?;
        self.count += 1;
        Ok(())
    }
    /// LP automatic edge/order selection: finite0<Rp<As<=300dB,Rp<=120dB,
    /// 0<pass<stop<rate/2. Type I derives rippleRp; Type II attenuationAs.
    /// Start at analytic minimum, choose its admissible warped-edge geometric
    /// midpoint and verify actual rounded supplied-edge amplitudes against both
    /// strict requested bounds. Failed/unrepresentable candidates retry through
    /// order32, otherwise reject. Report actual kind/edge/order. Nominal ripple
    /// shape and supplied-edge verification are distinct from continuous interval
    /// certification or a search of every representable f64 edge.
    pub fn lowpass_spec(
        family: ChebyshevType,
        rate: f64,
        pass_hz: f64,
        stop_hz: f64,
        rp: f64,
        atten: f64,
    ) -> Result<Self, FilterError> {
        Self::spec(false, family, rate, pass_hz, stop_hz, rp, atten)
    }
    /// High-pass equivalent of [`Self::lowpass_spec`], stop_hz<pass_hz.
    pub fn highpass_spec(
        family: ChebyshevType,
        rate: f64,
        pass_hz: f64,
        stop_hz: f64,
        rp: f64,
        atten: f64,
    ) -> Result<Self, FilterError> {
        Self::spec(true, family, rate, pass_hz, stop_hz, rp, atten)
    }
    fn spec(
        high: bool,
        family: ChebyshevType,
        rate: f64,
        pass: f64,
        stop: f64,
        rp: f64,
        atten: f64,
    ) -> Result<Self, FilterError> {
        if !rp.is_finite()
            || !atten.is_finite()
            || rp <= 0.
            || rp > 120.
            || atten <= rp
            || atten > 300.
            || (high && stop >= pass)
            || (!high && pass >= stop)
        {
            return Err(FilterError);
        }
        let wp = warp(rate, pass)?;
        let ws = warp(rate, stop)?;
        let ep = (rp * std::f64::consts::LN_10 / 10.).exp_m1();
        let es = (atten * std::f64::consts::LN_10 / 10.).exp_m1();
        let ratio = if high { wp / ws } else { ws / wp };
        let root = (es.sqrt() / ep.sqrt()).acosh();
        if !ep.is_finite()
            || ep <= 0.
            || !es.is_finite()
            || es <= ep
            || !ratio.is_finite()
            || ratio <= 1.
            || !root.is_finite()
        {
            return Err(FilterError);
        }
        let minimum = (root / ratio.acosh()).ceil();
        if !minimum.is_finite() || !(1.0..=32.0).contains(&minimum) {
            return Err(FilterError);
        }
        let kind = match family {
            ChebyshevType::Type1 => ChebyshevKind::Type1 { ripple_db: rp },
            ChebyshevType::Type2 => ChebyshevKind::Type2 {
                attenuation_db: atten,
            },
        };
        for order in minimum as usize..=32 {
            let qn = (root / order as f64).cosh();
            let (lo, hi) = match (family, high) {
                (ChebyshevType::Type1, false) => (wp, ws / qn),
                (ChebyshevType::Type1, true) => (ws * qn, wp),
                (ChebyshevType::Type2, false) => (wp * qn, ws),
                (ChebyshevType::Type2, true) => (ws, wp / qn),
            };
            if !lo.is_finite() || !hi.is_finite() || lo <= 0. || lo > hi {
                continue;
            }
            let edge_hz = (lo.sqrt() * hi.sqrt()).atan() / PI * rate;
            let response = if high {
                ChebyshevResponse::HighPass { edge_hz }
            } else {
                ChebyshevResponse::LowPass { edge_hz }
            };
            if let Ok(design) = Self::new(kind, response, rate, order)
                && let Ok(pass_gain) = design.magnitude_at(pass / rate)
                && let Ok(stop_gain) = design.magnitude_at(stop / rate)
                && pass_gain >= 10f64.powf(-rp / 20.)
                && stop_gain <= 10f64.powf(-atten / 20.)
            {
                return Ok(design);
            }
        }
        Err(FilterError)
    }
    /// Actual prototype family and ripple/attenuation parameter.
    pub fn kind(&self) -> ChebyshevKind {
        self.kind
    }
    /// Actual edge(s), including those selected from specifications.
    pub fn response(&self) -> ChebyshevResponse {
        self.response
    }
    /// Total digital order; band prototype degree is half this.
    pub fn order(&self) -> usize {
        self.order
    }
    /// Active normalized sections in deterministic processing order.
    pub fn stages(&self) -> &[BiquadCoefficients] {
        &self.sections[..self.count]
    }
    /// Actual rounded-section linear amplitude at finite normalized0..=0.5.
    pub fn magnitude_at(&self, f: f64) -> Result<f64, FilterError> {
        if !f.is_finite() || !(0.0..=0.5).contains(&f) {
            return Err(FilterError);
        }
        let mut gain = 1.;
        for s in self.stages() {
            gain *= s.magnitude_at(f)?;
        }
        if !gain.is_finite() {
            return Err(FilterError);
        }
        Ok(gain)
    }
}
/// One-channel fixed-state f64 processing. Sample/block/reset have no allocation,
/// lock or I/O. Nonfinite audio propagates; headroom/denormals/transitions are
/// consumer policy. No finite IIR tail, smoothing or native deadline guarantee.
///
/// ```
/// use ravet_signal::{ChebyshevDesign,ChebyshevFilter,ChebyshevKind,ChebyshevResponse};
/// let d=ChebyshevDesign::new(ChebyshevKind::Type2{attenuation_db:60.},
///     ChebyshevResponse::LowPass{edge_hz:9000.},48000.,7).unwrap();
/// let mut f=ChebyshevFilter::new(d);
/// assert!(f.process_sample(1.).is_finite());
/// ```
pub struct ChebyshevFilter {
    stages: [Biquad; 16],
    count: usize,
}
impl ChebyshevFilter {
    /// Start all histories at zero.
    pub fn new(design: ChebyshevDesign) -> Self {
        Self {
            stages: design.sections.map(Biquad::new),
            count: design.count,
        }
    }
    /// Replace the whole design and reset every history, without smoothing.
    pub fn set_design(&mut self, design: ChebyshevDesign) {
        *self = Self::new(design);
    }
    /// Process one sample, retaining one channel's state.
    pub fn process_sample(&mut self, mut input: f64) -> f64 {
        for s in &mut self.stages[..self.count] {
            input = s.process_sample(input);
        }
        input
    }
    /// Process an arbitrary block; empty blocks do nothing.
    pub fn process_in_place(&mut self, samples: &mut [f64]) {
        for sample in samples {
            *sample = self.process_sample(*sample);
        }
    }
    /// Clear every state without allocation.
    pub fn reset(&mut self) {
        for s in &mut self.stages {
            s.reset();
        }
    }
}
fn notch(imaginary: f64) -> Result<[f64; 3], FilterError> {
    let squared = imaginary * imaginary;
    if !squared.is_finite() {
        return Err(FilterError);
    }
    let cosine = (1. - squared) / (1. + squared);
    if !cosine.is_finite() {
        return Err(FilterError);
    }
    Ok([1., -2. * cosine, 1.])
}
fn imaginary_roots(sum: f64, center: f64) -> Result<[f64; 2], FilterError> {
    let discriminant = sum.hypot(2. * center);
    let first = if sum >= 0. {
        (sum + discriminant) * 0.5
    } else {
        (sum - discriminant) * 0.5
    };
    let second = -(center * center) / first;
    if !first.is_finite() || !second.is_finite() {
        return Err(FilterError);
    }
    Ok([first, second])
}
fn warp(rate: f64, frequency: f64) -> Result<f64, FilterError> {
    if !rate.is_finite()
        || rate <= 0.
        || !frequency.is_finite()
        || frequency <= 0.
        || frequency >= rate * 0.5
    {
        return Err(FilterError);
    }
    let w = (PI * (frequency / rate)).tan();
    if !w.is_finite() || w <= 0. {
        return Err(FilterError);
    }
    Ok(w)
}
#[derive(Clone, Copy)]
struct C {
    re: f64,
    im: f64,
}
impl C {
    fn real(re: f64) -> Self {
        Self { re, im: 0. }
    }
    fn add(self, b: Self) -> Self {
        Self {
            re: self.re + b.re,
            im: self.im + b.im,
        }
    }
    fn sub(self, b: Self) -> Self {
        Self {
            re: self.re - b.re,
            im: self.im - b.im,
        }
    }
    fn scale(self, v: f64) -> Self {
        Self {
            re: self.re * v,
            im: self.im * v,
        }
    }
    fn mul(self, b: Self) -> Self {
        Self {
            re: self.re * b.re - self.im * b.im,
            im: self.re * b.im + self.im * b.re,
        }
    }
    fn div(self, b: Self) -> Self {
        if b.re.abs() >= b.im.abs() {
            let r = b.im / b.re;
            let d = b.re + b.im * r;
            Self {
                re: (self.re + self.im * r) / d,
                im: (self.im - self.re * r) / d,
            }
        } else {
            let r = b.re / b.im;
            let d = b.im + b.re * r;
            Self {
                re: (self.re * r + self.im) / d,
                im: (self.im * r - self.re) / d,
            }
        }
    }
    fn norm_squared(self) -> f64 {
        self.re * self.re + self.im * self.im
    }
    fn finite(self) -> bool {
        self.re.is_finite() && self.im.is_finite()
    }
    fn sqrt(self) -> Self {
        let mag = self.re.hypot(self.im);
        if self.re >= 0. {
            let re = (mag * 0.5 + self.re * 0.5).sqrt();
            Self {
                re,
                im: if re == 0. { 0. } else { self.im / (2. * re) },
            }
        } else {
            let im = (mag * 0.5 - self.re * 0.5).sqrt().copysign(self.im);
            Self {
                re: self.im.abs() / (2. * im.abs()),
                im,
            }
        }
    }
}
fn digital(s: C) -> Result<C, FilterError> {
    let z = C::real(1.).add(s).div(C::real(1.).sub(s));
    if !s.finite() || s.re >= 0. || !z.finite() || z.norm_squared() >= 1. {
        return Err(FilterError);
    }
    Ok(z)
}
fn quadratic(q: C, product: f64) -> Result<[C; 2], FilterError> {
    let d = q.mul(q).sub(C::real(4. * product)).sqrt();
    let a = q.add(d).scale(0.5);
    let b = q.sub(d).scale(0.5);
    let first = if a.re.hypot(a.im) >= b.re.hypot(b.im) {
        a
    } else {
        b
    };
    let second = C::real(product).div(first);
    if !first.finite() || !second.finite() {
        return Err(FilterError);
    }
    Ok([first, second])
}
