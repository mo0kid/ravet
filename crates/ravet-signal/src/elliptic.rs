//! Selected-order elliptic filters with pass and stop ripple, finite zeros and bounded design.
use crate::{Biquad, BiquadCoefficients, FilterError};
use std::f64::consts::PI;

/// Hz ripple pass edges strictly inside Nyquist. Band order is the resulting
/// digital order, twice the prototype degree; edges are not -3dB cutoffs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EllipticResponse {
    LowPass { pass_edge_hz: f64 },
    HighPass { pass_edge_hz: f64 },
    BandPass { low_hz: f64, high_hz: f64 },
    BandStop { low_hz: f64, high_hz: f64 },
}
/// Fixed16-section f64 design: LP/HP1..32, band total even2..32. Finite
/// 0<ripple<attenuation<=300dB, ripple<=120dB. CONTROL design is allocation-free;
/// bounded AGM, degree and inverse solves reject unrepresentable arithmetic,
/// margins or residuals without parameter repairs. Not every otherwise valid
/// extreme parameter combination is numerically representable in this profile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EllipticDesign {
    sections: [BiquadCoefficients; 16],
    count: usize,
    order: usize,
    response: EllipticResponse,
    ripple_db: f64,
    attenuation_db: f64,
    modulus: f64,
}
impl EllipticDesign {
    /// Prewarp edges, derive elliptic prototype poles/finite zeros and transform
    /// into strictly stable real sections. Even prototype degree has reference
    /// gain10^(-ripple/20), odd degree1; far-stop gain is10^(-attenuation/20)
    /// for even degree and0 for odd. Finite stop notches survive every transform.
    pub fn new(
        response: EllipticResponse,
        rate: f64,
        order: usize,
        ripple_db: f64,
        attenuation_db: f64,
    ) -> Result<Self, FilterError> {
        let band = matches!(
            response,
            EllipticResponse::BandPass { .. } | EllipticResponse::BandStop { .. }
        );
        if !(1..=32).contains(&order) || (band && !order.is_multiple_of(2)) {
            return Err(FilterError);
        }
        let (ep, k1) = parameters(ripple_db, attenuation_db)?;
        let degree = if band { order / 2 } else { order };
        let pair = degree_pair(k1, degree)?;
        let (sv, cv, dv) = if degree == 1 {
            (1., ep, 1.)
        } else {
            let r = inverse_sc(1. / ep, k1.swapped())?;
            let v = pair.integral()? * r / (degree as f64 * k1.integral()?);
            jacobi(v, pair.swapped())?
        };
        let (low, high) = match response {
            EllipticResponse::LowPass { pass_edge_hz }
            | EllipticResponse::HighPass { pass_edge_hz } => (warp(rate, pass_edge_hz)?, None),
            EllipticResponse::BandPass { low_hz, high_hz }
            | EllipticResponse::BandStop { low_hz, high_hz } => {
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
            EllipticResponse::LowPass { .. } | EllipticResponse::BandStop { .. } => 0.,
            EllipticResponse::HighPass { .. } => 0.5,
            EllipticResponse::BandPass { .. } => center.atan() / PI,
        };
        let target = if degree.is_multiple_of(2) {
            10f64.powf(-ripple_db / 20.)
        } else {
            1.
        };
        let infinite = match response {
            EllipticResponse::LowPass { .. } => [1., 2., 1.],
            EllipticResponse::HighPass { .. } => [1., -2., 1.],
            EllipticResponse::BandPass { .. } => [1., 0., -1.],
            EllipticResponse::BandStop { .. } => notch(center)?,
        };
        let mut result = Self {
            sections: [BiquadCoefficients::new([1., 0., 0.], [0., 0.])?; 16],
            count: 0,
            order,
            response,
            ripple_db,
            attenuation_db,
            modulus: pair.k,
        };
        let complete = pair.integral()?;
        for index in 0..degree / 2 {
            let j = 2 * index + if degree.is_multiple_of(2) { 1 } else { 2 };
            let (sn, cn, dn) = jacobi(j as f64 * complete / degree as f64, pair)?;
            let denominator = (1. - dn * sv) * (1. + dn * sv);
            let pole = C {
                re: -cn * dn * sv * cv / denominator,
                im: sn * dv / denominator,
            };
            let zero = 1. / (pair.k * sn);
            if !denominator.is_finite() || denominator <= 0. || !zero.is_finite() {
                return Err(FilterError);
            }
            if band {
                let pass = matches!(response, EllipticResponse::BandPass { .. });
                let q = if pass {
                    pole.scale(bandwidth)
                } else {
                    C::real(bandwidth).div(pole)
                };
                let roots = quadratic(q, product)?;
                let zs = imaginary_roots(
                    if pass {
                        bandwidth * zero
                    } else {
                        -bandwidth / zero
                    },
                    center,
                )?;
                for (s, z) in roots.into_iter().zip(zs) {
                    let d = digital(s)?;
                    result.push(notch(z)?, [-2. * d.re, d.norm_squared()], reference, target)?;
                }
            } else {
                let pass = matches!(response, EllipticResponse::LowPass { .. });
                let s = if pass {
                    pole.scale(low)
                } else {
                    C::real(low).div(pole)
                };
                let z = digital(s)?;
                result.push(
                    notch(if pass { low * zero } else { -low / zero })?,
                    [-2. * z.re, z.norm_squared()],
                    reference,
                    target,
                )?;
            }
        }
        if !degree.is_multiple_of(2) {
            let pole = C::real(-sv / cv);
            if band {
                let q = if matches!(response, EllipticResponse::BandPass { .. }) {
                    pole.scale(bandwidth)
                } else {
                    C::real(bandwidth).div(pole)
                };
                let roots = quadratic(q, product)?;
                let a = digital(roots[0])?;
                let b = digital(roots[1])?;
                result.push(infinite, [-(a.re + b.re), a.mul(b).re], reference, target)?;
            } else {
                let pass = matches!(response, EllipticResponse::LowPass { .. });
                let z = digital(if pass {
                    pole.scale(low)
                } else {
                    C::real(low).div(pole)
                })?;
                result.push(
                    [1., if pass { 1. } else { -1. }, 0.],
                    [-z.re, 0.],
                    reference,
                    target,
                )?;
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
        let b = b.map(|v| v / magnitude * gain);
        if b[0] == 0. {
            return Err(FilterError);
        }
        self.sections[self.count] = BiquadCoefficients::new(b, a)?;
        self.count += 1;
        Ok(())
    }
    /// Finite0<ripple<attenuation<=300dB, ripple<=120dB and0<pass<stop<rate/2.
    /// Start at complete-integral analytic minimum. For each candidate<=32,
    /// choose the admissible warped pass-edge geometric midpoint and strictly
    /// verify actual rounded supplied-edge amplitudes. Unrepresentable candidates
    /// retry; failure through32 rejects. Actual order/edge/stop ratio are exposed.
    /// This is not a continuous postround ripple certificate or a search over all
    /// representable f64 edges.
    pub fn lowpass_spec(
        rate: f64,
        pass_hz: f64,
        stop_hz: f64,
        ripple_db: f64,
        attenuation_db: f64,
    ) -> Result<Self, FilterError> {
        Self::spec(false, rate, pass_hz, stop_hz, ripple_db, attenuation_db)
    }
    /// High-pass equivalent,0<stop<pass<rate/2.
    pub fn highpass_spec(
        rate: f64,
        pass_hz: f64,
        stop_hz: f64,
        ripple_db: f64,
        attenuation_db: f64,
    ) -> Result<Self, FilterError> {
        Self::spec(true, rate, pass_hz, stop_hz, ripple_db, attenuation_db)
    }
    fn spec(
        high: bool,
        rate: f64,
        pass: f64,
        stop: f64,
        rp: f64,
        atten: f64,
    ) -> Result<Self, FilterError> {
        let (_, k1) = parameters(rp, atten)?;
        if (high && stop >= pass) || (!high && pass >= stop) {
            return Err(FilterError);
        }
        let wp = warp(rate, pass)?;
        let ws = warp(rate, stop)?;
        let edge = Pair::from_k(if high { ws / wp } else { wp / ws })?;
        let minimum = (edge.integral()? * k1.swapped().integral()?
            / (edge.swapped().integral()? * k1.integral()?))
        .ceil();
        if !minimum.is_finite() || !(1.0..=32.0).contains(&minimum) {
            return Err(FilterError);
        }
        for order in minimum as usize..=32 {
            let Ok(k) = degree_pair(k1, order) else {
                continue;
            };
            let (lo, hi) = if high { (ws / k.k, wp) } else { (wp, ws * k.k) };
            if !lo.is_finite() || !hi.is_finite() || lo <= 0. || lo > hi {
                continue;
            }
            let pass_edge_hz = (lo.sqrt() * hi.sqrt()).atan() / PI * rate;
            let response = if high {
                EllipticResponse::HighPass { pass_edge_hz }
            } else {
                EllipticResponse::LowPass { pass_edge_hz }
            };
            if let Ok(d) = Self::new(response, rate, order, rp, atten)
                && let Ok(p) = d.magnitude_at(pass / rate)
                && let Ok(s) = d.magnitude_at(stop / rate)
                && p >= 10f64.powf(-rp / 20.)
                && s <= 10f64.powf(-atten / 20.)
            {
                return Ok(d);
            }
        }
        Err(FilterError)
    }
    /// Actual Hz pass edge(s), including those selected from specifications.
    pub fn response(&self) -> EllipticResponse {
        self.response
    }
    /// Total digital order; band prototype degree is half this.
    pub fn order(&self) -> usize {
        self.order
    }
    /// Nominal prototype passband ripple in dB.
    pub fn ripple_db(&self) -> f64 {
        self.ripple_db
    }
    /// Nominal prototype stopband attenuation in dB.
    pub fn attenuation_db(&self) -> f64 {
        self.attenuation_db
    }
    /// Derived nominal prototype stop/pass frequency ratio1/k. This is distinct
    /// from certification of the entire rounded-section stopband continuum.
    pub fn prototype_stop_ratio(&self) -> f64 {
        1. / self.modulus
    }
    /// Active normalized sections, in deterministic processing order.
    pub fn stages(&self) -> &[BiquadCoefficients] {
        &self.sections[..self.count]
    }
    /// Actual rounded-section amplitude at finite normalized0..=0.5.
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
fn parameters(rp: f64, atten: f64) -> Result<(f64, Pair), FilterError> {
    if !rp.is_finite() || !atten.is_finite() || rp <= 0. || rp > 120. || atten <= rp || atten > 300.
    {
        return Err(FilterError);
    }
    let ep = (rp * std::f64::consts::LN_10 / 10.).exp_m1().sqrt();
    let es = (atten * std::f64::consts::LN_10 / 10.).exp_m1().sqrt();
    if !ep.is_finite() || ep <= 0. || !es.is_finite() {
        return Err(FilterError);
    }
    Ok((ep, Pair::from_k(ep / es)?))
}
fn degree_pair(k1: Pair, n: usize) -> Result<Pair, FilterError> {
    if n == 1 {
        return Ok(k1);
    }
    let target = k1.swapped().integral()? / (n as f64 * k1.integral()?);
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
fn inverse_sc(target: f64, p: Pair) -> Result<f64, FilterError> {
    if !target.is_finite() || target <= 0. {
        return Err(FilterError);
    }
    let complete = p.integral()?;
    let mut lo = 0.;
    let mut hi = 1.;
    let mut best = 0.;
    let mut error = f64::INFINITY;
    for _ in 0..128 {
        let f = (lo + hi) * 0.5;
        let r = f * complete;
        let (sn, cn, _) = jacobi(r, p)?;
        let value = sn / cn;
        let e = (value / target - 1.).abs();
        if e < error {
            best = r;
            error = e;
        }
        if value < target {
            lo = f;
        } else {
            hi = f;
        }
    }
    if error > 1e-12 {
        return Err(FilterError);
    }
    Ok(best)
}
/// One-channel fixed-state f64 processing. Sample/block/reset have no allocation,
/// lock or I/O. Nonfinite audio propagates; headroom/denormals/transitions are
/// consumer policy. No finite IIR tail, smoothing or native deadline guarantee.
///
/// ```
/// use ravet_signal::{EllipticDesign,EllipticFilter,EllipticResponse};
/// let d=EllipticDesign::new(EllipticResponse::LowPass{pass_edge_hz:9000.},
///     48000.,7,1.,80.).unwrap();
/// let mut f=EllipticFilter::new(d);
/// assert!(f.process_sample(1.).is_finite());
/// ```
pub struct EllipticFilter {
    stages: [Biquad; 16],
    count: usize,
}
impl EllipticFilter {
    /// Start all histories at zero.
    pub fn new(design: EllipticDesign) -> Self {
        Self {
            stages: design.sections.map(Biquad::new),
            count: design.count,
        }
    }
    /// Replace the whole design and reset every history, without smoothing.
    pub fn set_design(&mut self, design: EllipticDesign) {
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

#[cfg(test)]
mod tests {
    use super::*;
    fn quadrature(phi: f64, k: f64) -> f64 {
        let count = 32768;
        let h = phi / count as f64;
        let mut sum = 0.;
        for i in 0..=count {
            let x = i as f64 * h;
            let y = 1. / ((1. - k * x.sin()) * (1. + k * x.sin())).sqrt();
            sum += y * if i == 0 || i == count {
                1.
            } else if i % 2 == 0 {
                2.
            } else {
                4.
            };
        }
        sum * h / 3.
    }
    #[test]
    fn bounded_special_functions_have_independent_integral_known_value_and_degree_evidence() {
        let p = Pair::from_k(0.65).unwrap();
        let (s, c, d) = jacobi(0.8, p).unwrap();
        assert!((s - 0.6950642165).abs() < 5e-11);
        assert!((c - 0.7189476580).abs() < 5e-11);
        assert!((d - 0.8921234349).abs() < 5e-11);
        assert!(
            (Pair::from_k(0.5f64.sqrt()).unwrap().integral().unwrap() - 1.8540746773013719).abs()
                < 2e-15
        );
        for k in [0.001, 0.5, 0.9, 0.999] {
            let p = Pair::from_k(k).unwrap();
            let complete = p.integral().unwrap();
            assert!((complete - quadrature(PI * 0.5, k)).abs() < 3e-12);
            assert_eq!(jacobi(0., p).unwrap(), (0., 1., 1.));
            assert_eq!(jacobi(complete, p).unwrap(), (1., 0., p.kp));
            for f in [0.01, 0.2, 0.7, 0.99] {
                let u = f * complete;
                let (s, c, d) = jacobi(u, p).unwrap();
                assert!((quadrature(s.asin(), k) - u).abs() < 4e-11);
                assert!((s * s + c * c - 1.).abs() < 4e-16);
                assert!((d * d + k * k * s * s - 1.).abs() < 5e-16);
            }
        }
        for k1 in [1e-150, 1e-8, 0.02, 0.5] {
            let p = Pair::from_k(k1).unwrap();
            assert_eq!(degree_pair(p, 1).unwrap().k, k1);
            let q = degree_pair(p, 2).unwrap();
            let closed = 2. * k1.sqrt() / (1. + k1);
            assert!((q.k / closed - 1.).abs() < 1e-12);
        }
        for k1 in [0.2, 0.5, 0.9] {
            let p = Pair::from_k(k1).unwrap().swapped();
            for value in [0.01, 0.5, 1., 4.] {
                let r = inverse_sc(value, p).unwrap();
                assert!((r - quadrature(value.atan(), p.k)).abs() < 2e-12);
            }
        }
        assert!(Pair::from_k(0.).is_err());
        assert!(Pair::from_k(1.).is_err());
        assert!(jacobi(-1., p).is_err());
        assert!(inverse_sc(f64::INFINITY, p).is_err());
        // Largest rounded modulus cannot meet this weak, high-degree transition.
        assert!(degree_pair(Pair::from_k(0.999).unwrap(), 32).is_err());
    }
}
