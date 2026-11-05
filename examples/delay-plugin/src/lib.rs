//! A tempo-synced stereo delay exported through RAVET's generic plug-in layer.
//!
//! [`dsp::SyncedDelay`] is an ordinary RAVET [`Processor`](ravet_foundation::Processor)
//! with no knowledge of plug-in formats. This crate root only describes the
//! product: its identity, parameters and web editor. Parameter IDs are explicit
//! and match the processor's constants, so saved sessions keep loading if
//! parameters are added or reordered.
pub mod dsp;

use dsp::{DEFAULTS, DIVISIONS, MAXIMUM_TIME_MS, MINIMUM_TIME_MS, SyncedDelay};
use ravet_foundation::AudioLayout;
use ravet_plugin::{Category, Editor, Parameter, PluginExport, Product, StateFormat};

const PARAMETERS: &[Parameter] = &[
    Parameter::logarithmic(
        "time",
        "Time",
        MINIMUM_TIME_MS,
        MAXIMUM_TIME_MS,
        DEFAULTS[0],
    )
    .with_id(dsp::TIME.0)
    .with_unit("ms"),
    Parameter::linear("feedback", "Feedback", 0.0, 95.0, DEFAULTS[1])
        .with_id(dsp::FEEDBACK.0)
        .with_unit("%"),
    Parameter::linear("mix", "Mix", 0.0, 100.0, DEFAULTS[2])
        .with_id(dsp::MIX.0)
        .with_unit("%"),
    Parameter::bypass("bypass", "Bypass").with_id(dsp::BYPASS.0),
    Parameter::toggle("ping_pong", "Ping-pong", false).with_id(dsp::PING_PONG.0),
    Parameter::toggle("sync", "Sync", true).with_id(dsp::SYNC.0),
    Parameter::stepped(
        "division",
        "Division",
        0.0,
        (DIVISIONS.len() - 1) as f32,
        (DIVISIONS.len() - 1) as u16,
        DEFAULTS[6],
    )
    .with_id(dsp::DIVISION.0),
    Parameter::logarithmic("tone", "Tone", 500.0, 20_000.0, DEFAULTS[7])
        .with_id(dsp::TONE.0)
        .with_unit("Hz"),
];

/// The exported product.
pub struct DelayPlugin;

impl PluginExport for DelayPlugin {
    type Processor = SyncedDelay;
    const PRODUCT: Product<'static> = Product::new(
        "RAVET Delay",
        "RAVET",
        "0.1.0",
        "org.ravet.delay",
        Category::Effect,
        StateFormat {
            tag: *b"RVTDLY01",
            version: 1,
        },
    )
    .with_layouts(&[AudioLayout::STEREO, AudioLayout::MONO])
    .with_parameters(PARAMETERS)
    .with_editor(Editor {
        html: include_str!("../editor/index.html"),
        width: 560,
        height: 340,
        resizable: true,
    })
    .with_url("https://djwaudio.co/ravet/");
    fn create() -> SyncedDelay {
        SyncedDelay::default()
    }
}

ravet_plugin::export_plugin!(DelayPlugin);
