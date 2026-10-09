//! Native f64 peak/RMS accumulation with scaled squares to avoid overflow.
/// Snapshot of native f64 audio levels; nonfinite samples count as silence.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeterReading64 {
    pub peak: f64,
    pub rms: f64,
    pub samples: u64,
    pub nonfinite: bool,
}
impl MeterReading64 {
    /// Explicit bounded telemetry conversion; finite levels beyond f32::MAX
    /// saturate, rather than inventing nonfinite input. Audio is not converted.
    pub fn telemetry_f32(self) -> crate::MeterReading {
        crate::MeterReading {
            peak: self.peak.min(f32::MAX as f64) as f32,
            rms: self.rms.min(f32::MAX as f64) as f32,
            samples: self.samples,
            nonfinite: self.nonfinite,
        }
    }
}
/// Prepared fixed storage, without heap/locks/I/O. RMS uses scaled sum-of-squares;
/// finite f64 maxima do not overflow and subnormal levels remain representable.
/// A full u64 sample window ignores further samples. No audio sanitization.
#[derive(Clone, Copy, Debug, Default)]
pub struct PeakRmsMeter64 {
    scale: f64,
    squares: f64,
    samples: u64,
    nonfinite: bool,
}
impl PeakRmsMeter64 {
    pub fn push(&mut self, sample: f64) {
        if self.samples == u64::MAX {
            return;
        }
        self.samples += 1;
        if !sample.is_finite() {
            self.nonfinite = true;
            return;
        }
        let magnitude = sample.abs();
        if magnitude == 0. {
            return;
        }
        if magnitude > self.scale {
            let ratio = self.scale / magnitude;
            self.squares = 1. + self.squares * ratio * ratio;
            self.scale = magnitude;
        } else {
            let ratio = magnitude / self.scale;
            self.squares += ratio * ratio;
        }
    }
    pub fn reading(&self) -> MeterReading64 {
        MeterReading64 {
            peak: self.scale,
            rms: if self.samples == 0 {
                0.
            } else {
                self.scale * (self.squares / self.samples as f64).min(1.).sqrt()
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
    fn extrema_subnormals_nonfinite_and_telemetry_preserve_native_meaning() {
        let mut meter = PeakRmsMeter64::default();
        assert_eq!(meter.reading(), MeterReading64::default());
        for sample in [f64::MAX, -f64::MAX] {
            meter.push(sample);
        }
        assert_eq!(meter.reading().rms, f64::MAX);
        assert_eq!(meter.reading().telemetry_f32().rms, f32::MAX);
        assert!(!meter.reading().telemetry_f32().nonfinite);
        meter.reset();
        let tiny = f64::from_bits(1);
        meter.push(tiny);
        assert_eq!(meter.reading().rms, tiny);
        meter.reset();
        for sample in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 2.] {
            meter.push(sample);
        }
        assert_eq!(
            meter.reading(),
            MeterReading64 {
                peak: 2.,
                rms: 1.,
                samples: 4,
                nonfinite: true
            }
        );
        meter.reset();
        for sample in [1e300, -1e300, 0., 0.] {
            meter.push(sample);
        }
        assert!((meter.reading().rms / 1e300 - 0.5_f64.sqrt()).abs() < 1e-14);
        let before = meter.reading();
        meter.samples = u64::MAX;
        meter.push(f64::MAX);
        assert_eq!(meter.reading().peak, before.peak);
    }
}
