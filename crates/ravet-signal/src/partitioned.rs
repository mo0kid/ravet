//! Uniform overlap-add convolution using prepared complex FFT partitions.

use crate::{Complex, FftPlan};

/// Invalid finite impulse response or partition size, rejected before preparation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartitionedConvolutionError;

/// Prepared single-channel f64 uniform partitioned convolution.
///
/// Preparation accepts 1..=262,144 finite taps and a power-of-two partition
/// of 16..=4,096 samples. Processing and reset allocate/deallocate nothing.
/// The output is delayed by exactly one partition, independently of caller
/// block sizes. Feed zeros explicitly to drain the tail. Replacing an impulse
/// requires constructing a new convolver outside the audio callback.
///
/// Each partition boundary performs two FFTs and sums all response partitions;
/// work is bursty and there is no callback-deadline or SIMD guarantee. Nonfinite
/// audio propagates. This is uniform partitioning, without response crossfades,
/// background preparation, multichannel routing or a zero-latency head.
#[derive(Clone, Debug)]
pub struct PartitionedConvolver {
    plan: FftPlan,
    block: usize,
    taps: usize,
    impulse: Vec<Complex>,
    history: Vec<Complex>,
    work: Vec<Complex>,
    input: Vec<f64>,
    output: Vec<f64>,
    overlap: Vec<f64>,
    position: usize,
    head: usize,
}

impl PartitionedConvolver {
    /// Copy and transform a finite response on the preparation thread.
    pub fn new(impulse: &[f64], partition: usize) -> Result<Self, PartitionedConvolutionError> {
        if impulse.is_empty()
            || impulse.len() > 262_144
            || impulse.iter().any(|sample| !sample.is_finite())
            || !(16..=4_096).contains(&partition)
            || !partition.is_power_of_two()
        {
            return Err(PartitionedConvolutionError);
        }
        let size = partition * 2;
        let count = impulse.len().div_ceil(partition);
        let plan = FftPlan::new(size).expect("validated FFT size");
        let mut spectra = vec![Complex::default(); count * size];
        for (response, spectrum) in impulse
            .chunks(partition)
            .zip(spectra.chunks_exact_mut(size))
        {
            for (sample, bin) in response.iter().zip(spectrum.iter_mut()) {
                bin.re = *sample;
            }
            plan.forward(spectrum).expect("prepared spectrum length");
        }
        Ok(Self {
            plan,
            block: partition,
            taps: impulse.len(),
            impulse: spectra,
            history: vec![Complex::default(); count * size],
            work: vec![Complex::default(); size],
            input: vec![0.; partition],
            output: vec![0.; partition],
            overlap: vec![0.; partition],
            position: 0,
            head: 0,
        })
    }

    /// Exact algorithmic delay, in samples.
    pub fn latency_samples(&self) -> usize {
        self.block
    }

    /// Number of response taps, including trailing zeros supplied by the caller.
    pub fn taps(&self) -> usize {
        self.taps
    }

    /// Conservative output tail after the last input, including algorithmic delay.
    pub fn tail_samples(&self) -> usize {
        self.taps - 1 + self.block
    }

    /// Clear all input, spectrum and overlap history, retaining the response.
    pub fn reset(&mut self) {
        self.history.fill(Complex::default());
        self.work.fill(Complex::default());
        self.input.fill(0.);
        self.output.fill(0.);
        self.overlap.fill(0.);
        self.position = 0;
        self.head = 0;
    }

    /// Consume one sample and emit one delayed output sample.
    pub fn process_sample(&mut self, sample: f64) -> f64 {
        let output = self.output[self.position];
        self.input[self.position] = sample;
        self.position += 1;
        if self.position == self.block {
            self.transform_block();
            self.position = 0;
        }
        output
    }

    /// Process any caller partition, including an empty slice, in place.
    pub fn process_in_place(&mut self, samples: &mut [f64]) {
        for sample in samples {
            *sample = self.process_sample(*sample);
        }
    }

    fn transform_block(&mut self) {
        let size = self.block * 2;
        let count = self.history.len() / size;
        let current = &mut self.history[self.head * size..(self.head + 1) * size];
        current.fill(Complex::default());
        for (bin, sample) in current.iter_mut().zip(&self.input) {
            bin.re = *sample;
        }
        self.plan
            .forward(current)
            .expect("prepared spectrum length");
        self.work.fill(Complex::default());
        for partition in 0..count {
            let index = (self.head + count - partition) % count;
            let input = &self.history[index * size..(index + 1) * size];
            let response = &self.impulse[partition * size..(partition + 1) * size];
            for ((sum, x), h) in self.work.iter_mut().zip(input).zip(response) {
                sum.re += x.re * h.re - x.im * h.im;
                sum.im += x.re * h.im + x.im * h.re;
            }
        }
        self.plan
            .inverse(&mut self.work)
            .expect("prepared spectrum length");
        for index in 0..self.block {
            self.output[index] = self.work[index].re + self.overlap[index];
            self.overlap[index] = self.work[index + self.block].re;
        }
        self.head = (self.head + 1) % count;
    }
}

impl std::fmt::Display for PartitionedConvolutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("response must contain 1..=262,144 finite taps; partition must be a power of two in 16..=4,096")
    }
}
impl std::error::Error for PartitionedConvolutionError {}
