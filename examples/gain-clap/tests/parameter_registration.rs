use clack_extensions::params::{ParamInfoBuffer, ParamInfoFlags, PluginParams};
use clack_host::prelude::*;
use clack_plugin::entry::SinglePluginEntry;
use ravet_gain_clap::GainPlugin;
#[test]
fn registered_gain_metadata_reaches_the_actual_host_extension() {
    let entry =
        PluginEntry::load_from_clack::<SinglePluginEntry<GainPlugin>>(c"ravet-gain.clap").unwrap();
    let mut instance = ravet_clap_smoke::instance(&entry, c"org.ravet.gain").unwrap();
    let handle = instance.plugin_handle();
    let params = handle.get_extension::<PluginParams>().unwrap();
    let mut buffer = ParamInfoBuffer::new();
    assert_eq!(params.count(&handle), 2);
    for (index, id, name, maximum, default, extra) in [
        (0, 1, b"Gain".as_slice(), 4., 1., ParamInfoFlags::empty()),
        (
            1,
            2,
            b"Bypass".as_slice(),
            1.,
            0.,
            ParamInfoFlags::IS_STEPPED | ParamInfoFlags::IS_BYPASS,
        ),
    ] {
        let info = params.get_info(&handle, index, &mut buffer).unwrap();
        assert_eq!(info.id.get(), id);
        assert_eq!(info.name, name);
        assert_eq!(info.module, b"");
        assert_eq!(
            (info.min_value, info.max_value, info.default_value),
            (0., maximum, default)
        );
        assert_eq!(
            info.flags,
            ParamInfoFlags::IS_AUTOMATABLE | ParamInfoFlags::REQUIRES_PROCESS | extra
        );
    }
    assert!(params.get_info(&handle, 2, &mut buffer).is_none());
}
