#![allow(unsafe_code)]
use ravet_foundation::*;
use ravet_native::*;
use std::{
    ptr,
    sync::atomic::{AtomicUsize, Ordering},
};
#[derive(Default)]
struct Refuse;
impl Processor for Refuse {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        Ok(())
    }
    fn reset(&mut self) {
        panic!("checked reset bypassed")
    }
    fn try_reset(&mut self) -> Result<(), ProcessError> {
        Err(ProcessError::RebuildRequired)
    }
    fn deactivate(&mut self) {}
    fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        b.clear_outputs();
        ProcessStatus::Continue
    }
}
#[test]
fn rust_adapter_preserves_exact_refusal_and_poison() {
    let mut p = Refuse;
    let c = ProcessConfig::new(48000., 8, 0, AudioLayout::MONO).unwrap();
    let mut a = NativeAudioAdapter::<f32>::prepare(&mut p, c).unwrap();
    assert_eq!(a.reset(&mut p), Err(ProcessError::RebuildRequired));
    assert_eq!(a.reset_error(), Some(ProcessError::RebuildRequired));
    assert!(a.is_poisoned());
    assert_eq!(a.reset(&mut p), Err(ProcessError::BackendFailure));
    assert_eq!(a.reset_error(), Some(ProcessError::RebuildRequired));
}
static CALLS: AtomicUsize = AtomicUsize::new(0);
#[derive(Default)]
struct NativeFixture;
impl Processor for NativeFixture {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        Ok(())
    }
    fn reset(&mut self) {
        panic!("legacy reset")
    }
    fn try_reset(&mut self) -> Result<(), ProcessError> {
        Err(ProcessError::RebuildRequired)
    }
    fn deactivate(&mut self) {}
    fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        CALLS.fetch_add(1, Ordering::Relaxed);
        b.clear_outputs();
        ProcessStatus::Continue
    }
}
#[test]
fn native_table_keeps_existing_poison_code_and_refuses_dsp() {
    let table = NativeProcessor::<NativeFixture, f32>::entry();
    let h = (table.create)();
    assert!(!h.is_null());
    let mut c = NativeProcessorConfigV1 {
        version: 1,
        maximum_frames: 8,
        bus_count: 1,
        sample_rate: 48000.,
        ..Default::default()
    };
    c.buses[0] = NativeBusLayoutV1 {
        inputs: 1,
        outputs: 1,
    };
    assert_eq!(unsafe { (table.prepare)(h, &c) }, 0);
    assert_eq!(unsafe { (table.reset)(h) }, 2);
    assert_eq!(unsafe { (table.reset)(h) }, 2);
    let input = [1f32, 2.];
    let inputs = [input.as_ptr()];
    let mut output = [9f32, 9., 81.];
    let outputs = [output.as_mut_ptr()];
    let bus = NativeProcessorBusV1 {
        input_channels: 1,
        output_channels: 1,
        inputs: inputs.as_ptr().cast(),
        outputs: outputs.as_ptr().cast(),
    };
    let mut r = NativeProcessorResultV1 {
        status: 19,
        event_count: 29,
    };
    assert_eq!(
        unsafe {
            (table.process)(
                h,
                &bus,
                1,
                2,
                ptr::null(),
                0,
                ptr::null(),
                ptr::null_mut(),
                0,
                &mut r,
            )
        },
        2
    );
    assert_eq!(output, [0., 0., 81.]);
    assert_eq!(
        r,
        NativeProcessorResultV1 {
            status: 2,
            event_count: 0
        }
    );
    assert_eq!(CALLS.load(Ordering::Relaxed), 0);
    unsafe { (table.destroy)(h) };
}
