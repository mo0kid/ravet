mod support;
#[test]
fn stereo_midi_instrument_state_ports_timing_release_faults_and_heap_through_abi() {
    let entry = clack_host::prelude::PluginEntry::load_from_clack::<
        clack_plugin::entry::SinglePluginEntry<ravet_sine_clap::SinePlugin>,
    >(c"ravet-sine.clap")
    .unwrap();
    assert_eq!(support::exercise(&entry).unwrap(), 3024);
}
#[test]
fn state_codec_rejects_malformed_records() {
    let valid = ravet_sine_clap::encode_state(0.8).unwrap();
    assert_eq!(ravet_sine_clap::decode_state(&valid).unwrap(), 0.8);
    for value in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        assert!(ravet_sine_clap::encode_state(value).is_err());
    }
    let mut bad = valid;
    bad[0] = 0;
    assert!(ravet_sine_clap::decode_state(&bad).is_err());
    assert!(ravet_sine_clap::decode_state(&[valid.as_slice(), &[0]].concat()).is_err());
}
