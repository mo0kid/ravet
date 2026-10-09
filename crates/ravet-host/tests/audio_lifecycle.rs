use clack_extensions::audio_ports::*;
use clack_host::prelude::*;
use clack_plugin::{entry::SinglePluginEntry, prelude::*};
use ravet_host::{
    HostRequests, HostedAudioItem, HostedAudioOfferFailure as Offer, HostedAudioSwap as Swap,
    RequestHost, hosted_audio_handoff,
};
use std::sync::{
    OnceLock,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};
use std::thread::ThreadId;
static MAIN: OnceLock<ThreadId> = OnceLock::new();
static AUDIO: OnceLock<ThreadId> = OnceLock::new();
static NEXT: AtomicUsize = AtomicUsize::new(0);
static WRONG: AtomicUsize = AtomicUsize::new(0);
static START: [AtomicUsize; 6] = [const { AtomicUsize::new(0) }; 6];
static RESET: [AtomicUsize; 6] = [const { AtomicUsize::new(0) }; 6];
static STOP: [AtomicUsize; 6] = [const { AtomicUsize::new(0) }; 6];
static PROCESS: [AtomicUsize; 6] = [const { AtomicUsize::new(0) }; 6];
static DEACTIVATE: [AtomicUsize; 6] = [const { AtomicUsize::new(0) }; 6];
static DESTROY: [AtomicUsize; 6] = [const { AtomicUsize::new(0) }; 6];
static FAIL_INITIAL: AtomicBool = AtomicBool::new(true);
fn owner(audio: bool) {
    let expected = if audio { AUDIO.get() } else { MAIN.get() };
    if expected != Some(&std::thread::current().id()) {
        WRONG.fetch_add(1, Ordering::SeqCst);
    }
}
struct Fixture;
struct Shared(usize);
impl clack_plugin::plugin::PluginShared<'_> for Shared {}
struct Main(usize);
struct Audio(usize);
impl Plugin for Fixture {
    type Shared<'a> = Shared;
    type MainThread<'a> = Main;
    type AudioProcessor<'a> = Audio;
    fn declare_extensions(builder: &mut PluginExtensions<Self>, _: Option<&Shared>) {
        builder.register::<PluginAudioPorts>();
    }
}
impl DefaultPluginFactory for Fixture {
    fn get_descriptor() -> PluginDescriptor {
        PluginDescriptor::new("org.ravet.lifecycle-fixture", "Synthetic lifecycle")
    }
    fn new_shared(_: HostSharedHandle<'_>) -> Result<Shared, PluginError> {
        owner(false);
        Ok(Shared(NEXT.fetch_add(1, Ordering::SeqCst)))
    }
    fn new_main_thread(_: HostMainThreadHandle<'_>, id: &Shared) -> Result<Main, PluginError> {
        owner(false);
        Ok(Main(id.0))
    }
}
impl PluginMainThread<'_, Shared> for Main {}
impl Drop for Main {
    fn drop(&mut self) {
        owner(false);
        DESTROY[self.0].fetch_add(1, Ordering::SeqCst);
    }
}
impl PluginAudioPortsImpl for Main {
    fn count(&self, input: bool) -> u32 {
        u32::from(!input)
    }
    fn get(&self, index: u32, input: bool, writer: &mut AudioPortInfoWriter) {
        if !input && index == 0 {
            writer.set(&AudioPortInfo {
                id: ClapId::new(0),
                name: b"Synthetic output",
                flags: AudioPortFlags::IS_MAIN,
                channel_count: 1,
                port_type: Some(AudioPortType::MONO),
                in_place_pair: None,
            });
        }
    }
}
impl<'a> clack_plugin::plugin::PluginAudioProcessor<'a, Shared, Main> for Audio {
    fn activate(
        _: HostAudioProcessorHandle<'a>,
        main: &Main,
        _: &'a Shared,
        _: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        owner(false);
        Ok(Self(main.0))
    }
    fn start_processing(&mut self) -> Result<(), PluginError> {
        owner(true);
        START[self.0].fetch_add(1, Ordering::SeqCst);
        if self.0 == 2 || (self.0 == 0 && FAIL_INITIAL.swap(false, Ordering::SeqCst)) {
            return Err(PluginError::Message("synthetic start rejection"));
        }
        Ok(())
    }
    fn reset(&mut self) {
        owner(true);
        RESET[self.0].fetch_add(1, Ordering::SeqCst);
    }
    fn stop_processing(&mut self) {
        owner(true);
        STOP[self.0].fetch_add(1, Ordering::SeqCst);
    }
    fn deactivate(self, _: &Main) {
        owner(false);
        DEACTIVATE[self.0].fetch_add(1, Ordering::SeqCst);
    }
    fn process(
        &mut self,
        _: Process,
        mut audio: clack_plugin::process::Audio,
        _: clack_plugin::process::Events,
    ) -> Result<ProcessStatus, PluginError> {
        owner(true);
        PROCESS[self.0].fetch_add(1, Ordering::SeqCst);
        for output in audio
            .output_port(0)
            .unwrap()
            .channels()
            .unwrap()
            .into_f32()
            .unwrap()
            .iter_mut()
        {
            output.fill(self.0 as f32);
        }
        Ok(ProcessStatus::Continue)
    }
}
fn render(engine: &mut ravet_host::HostedAudioRunning<RequestHost>) {
    let mut samples = [f32::NAN; 8];
    let mut ports = AudioPorts::with_capacity(1, 1);
    let expected = engine.tag() as f32;
    let allocations = allocation_counter::measure(|| {
        for _ in 0..1000 {
            let mut output = ports.with_output_buffers([AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f32_output_only([samples.as_mut_slice()]),
            }]);
            engine
                .process(
                    &InputAudioBuffers::empty(),
                    &mut output,
                    &InputEvents::empty(),
                    &mut OutputEvents::void(),
                    None,
                    None,
                )
                .unwrap();
            assert!(samples.iter().all(|sample| *sample == expected));
        }
    });
    assert_eq!(allocations.count_total, 0);
    assert_eq!(allocations.count_current, 0);
}
#[test]
fn native_owner_thread_start_failure_backpressure_retirement_and_shutdown() {
    MAIN.set(std::thread::current().id()).unwrap();
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<Fixture>>(c"").unwrap();
    let info = HostInfo::new("Synthetic lifecycle host", "RAVET", "", "0.1").unwrap();
    let mut instances = Vec::new();
    let mut items = Vec::new();
    for tag in 0..6 {
        let mut instance = PluginInstance::<RequestHost>::new(
            |_| HostRequests::new(),
            |_| (),
            &entry,
            c"org.ravet.lifecycle-fixture",
            &info,
        )
        .unwrap();
        let processor = instance
            .activate(
                |_, _| (),
                PluginAudioConfiguration {
                    sample_rate: 48000.,
                    min_frames_count: 1,
                    max_frames_count: 8,
                },
            )
            .unwrap();
        items.push(Some(HostedAudioItem { tag, processor }));
        instances.push(instance);
    }
    let initial = hosted_audio_handoff(items[0].take().unwrap(), 0)
        .err()
        .unwrap();
    let (mut control, ready) = hosted_audio_handoff(initial, 1).unwrap();
    control.offer(items[1].take().unwrap()).ok().unwrap();
    let full = control.offer(items[2].take().unwrap()).err().unwrap();
    assert_eq!(full.reason, Offer::Full);
    items[2] = Some(full.item);
    let (commands, receive) = mpsc::sync_channel(0);
    let (results, wait) = mpsc::sync_channel(0);
    let worker = std::thread::spawn(move || {
        AUDIO.set(std::thread::current().id()).unwrap();
        let failed = ready.start().err().unwrap();
        let mut engine = failed.ready.start().unwrap();
        engine.reset();
        render(&mut engine);
        results.send(Swap::Unchanged).unwrap();
        while receive.recv().unwrap() {
            let mut status = Swap::Unchanged;
            let allocation = allocation_counter::measure(|| {
                status = engine.apply_pending();
            });
            if !matches!(status, Swap::StartRejected { .. }) {
                assert_eq!(allocation.count_total, 0);
            }
            render(&mut engine);
            results.send(status).unwrap();
        }
        engine.finish()
    });
    assert_eq!(wait.recv().unwrap(), Swap::Unchanged);
    commands.send(true).unwrap();
    assert_eq!(
        wait.recv().unwrap(),
        Swap::Applied {
            previous: 0,
            current: 1
        }
    );
    control.offer(items[2].take().unwrap()).ok().unwrap();
    commands.send(true).unwrap();
    assert_eq!(wait.recv().unwrap(), Swap::RetirementFull);
    let retired = control.take_retired().unwrap();
    assert_eq!(retired.tag, 0);
    instances[0].deactivate(retired.processor);
    commands.send(true).unwrap();
    assert_eq!(wait.recv().unwrap(), Swap::StartRejected { tag: 2 });
    let retired = control.take_retired().unwrap();
    assert_eq!(retired.tag, 2);
    instances[2].deactivate(retired.processor);
    control.offer(items[3].take().unwrap()).ok().unwrap();
    commands.send(true).unwrap();
    assert_eq!(
        wait.recv().unwrap(),
        Swap::Applied {
            previous: 1,
            current: 3
        }
    );
    control.offer(items[4].take().unwrap()).ok().unwrap();
    commands.send(true).unwrap();
    assert_eq!(wait.recv().unwrap(), Swap::RetirementFull);
    let retired = control.take_retired().unwrap();
    assert_eq!(retired.tag, 1);
    instances[1].deactivate(retired.processor);
    commands.send(false).unwrap();
    let ready = worker.join().unwrap();
    let mut ready = ready.into_current().err().unwrap();
    let pending = ready.take_pending().unwrap();
    assert_eq!(pending.tag, 4);
    instances[4].deactivate(pending.processor);
    let current = ready.into_current().ok().unwrap();
    assert_eq!(current.tag, 3);
    instances[3].deactivate(current.processor);
    let gone = control.offer(items[5].take().unwrap()).err().unwrap();
    assert_eq!(gone.reason, Offer::EngineGone);
    instances[5].deactivate(gone.item.processor);
    assert!(control.take_retired().is_none());
    drop(control);
    drop(instances);
    assert_eq!(WRONG.load(Ordering::SeqCst), 0);
    assert_eq!(
        RESET.each_ref().map(|x| x.load(Ordering::SeqCst)),
        [1, 0, 0, 0, 0, 0]
    );
    assert_eq!(
        START.each_ref().map(|x| x.load(Ordering::SeqCst)),
        [2, 1, 1, 1, 0, 0]
    );
    assert_eq!(
        STOP.each_ref().map(|x| x.load(Ordering::SeqCst)),
        [1, 1, 0, 1, 0, 0]
    );
    assert_eq!(
        DEACTIVATE.each_ref().map(|x| x.load(Ordering::SeqCst)),
        [1; 6]
    );
    assert_eq!(DESTROY.each_ref().map(|x| x.load(Ordering::SeqCst)), [1; 6]);
    assert_eq!(
        PROCESS.each_ref().map(|x| x.load(Ordering::SeqCst)),
        [1000, 3000, 0, 2000, 0, 0]
    );
}

