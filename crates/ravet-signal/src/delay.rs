//! Prepared delay storage; writes, reads, advancement, and clearing do not allocate.

/// A circular double-precision delay with linear fractional interpolation.
///
/// The cursor names the next write slot. Read before writing for a feedback
/// delay; write before reading for a zero-delay feedforward path. Advance once
/// after each sample. Delay zero reads the cursor slot, which contains the
/// oldest sample until written. Finite delays clamp to `0..=length-1`;
/// nonfinite delays read zero without changing storage.
#[derive(Debug)]
pub struct DelayLine {
    samples: Vec<f64>,
    cursor: usize,
}

/// Zero-length storage cannot represent a delay line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmptyDelay;

impl std::fmt::Display for EmptyDelay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("delay storage must contain at least one sample")
    }
}
impl std::error::Error for EmptyDelay {}

impl DelayLine {
    /// Allocate zeroed storage outside the processing callback.
    pub fn new(length: usize) -> Result<Self, EmptyDelay> {
        if length == 0 {
            return Err(EmptyDelay);
        }
        Ok(Self {
            samples: vec![0.0; length],
            cursor: 0,
        })
    }

    #[inline]
    pub fn read(&self, delay: f64) -> f64 {
        if !delay.is_finite() {
            return 0.0;
        }
        let delay = delay.clamp(0.0, (self.samples.len() - 1) as f64);
        let whole = delay as usize;
        let fraction = delay - whole as f64;
        let index = if self.cursor >= whole {
            self.cursor - whole
        } else {
            self.samples.len() - (whole - self.cursor)
        };
        let previous = if index == 0 {
            self.samples.len() - 1
        } else {
            index - 1
        };
        self.samples[previous] * fraction + self.samples[index] * (1.0 - fraction)
    }

    /// Read a whole-sample delay without fractional interpolation.
    /// Delays beyond storage clamp to `length-1`; zero reads the cursor slot.
    #[inline]
    pub fn read_integer(&self, delay: usize) -> f64 {
        let delay = delay.min(self.samples.len() - 1);
        let index = if self.cursor >= delay {
            self.cursor - delay
        } else {
            self.samples.len() - (delay - self.cursor)
        };
        self.samples[index]
    }

    #[inline]
    pub fn write(&mut self, sample: f64) {
        self.samples[self.cursor] = sample;
    }

    #[inline]
    pub fn advance(&mut self) {
        self.cursor += 1;
        if self.cursor == self.samples.len() {
            self.cursor = 0;
        }
    }

    /// Clear history and return the cursor to zero, retaining its allocation.
    pub fn reset(&mut self) {
        self.samples.fill(0.0);
        self.cursor = 0;
    }
}

/// Feedback delay diffuser with a prepared integer delay.
///
/// Coefficients are supplied per sample; callers must keep them finite and
/// strictly between -1 and 1 for a stable fixed-coefficient filter. The
/// recurrence is `y = delayed - g*x; write = x + g*delayed`. This feedback
/// diffuser does not have unity allpass magnitude.
/// AArch64 and FMA-enabled targets fuse each multiply-add; other targets use
/// separate arithmetic, so final rounding can differ by target.
#[derive(Debug)]
pub struct DelayDiffuser {
    delay: DelayLine,
}

impl DelayDiffuser {
    pub fn new(length: usize) -> Result<Self, EmptyDelay> {
        Ok(Self {
            delay: DelayLine::new(length)?,
        })
    }

    #[inline]
    pub fn process(&mut self, input: f64, coefficient: f64) -> f64 {
        let delayed = self.delay.read_integer(0);
        #[cfg(any(target_arch = "aarch64", target_feature = "fma"))]
        let (output, written) = (
            coefficient.mul_add(-input, delayed),
            delayed.mul_add(coefficient, input),
        );
        #[cfg(not(any(target_arch = "aarch64", target_feature = "fma")))]
        let (output, written) = (
            -input * coefficient + delayed,
            input + delayed * coefficient,
        );
        self.delay.write(written);
        self.delay.advance();
        output
    }

    pub fn reset(&mut self) {
        self.delay.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractional_read_wrap_and_reset_follow_the_write_cursor() {
        assert!(DelayLine::new(0).is_err());
        let mut delay = DelayLine::new(4).unwrap();
        for value in [1.0, 2.0, 3.0, 4.0] {
            delay.write(value);
            delay.advance();
        }
        assert_eq!(delay.read(0.0), 1.0);
        assert_eq!(delay.read(1.0), 4.0);
        assert_eq!(delay.read(1.5), 3.5);
        assert_eq!(delay.read(3.0), 2.0);
        assert_eq!(delay.read(f64::NAN), 0.0);
        assert_eq!(delay.read(1e-300), 1.0);
        delay.write(5.0);
        assert_eq!(delay.read(0.0), 5.0);
        delay.reset();
        assert_eq!(delay.read(1.5), 0.0);
    }

    #[test]
    fn diffuser_impulse_energy_and_reset_follow_the_feedback_recurrence() {
        let mut diffuser = DelayDiffuser::new(3).unwrap();
        let render = |d: &mut DelayDiffuser| {
            (0..128)
                .map(|i| d.process(if i == 0 { 1.0 } else { 0.0 }, 0.5))
                .collect::<Vec<_>>()
        };
        let response = render(&mut diffuser);
        assert_eq!(response[0], -0.5);
        assert_eq!(response[3], 1.0);
        // The feedback diffuser's transfer-function numerator is
        // -g + (1+g²)z^-D, rather than a unity-magnitude canonical allpass.
        let energy: f64 = response.iter().map(|v| v * v).sum();
        assert!((energy - (0.25 + 1.0 / 0.75)).abs() < 1e-12);
        diffuser.reset();
        assert_eq!(render(&mut diffuser), response);
    }

    #[test]
    fn non_power_of_two_storage_preserves_reads_across_repeated_wraps() {
        for length in [3, 5, 17] {
            let mut delay = DelayLine::new(length).unwrap();
            for sample in 1..=length * 8 {
                delay.write(sample as f64);
                delay.advance();
                if sample < length {
                    continue;
                }
                assert_eq!(delay.read(0.0), (sample - length + 1) as f64);
                assert_eq!(delay.read(-1.0), delay.read(0.0));
                assert_eq!(delay.read(1.0), sample as f64);
                for distance in 0..=length + 2 {
                    assert_eq!(delay.read_integer(distance), delay.read(distance as f64));
                }
                assert_eq!(delay.read_integer(usize::MAX), delay.read(f64::MAX));
                assert_eq!(delay.read(1.25), sample as f64 - 0.25);
                assert_eq!(delay.read(f64::MAX), (sample - length + 2) as f64);
                assert_eq!(delay.read(f64::INFINITY), 0.0);
            }
        }
    }

    #[test]
    fn fractional_distances_interpolate_the_wrapped_history() {
        for length in [1, 3, 5, 17] {
            let mut delay = DelayLine::new(length).unwrap();
            for sample in 1..=length * 8 {
                delay.write(sample as f64);
                delay.advance();
                if sample < length {
                    continue;
                }
                for step in 0..=length * 100 {
                    let distance = (step as f64 / 100.0).min((length - 1) as f64);
                    let expected = if distance < 1.0 {
                        (sample - length + 1) as f64 * (1.0 - distance) + sample as f64 * distance
                    } else {
                        sample as f64 - (distance - 1.0)
                    };
                    assert!((delay.read(distance) - expected).abs() < 1e-12);
                }
            }
        }
    }
}
