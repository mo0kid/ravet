//! Product descriptions, format identities and versioned state for RAVET's
//! generic plug-in export layer ([decision 0281]).
//!
//! A [`Product`] describes a plug-in once: names, plug-in ID, category, bus
//! layouts, notes, precision and parameters. [`Product::resolve`] validates it,
//! resolves permanent parameter IDs and derives every format's identities. The
//! [`state`] module defines the one saved-state envelope all formats store, and
//! [`editor`] defines the message protocol between a plug-in and its web editor.
//!
//! ```
//! use ravet_plugin::{Category, Parameter, Product, StateFormat, state};
//!
//! const PARAMETERS: &[Parameter] = &[
//!     Parameter::linear("gain_db", "Gain", -60.0, 12.0, 0.0).with_unit("dB"),
//!     Parameter::bypass("bypass", "Bypass"),
//! ];
//! const PRODUCT: Product = Product::new(
//!     "Example Gain",
//!     "Example Audio",
//!     "1.0.0",
//!     "com.example.gain",
//!     Category::Effect,
//!     StateFormat { tag: *b"EXGAIN01", version: 1 },
//! )
//! .with_parameters(PARAMETERS);
//!
//! let product = PRODUCT.resolve().unwrap();
//! let bytes = state::encode(&product, &[-6.0, 0.0], &[]).unwrap();
//! let restored = state::restore(&product, &bytes, &state::NoHooks).unwrap();
//! assert_eq!(restored.values, [-6.0, 0.0]);
//! ```
//!
//! This crate is a prototype. It contains no format adapters yet; those follow
//! in the export layer's later steps. Everything here runs on control threads.
//!
//! [decision 0281]: https://github.com/mo0kid/ravet/blob/main/docs/decisions/0281-generic-export-layer.md

#[cfg(feature = "clap")]
pub mod clap;
#[cfg(all(feature = "clap", target_os = "macos"))]
mod clap_gui;
pub mod editor;
pub mod export;
pub mod identity;
pub mod product;
pub mod state;

pub use export::{Edit, EditQueue, ParameterCache, PluginExport};
pub use identity::{Identities, IdentityOverrides};
pub use product::{
    Category, Editor, MAXIMUM_EDITOR_HTML_BYTES, MAXIMUM_KEY_BYTES, MAXIMUM_PARAMETERS, NotePorts,
    Parameter, ParameterRole, Precision, Product, ProductError, ResolvedProduct, StateFormat,
    derive_parameter_id,
};

#[doc(hidden)]
#[cfg(feature = "clap")]
pub mod __private {
    pub use clack_plugin;
}

/// Export a [`PluginExport`] as a plug-in from this library crate.
///
/// Call it once, at the crate root of a `cdylib`, with the export type in
/// scope. It currently exports CLAP (feature `clap`); later formats join it.
/// The calling crate must not `forbid(unsafe_code)`: the CLAP entry point is a
/// foreign function. `deny` is fine.
///
/// ```ignore
/// ravet_plugin::export_plugin!(MyPlugin);
/// ```
#[macro_export]
macro_rules! export_plugin {
    ($plugin:ty) => {
        #[doc(hidden)]
        #[allow(unsafe_code)]
        pub mod __ravet_plugin_export {
            #[allow(unused_imports)]
            use super::*;
            $crate::__private::clack_plugin::clack_export_entry!(
                $crate::__private::clack_plugin::entry::SinglePluginEntry<
                    $crate::clap::ClapPlugin<$plugin>,
                >
            );
        }
    };
}
