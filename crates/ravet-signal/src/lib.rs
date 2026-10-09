//! Audio processing building blocks for RAVET.
//!
//! [`LinearSmoother`] provides finite, sample-counted parameter transitions.
//! It is independent of processors, plug-in formats, devices, and editors.
//! [`DelayLine`] and [`DelayDiffuser`] provide prepared double-precision
//! storage and feedback diffusion. [`HalfBand2x`] provides a fixed streaming
//! oversampling path for effects.
//! [`PeakRmsMeter`] measures caller-selected sample peak/RMS windows.
//! [`PhaseRotation`] advances caller-owned quadrature oscillator banks.

mod delay;
mod oversampling;
mod rotation;
mod smoothing;
pub use delay::{DelayDiffuser, DelayLine, EmptyDelay};
pub use oversampling::{HalfBand2x, Oversampler, OversamplingFactor};
pub use rotation::PhaseRotation;
pub use smoothing::{LinearSmoother, MultiplicativeSmoother, SmoothingError, smoothing_samples};

mod meter;
pub use meter::{MeterReading, PeakRmsMeter};

mod convolution;
mod fft;
pub use convolution::{ConvolutionError, FirConvolver};
mod partitioned;
pub use fft::{Complex, FftError, FftPlan};
pub use partitioned::{PartitionedConvolutionError, PartitionedConvolver};

mod filter;
pub use filter::{Biquad, BiquadCoefficients, FilterError, FilterKind, ShelfKind};

mod envelope;
pub use envelope::{Adsr, AdsrConfig, EnvelopeError, EnvelopePhase};

mod oscillator;
pub use oscillator::{Oscillator, OscillatorError, Waveform, WavetableOscillator};
mod dynamics;
pub use dynamics::{
    Compressor, CompressorConfig, DynamicsError, EnvelopeFollower, NoiseGate, NoiseGateConfig,
    PeakLimiter,
};

mod resampling;
pub use resampling::{ResamplingError, ResamplingProgress, SincResampler};

mod meter64;
pub use meter64::{MeterReading64, PeakRmsMeter64};

mod cascade;
pub use cascade::{BiquadCascade, ButterworthKind, CascadeCoefficients};

mod crossover;
pub use crossover::{
    CrossoverLengthError, CrossoverOutputs, LinkwitzRileyCoefficients, LinkwitzRileyCrossover,
};

mod state_variable;
pub use state_variable::{StateVariableCoefficients, StateVariableFilter, StateVariableOutputs};

mod reverb;
pub use reverb::{ReverbConfig, SchroederReverb, StereoReverbConfig, StereoSchroederReverb};

mod real_fft;
pub use real_fft::{RealFftError, RealFftPlan};
mod window;
pub use window::{WindowConvention, WindowError, WindowKind, WindowPlan};

mod fifo_clock;
pub use fifo_clock::{FifoClockConfig, FifoClockController, FifoClockError};

mod chorus;
pub use chorus::{ChorusControls, ChorusError, StereoChorus};
