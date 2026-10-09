mod support;
#[test]
fn linked_native_sidechain_reference_contracts() {
    let entry = clack_host::prelude::PluginEntry::load_from_clack::<
        clack_plugin::entry::SinglePluginEntry<ravet_sidechain_clap::SidechainPlugin>,
    >(c"sidechain-reference")
    .unwrap();
    assert_eq!(support::exercise(&entry).unwrap(), 6006);
}
