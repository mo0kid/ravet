//! The gain reference exported through RAVET's generic plug-in layer.
//!
//! The existing `GainProcessor` is exported unchanged: this crate only describes
//! the product. Parameter IDs 1 and 2 match the processor's schema.
use ravet_foundation::AudioLayout;
use ravet_gain::{GainProcessor, MAXIMUM_GAIN};
use ravet_plugin::{Category, Editor, EditorAsset, Parameter, PluginExport, Product, StateFormat};

const PARAMETERS: &[Parameter] = &[
    Parameter::linear("gain", "Gain", 0.0, MAXIMUM_GAIN, 1.0)
        .with_id(1)
        .with_unit("x"),
    Parameter::bypass("bypass", "Bypass").with_id(2),
];

/// The exported product.
pub struct GainPlugin;

impl PluginExport for GainPlugin {
    type Processor = GainProcessor;
    const PRODUCT: Product<'static> = Product::new(
        "RAVET Gain (generic)",
        "RAVET",
        "0.1.0",
        "org.ravet.gain-generic",
        Category::Effect,
        StateFormat {
            tag: *b"RVTGAIN2",
            version: 1,
        },
    )
    .with_layouts(&[AudioLayout::STEREO, AudioLayout::MONO])
    .with_parameters(PARAMETERS)
    .with_editor(Editor {
        html: include_str!("../editor/index.html"),
        width: 480,
        height: 320,
        resizable: true,
    })
    // The page links this stylesheet by relative URL; see `EditorAsset`.
    .with_editor_assets(&[EditorAsset::new(
        "asset.css",
        include_bytes!("../editor/asset.css"),
    )])
    .with_url("https://djwaudio.co/ravet/");
    fn create() -> GainProcessor {
        GainProcessor::default()
    }
}

ravet_plugin::export_plugin!(GainPlugin);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_description_is_valid() {
        let product = GainPlugin::PRODUCT.resolve().unwrap();
        assert_eq!(product.parameter_ids()[0].0, ravet_gain::GAIN_ID.0);
        assert_eq!(product.parameter_ids()[1].0, ravet_gain::BYPASS_ID.0);
        assert_eq!(product.bypass_index(), Some(1));
    }
}
