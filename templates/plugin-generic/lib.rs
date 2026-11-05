// Consumer-owned product and DSP. Edit these files; Foundry preserves them.
mod dsp;
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/.foundry/product.rs"));
use ravet_foundation::AudioLayout;
use ravet_plugin::{Category, Parameter, PluginExport, Product, StateFormat};

pub struct MyPlugin;
impl PluginExport for MyPlugin {
    type Processor = dsp::MyProcessor;
    const PRODUCT: Product<'static> = with_editor(Product::new(
        env!("RAVET_PRODUCT_NAME"), env!("RAVET_COMPANY"), env!("CARGO_PKG_VERSION"),
        env!("RAVET_PLUGIN_ID"), Category::@CATEGORY@,
        StateFormat { tag: STATE_TAG, version: 1 },
    )
    .with_layouts(&[AudioLayout::@LAYOUT@])
    .with_parameters(&[
        Parameter::linear("gain", "Gain", 0.0, 1.0, 0.5).with_id(1),
        Parameter::bypass("bypass", "Bypass").with_id(2),
    ])
    @NOTES@
    );
    fn create() -> Self::Processor { dsp::MyProcessor::default() }
}
ravet_plugin::export_plugin!(MyPlugin);

const fn with_editor(product: Product<'static>) -> Product<'static> {
    #[cfg(feature = "native-editor")]
    { product.with_editor(ravet_plugin::Editor {
        html: include_str!("../editor/index.html"), width: 480, height: 320, resizable: true,
    }) }
    #[cfg(not(feature = "native-editor"))]
    { product }
}
