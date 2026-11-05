//! Deterministic one-shot MIDI instrument for generic export and host checks.
//!
//! This deliberately uses a fixed 64-sample bipolar click instead of a musical
//! voice so rendered onsets, amplitude, finite tail and PDC can be checked exactly.
pub mod dsp;
use ravet_plugin::{Category, NotePorts, Parameter, PluginExport, Product, StateFormat};

/// Generic export of [`dsp::EvidenceSynth`]. No editor or audio input is needed.
pub struct SynthPlugin;
impl PluginExport for SynthPlugin {
    type Processor = dsp::EvidenceSynth;
    const PRODUCT: Product<'static> = Product::new(
        "RAVET Evidence Synth",
        "RAVET",
        "0.1.0",
        "org.ravet.evidence-synth",
        Category::Instrument,
        StateFormat {
            tag: *b"RVTSYN01",
            version: 1,
        },
    )
    .with_notes(NotePorts {
        input: true,
        output: true,
    })
    .with_parameters(&[
        Parameter::toggle("cc_response", "CC response", true).with_id(dsp::CC_RESPONSE.0),
        Parameter::toggle("lookahead", "Lookahead", true).with_id(dsp::LOOKAHEAD.0),
    ]);
    fn create() -> Self::Processor {
        dsp::EvidenceSynth::default()
    }
}
ravet_plugin::export_plugin!(SynthPlugin);
