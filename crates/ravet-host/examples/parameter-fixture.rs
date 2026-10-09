//! Public synthetic stepped gain for loaded native f32/f64 parameter graph probes.
mod support {
    pub mod parameter_fixture;
}
#[allow(unsafe_code)]
mod export {
    clack_plugin::clack_export_entry!(
        clack_plugin::entry::SinglePluginEntry<super::support::parameter_fixture::Fixture>
    );
}
