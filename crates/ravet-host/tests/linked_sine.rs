//! Hosted checks that link the sine example CLAP crate.
//!
//! Every example CLAP crate exports the unmangled `clap_entry` symbol. A test
//! binary that links two of them contains duplicate definitions; whether the
//! linker rejects them depends on codegen-unit partitioning (rust-lld on
//! x86_64 Linux rejects non-incremental builds). Each integration-test binary
//! therefore links at most one example CLAP crate; gain and sidechain checks
//! remain in their own files.
#[path = "../examples/support/sine_graph_probe.rs"]
mod sine_probe;
use clack_extensions::gui::{GuiApiType, GuiConfiguration};
use clack_host::prelude::*;
use clack_plugin::entry::SinglePluginEntry;
use ravet_host::{
    EditorError, HostRequests, HostedAudioConfiguration as Ports, PluginEditor, RequestHost,
};

fn sine(host: &str) -> PluginInstance<RequestHost> {
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<ravet_sine_clap::SinePlugin>>(c"")
        .unwrap();
    PluginInstance::<RequestHost>::new(
        |_| HostRequests::new(),
        |_| (),
        &entry,
        c"org.ravet.sine",
        &HostInfo::new(host, "RAVET", "", "0.1").unwrap(),
    )
    .unwrap()
}

#[test]
fn editor_rejects_absent_extension_before_native_creation() {
    let mut instance = sine("RAVET editor fixture");
    assert!(matches!(
        PluginEditor::create(
            &mut instance,
            GuiConfiguration {
                api_type: GuiApiType::COCOA,
                is_floating: false
            }
        ),
        Err(EditorError::Unavailable)
    ));
}

#[test]
fn linked_instrument_preserves_no_input_ports() {
    let mut instance = sine("Synthetic reference host");
    let ports = Ports::inspect(&mut instance).unwrap();
    assert!(ports.inputs.is_empty());
    assert_eq!(ports.outputs[0].channels, 2);
}

#[test]
fn linked_sine_note_only_node_has_timed_sound_and_release_silence_without_heap() {
    sine_probe::run(sine("Linked sine note-only probe")).unwrap();
}
