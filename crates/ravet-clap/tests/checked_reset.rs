use clack_plugin::events::io::{InputEvents, OutputEvents};
use ravet_clap::{BridgeError, EffectBridge, MainBusBridge, MultiBusBridge};
use ravet_foundation::{
    AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessError, ProcessStatus, Processor,
};
#[derive(Default)]
struct Refuse {
    legacy: usize,
    calls: usize,
}
impl Processor for Refuse {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        Ok(())
    }
    fn reset(&mut self) {
        self.legacy += 1;
    }
    fn try_reset(&mut self) -> Result<(), ProcessError> {
        Err(ProcessError::RebuildRequired)
    }
    fn deactivate(&mut self) {}
    fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        self.calls += 1;
        b.clear_outputs();
        ProcessStatus::Continue
    }
}
fn config() -> ProcessConfig {
    ProcessConfig::new(48000., 8, 0, AudioLayout::STEREO).unwrap()
}
#[test]
fn main_bridge_refusal_survives_void_reset_and_blocks_flush() {
    let mut b = MainBusBridge::<_, f32>::prepare(Refuse::default(), config(), None).unwrap();
    assert_eq!(b.try_reset(), Err(ProcessError::RebuildRequired));
    b.reset();
    assert_eq!(b.reset_error(), Some(ProcessError::RebuildRequired));
    assert_eq!(
        b.flush(&InputEvents::empty()),
        Err(BridgeError::Processor(ProcessError::RebuildRequired))
    );
    assert_eq!((b.processor().legacy, b.processor().calls), (0, 0));
}
#[test]
fn multi_bridge_refusal_survives_void_reset_and_blocks_flush() {
    let c = config()
        .with_auxiliary_buses(&[AudioLayout::new(1, 0).unwrap()])
        .unwrap();
    let mut b = MultiBusBridge::<_, f32>::prepare(Refuse::default(), c, None, None).unwrap();
    assert_eq!(b.try_reset(), Err(ProcessError::RebuildRequired));
    b.reset();
    assert_eq!(b.reset_error(), Some(ProcessError::RebuildRequired));
    assert_eq!(
        b.flush(&InputEvents::empty(), &mut OutputEvents::void()),
        Err(BridgeError::Processor(ProcessError::RebuildRequired))
    );
    assert_eq!((b.processor().legacy, b.processor().calls), (0, 0));
}
#[test]
fn effect_bridge_refusal_survives_void_reset_and_blocks_flush() {
    let mut b = EffectBridge::prepare(Refuse::default(), config()).unwrap();
    assert_eq!(b.try_reset(), Err(ProcessError::RebuildRequired));
    b.reset();
    assert_eq!(b.reset_error(), Some(ProcessError::RebuildRequired));
    assert_eq!(
        b.flush(&InputEvents::empty()),
        Err(BridgeError::Processor(ProcessError::RebuildRequired))
    );
    assert_eq!((b.processor().legacy, b.processor().calls), (0, 0));
}
