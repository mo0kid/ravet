//! Allocation-free sample peak and RMS accumulation in physical amplitude.

/// Finite meter results; nonfinite samples are counted as silence and flagged.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeterReading {
    pub peak: f32,
    pub rms: f32,
    pub samples: u64,
    pub nonfinite: bool,
}

/// Caller-selected measurement window; reset starts a new window.
/// No smoothing, decay, true-peak oversampling, or loudness weighting.
#[derive(Clone, Copy, Debug, Default)]
pub struct PeakRmsMeter {
    peak: f32,
    squared: f64,
    samples: u64,
    nonfinite: bool,
}
impl PeakRmsMeter {
    /// Accumulate one sample. A full u64 window ignores further samples.
    /// f64 accumulation accommodates squared finite f32 values without overflow.
    pub fn push(&mut self, sample: f32) {
        if self.samples == u64::MAX {
            return;
        }
        self.samples += 1;
        if !sample.is_finite() {
            self.nonfinite = true;
            return;
        }
        self.peak = self.peak.max(sample.abs());
        self.squared += (sample as f64) * (sample as f64);
    }
    pub fn reading(&self) -> MeterReading {
        MeterReading {
            peak: self.peak,
            rms: if self.samples == 0 {
                0.0
            } else {
                (self.squared / self.samples as f64)
                    .sqrt()
                    .min(f32::MAX as f64) as f32
            },
            samples: self.samples,
            nonfinite: self.nonfinite,
        }
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn peak_rms_extremes_nonfinite_and_reset() {
        let mut meter = PeakRmsMeter::default();
        assert_eq!(meter.reading(), MeterReading::default());
        for value in [1.0, -1.0, 0.0, 0.0] {
            meter.push(value);
        }
        let reading = meter.reading();
        assert_eq!(reading.peak, 1.0);
        assert!((reading.rms - 0.5_f32.sqrt()).abs() < 1e-6);
        assert_eq!(reading.samples, 4);
        meter.reset();
        meter.push(f32::MAX);
        assert_eq!(meter.reading().rms, f32::MAX);
        meter.reset();
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 2.0] {
            meter.push(value);
        }
        assert_eq!(
            meter.reading(),
            MeterReading {
                peak: 2.0,
                rms: 1.0,
                samples: 4,
                nonfinite: true
            }
        );
        meter.reset();
        assert_eq!(meter.reading(), MeterReading::default());
    }
}
