use super::*;
use ravet_foundation::Transport;
use std::{ptr, slice};
/// Concrete C-layout planar bus for one sample precision. Channel pointer arrays
/// cover the declared counts; samples cover active frames. Zero-frame processing
/// permits null arrays/sample pointers. Preparation order includes zero-sided buses.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NativePlanarBus<S: Sample> {
    pub input_channels: u32,
    pub output_channels: u32,
    pub inputs: *const *const S,
    pub outputs: *const *mut S,
}
/// Render status and typed generated events borrowed until the next adapter call.
/// Faults expose no events. Consumers own format conversion/delivery/refusal.
#[derive(Debug)]
pub struct NativeProcessReport<'a> {
    pub status: ProcessStatus,
    pub events: &'a [Event],
}
fn aligned<S>(p: *const S) -> bool {
    (p as usize).is_multiple_of(align_of::<S>())
}
pub(crate) unsafe fn silence<S: Sample>(buses: &[NativePlanarBus<S>], frames: usize) {
    if buses.len() > 9 || frames > 65536 || frames == 0 {
        return;
    }
    for bus in buses {
        if bus.output_channels > 64 || bus.outputs.is_null() || !aligned(bus.outputs) {
            continue;
        }
        for channel in 0..bus.output_channels as usize {
            let output = unsafe { *bus.outputs.add(channel) };
            if !output.is_null() && aligned(output) {
                unsafe { slice::from_raw_parts_mut(output, frames) }.fill(S::default());
            }
        }
    }
}
impl<S: Sample> NativeAudioAdapter<S> {
    /// Validate the complete shape/event prefix before invoking the processor,
    /// copy every input before any host output write, and expose generated events.
    /// Pointer/shape/input faults silence supported declared output prefixes;
    /// backing tails are retained. Backend faults keep advanced processor history.
    /// A contained processing panic poisons this adapter until fresh preparation.
    /// Normal processing has no allocation; exceptional panic handlers may allocate.
    ///
    /// # Safety
    /// Use the same exclusively owned, prepared processor. All nonnull foreign
    /// pointer arrays/sample ranges must be live, aligned and valid for declared
    /// channels/frames, even on rejection; arrays/counts must not change during
    /// this call. Input/output sample ranges may alias; overlapping outputs reject.
    /// Metadata, events, processor and adapter storage must not overlap writable
    /// samples. No concurrent rendering, reset, control access or destruction.
    pub unsafe fn process_native<P: Processor<S>>(
        &mut self,
        processor: &mut P,
        buses: &[NativePlanarBus<S>],
        frames: usize,
        events: &[Event],
        transport: Transport,
    ) -> NativeProcessReport<'_> {
        let result = if self.poisoned {
            Err(ProcessError::BackendFailure)
        } else {
            match catch_unwind(AssertUnwindSafe(|| unsafe {
                self.render(processor, buses, frames, events, transport)
            })) {
                Ok(result) => result,
                Err(_) => {
                    self.poisoned = true;
                    Err(ProcessError::BackendFailure)
                }
            }
        };
        match result {
            Ok((status, count)) => NativeProcessReport {
                status,
                events: &self.generated[..count],
            },
            Err(error) => {
                unsafe { silence(buses, frames) };
                NativeProcessReport {
                    status: ProcessStatus::Fault(error),
                    events: &[],
                }
            }
        }
    }
    unsafe fn render<P: Processor<S>>(
        &mut self,
        processor: &mut P,
        buses: &[NativePlanarBus<S>],
        frames: usize,
        events: &[Event],
        transport: Transport,
    ) -> Result<(ProcessStatus, usize), ProcessError> {
        if frames > self.config.maximum_frames() {
            return Err(ProcessError::TooManyFrames {
                frames,
                maximum: self.config.maximum_frames(),
            });
        }
        if buses.len() != 1 + self.config.auxiliary_buses().len() {
            return Err(ProcessError::LayoutMismatch);
        }
        let layouts = std::iter::once(self.config.layout())
            .chain(self.config.auxiliary_buses().iter().copied());
        let mut ranges = [(0usize, 0usize); CHANNELS];
        let mut used = 0;
        for (bus, layout) in buses.iter().zip(layouts) {
            if bus.input_channels as usize != layout.input_channels()
                || bus.output_channels as usize != layout.output_channels()
            {
                return Err(ProcessError::LayoutMismatch);
            }
            if frames == 0 {
                continue;
            }
            if (bus.input_channels > 0 && (bus.inputs.is_null() || !aligned(bus.inputs)))
                || (bus.output_channels > 0 && (bus.outputs.is_null() || !aligned(bus.outputs)))
            {
                return Err(ProcessError::LayoutMismatch);
            }
            for channel in 0..bus.input_channels as usize {
                let input = unsafe { *bus.inputs.add(channel) };
                if input.is_null() || !aligned(input) {
                    return Err(ProcessError::LayoutMismatch);
                }
            }
            for channel in 0..bus.output_channels as usize {
                let output = unsafe { *bus.outputs.add(channel) };
                if output.is_null() || !aligned(output) {
                    return Err(ProcessError::LayoutMismatch);
                }
                let start = output as usize;
                let end = start
                    .checked_add(frames * size_of::<S>())
                    .ok_or(ProcessError::LayoutMismatch)?;
                if ranges[..used].iter().any(|&(a, b)| start < b && a < end) {
                    return Err(ProcessError::LayoutMismatch);
                }
                ranges[used] = (start, end);
                used += 1;
            }
        }
        let mut input_index = 0;
        for bus in buses {
            for channel in 0..bus.input_channels as usize {
                if frames > 0 {
                    self.inputs[input_index][..frames].copy_from_slice(unsafe {
                        slice::from_raw_parts(*bus.inputs.add(channel), frames)
                    });
                }
                input_index += 1;
            }
        }
        let (status, count) = self.run(processor, frames, events, transport)?;
        let mut output_index = 0;
        for bus in buses {
            for channel in 0..bus.output_channels as usize {
                if frames > 0 {
                    unsafe {
                        ptr::copy_nonoverlapping(
                            self.outputs[output_index].as_ptr(),
                            *bus.outputs.add(channel),
                            frames,
                        )
                    };
                }
                output_index += 1;
            }
        }
        Ok((status, count))
    }
}
