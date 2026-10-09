use clack_extensions::{audio_ports::*, latency::*, tail::*};
use clack_host::prelude::*;
use clack_plugin::{entry::SinglePluginEntry, prelude::*};
use ravet_host::{
    AudioConfigurationRequests as Requests, ParameterMainRequests, ParameterSharedRequests,
    ProcessingHost, ProcessingMainRequests,
};
struct Fixture;
struct Main<'a>(HostMainThreadHandle<'a>);
struct Audio<'a>(HostAudioProcessorHandle<'a>);
impl Plugin for Fixture {
    type Shared<'a> = ();
    type MainThread<'a> = Main<'a>;
    type AudioProcessor<'a> = Audio<'a>;
}
impl DefaultPluginFactory for Fixture {
    fn get_descriptor() -> PluginDescriptor {
        PluginDescriptor::new(
            "org.ravet.audio-request-fixture",
            "Synthetic audio requests",
        )
    }
    fn new_shared(host: HostSharedHandle<'_>) -> Result<(), PluginError> {
        host.request_callback();
        Ok(())
    }
    fn new_main_thread<'a>(
        host: HostMainThreadHandle<'a>,
        _: &'a (),
    ) -> Result<Main<'a>, PluginError> {
        Ok(Main(host))
    }
}
impl<'a> PluginMainThread<'a, ()> for Main<'a> {
    fn on_main_thread(&self) {
        let ext = self.0.shared().get_extension::<HostAudioPorts>().unwrap();
        assert!(ext.is_rescan_flag_supported(&self.0, AudioPortRescanFlags::LIST));
        ext.rescan(&self.0, AudioPortRescanFlags::NAMES);
        ext.rescan(&self.0, AudioPortRescanFlags::CHANNEL_COUNT);
    }
}
impl<'a> clack_plugin::plugin::PluginAudioProcessor<'a, (), Main<'a>> for Audio<'a> {
    fn activate(
        host: HostAudioProcessorHandle<'a>,
        main: &Main<'a>,
        _: &'a (),
        _: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        main.0
            .shared()
            .get_extension::<HostLatency>()
            .unwrap()
            .changed(&main.0);
        Ok(Self(host))
    }
    fn process(
        &mut self,
        _: Process,
        _: clack_plugin::process::Audio,
        _: clack_plugin::process::Events,
    ) -> Result<ProcessStatus, PluginError> {
        self.0
            .shared()
            .get_extension::<HostTail>()
            .unwrap()
            .changed(&mut self.0);
        Ok(ProcessStatus::Continue)
    }
}
#[test]
fn actual_main_activation_and_audio_callbacks_reach_one_prepared_mailbox() {
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<Fixture>>(c"").unwrap();
    let info = HostInfo::new("Synthetic audio host", "RAVET", "", "0.1").unwrap();
    let shared = ParameterSharedRequests::new();
    let changes = Requests::new();
    let main = ProcessingMainRequests::new(ParameterMainRequests::new(), changes.clone());
    let mut instance = PluginInstance::<ProcessingHost>::new(
        |_| shared.clone(),
        |_| main.clone(),
        &entry,
        c"org.ravet.audio-request-fixture",
        &info,
    )
    .unwrap();
    assert!(shared.core().service_callback(&mut instance));
    let pending = changes.take();
    assert_eq!(
        pending.ports,
        AudioPortRescanFlags::NAMES | AudioPortRescanFlags::CHANNEL_COUNT
    );
    assert!(pending.ports.requires_deactivate());
    assert!(!pending.latency && !pending.tail);
    let stopped = instance
        .activate(
            |_, _| changes.clone(),
            PluginAudioConfiguration {
                sample_rate: 48000.,
                min_frames_count: 1,
                max_frames_count: 8,
            },
        )
        .unwrap();
    assert!(changes.take().latency);
    let stopped = std::thread::spawn(move || {
        let mut started = stopped.start_processing().unwrap();
        let mut samples = [0.; 8];
        let mut in_ports = AudioPorts::with_capacity(0, 0);
        let mut out_ports = AudioPorts::with_capacity(1, 1);
        let input = in_ports.with_input_buffers(std::iter::empty::<
            AudioPortBuffer<
                std::iter::Empty<InputChannel<'_, f32>>,
                std::iter::Empty<InputChannel<'_, f64>>,
            >,
        >());
        let mut output = out_ports.with_output_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_output_only([samples.as_mut_slice()]),
        }]);
        started
            .process(
                &input,
                &mut output,
                &InputEvents::empty(),
                &mut OutputEvents::void(),
                None,
                None,
            )
            .unwrap();
        started.stop_processing()
    })
    .join()
    .unwrap();
    assert!(changes.take().tail);
    instance.deactivate(stopped);
    let pending = changes.take();
    assert!(pending.ports.is_empty() && !pending.latency && !pending.tail);
}
#[test]
fn coalescing_defer_unknown_flags_and_prepared_heap_behavior() {
    let requests = Requests::new();
    let main = ProcessingMainRequests::new(ParameterMainRequests::new(), requests.clone());
    assert!(!main.is_rescan_flag_supported(AudioPortRescanFlags::empty()));
    assert!(!main.is_rescan_flag_supported(AudioPortRescanFlags::from_bits_retain(1 << 29)));
    let mut audio = requests.clone();
    let counts = allocation_counter::measure(|| {
        for _ in 0..10_000 {
            main.rescan(AudioPortRescanFlags::LIST);
            main.changed();
            audio.changed();
            let pending = requests.take();
            assert_eq!(pending.ports, AudioPortRescanFlags::LIST);
            assert!(pending.latency && pending.tail);
            requests.defer(pending);
            assert_eq!(requests.take(), pending);
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    requests.defer(ravet_host::PendingAudioChanges {
        ports: AudioPortRescanFlags::from_bits_retain(u32::MAX),
        latency: false,
        tail: false,
    });
    let pending = requests.take();
    assert_eq!(pending.ports, AudioPortRescanFlags::all());
    assert!(!pending.latency && !pending.tail);
}