#[test]
fn control_abandonment_retains_current_and_unstarted_native_gain_proposal() {
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<ravet_gain_clap::GainPlugin>>(c"")
        .unwrap();
    let info = HostInfo::new("Synthetic abandoned host", "RAVET", "", "0.1").unwrap();
    let mut instances = Vec::new();
    let mut items = Vec::new();
    for tag in [42, 43] {
        let mut instance = PluginInstance::<RequestHost>::new(
            |_| HostRequests::new(),
            |_| (),
            &entry,
            c"org.ravet.gain",
            &info,
        )
        .unwrap();
        let processor = instance
            .activate(
                |_, _| (),
                PluginAudioConfiguration {
                    sample_rate: 48000.,
                    min_frames_count: 1,
                    max_frames_count: 8,
                },
            )
            .unwrap();
        items.push(HostedAudioItem { tag, processor });
        instances.push(instance);
    }
    let candidate = items.pop().unwrap();
    let initial = items.pop().unwrap();
    let (mut control, ready) = hosted_audio_handoff(initial, 1).unwrap();
    control.offer(candidate).unwrap();
    drop(control);
    let mut ready = std::thread::spawn(move || {
        let mut engine = ready.start().unwrap();
        assert_eq!(engine.apply_pending(), Swap::ControlGone);
        assert_eq!(engine.tag(), 42);
        let mut input = [[0.25_f32; 8], [-0.5; 8]];
        let mut output = [[99_f32; 8]; 2];
        let mut inputs = AudioPorts::with_capacity(2, 1);
        let mut outputs = AudioPorts::with_capacity(2, 1);
        let input_ports = inputs.with_input_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_input_only(
                input.iter_mut().map(InputChannel::variable),
            ),
        }]);
        let mut output_ports = outputs.with_output_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_output_only(
                output.iter_mut().map(|x| x.as_mut_slice()),
            ),
        }]);
        engine
            .process(
                &input_ports,
                &mut output_ports,
                &InputEvents::empty(),
                &mut OutputEvents::void(),
                None,
                None,
            )
            .unwrap();
        assert_eq!(output, input);
        engine.finish()
    })
    .join()
    .unwrap();
    let candidate = ready.take_pending().unwrap();
    assert_eq!(candidate.tag, 43);
    instances[1].deactivate(candidate.processor);
    let current = ready.into_current().ok().unwrap();
    assert_eq!(current.tag, 42);
    instances[0].deactivate(current.processor);
}
