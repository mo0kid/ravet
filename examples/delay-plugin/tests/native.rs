//! The delay's exported `ravet_plugin_v1` table, used as the VST3, AUv2, AAX
//! and LV2 adapters use it: through its C function pointers.
#![allow(unsafe_code)]
use ravet_delay_plugin::{
    __ravet_plugin_export::ravet_plugin_v1, DelayPlugin, dsp::DEFAULTS, dsp::synced_time_ms,
};
use ravet_foundation::Transport;
use ravet_plugin::{
    PluginExport,
    native::{
        NativeBusLayoutV1, NativeEventV1, NativeProcessorBusV1, NativeProcessorConfigV1,
        NativeProcessorResultV1, NativeTransportV1, RavetPluginEntryV1, RavetPluginParameterV1,
        native_event_kind,
    },
    state,
};
use std::{
    ffi::{CStr, c_char, c_void},
    ptr, slice,
};

const RATE: f64 = 8000.0;
// Declaration indices.
const TIME: u32 = 0;
const FEEDBACK: u32 = 1;
const MIX: u32 = 2;
const SYNC: u32 = 5;
const DIVISION: u32 = 6;

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
        maximum_output_events: 0,
        bus_count: 1,
        reserved: 0,
        sample_rate: RATE,
        buses: Default::default(),
    };
    config.buses[0] = NativeBusLayoutV1 {
        inputs: 2,
        outputs: 2,
    };
    assert_eq!(unsafe { (table.prepare)(instance, &config) }, 0);
    instance
}

fn tempo(bpm: f64) -> NativeTransportV1 {
    NativeTransportV1::from_transport(Transport {
        tempo: Some(bpm),
        ..Transport::default()
    })
    .unwrap()
}

fn process(
    instance: *mut c_void,
    input: &[f32],
    events: &[NativeEventV1],
    transport: Option<&NativeTransportV1>,
) -> [Vec<f32>; 2] {
    let frames = input.len();
    let mut output = [vec![9.0f32; frames], vec![9.0f32; frames]];
    let inputs = [input.as_ptr(), input.as_ptr()];
    let outputs = [output[0].as_mut_ptr(), output[1].as_mut_ptr()];
    let bus = NativeProcessorBusV1 {
        input_channels: 2,
        output_channels: 2,
        inputs: inputs.as_ptr().cast(),
        outputs: outputs.as_ptr().cast(),
    };
    let mut result = NativeProcessorResultV1 {
        status: 9,
        event_count: 9,
    };
    let code = unsafe {
        (table().process)(
            instance,
            &bus,
            1,
            frames as u32,
            events.as_ptr(),
            events.len() as u32,
            transport.map_or(ptr::null(), |t| t),
            ptr::null_mut(),
            0,
            &mut result,
        )
    };
    assert_eq!(code, 0);
    assert_eq!(result.event_count, 0);
    output
}

fn set(instance: *mut c_void, index: u32, value: f64) {
    assert_eq!(unsafe { (table().set_value)(instance, index, value) }, 0);
}

fn impulse(frames: usize) -> Vec<f32> {
    let mut samples = vec![0.0; frames];
    samples[0] = 1.0;
    samples
}

fn peaks(samples: &[f32]) -> Vec<usize> {
    (0..samples.len())
        .filter(|&i| samples[i].abs() > 1e-6)
        .collect()
}

#[test]
fn product_and_identities_match_the_resolved_product() {
    let table = table();
    let resolved = DelayPlugin::PRODUCT.resolve().unwrap();
    let identities = resolved.identities();
    let record = unsafe { &*table.product };
    assert_eq!(text(record.name), "RAVET Delay");
    assert_eq!(text(record.plugin_id), "org.ravet.delay");
    assert_eq!(text(record.url), "https://djwaudio.co/ravet/");
    assert_eq!(text(record.clap_id), identities.clap_id);
    assert_eq!(text(record.lv2_uri), identities.lv2_uri);
    assert_eq!(record.vst3_processor_cid, identities.vst3_processor);
    assert_eq!(record.vst3_controller_cid, identities.vst3_controller);
    assert_eq!(record.au_type, u32::from_be_bytes(identities.au_type));
    assert_eq!(record.au_subtype, u32::from_be_bytes(identities.au_subtype));
    assert_eq!(record.aax_product, identities.aax_product);
    assert_eq!(record.bypass_index, resolved.bypass_index().unwrap() as i32);
    assert_eq!((record.editor_width, record.editor_height), (560, 340));
    let layouts = unsafe { slice::from_raw_parts(record.layouts, record.layout_count as usize) };
    let shapes: Vec<_> = layouts.iter().map(|l| (l.inputs, l.outputs)).collect();
    assert_eq!(shapes, [(2, 2), (1, 1)]);
    assert_eq!(
        record.parameter_count as usize,
        resolved.parameter_ids().len()
    );
    for (index, parameter) in resolved.product().parameters().iter().enumerate() {
        let mut info: RavetPluginParameterV1 = unsafe { std::mem::zeroed() };
        assert_eq!(unsafe { (table.parameter)(index as u32, &mut info) }, 0);
        assert_eq!(info.id, resolved.parameter_ids()[index].0);
        assert_eq!(text(info.key), parameter.key());
        assert_eq!(info.default_value, f64::from(parameter.default()));
        assert_eq!((table.parameter_index)(info.id), index as i32);
    }
}

