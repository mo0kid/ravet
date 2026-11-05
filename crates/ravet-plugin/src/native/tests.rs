//! The C table, exercised through its function pointers in-process.
use super::*;
use crate::{
    BypassPolicy, Editor, Parameter, Product, StateFormat,
    state::{SavedState, StateError},
};
use ravet_foundation::{AudioLayout, PrepareError, ProcessBlock, ProcessError};
use std::sync::OnceLock;

const GAIN: u32 = 11;
const MODE: u32 = 12;
const CRASH: u32 = 13;
const BYPASS: u32 = 14;

const PARAMETERS: &[Parameter] = &[
    Parameter::linear("gain", "Gain", 0.0, 2.0, 1.0)
        .with_id(GAIN)
        .with_unit("x"),
    Parameter::stepped("mode", "Mode", 0.0, 3.0, 3, 0.0).with_id(MODE),
    Parameter::toggle("crash", "Crash", false).with_id(CRASH),
    Parameter::bypass("bypass", "Bypass").with_id(BYPASS),
];

/// Applies gain sample-accurately, panics while `crash` is on, reports
/// latency `mode × 10` and records the values it receives.
#[derive(Default)]
struct Recorder {
    config: Option<ProcessConfig>,
    gain: f32,
    mode: f32,
    crash: bool,
    resets: usize,
    values: Vec<(usize, u32, f32)>,
}

impl Recorder {
    fn apply(&mut self, frames: usize, event: &Event) {
        if let EventData::ParameterValue { id, value } = event.data {
            if self.values.len() < self.values.capacity() {
                self.values.push((frames, id.0, value));
            }
            match id.0 {
                GAIN => self.gain = value,
                MODE => self.mode = value,
                CRASH => self.crash = value >= 0.5,
                _ => {}
            }
        }
    }
}

impl Processor<f32> for Recorder {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = Some(config);
        self.values = Vec::with_capacity(4096);
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, f32>) -> ProcessStatus {
        let Some(config) = self.config else {
            block.clear_outputs();
            return ProcessStatus::Fault(ProcessError::NotPrepared);
        };
        if let Err(error) = config.validate_block(block) {
            block.clear_outputs();
            return ProcessStatus::Fault(error);
        }
        let frames = block.audio.frames();
        let mut next = 0;
        for frame in 0..frames {
            while next < block.events.len() && block.events[next].frame_offset <= frame {
                self.apply(frames, &block.events[next]);
                next += 1;
            }
            for channel in 0..block.audio.output_channels() {
                let input = block.audio.input(channel).map_or(0.0, |c| c[frame]);
                block.audio.output_mut(channel).unwrap()[frame] = input * self.gain;
            }
        }
        for event in &block.events[next..] {
            self.apply(frames, event);
        }
        if self.crash {
            panic!("crash requested");
        }
        ProcessStatus::Continue
    }
    fn reset(&mut self) {
        self.resets += 1;
    }
    fn deactivate(&mut self) {
        self.config = None;
    }
    fn latency_samples(&self) -> u32 {
        self.mode as u32 * 10
    }
    fn tail(&self) -> Tail {
        Tail::Samples(42)
    }
}

struct TestPlugin;
impl PluginExport for TestPlugin {
    type Processor = Recorder;
    const PRODUCT: Product<'static> = Product::new(
        "Native Test",
        "RAVET",
        "1.2.3",
        "org.ravet.native-test",
        Category::Effect,
        StateFormat {
            tag: *b"RVTNAT01",
            version: 2,
        },
    )
    .with_layouts(&[AudioLayout::STEREO, AudioLayout::MONO])
    .with_parameters(PARAMETERS)
    .with_bypass(BypassPolicy::Hard)
    .with_editor(Editor {
        html: "<p>test</p>",
        width: 300,
        height: 200,
        resizable: true,
    });
    fn create() -> Recorder {
        Recorder::default()
    }
    fn migrate_state(saved: &mut SavedState) -> Result<(), StateError> {
        // Version 1 stored gain doubled.
        for (id, value) in &mut saved.parameters {
            if id.0 == GAIN {
                *value /= 2.0;
            }
        }
        Ok(())
    }
    fn import_state(bytes: &[u8]) -> Option<SavedState> {
        match bytes {
            b"panic" => panic!("import hook panicked"),
            b"legacy" => Some(SavedState {
                version: 0,
                parameters: vec![(ParameterId(GAIN), 0.25)],
                extra: Vec::new(),
            }),
            _ => None,
        }
    }
}

