//! Bounded selected-order Butterworth filters, designed on CONTROL.
use crate::{Biquad, BiquadCoefficients, FilterError};
use std::f64::consts::PI;

/// Cutoff frequencies in Hz. Band order is the resulting digital order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ButterworthResponse {
    LowPass { cutoff_hz: f64 },
    HighPass { cutoff_hz: f64 },
    BandPass { low_hz: f64, high_hz: f64 },
    BandStop { low_hz: f64, high_hz: f64 },
}

/// Finite, strictly stable sections for low/high order 1..32 or even band order
/// 2..32. Design is allocation-free CONTROL work. Extreme unrepresentable poles,
/// gains, cutoffs or stability margins reject rather than being clamped.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButterworthDesign {
    sections: [BiquadCoefficients; 16],
    count: usize,
    order: usize,
    response: ButterworthResponse,
}
impl ButterworthDesign {
    /// Prewarp each frequency, transform the Butterworth prototype and bilinear
    /// map into at most 16 sections. Normalize LP at DC, HP at Nyquist, BP at
    /// its warped center and BS at DC. Band order is twice the prototype degree;
    /// odd LP/HP uses one first-order section with zero second-order terms.
    /// Rate is finite/positive, frequencies strictly inside Nyquist and ordered.
    pub fn new(
        response: ButterworthResponse,
        sample_rate: f64,
        order: usize,
    ) -> Result<Self, FilterError> {
        let band = matches!(
            response,
            ButterworthResponse::BandPass { .. } | ButterworthResponse::BandStop { .. }
        );
        if !(1..=32).contains(&order) || (band && !order.is_multiple_of(2)) {
            return Err(FilterError);
        }
        let (low, high) = match response {
            ButterworthResponse::LowPass { cutoff_hz }
            | ButterworthResponse::HighPass { cutoff_hz } => (warp(sample_rate, cutoff_hz)?, None),
            ButterworthResponse::BandPass { low_hz, high_hz }
            | ButterworthResponse::BandStop { low_hz, high_hz } => {
                if low_hz >= high_hz {
                    return Err(FilterError);
                }
                (
                    warp(sample_rate, low_hz)?,
                    Some(warp(sample_rate, high_hz)?),
                )
            }
        };
        let degree = if band { order / 2 } else { order };
        let bandwidth = high.map_or(0., |h| h - low);
        let center = high.map_or(0., |h| low.sqrt() * h.sqrt());
        if band
            && (!bandwidth.is_finite()
                || bandwidth <= 0.
                || !center.is_finite()
                || center <= 0.
                || !(center * center).is_finite()
                || center * center == 0.)
        {
            return Err(FilterError);
        }
        let mut result = Self {
            sections: [BiquadCoefficients::new([1., 0., 0.], [0., 0.])?; 16],
            count: 0,
            order,
            response,
        };
        let reference = match response {
            ButterworthResponse::LowPass { .. } | ButterworthResponse::BandStop { .. } => 0.,
            ButterworthResponse::HighPass { .. } => 0.5,
            ButterworthResponse::BandPass { .. } => center.atan() / PI,
        };
        let numerator = match response {
            ButterworthResponse::LowPass { .. } => [1., 2., 1.],
            ButterworthResponse::HighPass { .. } => [1., -2., 1.],
            ButterworthResponse::BandPass { .. } => [1., 0., -1.],
            ButterworthResponse::BandStop { .. } => [
                1.,
                -2. * (1. - center * center) / (1. + center * center),
                1.,
            ],
        };
        for index in 0..degree / 2 {
            let angle = PI * (2 * index + 1 + degree) as f64 / (2 * degree) as f64;
            let pole = C {
                re: angle.cos(),
                im: angle.sin(),
            };
            if band {
                let q = if matches!(response, ButterworthResponse::BandPass { .. }) {
                    pole.scale(bandwidth)
                } else {
                    C::real(bandwidth).div(pole)
                };
                let roots = quadratic(q, center * center)?;
                for s in roots {
                    let z = digital(s)?;
                    result.push(numerator, [-2. * z.re, z.norm_squared()], reference)?;
                }
            } else {
                let s = if matches!(response, ButterworthResponse::LowPass { .. }) {
                    pole.scale(low)
                } else {
                    C::real(low).div(pole)
                };
                let z = digital(s)?;
                result.push(numerator, [-2. * z.re, z.norm_squared()], reference)?;
            }
        }
        if !degree.is_multiple_of(2) {
            if band {
                // Exact real prototype pole: no sin(pi) rounding in the real-root branch.
                let roots = quadratic(C::real(-bandwidth), center * center)?;
                let a = digital(roots[0])?;
                let b = digital(roots[1])?;
                let product = a.mul(b);
                result.push(numerator, [-(a.re + b.re), product.re], reference)?;
            } else {
                let z = digital(C::real(-low))?;
                let sign = if matches!(response, ButterworthResponse::LowPass { .. }) {
                    1.
                } else {
                    -1.
                };
                result.push([1., sign, 0.], [-z.re, 0.], reference)?;
            }
        }
        Ok(result)
    }
    fn push(&mut self, b: [f64; 3], a: [f64; 2], reference: f64) -> Result<(), FilterError> {
        let raw = BiquadCoefficients::new(b, a)?;
        let magnitude = raw.magnitude_at(reference)?;
        if !magnitude.is_finite() || magnitude <= 0. {
            return Err(FilterError);
        }
        let normalized = b.map(|v| v / magnitude);
        if normalized[0] == 0. {
            return Err(FilterError);
        }
        self.sections[self.count] = BiquadCoefficients::new(normalized, a)?;
        self.count += 1;
        Ok(())
    }
    /// Select an actual low-pass cutoff and order from finite constraints
    /// `0 < pass_hz < stop_hz < rate/2`, `0 < pass_loss_db < stop_loss_db <= 300`.
    /// Start at the analytic minimum order, choose the geometric midpoint of
    /// its admissible *warped* cutoff interval and verify the rounded sections
    /// against both strict amplitude bounds. Retry higher orders through 32 if
    /// that midpoint is unrepresentable or fails; otherwise return an error.
    /// This is a verified midpoint policy, not a search of every f64 cutoff.
    pub fn lowpass_spec(
        sample_rate: f64,
        pass_hz: f64,
        stop_hz: f64,
        pass_loss_db: f64,
        stop_loss_db: f64,
    ) -> Result<Self, FilterError> {
        Self::spec(
            false,
            sample_rate,
            pass_hz,
            stop_hz,
            pass_loss_db,
            stop_loss_db,
        )
    }
    /// High-pass equivalent of [`Self::lowpass_spec`], with `stop_hz < pass_hz`.
    /// Returned [`Self::response`] and [`Self::order`] report the actual result.
    pub fn highpass_spec(
        sample_rate: f64,
        pass_hz: f64,
        stop_hz: f64,
        pass_loss_db: f64,
        stop_loss_db: f64,
    ) -> Result<Self, FilterError> {
        Self::spec(
            true,
            sample_rate,
            pass_hz,
            stop_hz,
            pass_loss_db,
            stop_loss_db,
        )
    }
    fn spec(
        high: bool,
        rate: f64,
        pass: f64,
        stop: f64,
        rp: f64,
        atten: f64,
    ) -> Result<Self, FilterError> {
        if !rp.is_finite()
            || !atten.is_finite()
            || rp <= 0.
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
        if !ep.is_finite()
            || !es.is_finite()
            || ep <= 0.
            || es <= ep
            || !ratio.is_finite()
            || ratio <= 1.
        {
            return Err(FilterError);
        }
        let minimum = ((es.ln() - ep.ln()) / (2. * ratio.ln())).ceil();
        if !minimum.is_finite() || !(1.0..=32.0).contains(&minimum) {
            return Err(FilterError);
        }
        let pass_bound = 10f64.powf(-rp / 20.);
        let stop_bound = 10f64.powf(-atten / 20.);
        for order in minimum as usize..=32 {
            let p = ep.powf(1. / (2 * order) as f64);
            let s = es.powf(1. / (2 * order) as f64);
            let (lo, hi) = if high {
                (ws * s, wp * p)
            } else {
                (wp / p, ws / s)
            };
            if !lo.is_finite() || !hi.is_finite() || lo <= 0. || lo > hi {
                continue;
            }
            let cutoff_hz = (lo.sqrt() * hi.sqrt()).atan() / PI * rate;
            let response = if high {
                ButterworthResponse::HighPass { cutoff_hz }
            } else {
                ButterworthResponse::LowPass { cutoff_hz }
            };
            if let Ok(design) = Self::new(response, rate, order)
                && design.magnitude_at(pass / rate)? >= pass_bound
                && design.magnitude_at(stop / rate)? <= stop_bound
            {
                return Ok(design);
            }
        }
        Err(FilterError)
    }
    /// Selected response, including the actual cutoff chosen by specification.
    pub fn response(&self) -> ButterworthResponse {
        self.response
    }
    /// Resulting digital order (not the band prototype degree).
    pub fn order(&self) -> usize {
        self.order
    }
    /// Active normalized sections in deterministic processing order.
    pub fn stages(&self) -> &[BiquadCoefficients] {
        &self.sections[..self.count]
    }
    /// Linear amplitude at finite normalized frequency 0..=0.5, on CONTROL.
    pub fn magnitude_at(&self, cycles_per_sample: f64) -> Result<f64, FilterError> {
        if !cycles_per_sample.is_finite() || !(0.0..=0.5).contains(&cycles_per_sample) {
            return Err(FilterError);
        }
        let mut magnitude = 1.;
        for stage in self.stages() {
            magnitude *= stage.magnitude_at(cycles_per_sample)?;
        }
        if !magnitude.is_finite() {
            return Err(FilterError);
        }
        Ok(magnitude)
    }
}

