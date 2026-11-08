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
// Every unsafe block in library code states why it is sound.
#![deny(clippy::undocumented_unsafe_blocks)]

#[cfg(feature = "clap")]
pub mod clap;
#[cfg(all(
    feature = "clap",
    feature = "editor",
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
mod clap_gui;
pub mod editor;
#[cfg(all(
    any(feature = "clap", feature = "native"),
    feature = "editor",
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
mod editor_host;
pub mod export;
pub mod identity;
pub mod policy;
pub mod product;
pub mod state;
pub mod text;

pub use export::{Edit, EditQueue, FnHooks, ParameterCache, PluginExport};
pub use identity::{Identities, IdentityOverrides};
pub use policy::{AutomationPolicy, BypassPolicy, PolicyProcessor};
pub use product::{
    AuxiliaryBus, Category, Editor, EditorAsset, MAXIMUM_BUS_NAME_BYTES,
    MAXIMUM_EDITOR_ASSET_BYTES, MAXIMUM_EDITOR_ASSETS, MAXIMUM_EDITOR_ASSETS_TOTAL_BYTES,
    MAXIMUM_EDITOR_HTML_BYTES, MAXIMUM_KEY_BYTES, MAXIMUM_PARAMETERS, NotePorts, Parameter,
    ParameterRole, Precision, Product, ProductError, ResolvedProduct, StateFormat,
    derive_parameter_id,
};

#[cfg(feature = "native")]
#[allow(unsafe_code)]
pub mod native;

#[doc(hidden)]
#[cfg(feature = "clap")]
pub mod __private {
    pub use clack_plugin;
}

/// Export a [`PluginExport`] as a plug-in from this library crate.
///
/// Call it once, at the crate root of a `cdylib`, with the export type in
/// scope. With feature `clap` it exports the CLAP entry (`clap_entry`); with
/// feature `native` it exports `ravet_plugin_v1`, the generic C table
/// ([`native`], `include/RavetPlugin.h`) that the VST3, AUv2, AAX and LV2
/// adapters load. The calling crate must not `forbid(unsafe_code)`: the entry
/// points are foreign functions. `deny` is fine.
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
            $crate::__export_clap!($plugin);
            $crate::__export_native!($plugin);
        }
    };
}

#[doc(hidden)]
#[cfg(feature = "clap")]
#[macro_export]
macro_rules! __export_clap {
    ($plugin:ty) => {
        $crate::__private::clack_plugin::clack_export_entry!(
            $crate::__private::clack_plugin::entry::SinglePluginEntry<
                $crate::clap::ClapPlugin<$plugin>,
            >
        );
    };
}
#[doc(hidden)]
#[cfg(not(feature = "clap"))]
#[macro_export]
macro_rules! __export_clap {
    ($plugin:ty) => {};
}

#[doc(hidden)]
#[cfg(feature = "native")]
#[macro_export]
macro_rules! __export_native {
    ($plugin:ty) => {
        /// Connects the export to its static native table data.
        pub struct NativeExport;
        impl $crate::native::NativeExport for NativeExport {
            type Plugin = $plugin;
            fn statics() -> ::core::option::Option<&'static $crate::native::NativeStatics> {
                static STATICS: ::std::sync::OnceLock<
                    ::core::option::Option<$crate::native::NativeStatics>,
                > = ::std::sync::OnceLock::new();
                STATICS
                    .get_or_init($crate::native::NativeStatics::new::<Self>)
                    .as_ref()
            }
        }
        /// Run a C callback under a current-thread denormal scope.
        /// See `include/RavetPlugin.h` for ownership and return codes.
        ///
        /// # Safety
        /// The callback/context must obey `ravet_plugin::native::with_denormal_mode`.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn ravet_plugin_with_denormal_mode_v1(
            mode: u32,
            callback: Option<unsafe extern "C" fn(*mut ::core::ffi::c_void)>,
            context: *mut ::core::ffi::c_void,
        ) -> i32 {
            // SAFETY: forwarded unchanged under the caller's callback contract.
            unsafe { $crate::native::with_denormal_mode(mode, callback, context) }
        }
        /// The generic RAVET plug-in table (`include/RavetPlugin.h`), or null
        /// when the product description is invalid.
        #[unsafe(no_mangle)]
        pub extern "C" fn ravet_plugin_v1() -> *const $crate::native::RavetPluginEntryV1 {
            $crate::native::entry::<NativeExport>()
        }
    };
}
#[doc(hidden)]
#[cfg(not(feature = "native"))]
#[macro_export]
macro_rules! __export_native {
    ($plugin:ty) => {};
}