struct Host;
impl NativeExport for Host {
    type Plugin = TestPlugin;
    fn statics() -> Option<&'static NativeStatics> {
        static STATICS: OnceLock<Option<NativeStatics>> = OnceLock::new();
        STATICS.get_or_init(NativeStatics::new::<Host>).as_ref()
    }
}

fn table() -> &'static RavetPluginEntryV1 {
    // SAFETY: a nonnull table lives as long as the library.
    unsafe { entry::<Host>().as_ref() }.expect("table")
}

fn text(pointer: *const c_char) -> &'static str {
    // SAFETY: table strings are NUL-terminated and live forever.
    unsafe { CStr::from_ptr(pointer) }.to_str().unwrap()
}

fn config(frames: u32, layout: (u32, u32)) -> NativeProcessorConfigV1 {
    let mut config = NativeProcessorConfigV1 {
        version: 1,
        maximum_frames: frames,
        maximum_events: 64,
        maximum_output_events: 0,
        bus_count: 1,
        reserved: 0,
        sample_rate: 48_000.0,
        buses: Default::default(),
    };
    config.buses[0] = NativeBusLayoutV1 {
        inputs: layout.0,
        outputs: layout.1,
    };
    config
}

/// A created and prepared stereo instance.
fn instance() -> *mut c_void {
    let table = table();
    let instance = (table.create)();
    assert!(!instance.is_null());
    // SAFETY: valid instance and configuration.
    let prepared = unsafe { (table.prepare)(instance, &config(512, (2, 2))) };
    assert_eq!(prepared, 0);
    instance
}

fn recorder(instance: *mut c_void) -> &'static Recorder {
    // SAFETY: test-only peek at a stopped instance created by this table.
    let owner = unsafe { &*instance.cast::<Instance<TestPlugin>>() };
    // SAFETY: no processing runs while the test inspects the instance.
    let audio = unsafe { &*owner.audio.get() };
    audio.processor.as_ref().unwrap().processor().processor()
}

fn parameter_value(id: u32, offset: u32, value: f64) -> NativeEventV1 {
    NativeEventV1 {
        offset,
        kind: native_event_kind::PARAMETER_VALUE,
        parameter_id: id,
        value,
        ..NativeEventV1::default()
    }
}

struct Block {
    code: i32,
    output: [Vec<f32>; 2],
    result: NativeProcessorResultV1,
}

fn process(instance: *mut c_void, input: &[f32], events: &[NativeEventV1]) -> Block {
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
    // SAFETY: every pointer is valid for the declared shape.
    let code = unsafe {
        (table().process)(
            instance,
            &bus,
            1,
            frames as u32,
            events.as_ptr(),
            events.len() as u32,
            ptr::null(),
            ptr::null_mut(),
            0,
            &mut result,
        )
    };
    Block {
        code,
        output,
        result,
    }
}

fn destroy(instance: *mut c_void) {
    // SAFETY: created by this table.
    unsafe { (table().destroy)(instance) };
}

