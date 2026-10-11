//! Test-support Foundation `Processor` adapters for prepared convolution banks.
//! Not public API: `ravet-signal` has no runtime dependency on Foundation. Each
//! test binary includes this file with `#[path]` and uses a subset of it.
#![allow(dead_code)]
use ravet_foundation::{
    PrepareError, ProcessBlock, ProcessConfig, ProcessError, ProcessStatus, Processor, Tail,
};
use ravet_signal::{ConvolutionBank32, ConvolutionBank64};
macro_rules! processor {
    ($name:ident,$failure:ident,$bank:ident,$sample:ty) => {
        /// Exact prepared bank returned untouched by constructor refusal.
        pub struct $failure {
            bank: $bank,
            error: PrepareError,
        }
        impl $failure {
            /// Foundation preparation refusal that caused the bank to be returned.
            pub fn error(&self) -> PrepareError {
                self.error
            }
            /// Borrow the untouched bank.
            pub fn bank(&self) -> &$bank {
                &self.bank
            }
            /// Recover the untouched bank and the refusal.
            pub fn into_parts(self) -> ($bank, PrepareError) {
                (self.bank, self.error)
            }
        }
        /// Static diagonal effect adapter. Sample rate is an explicit target metadata
        /// contract; no IR conversion/trim/normalization or live reload is performed.
        /// All valid portable input events are ignored after complete validation.
        /// Auxiliary buses are refused. Every output sample is initialized; nonfinite
        /// arithmetic propagates without sanitization, reset clears history.
        /// Manual start/stop sessions require exclusive ownership. CONTROL prepare,
        /// deactivate/extraction and Drop must occur after callbacks/session stop.
        pub struct $name {
            bank: $bank,
            sample_rate: f64,
            config: Option<ProcessConfig>,
            session: bool,
        }
        impl $name {
            /// Invalid finite-positive target rate returns the exact original bank.
            /// This CONTROL metadata check is not physical-device qualification.
            pub fn new(bank: $bank, sample_rate: f64) -> Result<Self, $failure> {
                if !sample_rate.is_finite() || sample_rate <= 0. {
                    return Err($failure {
                        bank,
                        error: PrepareError::InvalidSampleRate,
                    });
                }
                Ok(Self {
                    bank,
                    sample_rate,
                    config: None,
                    session: false,
                })
            }
            /// Borrow the owned bank (engine info, work observations).
            pub fn bank(&self) -> &$bank {
                &self.bank
            }
            /// Only inactive/stopped ownership can be extracted. A refusal returns
            /// the exact adapter/config/history, boxed on CONTROL (one heap
            /// allocation of the adapter shell) so the success path stays small; the
            /// prepared numeric storage itself is moved, not copied or disposed.
            pub fn into_bank(self) -> Result<$bank, Box<Self>> {
                if self.config.is_some() || self.session {
                    Err(Box::new(self))
                } else {
                    Ok(self.bank)
                }
            }
            /// Prepared configuration, if any. `None` after refusal or deactivation.
            pub fn config(&self) -> Option<ProcessConfig> {
                self.config
            }
            /// Explicit target sample-rate metadata supplied at construction.
            pub fn sample_rate(&self) -> f64 {
                self.sample_rate
            }
        }
        impl Processor<$sample> for $name {
            fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
                if self.session {
                    return Err(PrepareError::AudioSessionActive);
                }
                self.config = None;
                if c.sample_rate() != self.sample_rate {
                    return Err(PrepareError::InvalidSampleRate);
                }
                if c.layout().input_channels() != self.bank.channels()
                    || c.layout().output_channels() != self.bank.channels()
                {
                    return Err(PrepareError::UnsupportedLayout);
                }
                if !c.auxiliary_buses().is_empty() {
                    return Err(PrepareError::TooManyAuxiliaryBuses);
                }
                if c.maximum_frames() > self.bank.maximum_frames() {
                    return Err(PrepareError::InvalidMaximumFrames);
                }
                self.bank.reset();
                self.config = Some(c);
                Ok(())
            }
            fn start_audio(&mut self) -> Result<(), ProcessError> {
                if self.config.is_none() {
                    return Err(ProcessError::NotPrepared);
                }
                self.session = true;
                Ok(())
            }
            fn stop_audio(&mut self) {
                self.session = false;
            }
            fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_, $sample>) -> ProcessStatus {
                let Some(config) = self.config else {
                    b.clear_outputs();
                    return ProcessStatus::Fault(ProcessError::NotPrepared);
                };
                if let Err(e) = config.validate_block(b) {
                    b.clear_outputs();
                    return ProcessStatus::Fault(e);
                }
                let n = b.audio.frames();
                // A per-frame copy releases the input borrow before any output edit,
                // including canonical in-place AudioBuffer. No scratch allocation.
                for ch in 0..self.bank.channels() {
                    for i in 0..n {
                        let x = b.audio.input(ch).expect("prepared layout")[i];
                        let y = self
                            .bank
                            .engine_mut(ch)
                            .expect("prepared layout")
                            .process_sample(x);
                        b.audio.output_mut(ch).expect("prepared layout")[i] = y;
                    }
                }
                ProcessStatus::Continue
            }
            fn reset(&mut self) {
                self.bank.reset();
            }
            fn deactivate(&mut self) {
                assert!(
                    !self.session,
                    "stop the audio-owner session before convolution deactivation"
                );
                self.config = None;
            }
            fn latency_samples(&self) -> u32 {
                self.bank
                    .engine(0)
                    .expect("nonempty bank")
                    .info()
                    .latency_samples as u32
            }
            fn tail(&self) -> Tail {
                Tail::Samples(
                    (0..self.bank.channels())
                        .map(|ch| {
                            self.bank
                                .engine(ch)
                                .expect("existing channel")
                                .info()
                                .tail_samples
                        })
                        .max()
                        .expect("nonempty bank") as u64,
                )
            }
        }
    };
}
processor!(
    ConvolutionProcessor32,
    ConvolutionProcessorFailure32,
    ConvolutionBank32,
    f32
);
processor!(
    ConvolutionProcessor64,
    ConvolutionProcessorFailure64,
    ConvolutionBank64,
    f64
);