#[test]
fn parameter_text_round_trips() {
    let table = table();
    let mut buffer = [0 as c_char; 64];
    for (index, default) in DEFAULTS.iter().enumerate() {
        let index = index as u32;
        let mut value = f64::NAN;
        unsafe {
            assert_eq!(
                (table.value_to_text)(index, f64::from(*default), buffer.as_mut_ptr(), 64),
                0
            );
            assert_eq!((table.text_to_value)(index, buffer.as_ptr(), &mut value), 0);
        }
        assert_eq!(value, f64::from(*default), "{}", text(buffer.as_ptr()));
    }
    unsafe { (table.value_to_text)(TIME, 820.0, buffer.as_mut_ptr(), 64) };
    assert_eq!(text(buffer.as_ptr()), "820 ms");
}

#[test]
fn set_values_are_saved_and_loading_resets_and_delivers() {
    let table = table();
    let instance = prepared();
    set(instance, FEEDBACK, 70.0);
    let mut length = 0;
    let mut bytes = vec![0u8; 512];
    unsafe {
        assert_eq!(
            (table.save_state)(instance, bytes.as_mut_ptr(), 512, &mut length),
            0
        );
    }
    let resolved = DelayPlugin::PRODUCT.resolve().unwrap();
    let saved = state::restore(&resolved, &bytes[..length as usize], &state::NoHooks).unwrap();
    assert_eq!(saved.values[FEEDBACK as usize], 70.0);

    // A 10 ms free-time delay with no feedback, fully wet, loaded as state.
    let mut values = DEFAULTS;
    values[SYNC as usize] = 0.0;
    values[TIME as usize] = 10.0;
    values[FEEDBACK as usize] = 0.0;
    values[MIX as usize] = 100.0;
    let bytes = state::encode(&resolved, &values, &[]).unwrap();
    unsafe {
        assert_eq!(
            (table.load_state)(instance, bytes.as_ptr(), bytes.len() as u32),
            0
        );
        assert_eq!((table.get_value)(instance, TIME), 10.0);
    }
    // The reset completes the time glide, so the repeat lands exactly.
    let output = process(instance, &impulse(200), &[], None);
    assert_eq!(peaks(&output[0]), [80]);
    unsafe { (table.destroy)(instance) };
}

#[test]
fn tempo_sync_places_the_repeat() {
    let table = table();
    let instance = prepared();
    set(instance, DIVISION, 6.0);
    set(instance, FEEDBACK, 0.0);
    set(instance, MIX, 100.0);
    let silence = vec![0.0; 64];
    process(instance, &silence, &[], Some(&tempo(120.0)));
    assert_eq!(unsafe { (table.reset)(instance) }, 0);
    // An eighth note at 120 BPM is 250 ms: 2000 samples at 8 kHz.
    assert_eq!(synced_time_ms(6, 120.0), 250.0);
    let output = process(instance, &impulse(2100), &[], Some(&tempo(120.0)));
    assert_eq!(peaks(&output[0]), [2000]);
    assert_eq!(peaks(&output[1]), [2000]);
    // Automation through events: dotted eighth (375 ms) from the next block.
    let event = NativeEventV1 {
        kind: native_event_kind::PARAMETER_VALUE,
        parameter_id: ravet_delay_plugin::dsp::DIVISION.0,
        value: 4.0,
        ..NativeEventV1::default()
    };
    process(instance, &silence, &[event], Some(&tempo(120.0)));
    assert_eq!(unsafe { (table.get_value)(instance, DIVISION) }, 4.0);
    unsafe { (table.destroy)(instance) };
}

#[test]
fn processing_through_the_table_allocates_nothing() {
    let table = table();
    let instance = prepared();
    let input = [0.1f32; 256];
    let mut output = [[0.0f32; 256]; 2];
    let inputs = [input.as_ptr(), input.as_ptr()];
    let outputs = [output[0].as_mut_ptr(), output[1].as_mut_ptr()];
    let bus = NativeProcessorBusV1 {
        input_channels: 2,
        output_channels: 2,
        inputs: inputs.as_ptr().cast(),
        outputs: outputs.as_ptr().cast(),
    };
    let mut result = NativeProcessorResultV1 {
        status: 0,
        event_count: 0,
    };
    let measured = allocation_counter::measure(|| {
        for i in 0..500u32 {
            let transport = tempo(60.0 + f64::from(i % 120));
            let event = NativeEventV1 {
                offset: 17,
                kind: native_event_kind::PARAMETER_VALUE,
                parameter_id: ravet_delay_plugin::dsp::TIME.0,
                value: 1.0 + f64::from(i % 2000),
                ..NativeEventV1::default()
            };
            set(instance, DIVISION, f64::from(i % 12));
            let code = unsafe {
                (table.process)(
                    instance,
                    &bus,
                    1,
                    256,
                    &event,
                    1,
                    &transport,
                    ptr::null_mut(),
                    0,
                    &mut result,
                )
            };
            assert_eq!(code, 0);
        }
        assert_eq!(unsafe { (table.reset)(instance) }, 0);
    });
    assert_eq!(measured.count_total, 0);
    unsafe { (table.destroy)(instance) };
}