/// One-channel allocation-free processing using at most 16 fixed sections.
/// Nonfinite audio propagates. There is no transition smoothing, anti-denormal,
/// headroom or real-time deadline guarantee. Each channel needs its own owner.
///
/// ```
/// use ravet_signal::{ButterworthDesign, ButterworthFilter, ButterworthResponse};
/// let design = ButterworthDesign::new(
///     ButterworthResponse::LowPass { cutoff_hz: 4000. }, 48000., 7).unwrap();
/// let mut filter = ButterworthFilter::new(design);
/// let output = filter.process_sample(1.);
/// assert!(output.is_finite());
/// ```
pub struct ButterworthFilter {
    stages: [Biquad; 16],
    count: usize,
}
impl ButterworthFilter {
    /// Start with zero histories and the selected design.
    pub fn new(design: ButterworthDesign) -> Self {
        Self {
            stages: design.sections.map(Biquad::new),
            count: design.count,
        }
    }
    /// Replace the complete selected-order design and reset *all* histories.
    /// This policy differs from existing const-generic cascade replacement,
    /// which retains histories. Prepare the design on CONTROL first.
    pub fn set_design(&mut self, design: ButterworthDesign) {
        *self = Self::new(design);
    }
    /// Process one sample; finite stable coefficients do not sanitize audio.
    pub fn process_sample(&mut self, mut input: f64) -> f64 {
        for stage in &mut self.stages[..self.count] {
            input = stage.process_sample(input);
        }
        input
    }
    /// Process an arbitrary block, retaining history between blocks.
    pub fn process_in_place(&mut self, samples: &mut [f64]) {
        for sample in samples {
            *sample = self.process_sample(*sample);
        }
    }
    /// Clear every history without allocation.
    pub fn reset(&mut self) {
        for stage in &mut self.stages {
            stage.reset();
        }
    }
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