#[test]
fn static_data_matches_the_resolved_product() {
    let table = table();
    assert_eq!(table.version, 1);
    assert_eq!(table.byte_size as usize, size_of::<RavetPluginEntryV1>());
    let resolved = TestPlugin::PRODUCT.resolve().unwrap();
    let identities = resolved.identities();
    // SAFETY: the record lives as long as the table.
    let record = unsafe { &*table.product };
    assert_eq!(text(record.name), "Native Test");
    assert_eq!(text(record.vendor), "RAVET");
    assert_eq!(text(record.version), "1.2.3");
    assert_eq!(text(record.plugin_id), "org.ravet.native-test");
    assert_eq!(text(record.url), "");
    assert_eq!(record.category, category::EFFECT);
    assert_eq!((record.note_input, record.note_output), (0, 0));
    assert_eq!(record.parameter_count, 4);
    assert_eq!(record.bypass_index, 3);
    assert_eq!(text(record.clap_id), identities.clap_id);
    assert_eq!(record.vst3_processor_cid, identities.vst3_processor);
    assert_eq!(record.vst3_controller_cid, identities.vst3_controller);
    assert_eq!(record.au_type, u32::from_be_bytes(*b"aufx"));
    assert_eq!(record.au_subtype, u32::from_be_bytes(identities.au_subtype));
    assert_eq!(
        record.au_manufacturer,
        u32::from_be_bytes(identities.au_manufacturer)
    );
    assert_eq!(record.aax_manufacturer, identities.aax_manufacturer);
    assert_eq!(record.aax_product, identities.aax_product);
    assert_eq!(text(record.lv2_uri), identities.lv2_uri);
    assert_eq!(
        (
            record.editor_width,
            record.editor_height,
            record.editor_resizable
        ),
        // Without the `editor` feature the table offers no editor.
        if cfg!(feature = "editor") {
            (300, 200, 1)
        } else {
            (0, 0, 0)
        }
    );
    // SAFETY: layout_count records.
    let layouts = unsafe { slice::from_raw_parts(record.layouts, record.layout_count as usize) };
    assert_eq!(layouts.len(), 2);
    for (layout, (expected, aax)) in layouts.iter().zip(&identities.aax_types) {
        assert_eq!(layout.inputs as usize, expected.input_channels());
        assert_eq!(layout.outputs as usize, expected.output_channels());
        assert_eq!(layout.aax_type_id, *aax);
    }
}

#[test]
fn parameter_metadata_text_and_normalization() {
    let table = table();
    let mut record = RavetPluginParameterV1 {
        id: 0,
        flags: 0,
        scale: 9,
        steps: 9,
        minimum: 0.0,
        maximum: 0.0,
        default_value: 0.0,
        key: ptr::null(),
        name: ptr::null(),
        unit: ptr::null(),
    };
    // SAFETY: valid output record.
    unsafe {
        assert_eq!((table.parameter)(0, &mut record), 0);
        assert_eq!((record.id, record.scale, record.steps), (GAIN, 0, 0));
        assert_eq!(record.flags, parameter_flags::AUTOMATABLE);
        assert_eq!(
            (record.minimum, record.maximum, record.default_value),
            (0.0, 2.0, 1.0)
        );
        assert_eq!(
            (text(record.key), text(record.name), text(record.unit)),
            ("gain", "Gain", "x")
        );
        assert_eq!((table.parameter)(1, &mut record), 0);
        assert_eq!(record.steps, 3);
        assert_eq!(
            record.flags,
            parameter_flags::AUTOMATABLE | parameter_flags::STEPPED
        );
        assert_eq!((table.parameter)(3, &mut record), 0);
        assert_eq!(
            record.flags,
            parameter_flags::AUTOMATABLE | parameter_flags::BYPASS | parameter_flags::STEPPED
        );
        assert_eq!((table.parameter)(4, &mut record), 1);
        assert_eq!((table.parameter)(0, ptr::null_mut()), 1);
    }
    assert_eq!((table.parameter_index)(MODE), 1);
    assert_eq!((table.parameter_index)(BYPASS), 3);
    assert_eq!((table.parameter_index)(999), -1);

    let mut buffer = [0 as c_char; 32];
    let mut value = 0.0;
    // SAFETY: valid buffers.
    unsafe {
        assert_eq!((table.value_to_text)(0, 1.5, buffer.as_mut_ptr(), 32), 0);
        assert_eq!(text(buffer.as_ptr()), "1.50 x");
        assert_eq!((table.text_to_value)(0, buffer.as_ptr(), &mut value), 0);
        assert_eq!(value, 1.5);
        assert_eq!((table.value_to_text)(3, 1.0, buffer.as_mut_ptr(), 32), 0);
        assert_eq!(text(buffer.as_ptr()), "On");
        // Truncated text is NUL-terminated and reported.
        assert_eq!((table.value_to_text)(0, 1.5, buffer.as_mut_ptr(), 4), 1);
        assert_eq!(text(buffer.as_ptr()), "1.5");
        assert_eq!(
            (table.text_to_value)(0, c"nonsense".as_ptr(), &mut value),
            1
        );
        assert_eq!((table.value_to_text)(9, 1.0, buffer.as_mut_ptr(), 32), 1);
        assert_eq!((table.normalize)(0, 0.5, &mut value), 0);
        assert_eq!(value, 0.25);
        assert_eq!((table.normalize)(0, 7.0, &mut value), 0);
        assert_eq!(value, 1.0, "clamped");
        assert_eq!((table.denormalize)(1, 0.4, &mut value), 0);
        assert_eq!(value, 1.0, "snapped to a step");
        assert_eq!((table.normalize)(0, f64::NAN, &mut value), 1);
    }
}

