//! The instrument's exported `ravet_plugin_v1` table, used as the VST3, AUv2, AAX
//! and LV2 adapters use it: through its C function pointers.
#![allow(unsafe_code)]
use ravet_plugin::native::{
    NativeBusLayoutV1, NativeEventV1, NativeProcessorBusV1, NativeProcessorConfigV1,
    NativeProcessorResultV1, RavetPluginEntryV1, native_event_kind,
};
use ravet_synth_plugin::__ravet_plugin_export::ravet_plugin_v1;
use std::{
    ffi::{CStr, c_char, c_void},
    ptr,
};

fn table() -> &'static RavetPluginEntryV1 {
    unsafe { ravet_plugin_v1().as_ref() }.expect("valid product")
}

fn text(pointer: *const c_char) -> &'static str {
    unsafe { CStr::from_ptr(pointer) }.to_str().unwrap()
}

fn prepared() -> *mut c_void {
    let table = table();
    let instance = (table.create)();
    assert!(!instance.is_null());
    let mut config = NativeProcessorConfigV1 {
        version: 1,
        maximum_frames: 4096,
        maximum_events: 32,
        maximum_output_events: 32,
        bus_count: 1,
        reserved: 0,
        sample_rate: 48000.0,
        buses: Default::default(),
    };
    config.buses[0] = NativeBusLayoutV1 {
        inputs: 0,
        outputs: 2,
    };
    assert_eq!(unsafe { (table.prepare)(instance, &config) }, 0);
    instance
}

fn render(
    instance: *mut c_void,
    events: &[NativeEventV1],
    frames: usize,
) -> (Vec<f32>, Vec<NativeEventV1>, u64) {
    let mut audio = vec![99.0f32; frames + 1];
    let mut right = audio.clone();
    let outputs = [audio.as_mut_ptr(), right.as_mut_ptr()];
    let bus = NativeProcessorBusV1 {
        input_channels: 0,
        output_channels: 2,
        inputs: ptr::null(),
        outputs: outputs.as_ptr().cast(),
    };
    let mut generated = [NativeEventV1::default(); 32];
    let mut result = NativeProcessorResultV1 {
        status: 0,
        event_count: 0,
    };
    let allocated = allocation_counter::measure(|| unsafe {
        assert_eq!(
            (table().process)(
                instance,
                &bus,
                1,
                frames as u32,
                events.as_ptr(),
                events.len() as u32,
                ptr::null(),
                generated.as_mut_ptr(),
                32,
                &mut result
            ),
            0
        );
    })
    .count_total;
    assert_eq!(audio, right);
    assert_eq!(audio[frames], 99.0);
    audio.truncate(frames);
    (
        audio,
        generated[..result.event_count as usize].to_vec(),
        allocated,
    )
}
fn midi(offset: u32, bytes: [u8; 3]) -> NativeEventV1 {
    NativeEventV1 {
        offset,
        kind: native_event_kind::MIDI,
        detail: 3,
        words: [
            u32::from(bytes[0]) | u32::from(bytes[1]) << 8 | u32::from(bytes[2]) << 16,
            0,
            0,
            0,
        ],
        ..Default::default()
    }
}
#[test]
fn timed_click_cc_echo_latency_change_tail_and_reset() {
    let t = table();
    let instance = prepared();
    let product = unsafe { &*t.product };
    assert_eq!(text(product.plugin_id), "org.ravet.evidence-synth");
    assert_eq!((product.note_input, product.note_output), (1, 1));
    let events = [
        midi(7, [0x90, 60, 127]),
        midi(15, [0xb0, 1, 64]),
        midi(17, [0x80, 60, 0]),
    ];
    let (audio, echo, allocations) = render(instance, &events, 128);
    assert_eq!(allocations, 0);
    for (a, b) in echo.iter().zip(events) {
        assert_eq!((a.offset, a.kind, a.words), (b.offset, b.kind, b.words));
    }
    assert_eq!(echo.len(), 3);
    for (i, &v) in audio.iter().enumerate() {
        let expected = if (39..103).contains(&i) {
            (if (i - 39) % 2 == 0 { 1.0 } else { -1.0 })
                * 0.25
                * if i < 47 { 1.0 } else { 64.0 / 127.0 }
        } else {
            0.0
        };
        assert_eq!(v, expected, "sample {i}");
    }
    let mut timing = unsafe { std::mem::zeroed() };
    unsafe {
        assert_eq!((t.timing)(instance, &mut timing), 0);
    }
    assert_eq!(timing.latency_samples, 32);
    assert_eq!(timing.tail_samples, 96);
    unsafe {
        assert_eq!((t.set_value)(instance, 1, 0.0), 0);
    }
    render(instance, &[], 0);
    assert_eq!(unsafe { (t.latency_changed)(instance) }, 1);
    unsafe {
        assert_eq!((t.timing)(instance, &mut timing), 0);
    }
    assert_eq!(timing.latency_samples, 0);
    assert_eq!(timing.tail_samples, 64);
    assert_eq!(unsafe { (t.latency_changed)(instance) }, 0);
    unsafe {
        assert_eq!((t.set_value)(instance, 0, 0.0), 0);
        assert_eq!((t.reset)(instance), 0);
    }
    let (audio, _, _) = render(
        instance,
        &[midi(3, [0x90, 60, 127]), midi(4, [0xb0, 1, 0])],
        128,
    );
    assert_eq!(audio[3], 0.25);
    assert_eq!(audio[4], -0.25);
    assert_eq!(audio[67], 0.0);
    unsafe { (t.destroy)(instance) };
}
#[test]
fn c_scope_callback_restores_and_rejects_invalid_arguments() {
    use ravet_synth_plugin::__ravet_plugin_export::ravet_plugin_with_denormal_mode_v1 as scope;
    unsafe extern "C" fn callback(context: *mut c_void) {
        let state = ravet_native_dsp::current_float_state().unwrap();
        assert!(state.input_flush() && state.result_flush());
        unsafe {
            *context.cast::<bool>() = true;
        }
        unsafe {
            assert_eq!(scope(0, Some(inner), std::ptr::null_mut()), 0);
        }
    }
    unsafe extern "C" fn inner(_: *mut c_void) {
        let state = ravet_native_dsp::current_float_state().unwrap();
        assert!(state.input_flush() && state.result_flush());
    }
    let before = ravet_native_dsp::current_float_state().unwrap();
    let mut called = false;
    unsafe {
        assert_eq!(
            scope(99, Some(callback), (&mut called as *mut bool).cast()),
            1
        );
        assert!(!called);
        assert_eq!(scope(1, None, ptr::null_mut()), 1);
        assert_eq!(
            scope(1, Some(callback), (&mut called as *mut bool).cast()),
            0
        );
        assert!(called);
        assert_eq!(ravet_native_dsp::current_float_state().unwrap(), before);
    }
}
