//! Prepared direct convolution, appropriate for short finite impulse responses.

/// Invalid preparation rejects empty, over-limit or nonfinite impulse responses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConvolutionError;

/// Single-channel f64 FIR convolution with zero algorithmic block latency.
/// Preparation copies 1..=65,536 finite taps and allocates a history ring.
/// Processing/reset allocate nothing. Cost is O(taps) per sample; this is not
/// partitioned FFT convolution or a callback-deadline guarantee. Nonfinite audio
/// propagates. Replacing a response requires control-thread re-preparation.
#[derive(Clone, Debug)]
pub struct FirConvolver {
    impulse: Vec<f64>,
    history: Vec<f64>,
    cursor: usize,
}
impl FirConvolver {
    pub fn new(impulse: &[f64]) -> Result<Self, ConvolutionError> {
        if impulse.is_empty()
            || impulse.len() > 65_536
            || impulse.iter().any(|tap| !tap.is_finite())
        {
            return Err(ConvolutionError);
        }
        Ok(Self {
            impulse: impulse.to_vec(),
            history: vec![0.; impulse.len()],
            cursor: 0,
        })
    }
    pub fn taps(&self) -> usize {
        self.impulse.len()
    }
    pub fn tail_samples(&self) -> usize {
        self.impulse.len() - 1
    }
    pub fn reset(&mut self) {
        self.history.fill(0.);
        self.cursor = 0;
    }
    pub fn process_sample(&mut self, sample: f64) -> f64 {
        self.history[self.cursor] = sample;
        let mut output = 0.;
        let mut read = self.cursor;
        for tap in &self.impulse {
            output += tap * self.history[read];
            read = if read == 0 {
                self.history.len() - 1
            } else {
                read - 1
            };
        }
        self.cursor = (self.cursor + 1) % self.history.len();
        output
    }
    pub fn process_in_place(&mut self, samples: &mut [f64]) {
        for sample in samples {
            *sample = self.process_sample(*sample);
        }
    }
}
impl std::fmt::Display for ConvolutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("impulse must contain 1..=65,536 finite taps")
    }
}
impl std::error::Error for ConvolutionError {}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finite_convolution_matches_independent_sum_and_block_partitions() {
        let impulse = [0.5, -0.25, 0.125];
        let input = [1., 2., 3., 4., 0., 0.];
        let expected: Vec<f64> = (0..input.len())
            .map(|n| {
                impulse
                    .iter()
                    .enumerate()
                    .filter(|(k, _)| *k <= n)
                    .map(|(k, h)| h * input[n - k])
                    .sum()
            })
            .collect();
        let mut convolver = FirConvolver::new(&impulse).unwrap();
        let mut output = input;
        for chunk in output.chunks_mut(2) {
            convolver.process_in_place(chunk);
        }
        assert_eq!(output.as_slice(), expected);
        assert_eq!(convolver.tail_samples(), 2);
        convolver.reset();
        assert_eq!(
            (0..3)
                .map(|_| convolver.process_sample(0.))
                .collect::<Vec<_>>(),
            [0.; 3]
        );
        for impulse in [&[][..], &[f64::NAN], &[f64::INFINITY]] {
            assert!(FirConvolver::new(impulse).is_err());
        }
    }
}