#[test]
fn host_changes_reach_the_processor_and_saved_state() {
    let table = table();
    let instance = instance();
    let input = [0.5f32; 16];
    let block = process(instance, &input, &[]);
    assert_eq!(block.code, 0);
    assert_eq!(
        block.result,
        NativeProcessorResultV1 {
            status: 0,
            event_count: 0
        }
    );
    assert_eq!(block.output[0], input);

    // SAFETY: valid instance.
    unsafe {
        assert_eq!((table.set_value)(instance, 0, 1.5), 0);
        assert_eq!((table.get_value)(instance, 0), 1.5);
        assert_eq!((table.set_value)(instance, 0, f64::NAN), 1);
        assert_eq!((table.set_value)(instance, 9, 1.0), 1);
        assert!((table.get_value)(instance, 9).is_nan());
    }
    let mut length = 0;
    let mut bytes = vec![0u8; 256];
    // SAFETY: valid buffers.
    unsafe {
        assert_eq!(
            (table.save_state)(instance, ptr::null_mut(), 0, &mut length),
            0
        );
        assert_eq!(
            (table.save_state)(instance, bytes.as_mut_ptr(), 4, &mut length),
            1
        );
        assert_eq!(
            (table.save_state)(instance, bytes.as_mut_ptr(), 256, &mut length),
            0
        );
    }
    let product = TestPlugin::PRODUCT.resolve().unwrap();
    let restored = state::restore(&product, &bytes[..length as usize], &state::NoHooks).unwrap();
    assert_eq!(restored.values, [1.5, 0.0, 0.0, 0.0]);

    // The cached change arrives at offset 0 of the next block.
    let block = process(instance, &input, &[]);
    assert_eq!(block.output[0], [0.75; 16]);
    // Host automation keeps its offset, and need not be an exact float.
    let events = [
        parameter_value(999, 0, 1.0),
        parameter_value(GAIN, 4, 0.1),
        parameter_value(GAIN, 8, f64::INFINITY),
    ];
    let block = process(instance, &input, &events);
    assert_eq!(block.code, 0);
    assert_eq!(block.output[1][3], 0.75);
    assert_eq!(block.output[1][4], 0.5 * 0.1f32);
    assert_eq!(block.output[1][15], 0.5 * 0.1f32);
    // SAFETY: valid instance.
    assert_eq!(unsafe { (table.get_value)(instance, 0) }, f64::from(0.1f32));
    // Out-of-range offsets reject the block and silence it.
    let block = process(instance, &input, &[parameter_value(GAIN, 16, 1.0)]);
    assert_eq!(block.code, 1);
    assert_eq!(block.result.status, 2);
    assert_eq!(block.output[0], [0.0; 16]);
    destroy(instance);
}

