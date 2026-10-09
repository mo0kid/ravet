//! Phase-continuous oscillators with explicit waveform quality.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Waveform {
    Sine,
    Saw,
    Square,
    Triangle,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OscillatorError;
/// One-channel f64 oscillator. Sine uses per-sample trig; saw/square use polyBLEP
/// edge correction. Triangle is a naive phase waveform and can alias. There is
/// no claim of alias-free synthesis or a hard callback deadline.
#[derive(Clone, Debug)]
pub struct Oscillator {
    phase: Phase,
    waveform: Waveform,
}
impl Oscillator {
    /// Finite positive sample rate; frequency is 0 <= f < Nyquist.
    pub fn new(rate: f64, frequency: f64, waveform: Waveform) -> Result<Self, OscillatorError> {
        Ok(Self {
            phase: Phase::new(rate, frequency)?,
            waveform,
        })
    }
    /// Reject before mutation; frequency changes retain current phase.
    pub fn set_frequency(&mut self, frequency: f64) -> Result<(), OscillatorError> {
        self.phase.set_frequency(frequency)
    }
    pub fn phase(&self) -> f64 {
        self.phase.value
    }
    /// Select a waveform without resetting phase or changing frequency.
    /// The next sample uses the new shape; this does not crossfade the change.
    pub fn set_waveform(&mut self, waveform: Waveform) {
        self.waveform = waveform;
    }
    pub fn reset(&mut self) {
        self.phase.value = 0.;
    }
    /// Emit at current phase, then advance. No allocation or deallocation.
    pub fn process_sample(&mut self) -> f64 {
        let p = self.phase.value;
        let sample = match self.waveform {
            Waveform::Sine => (std::f64::consts::TAU * p).sin(),
            Waveform::Saw => 2. * p - 1. - blep(p, self.phase.step),
            Waveform::Square => {
                (if p < 0.5 { 1. } else { -1. }) + blep(p, self.phase.step)
                    - blep((p + 0.5).fract(), self.phase.step)
            }
            Waveform::Triangle => 1. - 4. * (p - 0.5).abs(),
        };
        self.phase.advance();
        sample
    }
    pub fn process(&mut self, output: &mut [f64]) {
        for sample in output {
            *sample = self.process_sample();
        }
    }
}
fn blep(phase: f64, step: f64) -> f64 {
    if step == 0. {
        return 0.;
    }
    if phase < step {
        let t = phase / step;
        2. * t - t * t - 1.
    } else if phase > 1. - step {
        let t = (phase - 1.) / step;
        t * t + 2. * t + 1.
    } else {
        0.
    }
}

#[derive(Clone, Debug)]
struct Phase {
    rate: f64,
    step: f64,
    value: f64,
}
impl Phase {
    fn new(rate: f64, frequency: f64) -> Result<Self, OscillatorError> {
        if !rate.is_finite() || rate <= 0. {
            return Err(OscillatorError);
        }
        let mut phase = Self {
            rate,
            step: 0.,
            value: 0.,
        };
        phase.set_frequency(frequency)?;
        Ok(phase)
    }
    fn set_frequency(&mut self, frequency: f64) -> Result<(), OscillatorError> {
        if !frequency.is_finite() || frequency < 0. || frequency >= self.rate * 0.5 {
            return Err(OscillatorError);
        }
        self.step = frequency / self.rate;
        Ok(())
    }
    fn advance(&mut self) {
        self.value += self.step;
        if self.value >= 1. {
            self.value -= 1.;
        }
    }
}

/// Prepared one-channel f64 periodic wavetable oscillator. Owns 2..=65536 finite
/// table samples at uniformly spaced phases i/N, without duplicating the endpoint.
/// Linear interpolation wraps from the final sample to sample zero. Table content
/// owns waveform/DC/level policy; interpolation is not bandlimited or alias-free.
/// Frequency changes preserve phase, with no smoothing. No callback deadline,
/// denormal, headroom or nonfinite output policy is inferred. Large finite table
/// amplitudes may overflow interpolation; prepare/clone/drop are control work.
#[derive(Clone, Debug)]
pub struct WavetableOscillator {
    table: Box<[f64]>,
    phase: Phase,
}
impl WavetableOscillator {
    /// Copy a complete validated cycle outside processing. Reject invalid rate,
    /// frequency, capacity or nonfinite samples before allocating the owned table.
    /// Rate is finite positive; frequency is finite 0 <= f < Nyquist.
    pub fn new(rate: f64, frequency: f64, table: &[f64]) -> Result<Self, OscillatorError> {
        let phase = Phase::new(rate, frequency)?;
        if !(2..=65536).contains(&table.len()) || table.iter().any(|sample| !sample.is_finite()) {
            return Err(OscillatorError);
        }
        Ok(Self {
            table: table.into(),
            phase,
        })
    }
    pub fn table(&self) -> &[f64] {
        &self.table
    }
    pub fn phase(&self) -> f64 {
        self.phase.value
    }
    /// Reject before mutation; retain current phase and table.
    pub fn set_frequency(&mut self, frequency: f64) -> Result<(), OscillatorError> {
        self.phase.set_frequency(frequency)
    }
    /// Return to phase zero while preserving frequency and the prepared table.
    pub fn reset(&mut self) {
        self.phase.value = 0.;
    }
    /// Emit at the current phase then advance; no allocation or transcendental work.
    pub fn process_sample(&mut self) -> f64 {
        let position = self.phase.value * self.table.len() as f64;
        let index = (position as usize).min(self.table.len() - 1);
        let fraction = position - index as f64;
        let next = if index + 1 == self.table.len() {
            0
        } else {
            index + 1
        };
        let output = self.table[index] * (1. - fraction) + self.table[next] * fraction;
        self.phase.advance();
        output
    }
    pub fn process(&mut self, output: &mut [f64]) {
        for sample in output {
            *sample = self.process_sample();
        }
    }
}
