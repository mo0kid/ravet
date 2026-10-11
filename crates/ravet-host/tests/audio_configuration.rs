use clack_extensions::{audio_ports::*, latency::*, tail::*};
use clack_host::prelude::*;
use clack_plugin::{entry::SinglePluginEntry, prelude::*};
use ravet_host::{
    HostRequests, HostedAudioConfiguration as Ports, HostedAudioError as E, RequestHost,
    hosted_latency, hosted_tail,
};
use std::sync::atomic::{AtomicU8, Ordering};
static MODE: AtomicU8 = AtomicU8::new(0);
struct Fixture;
struct Main;
struct Audio;
impl Plugin for Fixture {
    type Shared<'a> = ();
    type MainThread<'a> = Main;
    type AudioProcessor<'a> = Audio;
    fn declare_extensions(builder: &mut PluginExtensions<Self>, _: Option<&()>) {
        builder
            .register::<PluginAudioPorts>()
            .register::<PluginLatency>()
            .register::<PluginTail>();
    }
}
impl DefaultPluginFactory for Fixture {
    fn get_descriptor() -> PluginDescriptor {
        PluginDescriptor::new(
            "org.ravet.audio-metadata-fixture",
            "Synthetic audio metadata",
        )
    }
    fn new_shared(_: HostSharedHandle<'_>) -> Result<(), PluginError> {
        Ok(())
    }
    fn new_main_thread(_: HostMainThreadHandle<'_>, _: &()) -> Result<Main, PluginError> {
        Ok(Main)
    }
}
impl PluginMainThread<'_, ()> for Main {}
impl PluginLatencyImpl for Main {
    fn get(&self) -> u32 {
        65_537
    }
}
impl PluginTailImpl for Audio {
    fn get(&self) -> TailLength {
        if MODE.load(Ordering::Relaxed) == 10 {
            TailLength::Infinite
        } else {
            TailLength::Finite(12345)
        }
    }
}
impl PluginAudioPortsImpl for Main {
    fn count(&self, _: bool) -> u32 {
        match MODE.load(Ordering::Relaxed) {
            1 => 10,
            11 => 0,
            _ => 2,
        }
    }
    fn get(&self, index: u32, input: bool, writer: &mut AudioPortInfoWriter) {
        let mode = MODE.load(Ordering::Relaxed);
        if mode == 2 {
            return;
        }
        let mut flags = AudioPortFlags::SUPPORTS_64BITS;
        if index == 0 || mode == 3 {
            flags |= AudioPortFlags::IS_MAIN;
        }
        if mode == 4 {
            flags = AudioPortFlags::PREFERS_64BITS;
        }
        writer.set(&AudioPortInfo {
            id: ClapId::new(if mode == 5 { 0 } else { index }),
            name: &[255, b'X'],
            flags,
            channel_count: match mode {
                6 => 0,
                7 => 65,
                9 if !input => 1,
                _ => 2,
            },
            port_type: Some(AudioPortType(c"org.ravet.opaque-layout")),
            in_place_pair: Some(ClapId::new(if mode == 8 { 99 } else { index })),
        });
    }
}
impl<'a> clack_plugin::plugin::PluginAudioProcessor<'a, (), Main> for Audio {
    fn activate(
        _: HostAudioProcessorHandle<'a>,
        _: &Main,
        _: &'a (),
        _: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        Ok(Self)
    }
    fn process(
        &mut self,
        _: Process,
        _: clack_plugin::process::Audio,
        _: clack_plugin::process::Events,
    ) -> Result<ProcessStatus, PluginError> {
        Ok(ProcessStatus::Continue)
    }
}
#[test]
fn directional_ids_owned_bytes_invalid_refresh_and_thread_correct_timing_queries() {
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<Fixture>>(c"").unwrap();
    let info = HostInfo::new("Synthetic metadata host", "RAVET", "", "0.1").unwrap();
    let mut instance = PluginInstance::<RequestHost>::new(
        |_| HostRequests::new(),
        |_| (),
        &entry,
        c"org.ravet.audio-metadata-fixture",
        &info,
    )
    .unwrap();
    let mut ports = Ports::inspect(&mut instance).unwrap();
    assert_eq!(ports.inputs.len(), 2);
    assert_eq!(ports.inputs[0].id, ports.outputs[0].id);
    assert_eq!(ports.inputs[0].name, [255, b'X']);
    assert_eq!(
        ports.inputs[0].port_type.as_ref().unwrap().as_c_str(),
        c"org.ravet.opaque-layout"
    );
    let original = ports.clone();
    for mode in 1..=9 {
        MODE.store(mode, Ordering::Relaxed);
        assert_eq!(
            ports.refresh(&mut instance),
            Err(if mode == 1 { E::Capacity } else { E::Metadata })
        );
        assert_eq!(ports, original);
    }
    MODE.store(0, Ordering::Relaxed);
    assert_eq!(hosted_latency(&mut instance), Some(65_537));
    let mut stopped = instance
        .activate(
            |_, _| (),
            PluginAudioConfiguration {
                sample_rate: 48000.,
                min_frames_count: 1,
                max_frames_count: 64,
            },
        )
        .unwrap();
    assert_eq!(ports.refresh(&mut instance), Err(E::Active));
    assert_eq!(ports, original);
    let counts = allocation_counter::measure(|| {
        for _ in 0..10_000 {
            assert_eq!(
                hosted_tail(&stopped.plugin_handle()),
                Some(TailLength::Finite(12345))
            );
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    MODE.store(10, Ordering::Relaxed);
    assert_eq!(
        hosted_tail(&stopped.plugin_handle()),
        Some(TailLength::Infinite)
    );
    instance.deactivate(stopped);
    MODE.store(11, Ordering::Relaxed);
    ports.refresh(&mut instance).unwrap();
    assert_eq!(ports, Ports::default());
    MODE.store(0, Ordering::Relaxed);
}
// The instrument (no input ports) half links the sine example and lives in
// linked_sine.rs: each test binary links at most one example CLAP crate.
#[test]
fn linked_sidechain_preserves_directional_auxiliary_ports() {
    let info = HostInfo::new("Synthetic reference host", "RAVET", "", "0.1").unwrap();
    let entry = PluginEntry::load_from_clack::<
        SinglePluginEntry<ravet_sidechain_clap::SidechainPlugin>,
    >(c"")
    .unwrap();
    let mut instance = PluginInstance::<RequestHost>::new(
        |_| HostRequests::new(),
        |_| (),
        &entry,
        c"org.ravet.sidechain",
        &info,
    )
    .unwrap();
    let ports = Ports::inspect(&mut instance).unwrap();
    assert_eq!(
        ports.inputs.iter().map(|p| p.channels).collect::<Vec<_>>(),
        [2, 1]
    );
    assert_eq!(
        ports.outputs.iter().map(|p| p.channels).collect::<Vec<_>>(),
        [2]
    );
    assert!(
        ports.inputs[0]
            .flags
            .contains(AudioPortFlags::SUPPORTS_64BITS)
    );
    assert!(!ports.inputs[1].flags.contains(AudioPortFlags::IS_MAIN));
    assert_eq!(ports.inputs[1].in_place_pair, None);
    assert_eq!(hosted_latency(&mut instance), None);
    let mut stopped = instance
        .activate(
            |_, _| (),
            PluginAudioConfiguration {
                sample_rate: 48000.,
                min_frames_count: 1,
                max_frames_count: 64,
            },
        )
        .unwrap();
    assert_eq!(
        hosted_tail(&stopped.plugin_handle()),
        Some(TailLength::Finite(0))
    );
    instance.deactivate(stopped);
}