#[test]
fn loading_state_resets_and_delivers_every_value() {
    let table = table();
    let instance = instance();
    let product = TestPlugin::PRODUCT.resolve().unwrap();
    let resets = recorder(instance).resets;
    let bytes = state::encode(&product, &[0.5, 2.0, 0.0, 0.0], &[]).unwrap();
    // SAFETY: valid buffers.
    unsafe {
        assert_eq!(
            (table.load_state)(instance, bytes.as_ptr(), bytes.len() as u32),
            0
        );
        assert_eq!((table.get_value)(instance, 1), 2.0);
        assert_eq!((table.load_state)(instance, b"junk".as_ptr(), 4), 1);
        assert_eq!((table.get_value)(instance, 1), 2.0, "nothing applied");
    }
    let before = recorder(instance).values.len();
    let block = process(instance, &[1.0; 8], &[]);
    assert_eq!(block.output[0], [0.5; 8]);
    let recorder = recorder(instance);
    assert_eq!(recorder.resets, resets + 1);
    // Every value except the consumed bypass, in a zero-frame block.
    assert_eq!(
        recorder.values[before..],
        [(0, GAIN, 0.5), (0, MODE, 2.0), (0, CRASH, 0.0)]
    );
    // Earlier versions migrate and foreign data imports.
    let mut old = state::encode(&product, &[1.0, 0.0, 0.0, 0.0], &[]).unwrap();
    old[20..24].copy_from_slice(&1u32.to_le_bytes());
    // SAFETY: valid buffers.
    unsafe {
        assert_eq!(
            (table.load_state)(instance, old.as_ptr(), old.len() as u32),
            0
        );
        assert_eq!((table.get_value)(instance, 0), 0.5);
        assert_eq!((table.load_state)(instance, b"legacy".as_ptr(), 6), 0);
        assert_eq!((table.get_value)(instance, 0), 0.25);
    }
    destroy(instance);
}

#[test]
fn hard_bypass_policy_applies_through_the_table() {
    let instance = instance();
    let input: Vec<f32> = (0..8).map(|i| i as f32).collect();
    // SAFETY: valid instance.
    unsafe { (table().set_value)(instance, 0, 2.0) };
    let block = process(instance, &input, &[parameter_value(BYPASS, 4, 1.0)]);
    assert_eq!(block.output[0], [0.0, 2.0, 4.0, 6.0, 4.0, 5.0, 6.0, 7.0]);
    assert!(
        recorder(instance)
            .values
            .iter()
            .all(|(_, id, _)| *id != BYPASS),
        "the bypass parameter is consumed"
    );
    destroy(instance);
}

#[test]
fn latency_changes_are_reported_until_read() {
    let table = table();
    let instance = instance();
    let mut timing = NativeProcessorTimingV1::default();
    // SAFETY: valid instance and output.
    unsafe {
        assert_eq!((table.latency_changed)(instance), 0);
        (table.set_value)(instance, 1, 3.0);
        process(instance, &[0.0; 4], &[]);
        assert_eq!((table.latency_changed)(instance), 1);
        assert_eq!((table.timing)(instance, &mut timing), 0);
        assert_eq!(
            timing,
            NativeProcessorTimingV1 {
                latency_samples: 30,
                tail_kind: 1,
                tail_samples: 42
            }
        );
        assert_eq!((table.latency_changed)(instance), 0);
    }
    destroy(instance);
}

#[test]
fn preparation_validates_the_configuration() {
    let table = table();
    let instance = (table.create)();
    // SAFETY: valid instance; configurations are valid memory.
    unsafe {
        assert_eq!((table.prepare)(instance, &config(512, (2, 1))), 1);
        let mut two_buses = config(512, (2, 2));
        two_buses.bus_count = 2;
        two_buses.buses[1] = NativeBusLayoutV1 {
            inputs: 2,
            outputs: 0,
        };
        assert_eq!((table.prepare)(instance, &two_buses), 1);
        assert_eq!((table.prepare)(instance, ptr::null()), 1);
        assert_eq!(process(instance, &[0.0; 4], &[]).code, 1, "not prepared");
        assert_eq!((table.prepare)(instance, &config(512, (1, 1))), 0);
        assert_eq!(process(instance, &[0.0; 4], &[]).code, 1, "wrong bus shape");
        assert_eq!((table.deactivate)(instance), 0);
        assert_eq!((table.reset)(instance), 1);
    }
    destroy(instance);
}

