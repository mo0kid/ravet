//! Audio processing building blocks for RAVET.
//!
//! [`LinearSmoother`] provides finite, sample-counted parameter transitions.
//! It is independent of processors, plug-in formats, devices, and editors.
//! [`DelayLine`] and [`DelayDiffuser`] provide prepared double-precision
//! storage and feedback diffusion. [`HalfBand2x`] provides a fixed streaming
//! oversampling path for effects.
//! [`CombFilter`] provides fixed-delay feedforward and feedback comb filtering.
//! [`PeakRmsMeter`] measures caller-selected sample peak/RMS windows.
//! [`PhaseRotation`] advances caller-owned quadrature oscillator banks.
//! [`Waveshaper`] applies custom transfer functions or explicit clipping curves.

mod delay;
mod oversampling;
mod rotation;
mod smoothing;
pub use delay::{DelayDiffuser, DelayLine, EmptyDelay};
mod comb;
pub use comb::{CombError, CombFilter, CombKind};
pub use oversampling::{HalfBand2x, Oversampler, OversamplingFactor};
pub use rotation::PhaseRotation;
pub use smoothing::{LinearSmoother, MultiplicativeSmoother, SmoothingError, smoothing_samples};

mod meter;
pub use meter::{MeterReading, PeakRmsMeter};

mod convolution;
mod fft;
pub use convolution::{ConvolutionError, FirConvolver};
mod partitioned;
pub use fft::{Complex, FftError, FftPlan, FftPreparationError};
mod fft32;
pub use fft32::{Complex32, FftPlan32};
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
pub use resampling::{
    IntegerResamplingPosition, ResamplingError, ResamplingProgress, SincResampler,
};

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

mod waveshaper;
pub use waveshaper::{Waveshaper, WaveshaperLengthError};

mod interpolation;
pub use interpolation::{InterpolationError, InterpolationKind, StreamInterpolator};

mod fractional_delay;
pub use fractional_delay::{DelayInterpolation, FractionalDelay, FractionalDelayError};

mod butterworth;
pub use butterworth::{ButterworthDesign, ButterworthFilter, ButterworthResponse};

mod fir_design;
pub use fir_design::{FirBand, FirDesign, FirDesignError, FirResponse, FirWindow, KaiserFirDesign};

mod chebyshev;
pub use chebyshev::{
    ChebyshevDesign, ChebyshevFilter, ChebyshevKind, ChebyshevResponse, ChebyshevType,
};

mod elliptic;
pub use elliptic::{EllipticDesign, EllipticFilter, EllipticResponse};

mod fir_equiripple;
pub use fir_equiripple::{
    EquirippleError, EquirippleExtremum, EquirippleFirDesign, EquirippleFirResponse,
    EquirippleOptions,
};

mod halfband_iir;
pub use halfband_iir::{
    ConfiguredHalfBand2x, ConfiguredHalfBand2x32, ConfiguredOversampler, ConfiguredOversampler32,
    HalfBandIirDesign, HalfBandIirDesign32, HalfBandIirFilter, HalfBandIirFilter32,
    HalfBandStopPeak,
};

mod ballistics;
pub use ballistics::{
    BallisticsBank, BallisticsBank32, BallisticsConfig, BallisticsConfig32, BallisticsFilter,
    BallisticsFilter32, BallisticsMode,
};

mod first_order_tpt;
pub use first_order_tpt::{
    FirstOrderTptBank, FirstOrderTptBank32, FirstOrderTptDesign, FirstOrderTptDesign32,
    FirstOrderTptFilter, FirstOrderTptFilter32, FirstOrderTptOutputs, FirstOrderTptOutputs32,
    FirstOrderTptResponse,
};

mod panning;
pub use panning::{
    PanningError, PanningGains, PanningGains32, PanningLaw, PanningNormalization, StereoPanner,
    StereoPanner32,
};

mod dry_wet;
pub use dry_wet::{
    DryWetError, DryWetGains, DryWetGains32, DryWetMixer, DryWetMixer32, DryWetMixerConfig,
};

mod phaser;
pub use phaser::{Phaser, Phaser32, PhaserConfig, PhaserControls, PhaserError};

mod ladder;
pub use ladder::{
    LadderBank, LadderBank32, LadderDesign, LadderDesign32, LadderError, LadderFilter,
    LadderFilter32, LadderMode, LadderOutputs, LadderOutputs32, LadderSolveInfo, LadderSolveInfo32,
};

mod true_peak;
pub use true_peak::{
    TruePeakBank64, TruePeakDesign, TruePeakError, TruePeakFactor, TruePeakMeter64,
    TruePeakReading64,
};

mod loudness;
pub use loudness::{
    KWeightingDesign, LoudnessChannelRole, LoudnessError, LoudnessFault, LoudnessLiveReading64,
    LoudnessProgrammeReport64, LoudnessRecordCapacities, LoudnessTailPolicy,
    PreparedLoudnessMeter64,
};

mod convolution_family;
pub use convolution_family::{
    ConvolutionBank32, ConvolutionBank64, ConvolutionInfo, ConvolutionPartitioning,
    ConvolutionPreparationError, ConvolutionProcessError, ConvolutionStageInfo,
    ConvolutionStageWork, Convolver32, Convolver64,
};

mod impulse_response;
pub use impulse_response::{
    IMPULSE_RESPONSE_MAX_CHANNELS, IMPULSE_RESPONSE_MAX_SOURCE_FRAMES, IMPULSE_RESPONSE_MAX_TAPS,
    IMPULSE_RESPONSE_MAX_WORKING_BYTES, ImpulseResponseError, ImpulseResponsePlan,
    ImpulseResponseReport, ImpulseResponseRequest, PreparedImpulseResponse, ResponseChannelMap,
    ResponseDelay, ResponseNormalization, ResponseRateConversion, ResponseRateGain, ResponseTrim,
    TrimThreshold, prepare_impulse_response,
};
