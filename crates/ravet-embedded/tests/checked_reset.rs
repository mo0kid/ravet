use ravet_embedded::{BlockError, FixedBlockAdapter};
use ravet_foundation::*;
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
#[test]
fn fixed_adapter_retains_refusal_silences_prefix_and_never_calls_legacy_or_dsp() {
    let mut a = FixedBlockAdapter::<_, 1, 1, 8>::new(Refuse::default());
    a.prepare(ProcessConfig::new(48000., 8, 0, AudioLayout::MONO).unwrap())
        .unwrap();
    assert_eq!(a.try_reset(), Err(ProcessError::RebuildRequired));
    a.reset();
    assert_eq!(a.reset_error(), Some(ProcessError::RebuildRequired));
    let mut out = [9., 9., 81.];
    assert_eq!(
        a.process_interleaved(&[1f32, 2.], &mut out, 2, &[], Transport::default()),
        Err(BlockError::Process(ProcessError::RebuildRequired))
    );
    assert_eq!(out, [0., 0., 81.]);
    assert_eq!((a.processor().legacy, a.processor().calls), (0, 0));
}