#[test]
fn null_and_misaligned_instances_are_rejected() {
    let table = table();
    let instance = instance();
    // An address no `create` returns; it is never dereferenced.
    let misaligned = instance.cast::<u8>().wrapping_add(1).cast::<c_void>();
    let mut length = 0;
    let mut timing = NativeProcessorTimingV1::default();
    for pointer in [ptr::null_mut(), misaligned] {
        // SAFETY: the table rejects these pointers before dereferencing them.
        unsafe {
            assert!((table.get_value)(pointer, 0).is_nan());
            assert_eq!((table.set_value)(pointer, 0, 1.0), 1);
            assert_eq!(
                (table.save_state)(pointer, ptr::null_mut(), 0, &mut length),
                1
            );
            assert_eq!((table.reset)(pointer), 1);
            assert_eq!((table.timing)(pointer, &mut timing), 1);
            assert_eq!((table.latency_changed)(pointer), 0);
            (table.editor_idle)(pointer);
        }
        assert_eq!(process(pointer, &[1.0; 8], &[]).code, 1);
    }
    // The real instance is untouched.
    assert_eq!(process(instance, &[1.0; 8], &[]).code, 0);
    destroy(instance);
}

#[test]
fn panics_return_2_and_fault_processing_until_prepare() {
    let table = table();
    let instance = instance();
    // SAFETY: valid instance.
    unsafe { (table.set_value)(instance, 2, 1.0) };
    let block = process(instance, &[1.0; 8], &[]);
    assert_eq!(block.code, 2);
    assert_eq!(block.result.status, 2);
    assert_eq!(block.output[0], [0.0; 8], "silenced");
    let mut timing = NativeProcessorTimingV1::default();
    // SAFETY: valid instance and output.
    unsafe {
        assert_eq!((table.reset)(instance), 2);
        assert_eq!((table.timing)(instance, &mut timing), 2);
        assert_eq!(process(instance, &[1.0; 8], &[]).code, 2);
        // Parameters and state keep working.
        assert_eq!((table.set_value)(instance, 2, 0.0), 0);
        let mut length = 0;
        assert_eq!(
            (table.save_state)(instance, ptr::null_mut(), 0, &mut length),
            0
        );
        // A panicking state hook returns 2 and applies nothing.
        assert_eq!((table.load_state)(instance, b"panic".as_ptr(), 5), 2);
        // Prepare recreates the processor and clears the fault.
        assert_eq!((table.prepare)(instance, &config(512, (2, 2))), 0);
    }
    let block = process(instance, &[1.0; 8], &[]);
    assert_eq!(block.code, 0);
    assert_eq!(block.output[0], [1.0; 8]);
    destroy(instance);
}

