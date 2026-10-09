//! Fixed-storage second-order cascades and even-order Butterworth designs.
use crate::{Biquad, BiquadCoefficients, FilterError, FilterKind};
/// Passband used by an even-order Butterworth cascade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButterworthKind {
    LowPass,
    HighPass,
}
/// Validated 1..16 second-order sections; immutable designs belong on control
/// threads. Explicit arbitrary stages need not be Butterworth or unity gain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CascadeCoefficients<const N: usize> {
    stages: [BiquadCoefficients; N],
}
impl<const N: usize> CascadeCoefficients<N> {
    pub fn new(stages: [BiquadCoefficients; N]) -> Result<Self, FilterError> {
        if !(1..=16).contains(&N) {
            return Err(FilterError);
        }
        Ok(Self { stages })
    }
    /// Prepare an order-2N low/high pass, with N in 1..16 (orders 2..32).
    /// Rate must be finite/positive and cutoff strictly between DC/Nyquist.
    /// Each pole pair uses its Butterworth Q and the existing bilinear biquad
    /// design. Stages run from low to high Q; cutoff magnitude is 1/sqrt(2).
    /// Unrepresentable stability margins reject before returning a design.
    pub fn butterworth(kind: ButterworthKind, rate: f64, cutoff: f64) -> Result<Self, FilterError> {
        if !(1..=16).contains(&N) {
            return Err(FilterError);
        }
        let identity = BiquadCoefficients::new([1., 0., 0.], [0., 0.])?;
        let mut stages = [identity; N];
        let filter = match kind {
            ButterworthKind::LowPass => FilterKind::LowPass,
            ButterworthKind::HighPass => FilterKind::HighPass,
        };
        for (index, stage) in stages.iter_mut().enumerate() {
            let angle = std::f64::consts::PI * ((2 * index + 1) as f64) / (4. * N as f64);
            let q = 1. / (2. * angle.cos());
            *stage = BiquadCoefficients::design(filter, rate, cutoff, q)?;
        }
        Self::new(stages)
    }
    pub fn stages(&self) -> &[BiquadCoefficients; N] {
        &self.stages
    }
    /// Control-thread linear response at finite normalized frequency 0..=0.5.
    /// Product/headroom overflow is not sanitized for arbitrary stage designs.
    pub fn magnitude_at(&self, frequency: f64) -> Result<f64, FilterError> {
        self.stages
            .iter()
            .try_fold(1., |gain, stage| Ok(gain * stage.magnitude_at(frequency)?))
    }
}
/// Prepared one-channel f64 cascade; sample/block/reset/replacement use fixed
/// storage. Coefficient replacement preserves every stage's delay state without
/// smoothing. No denormal/nonfinite/headroom/deadline policy is imposed.
#[derive(Clone, Debug)]
pub struct BiquadCascade<const N: usize> {
    stages: [Biquad; N],
}
impl<const N: usize> BiquadCascade<N> {
    pub fn new(coefficients: CascadeCoefficients<N>) -> Self {
        Self {
            stages: coefficients.stages.map(Biquad::new),
        }
    }
    pub fn set_coefficients(&mut self, coefficients: CascadeCoefficients<N>) {
        for (stage, coefficient) in self.stages.iter_mut().zip(coefficients.stages) {
            stage.set_coefficients(coefficient);
        }
    }
    pub fn reset(&mut self) {
        for stage in &mut self.stages {
            stage.reset();
        }
    }
    pub fn process_sample(&mut self, mut sample: f64) -> f64 {
        for stage in &mut self.stages {
            sample = stage.process_sample(sample);
        }
        sample
    }
    pub fn process_in_place(&mut self, samples: &mut [f64]) {
        for sample in samples {
            *sample = self.process_sample(*sample);
        }
    }
}
