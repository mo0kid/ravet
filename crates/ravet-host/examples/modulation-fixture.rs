//! Public synthetic global modulation fixture, independent of products.
#[path = "support/parameter_fixture.rs"]
mod fixture;
#[allow(unsafe_code)]
mod export {
    clack_plugin::clack_export_entry!(
        clack_plugin::entry::SinglePluginEntry<super::fixture::Fixture<false, true>>
    );
}