#[test]
fn processing_and_reset_allocate_nothing() {
    let table = table();
    let instance = instance();
    let product = TestPlugin::PRODUCT.resolve().unwrap();
    let bytes = state::encode(&product, &[0.5, 1.0, 0.0, 0.0], &[]).unwrap();
    let input = [0.25f32; 256];
    let mut output = [[0.0f32; 256]; 2];
    let inputs = [input.as_ptr(), input.as_ptr()];
    let outputs = [output[0].as_mut_ptr(), output[1].as_mut_ptr()];
    let bus = NativeProcessorBusV1 {
        input_channels: 2,
        output_channels: 2,
        inputs: inputs.as_ptr().cast(),
        outputs: outputs.as_ptr().cast(),
    };
    let transport = NativeTransportV1::from_transport(Transport {
        tempo: Some(120.0),
        ..Transport::default()
    })
    .unwrap();
    let mut result = NativeProcessorResultV1 {
        status: 0,
        event_count: 0,
    };
    // A restore makes the next block synchronize every value.
    // SAFETY: valid buffers.
    unsafe { (table.load_state)(instance, bytes.as_ptr(), bytes.len() as u32) };
    let measured = allocation_counter::measure(|| {
        for i in 0..500 {
            let events = [
                parameter_value(GAIN, 3, 0.3 + f64::from(i % 7) * 0.1),
                parameter_value(BYPASS, 100, f64::from(i % 2)),
            ];
            // SAFETY: valid instance and buffers.
            unsafe {
                (table.set_value)(instance, 1, f64::from(i % 3));
                let code = (table.process)(
                    instance,
                    &bus,
                    1,
                    256,
                    events.as_ptr(),
                    2,
                    &transport,
                    ptr::null_mut(),
                    0,
                    &mut result,
                );
                assert_eq!(code, 0);
            }
        }
        // SAFETY: valid instance.
        assert_eq!(unsafe { (table.reset)(instance) }, 0);
    });
    assert_eq!(measured.count_total, 0);
    assert!(output.iter().flatten().all(|s| s.is_finite()));
    destroy(instance);
}

