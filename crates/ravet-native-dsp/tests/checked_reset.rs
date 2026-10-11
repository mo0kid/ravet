#![cfg(feature = "processor")]
//! Checked reset through the native denormal-policy wrapper (W255).
use ravet_foundation::{
    AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessError,
    ProcessStatus, Processor, Transport,
};
use ravet_native_dsp::{DenormalMode, DenormalProcessor};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
struct Probe {
    refuse: Arc<AtomicBool>,
    attempts: Arc<AtomicUsize>,
    legacy: Arc<AtomicUsize>,
    calls: Arc<AtomicUsize>,
}
impl Probe {
    fn new(refuse: bool) -> Self {
        Self {
            refuse: Arc::new(AtomicBool::new(refuse)),
            attempts: Arc::new(AtomicUsize::new(0)),
            legacy: Arc::new(AtomicUsize::new(0)),
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}
macro_rules! processor {
    ($s:ty) => {
        impl Processor<$s> for Probe {
            fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
                Ok(())
            }
            fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_, $s>) -> ProcessStatus {
                self.calls.fetch_add(1, Ordering::Relaxed);
                b.audio.output_mut(0).unwrap().fill(2.);
                ProcessStatus::Continue
            }
            fn reset(&mut self) {
                self.legacy.fetch_add(1, Ordering::Relaxed);
            }
            fn try_reset(&mut self) -> Result<(), ProcessError> {
                self.attempts.fetch_add(1, Ordering::Relaxed);
                if self.refuse.load(Ordering::Relaxed) {
                    Err(ProcessError::RebuildRequired)
                } else {
                    Ok(())
                }
            }
            fn deactivate(&mut self) {}
        }
    };
}
processor!(f32);
processor!(f64);
fn config() -> ProcessConfig {
    ProcessConfig::new(48000., 8, 0, AudioLayout::MONO).unwrap()
}
#[test]
fn denormal_wrapper_returns_child_refusal_and_keeps_render_disabled() {
    let p = Probe::new(true);
    let count = p.calls.clone();
    let legacy = p.legacy.clone();
    let mut wrapper =
        DenormalProcessor::new(p, DenormalMode::Preserve).unwrap_or_else(|_| panic!("preserve"));
    Processor::<f32>::prepare(&mut wrapper, config()).unwrap();
    assert_eq!(
        Processor::<f32>::try_reset(&mut wrapper),
        Err(ProcessError::RebuildRequired)
    );
    Processor::<f32>::reset(&mut wrapper);
    let mut samples = [8., 8., 99.];
    let mut views = [&mut samples[..]];
    let audio = AudioBuffer::in_place(&mut views, 2).unwrap();
    let mut block = ProcessBlock::new(audio, &[], Transport::default());
    assert_eq!(
        wrapper.process(&mut block),
        ProcessStatus::Fault(ProcessError::BackendFailure)
    );
    assert_eq!(samples, [0., 0., 99.]);
    assert_eq!(count.load(Ordering::Relaxed), 0);
    assert_eq!(legacy.load(Ordering::Relaxed), 0);
}