#[cfg(all(
    feature = "editor",
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
#[test]
fn editor_edits_flow_to_take_edits_and_the_processor() {
    use ravet_webview::Callbacks;
    let table = table();
    let instance = instance();
    // SAFETY: test-only access to the instance's shared state.
    let shared = unsafe { &*instance.cast::<Instance<TestPlugin>>() }
        .shared
        .clone();
    let mut bridge = crate::editor_host::bridge(NativeTarget {
        shared,
        editor_message: TestPlugin::editor_message,
    });
    bridge.message(r#"{"type":"ready","protocol":1}"#);
    assert!(bridge.snapshot().unwrap().contains("\"info\""));
    bridge.message(r#"{"type":"commit","key":"gain","value":0.5}"#);
    // SAFETY: valid instance.
    assert_eq!(unsafe { (table.get_value)(instance, 0) }, 0.5);
    let mut edits = [RavetPluginEditV1::default(); 2];
    // SAFETY: valid buffers.
    unsafe {
        assert_eq!((table.take_edits)(instance, edits.as_mut_ptr(), 2), 2);
        assert_eq!(
            edits[0],
            RavetPluginEditV1 {
                kind: edit_kind::BEGIN,
                index: 0,
                value: 0.0
            }
        );
        assert_eq!(
            edits[1],
            RavetPluginEditV1 {
                kind: edit_kind::VALUE,
                index: 0,
                value: 0.5
            }
        );
        assert_eq!((table.take_edits)(instance, edits.as_mut_ptr(), 2), 1);
        assert_eq!(edits[0].kind, edit_kind::END);
        assert_eq!((table.take_edits)(instance, edits.as_mut_ptr(), 2), 0);
    }
    let block = process(instance, &[1.0; 4], &[]);
    assert_eq!(block.output[0], [0.5; 4]);
    drop(bridge);
    destroy(instance);
}

#[test]
fn editor_open_rejects_unsupported_views() {
    let table = table();
    let instance = instance();
    let mut parent = 0u8;
    // SAFETY: the view kinds are rejected before the parent is used.
    unsafe {
        let parent = (&mut parent as *mut u8).cast();
        // Only the platform's own view kind is accepted (and only with the
        // editor feature); the others, unknown kinds and null parents are not.
        for kind in [view_kind::COCOA, view_kind::WIN32, view_kind::X11, 0, 4] {
            if kind != PLATFORM_VIEW_KIND {
                assert_eq!((table.editor_open)(instance, kind, parent, 300, 200), 1);
            }
        }
        assert_eq!(
            (table.editor_open)(instance, PLATFORM_VIEW_KIND, ptr::null_mut(), 300, 200),
            1
        );
        assert_eq!((table.editor_resize)(instance, 300, 200), 1, "not open");
        (table.editor_idle)(instance);
        (table.editor_close)(instance);
    }
    destroy(instance);
}

/// Compile a C program against the header and compare every offset and size
/// with these Rust definitions. Skipped when no C compiler is installed.
#[test]
fn rust_records_match_the_c_header() {
    use std::{mem::offset_of, process::Command};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut checks: Vec<(String, usize)> = Vec::new();
    macro_rules! check {
        ($c:literal, $rust:ty; $($field:ident),*) => {
            checks.push((format!("sizeof({})", $c), size_of::<$rust>()));
            $(checks.push((
                format!("offsetof({}, {})", $c, stringify!($field)),
                offset_of!($rust, $field),
            ));)*
        };
    }
    check!("RavetPluginLayoutV1", RavetPluginLayoutV1; inputs, outputs, aax_type_id, reserved);
    check!("RavetPluginProductV1", RavetPluginProductV1; name, vendor, version, plugin_id, url,
        category, note_input, note_output, layout_count, layouts, parameter_count, bypass_index,
        clap_id, vst3_processor_cid, vst3_controller_cid, au_type, au_subtype, au_manufacturer,
        aax_manufacturer, aax_product, lv2_uri, editor_width, editor_height, editor_resizable,
        reserved);
    check!("RavetPluginParameterV1", RavetPluginParameterV1; id, flags, scale, steps, minimum,
        maximum, default_value, key, name, unit);
    check!("RavetPluginEditV1", RavetPluginEditV1; kind, index, value);
    check!("RavetPluginEntryV1", RavetPluginEntryV1; version, byte_size, product, parameter,
        parameter_index, value_to_text, text_to_value, normalize, denormalize, create, destroy,
        get_value, set_value, save_state, load_state, prepare, process, reset, deactivate, timing,
        latency_changed, take_edits, editor_open, editor_resize, editor_close, editor_idle);
    check!("RavetNativeProcessorConfigV1", NativeProcessorConfigV1; version, maximum_frames,
        maximum_events, maximum_output_events, bus_count, reserved, sample_rate, buses);
    check!("RavetNativeProcessorBusV1", NativeProcessorBusV1; input_channels, output_channels,
        inputs, outputs);
    check!("RavetNativeEventV1", NativeEventV1; offset, kind, parameter_id, note_id, scope,
        channel, key, detail, value, words);
    check!("RavetNativeTransportV1", NativeTransportV1; present, flags, sample_position,
        beat_position, tempo, tempo_increment, beat_loop_start, beat_loop_end, sample_loop_start,
        sample_loop_end, bar_start, bar_number, signature_numerator, signature_denominator,
        reserved);
    let mut program = String::from(
        "#include <stdio.h>\n#include <stddef.h>\n#include \"RavetPlugin.h\"\nint main(void) {\n",
    );
    for (expression, _) in &checks {
        program.push_str(&format!("printf(\"%zu\\n\", (size_t){expression});\n"));
    }
    program.push_str("return 0; }\n");
    let directory =
        std::env::temp_dir().join(format!("ravet-plugin-layout-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let source = directory.join("layout.c");
    let binary = directory.join("layout");
    std::fs::write(&source, program).unwrap();
    let compiled = Command::new(if cfg!(windows) { "clang" } else { "cc" })
        .arg("-std=c99")
        .arg("-Wall")
        .arg("-Werror")
        .arg("-I")
        .arg(root.join("include"))
        .arg("-I")
        .arg(root.join("../ravet-native/include"))
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .status();
    let Ok(status) = compiled else {
        eprintln!("no C compiler; layout check skipped");
        return;
    };
    assert!(status.success(), "the header must compile as C99");
    let output = Command::new(&binary).output().unwrap();
    let values: Vec<usize> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.parse().unwrap())
        .collect();
    assert_eq!(values.len(), checks.len());
    for ((expression, rust), c) in checks.iter().zip(values) {
        assert_eq!(*rust, c, "{expression}");
    }
    let _ = std::fs::remove_dir_all(directory);
}
