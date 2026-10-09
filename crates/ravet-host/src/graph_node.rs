//! One-shot main-activated native f32/f64 node for explicit graph audio sessions.
use crate::{HostRequests, HostedAudioConfiguration, HostedAudioError};
use clack_extensions::audio_ports::AudioPortFlags;
use clack_host::{
    host::HostHandlers,
    plugin::{PluginInstance, PluginInstanceError},
    prelude::*,
};
use ravet_foundation::{
    PrepareError, ProcessBlock, ProcessConfig, ProcessError, ProcessStatus as PortableStatus,
    Processor, Tail,
};
use rtrb::{Consumer, Producer, RingBuffer};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    thread::ThreadId,
};

/// Explicit prepared node contract and consumer-declared graph timing. Native
/// latency/tail are not guessed or queried on the wrong thread. Precision is selected by the factory; main
/// audio only, 1..65536 frames, no foundation auxiliary buses. Parameter
/// input/output use explicit preparation profiles and bounded mappings. The selected factory maps main-only or packed native ports; native
/// transport is separately opted in while stopped.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HostedGraphNodeOptions {
    pub configuration: ProcessConfig,
    pub declared_latency: u32,
    pub declared_tail: Tail,
}
#[derive(Debug)]
pub enum HostedGraphNodeBuildFailure {
    Configuration,
    /// Every mapped port must explicitly advertise native 64-bit samples.
    Precision,
    Metadata(HostedAudioError),
    Parameters(crate::HostedParameterError),
    Notes(crate::HostedGraphNoteError),
    Activation(PluginInstanceError),
}
/// Failed preparation retains the caller's main-thread instance.
pub struct HostedGraphNodeBuildError<H: HostHandlers> {
    pub reason: HostedGraphNodeBuildFailure,
    pub instance: PluginInstance<H>,
}
impl<H: HostHandlers> std::fmt::Debug for HostedGraphNodeBuildError<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostedGraphNodeBuildError")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}
impl<H: HostHandlers> std::fmt::Display for HostedGraphNodeBuildError<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "hosted graph preparation: {:?}", self.reason)
    }
}
impl<H: HostHandlers> std::error::Error for HostedGraphNodeBuildError<H> {}
/// Independent relaxed counters, not a coherent block snapshot. Successful
/// positive-frame native calls with accepted mapped output alone advance processed counters/status; empty
/// blocks/faults retain prior status. Start and phase rejection counts separate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostedGraphNodeStatistics {
    pub processed_blocks: u64,
    pub processed_frames: u64,
    /// Dispatch count for valid zero-frame parameter flush on audio; not acknowledgement.
    pub parameter_flushes: u64,
    pub filtered_native_events: u64,
    pub rejected_native_events: u64,
    pub backend_failures: u64,
    pub start_rejections: u64,
    pub phase_rejections: u64,
    pub last_status: Option<clack_host::process::ProcessStatus>,
}
#[derive(Default)]
struct Counters {
    blocks: AtomicU64,
    frames: AtomicU64,
    flushes: AtomicU64,
    filtered: AtomicU64,
    rejected: AtomicU64,
    faults: AtomicU64,
    starts: AtomicU64,
    phases: AtomicU64,
    status: AtomicU64,
}
impl Counters {
    fn snapshot(&self) -> HostedGraphNodeStatistics {
        HostedGraphNodeStatistics {
            processed_blocks: self.blocks.load(Ordering::Relaxed),
            processed_frames: self.frames.load(Ordering::Relaxed),
            parameter_flushes: self.flushes.load(Ordering::Relaxed),
            filtered_native_events: self.filtered.load(Ordering::Relaxed),
            rejected_native_events: self.rejected.load(Ordering::Relaxed),
            backend_failures: self.faults.load(Ordering::Relaxed),
            start_rejections: self.starts.load(Ordering::Relaxed),
            phase_rejections: self.phases.load(Ordering::Relaxed),
            last_status: match self.status.load(Ordering::Relaxed) {
                1 => Some(clack_host::process::ProcessStatus::Continue),
                2 => Some(clack_host::process::ProcessStatus::ContinueIfNotQuiet),
                3 => Some(clack_host::process::ProcessStatus::Tail),
                4 => Some(clack_host::process::ProcessStatus::Sleep),
                _ => None,
            },
        }
    }
}
/// Main-thread native instance owner. Retain until its graph/node has stopped
/// on audio and deactivated/dropped on control, then reclaim the stopped token.
/// Dropping early follows Clack's protective leak policy, not wrong-thread native
/// destruction. Deferred callbacks require explicit service; editing/rebuild follows
/// returned inactive instance ownership. No polling/event loop is created.
///
/// ```compile_fail
/// use ravet_host::{HostedGraphNodeControl, RequestHost};
/// fn require_send<T: Send>() {}
/// require_send::<HostedGraphNodeControl<RequestHost>>();
/// ```
pub struct HostedGraphNodeControl<H: HostHandlers> {
    instance: Option<PluginInstance<H>>,
    returned: Consumer<StoppedPluginAudioProcessor<H>>,
    counters: Arc<Counters>,
    deactivated: bool,
}
impl<H: HostHandlers> HostedGraphNodeControl<H> {
    /// Create a guarded native editor on the plugin/OS main thread, while the
    /// separately owned audio node may run. The guard exclusively borrows this
    /// control's retained instance, preventing reclaim/drop until GUI destruction.
    /// Native parent lifetime and event-loop/request servicing stay explicit.
    ///
    /// ```compile_fail
    /// use ravet_host::{HostedGraphNodeControl, RequestHost};
    /// use clack_extensions::gui::GuiConfiguration;
    /// fn retire(control: &mut HostedGraphNodeControl<RequestHost>, config: GuiConfiguration<'_>) {
    ///     let editor = control.editor(config).unwrap();
    ///     control.reclaim(); // editor still owns the exclusive instance borrow
    ///     drop(editor);
    /// }
    /// ```
    pub fn editor(
        &mut self,
        configuration: clack_extensions::gui::GuiConfiguration<'_>,
    ) -> Result<crate::PluginEditor<'_, H>, crate::EditorError> {
        crate::PluginEditor::create(
            self.instance
                .as_mut()
                .ok_or(crate::EditorError::Unavailable)?,
            configuration,
        )
    }
    /// Main-thread deactivation, at most once, after graph/node control retirement.
    /// Returns false while the token has not returned or was already reclaimed.
    pub fn reclaim(&mut self) -> bool {
        let Ok(stopped) = self.returned.pop() else {
            return false;
        };
        self.instance
            .as_mut()
            .expect("retained main instance")
            .deactivate(stopped);
        self.deactivated = true;
        true
    }
    /// Service at most one deferred core callback on main. The mailbox must be
    /// the one installed in this instance's host handlers; restart/process
    /// requests remain consumer-owned. No polling/event loop is created.
    pub fn service_callback(&mut self, requests: &HostRequests) -> bool {
        self.instance
            .as_mut()
            .is_some_and(|instance| requests.service_callback(instance))
    }
    /// After reclaim, recover the inactive main instance for state/parameter
    /// editing or a fresh activation/node. Otherwise retain complete ownership.
    pub fn into_instance(mut self) -> Result<PluginInstance<H>, Self> {
        if !self.deactivated {
            return Err(self);
        }
        Ok(self.instance.take().expect("retained main instance"))
    }
    pub fn is_deactivated(&self) -> bool {
        self.deactivated
    }
    pub fn statistics(&self) -> HostedGraphNodeStatistics {
        self.counters.snapshot()
    }
}
impl<H: HostHandlers> Drop for HostedGraphNodeControl<H> {
    fn drop(&mut self) {
        self.reclaim();
    }
}
enum AudioToken<H: HostHandlers> {
    Stopped(StoppedPluginAudioProcessor<H>),
    Started(StartedPluginAudioProcessor<H>),
}
/// Send prepared native audio node for graph-owned explicit audio sessions.
/// Main prepare validates the already activated fixed contract. Audio start,
/// process/reset/stop run on the guarded owner; deactivation returns its stopped
/// token through one prepared slot, without executing native main callbacks.
/// One-shot: reactivation requires a fresh activation/node after reclaim, not
/// graph reprepare. Recover the inactive instance through control.into_instance().
/// Node-owned audio scratch is <=32 MiB, outside graph's own 64 MiB cap; plugin
/// memory/time are separate. Main instance control must outlive this token.
pub struct HostedGraphNode<H: HostHandlers> {
    inner: NativeGraphNode<H, f32>,
}
/// Native f64 counterpart with prepared double buffers and no sample narrowing.
/// Each mapped port must advertise SUPPORTS_64BITS. Native scratch is at most
/// 64 MiB, outside graph's own cap; all lifecycle/one-shot contracts match f32.
pub struct HostedGraphNode64<H: HostHandlers> {
    inner: NativeGraphNode<H, f64>,
}
struct NativeGraphNode<H: HostHandlers, S: NativeSample> {
    options: HostedGraphNodeOptions,
    main: ThreadId,
    owner: Option<ThreadId>,
    prepared: bool,
    audio: Option<AudioToken<H>>,
    returned: Producer<StoppedPluginAudioProcessor<H>>,
    counters: Arc<Counters>,
    buffers: Buffers<S>,
    pending_reset: bool,
    steady_time: Option<u64>,
    transport_enabled: bool,
    parameters: Option<crate::graph_parameters::ParameterInput>,
    parameter_output: crate::graph_parameter_output::ParameterOutput,
}
impl<H: HostHandlers, S: NativeSample> NativeGraphNode<H, S> {
    fn collect_parameter_output(
        &mut self,
        block: &mut ProcessBlock<'_, '_, '_, S>,
    ) -> Result<(), ProcessError> {
        self.counters
            .filtered
            .fetch_add(self.parameter_output.filtered, Ordering::Relaxed);
        self.counters
            .rejected
            .fetch_add(self.parameter_output.rejected, Ordering::Relaxed);
        if self.parameter_output.failed {
            return Err(ProcessError::BackendFailure);
        }
        for event in &self.parameter_output.events {
            block
                .output_events
                .as_mut()
                .expect("validated output event profile")
                .push(*event)
                .map_err(|_| ProcessError::BackendFailure)?;
        }
        Ok(())
    }
    fn reject_phase(&self) -> ProcessError {
        self.counters.phases.fetch_add(1, Ordering::Relaxed);
        ProcessError::BackendFailure
    }
    fn retire(&mut self) {
        let Some(audio) = self.audio.take() else {
            return;
        };
        let stopped = match audio {
            AudioToken::Stopped(stopped) => stopped,
            AudioToken::Started(started) if self.owner == Some(std::thread::current().id()) => {
                started.stop_processing()
            }
            AudioToken::Started(started) => {
                // Preserve native lifetime if caller violates owner/session
                // shutdown, rather than destroying/deactivating on wrong thread.
                std::mem::forget(started);
                return;
            }
        };
        self.owner = None;
        assert!(
            self.returned.push(stopped).is_ok(),
            "single one-shot return token"
        );
    }
}
impl<H: HostHandlers, S: NativeSample> Processor<S> for NativeGraphNode<H, S> {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        if std::thread::current().id() != self.main
            || config != self.options.configuration
            || !matches!(self.audio, Some(AudioToken::Stopped(_)))
        {
            return Err(PrepareError::UnsupportedLayout);
        }
        self.prepared = true;
        Ok(())
    }
    fn start_audio(&mut self) -> Result<(), ProcessError> {
        if !self.prepared {
            return Err(ProcessError::NotPrepared);
        }
        if std::thread::current().id() == self.main
            || !matches!(self.audio, Some(AudioToken::Stopped(_)))
        {
            return Err(self.reject_phase());
        }
        let Some(AudioToken::Stopped(mut stopped)) = self.audio.take() else {
            unreachable!("checked stopped token")
        };
        if self.pending_reset {
            stopped.reset();
            self.pending_reset = false;
        }
        match stopped.start_processing() {
            Ok(started) => {
                self.audio = Some(AudioToken::Started(started));
                self.owner = Some(std::thread::current().id());
                Ok(())
            }
            Err(error) => {
                self.audio = Some(AudioToken::Stopped(error.into_stopped_processor()));
                self.counters.starts.fetch_add(1, Ordering::Relaxed);
                Err(ProcessError::BackendFailure)
            }
        }
    }
    fn stop_audio(&mut self) {
        assert_eq!(
            self.owner,
            Some(std::thread::current().id()),
            "stop native graph node on audio owner"
        );
        let Some(AudioToken::Started(started)) = self.audio.take() else {
            unreachable!("started graph node")
        };
        self.audio = Some(AudioToken::Stopped(started.stop_processing()));
        self.owner = None;
    }
    fn reset(&mut self) {
        if let Some(AudioToken::Started(started)) = self.audio.as_mut() {
            assert_eq!(
                self.owner,
                Some(std::thread::current().id()),
                "reset native node on audio owner"
            );
            started.reset();
        } else {
            self.pending_reset = true;
        }
        self.steady_time = Some(0);
    }
    fn deactivate(&mut self) {
        assert!(
            self.owner.is_none(),
            "finish native graph audio session before deactivation"
        );
        if self.prepared {
            self.prepared = false;
            self.retire();
        }
    }
    fn latency_samples(&self) -> u32 {
        self.options.declared_latency
    }
    fn tail(&self) -> Tail {
        self.options.declared_tail
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, S>) -> PortableStatus {
        let error = if !self.prepared || !matches!(self.audio, Some(AudioToken::Started(_))) {
            Some(ProcessError::NotPrepared)
        } else if self.owner != Some(std::thread::current().id()) {
            Some(self.reject_phase())
        } else {
            self.options.configuration.validate_block(block).err()
        };
        if let Some(error) = error {
            block.clear_outputs();
            return PortableStatus::Fault(error);
        }
        let transport = if self.transport_enabled {
            match crate::hosted_transport(block.transport, self.options.configuration.sample_rate())
            {
                Ok(transport) => transport,
                Err(error) => {
                    block.clear_outputs();
                    return PortableStatus::Fault(error);
                }
            }
        } else {
            None
        };
        if let Some(parameters) = &mut self.parameters
            && let Err(error) = parameters.encode(block.events, block.audio.frames())
        {
            block.clear_outputs();
            return PortableStatus::Fault(error);
        }
        let frames = block.audio.frames();
        self.parameter_output.begin(frames);
        if frames == 0 {
            let extension = self
                .parameters
                .as_ref()
                .filter(|parameters| !parameters.is_empty())
                .and_then(|parameters| parameters.extension)
                .or(self.parameter_output.extension);
            if let Some(extension) = extension {
                let Some(AudioToken::Started(started)) = self.audio.as_mut() else {
                    unreachable!("validated audio");
                };
                let empty = crate::graph_parameters::ParameterEvents::default();
                let events = self
                    .parameters
                    .as_ref()
                    .map(|parameters| &parameters.events)
                    .unwrap_or(&empty);
                extension.flush_active(
                    &mut started.plugin_handle(),
                    &InputEvents::from_buffer(events),
                    &mut OutputEvents::from_buffer(&mut self.parameter_output),
                );
                self.counters.flushes.fetch_add(1, Ordering::Relaxed);
                if let Err(error) = self.collect_parameter_output(block) {
                    self.counters.faults.fetch_add(1, Ordering::Relaxed);
                    block.clear_outputs();
                    return PortableStatus::Fault(error);
                }
            }
            return PortableStatus::Continue;
        }
        for (channel, input) in self.buffers.inputs.iter_mut().flatten().enumerate() {
            input[..frames].copy_from_slice(block.audio.input(channel).expect("validated input"));
        }
        for output in self.buffers.outputs.iter_mut().flatten() {
            output[..frames].fill(S::default());
        }
        let Some(AudioToken::Started(started)) = self.audio.as_mut() else {
            unreachable!("validated audio token")
        };
        let empty_events = crate::graph_parameters::ParameterEvents::default();
        let events = self
            .parameters
            .as_ref()
            .map(|parameters| &parameters.events)
            .unwrap_or(&empty_events);
        let input_events = InputEvents::from_buffer(events);
        let result = S::call(
            &mut self.buffers,
            started,
            frames,
            self.steady_time,
            transport.as_ref(),
            &input_events,
            &mut OutputEvents::from_buffer(&mut self.parameter_output),
        );
        self.steady_time = self
            .steady_time
            .and_then(|time| time.checked_add(frames as u64))
            .filter(|time| *time <= i64::MAX as u64);
        match result {
            Ok(status) => {
                if let Err(error) = self.collect_parameter_output(block) {
                    self.counters.faults.fetch_add(1, Ordering::Relaxed);
                    block.clear_outputs();
                    return PortableStatus::Fault(error);
                }
                self.counters.blocks.fetch_add(1, Ordering::Relaxed);
                self.counters
                    .frames
                    .fetch_add(frames as u64, Ordering::Relaxed);
                self.counters.status.store(
                    match status {
                        clack_host::process::ProcessStatus::Continue => 1,
                        clack_host::process::ProcessStatus::ContinueIfNotQuiet => 2,
                        clack_host::process::ProcessStatus::Tail => 3,
                        clack_host::process::ProcessStatus::Sleep => 4,
                    },
                    Ordering::Relaxed,
                );
                for (channel, samples) in self.buffers.outputs.iter().flatten().enumerate() {
                    block
                        .audio
                        .output_mut(channel)
                        .expect("validated output")
                        .copy_from_slice(&samples[..frames]);
                }
                PortableStatus::Continue
            }
            Err(_) => {
                self.counters
                    .filtered
                    .fetch_add(self.parameter_output.filtered, Ordering::Relaxed);
                self.counters
                    .rejected
                    .fetch_add(self.parameter_output.rejected, Ordering::Relaxed);
                self.counters.faults.fetch_add(1, Ordering::Relaxed);
                block.clear_outputs();
                PortableStatus::Fault(ProcessError::BackendFailure)
            }
        }
    }
}
impl<H: HostHandlers, S: NativeSample> Drop for NativeGraphNode<H, S> {
    fn drop(&mut self) {
        self.retire();
    }
}
/// Main-thread-only inspection, scratch/queue preparation and activation of a
/// caller-created trusted Clack instance. Require zero/one main port per direction,
/// exact 0..64 layout channels, <=65536 scratch frames and no events/auxiliary buses.
/// Declare graph timing explicitly; native events are not mapped. Block-start
/// transport requires explicit stopped main-thread opt-in.
/// Retain control through stopped graph retirement, then reclaim on main.
pub fn prepare_hosted_graph_node<H: HostHandlers, FA>(
    instance: PluginInstance<H>,
    options: HostedGraphNodeOptions,
    audio_handler: FA,
) -> Result<(HostedGraphNodeControl<H>, HostedGraphNode<H>), HostedGraphNodeBuildError<H>>
where
    FA: for<'a> FnOnce(
        &'a <H as HostHandlers>::Shared<'a>,
        &<H as HostHandlers>::MainThread<'a>,
    ) -> <H as HostHandlers>::AudioProcessor<'a>,
{
    prepare_native_graph_node::<H, f32, FA>(
        instance,
        options,
        audio_handler,
        NativeProfile::audio(false),
    )
    .map(|(control, inner)| (control, HostedGraphNode { inner }))
}
/// Prepare the same explicit main-audio profile with native f64 scratch. All
/// mapped ports must advertise SUPPORTS_64BITS; rejection retains inactive
/// instance ownership before activation. No f32 conversion/fallback occurs.
pub fn prepare_hosted_graph_node64<H: HostHandlers, FA>(
    instance: PluginInstance<H>,
    options: HostedGraphNodeOptions,
    audio_handler: FA,
) -> Result<(HostedGraphNodeControl<H>, HostedGraphNode64<H>), HostedGraphNodeBuildError<H>>
where
    FA: for<'a> FnOnce(
        &'a <H as HostHandlers>::Shared<'a>,
        &<H as HostHandlers>::MainThread<'a>,
    ) -> <H as HostHandlers>::AudioProcessor<'a>,
{
    prepare_native_graph_node::<H, f64, FA>(
        instance,
        options,
        audio_handler,
        NativeProfile::audio(false),
    )
    .map(|(control, inner)| (control, HostedGraphNode64 { inner }))
}
/// Prepare native f32 audio ports packed in enumeration/channel order into the
/// graph layout. At most nine ports per direction and 64 total channels per
/// direction; no automatic main/aux pairing or layout negotiation. The consumer
/// inspects inactive port metadata and supplies matching summed layout channels.
/// Foundation auxiliary buses stay empty: graph routes address the packed main
/// channel indices. Lifecycle and timing match the main-only factory.
pub fn prepare_hosted_graph_ports_node<H: HostHandlers, FA>(
    instance: PluginInstance<H>,
    options: HostedGraphNodeOptions,
    audio_handler: FA,
) -> Result<(HostedGraphNodeControl<H>, HostedGraphNode<H>), HostedGraphNodeBuildError<H>>
where
    FA: for<'a> FnOnce(
        &'a <H as HostHandlers>::Shared<'a>,
        &<H as HostHandlers>::MainThread<'a>,
    ) -> <H as HostHandlers>::AudioProcessor<'a>,
{
    prepare_native_graph_node::<H, f32, FA>(
        instance,
        options,
        audio_handler,
        NativeProfile::audio(true),
    )
    .map(|(control, inner)| (control, HostedGraphNode { inner }))
}
/// Prepare native f64 audio ports packed in enumeration/channel order into the
/// graph layout. At most nine ports per direction and 64 total channels per
/// direction; no automatic main/aux pairing or layout negotiation. The consumer
/// inspects inactive port metadata and supplies matching summed layout channels.
/// Foundation auxiliary buses stay empty: graph routes address the packed main
/// channel indices. Lifecycle and timing match the main-only factory.
pub fn prepare_hosted_graph_ports_node64<H: HostHandlers, FA>(
    instance: PluginInstance<H>,
    options: HostedGraphNodeOptions,
    audio_handler: FA,
) -> Result<(HostedGraphNodeControl<H>, HostedGraphNode64<H>), HostedGraphNodeBuildError<H>>
where
    FA: for<'a> FnOnce(
        &'a <H as HostHandlers>::Shared<'a>,
        &<H as HostHandlers>::MainThread<'a>,
    ) -> <H as HostHandlers>::AudioProcessor<'a>,
{
    prepare_native_graph_node::<H, f64, FA>(
        instance,
        options,
        audio_handler,
        NativeProfile::audio(true),
    )
    .map(|(control, inner)| (control, HostedGraphNode64 { inner }))
}
/// Explicit native audio port profile; no layout negotiation or pairing inference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostedGraphPortMode {
    MainOnly,
    Packed,
}
/// Prepare native f32 audio with bounded global parameter input. Main inspects
/// current native metadata, validates unique explicit identities/writable ranges
/// and allocates 1..4096 event slots. Known values use plain units; zero-frame
/// input flushes on audio without advancing native process/steady counters.
/// Unknown IDs/other event kinds reject the whole block before native callbacks.
/// Refresh/state/rescan changes require stopped retirement and fresh preparation.
pub fn prepare_hosted_parameter_graph_node<H: HostHandlers, FA>(
    instance: PluginInstance<H>,
    options: HostedGraphNodeOptions,
    port_mode: HostedGraphPortMode,
    mappings: Vec<crate::HostedGraphParameterMapping>,
    audio_handler: FA,
) -> Result<(HostedGraphNodeControl<H>, HostedGraphNode<H>), HostedGraphNodeBuildError<H>>
where
    FA: for<'a> FnOnce(
        &'a <H as HostHandlers>::Shared<'a>,
        &<H as HostHandlers>::MainThread<'a>,
    ) -> <H as HostHandlers>::AudioProcessor<'a>,
{
    prepare_native_graph_node::<H, f32, FA>(
        instance,
        options,
        audio_handler,
        NativeProfile {
            packed_ports: port_mode == HostedGraphPortMode::Packed,
            parameter_mappings: Some(mappings),
            ..NativeProfile::default()
        },
    )
    .map(|(control, inner)| (control, HostedGraphNode { inner }))
}
/// Prepare native f64 audio with bounded global parameter input. Main inspects
/// current native metadata, validates unique explicit identities/writable ranges
/// and allocates 1..4096 event slots. Known values use plain units; zero-frame
/// input flushes on audio without advancing native process/steady counters.
/// Unknown IDs/other event kinds reject the whole block before native callbacks.
/// Refresh/state/rescan changes require stopped retirement and fresh preparation.
pub fn prepare_hosted_parameter_graph_node64<H: HostHandlers, FA>(
    instance: PluginInstance<H>,
    options: HostedGraphNodeOptions,
    port_mode: HostedGraphPortMode,
    mappings: Vec<crate::HostedGraphParameterMapping>,
    audio_handler: FA,
) -> Result<(HostedGraphNodeControl<H>, HostedGraphNode64<H>), HostedGraphNodeBuildError<H>>
where
    FA: for<'a> FnOnce(
        &'a <H as HostHandlers>::Shared<'a>,
        &<H as HostHandlers>::MainThread<'a>,
    ) -> <H as HostHandlers>::AudioProcessor<'a>,
{
    prepare_native_graph_node::<H, f64, FA>(
        instance,
        options,
        audio_handler,
        NativeProfile {
            packed_ports: port_mode == HostedGraphPortMode::Packed,
            parameter_mappings: Some(mappings),
            ..NativeProfile::default()
        },
    )
    .map(|(control, inner)| (control, HostedGraphNode64 { inner }))
}
#[derive(Default)]
struct NativeProfile {
    packed_ports: bool,
    parameter_mappings: Option<Vec<crate::HostedGraphParameterMapping>>,
    output_mappings: Option<Vec<crate::HostedGraphParameterMapping>>,
    transport_enabled: bool,
    note_input: Option<crate::HostedGraphNoteInput>,
    note_output: Option<crate::HostedGraphNoteOutput>,
    gesture_output: bool,
    modulation_input: bool,
    gesture_input: bool,
    note_identity_input: bool,
    note_identity_output: bool,
    note_expression_input: bool,
    note_expression_output: bool,
    scoped_note_input: bool,
    scoped_note_output: bool,
    scoped_parameter_input: bool,
    scoped_parameter_output: bool,
    note_end_output: bool,
    modulation_output: bool,
}
impl NativeProfile {
    fn audio(packed_ports: bool) -> Self {
        Self {
            packed_ports,
            ..Self::default()
        }
    }
}
/// Main-thread preparation choices for fixed native audio/global parameters and
/// optional checked transport. Validation/metadata/allocations occur in prepare;
/// errors retain the inactive instance. Defaults match the f32 main-only factory.
/// Input mappings require 1..4096 input slots; output mappings similarly require
/// output slots and permit read-only native parameters. Each direction has unique
/// graph/native IDs. Unmapped/unsupported output filters; native identities,
/// scopes and completions require explicit independent profiles. Typed
/// invalid/out-of-range/unrepresentable/overflowing mapped output poisons the whole block
/// with BackendFailure (native histories may have advanced). Output values narrow
/// to portable f32 with finite/range/step checks; scoped values retain full f64.
pub struct HostedGraphNodeBuilder {
    options: HostedGraphNodeOptions,
    profile: NativeProfile,
}
impl HostedGraphNodeBuilder {
    pub fn new(options: HostedGraphNodeOptions) -> Self {
        Self {
            options,
            profile: NativeProfile::default(),
        }
    }
    pub fn with_port_mode(mut self, mode: HostedGraphPortMode) -> Self {
        self.profile.packed_ports = mode == HostedGraphPortMode::Packed;
        self
    }
    /// Select one inactive-validated native note port/dialect. Requires bounded
    /// positive input capacity. Unsupported kinds reject before native calls;
    /// zero-frame performance events reject rather than misuse parameter flush.
    pub fn with_note_input(mut self, route: crate::HostedGraphNoteInput) -> Self {
        self.profile.note_input = Some(route);
        self
    }
    /// Select one inactive-validated native output index/dialect, sharing bounded
    /// output slots with optional mapped parameters. Unsupported targets filter;
    /// invalid selected events poison the block with nonrollback BackendFailure.
    /// Performance output during zero-frame parameter flush rejects.
    pub fn with_note_output(mut self, route: crate::HostedGraphNoteOutput) -> Self {
        self.profile.note_output = Some(route);
        self
    }
    /// Preserve completion notifications addressed to the selected CLAP note
    /// INPUT endpoint, independently of note output, identities and parameters.
    /// Requires positive output capacity. Default filters; selected malformed
    /// scope/capacity faults retain native history. Native time/velocity are
    /// ignored; carrier offset equals the previous retained event's offset or 0.
    /// Zero-frame parameter flush rejects completions. No voice tracking inferred.
    pub fn with_note_end_output(mut self, enabled: bool) -> Self {
        self.profile.note_end_output = enabled;
        self
    }
    /// Opt in to mapped global native parameter gesture output. Requires an
    /// output parameter mapping; read-only mapped gesture targets reject. Timing,
    /// order/capacity and nonrollback faults match parameter output. Pairing,
    /// ownership, cancellation and automation recording remain receiver policy.
    pub fn with_parameter_gesture_output(mut self, enabled: bool) -> Self {
        self.profile.gesture_output = enabled;
        self
    }
    /// Enable mapped scoped parameter input on the selected CLAP note input
    /// endpoint. Requires writable input mappings before activation. Native flags
    /// validate the most specific target (ID/key/channel/port), retaining f64.
    /// Global modulation remains independently selected; flush uses zero time.
    pub fn with_scoped_parameter_input(mut self, enabled: bool) -> Self {
        self.profile.scoped_parameter_input = enabled;
        self
    }
    /// Include mapped scoped values/modulation targeting the selected note INPUT
    /// endpoint. Requires output mappings before activation; writable scope metadata, value
    /// range/step and native IDs validate without f32 narrowing. Default filters;
    /// malformed selected output faults without native rollback.
    pub fn with_scoped_parameter_output(mut self, enabled: bool) -> Self {
        self.profile.scoped_parameter_output = enabled;
        self
    }
    /// Enable scoped Off and choke on an explicitly selected CLAP input port.
    /// Default false rejects these events; matching/voice release stays receiver-owned.
    pub fn with_scoped_note_input(mut self, enabled: bool) -> Self {
        self.profile.scoped_note_input = enabled;
        self
    }
    /// Preserve wildcard-channel/key Off and choke from a selected CLAP output
    /// port. Concrete Off retains the legacy/identity profile; default filters
    /// scoped forms. Malformed selected scope rejects; no voice policy is inferred.
    pub fn with_scoped_note_output(mut self, enabled: bool) -> Self {
        self.profile.scoped_note_output = enabled;
        self
    }
    /// Enable standard f64 note expressions with wildcard targets on a selected
    /// CLAP input port. Requires that direction before activation; default false.
    pub fn with_note_expression_input(mut self, enabled: bool) -> Self {
        self.profile.note_expression_input = enabled;
        self
    }
    /// Include standard f64 note expressions from the selected CLAP output port.
    /// Unknown types/other or wildcard ports filter; malformed selected data faults.
    pub fn with_note_expression_output(mut self, enabled: bool) -> Self {
        self.profile.note_expression_output = enabled;
        self
    }
    /// Preserve explicit note IDs on an explicitly selected CLAP input port.
    /// IDs must fit native nonnegative i32; no MIDI identity loss is inferred.
    pub fn with_note_identity_input(mut self, enabled: bool) -> Self {
        self.profile.note_identity_input = enabled;
        self
    }
    /// Preserve concrete native note IDs from an explicitly selected CLAP output
    /// port. Default continues to filter them. Voice lifetime remains receiver policy.
    pub fn with_note_identity_output(mut self, enabled: bool) -> Self {
        self.profile.note_identity_output = enabled;
        self
    }
    /// Enable mapped raw begin/end input for writable parameters. Requires an
    /// explicit input map before activation; no pairing/base edits are inferred.
    pub fn with_parameter_gesture_input(mut self, enabled: bool) -> Self {
        self.profile.gesture_input = enabled;
        self
    }
    /// Enable finite global physical modulation on explicitly mapped writable
    /// input parameters advertising native IS_MODULATABLE. Values and modulation
    /// share capacity; amounts do not undergo base range/step checks. Unsupported
    /// targets reject the complete input before native mutation. Default disabled.
    pub fn with_parameter_modulation_input(mut self, enabled: bool) -> Self {
        self.profile.modulation_input = enabled;
        self
    }
    /// Include mapped global modulation output on writable native modulatable
    /// parameters. Finite f64 amounts narrow to finite f32 without base range/step
    /// quantization. Disabled/unmapped/scoped output filters; selected invalid
    /// data/order/capacity poisons the block with nonrollback BackendFailure.
    pub fn with_parameter_modulation_output(mut self, enabled: bool) -> Self {
        self.profile.modulation_output = enabled;
        self
    }
    pub fn with_parameter_input(
        mut self,
        mappings: Vec<crate::HostedGraphParameterMapping>,
    ) -> Self {
        self.profile.parameter_mappings = Some(mappings);
        self
    }
    pub fn with_parameter_output(
        mut self,
        mappings: Vec<crate::HostedGraphParameterMapping>,
    ) -> Self {
        self.profile.output_mappings = Some(mappings);
        self
    }
    pub fn with_transport_enabled(mut self, enabled: bool) -> Self {
        self.profile.transport_enabled = enabled;
        self
    }
    pub fn prepare<H: HostHandlers, FA>(
        self,
        instance: PluginInstance<H>,
        audio_handler: FA,
    ) -> Result<(HostedGraphNodeControl<H>, HostedGraphNode<H>), HostedGraphNodeBuildError<H>>
    where
        FA: for<'a> FnOnce(
            &'a <H as HostHandlers>::Shared<'a>,
            &<H as HostHandlers>::MainThread<'a>,
        ) -> <H as HostHandlers>::AudioProcessor<'a>,
    {
        prepare_native_graph_node::<H, f32, FA>(instance, self.options, audio_handler, self.profile)
            .map(|(control, inner)| (control, HostedGraphNode { inner }))
    }
    pub fn prepare64<H: HostHandlers, FA>(
        self,
        instance: PluginInstance<H>,
        audio_handler: FA,
    ) -> Result<(HostedGraphNodeControl<H>, HostedGraphNode64<H>), HostedGraphNodeBuildError<H>>
    where
        FA: for<'a> FnOnce(
            &'a <H as HostHandlers>::Shared<'a>,
            &<H as HostHandlers>::MainThread<'a>,
        ) -> <H as HostHandlers>::AudioProcessor<'a>,
    {
        prepare_native_graph_node::<H, f64, FA>(instance, self.options, audio_handler, self.profile)
            .map(|(control, inner)| (control, HostedGraphNode64 { inner }))
    }
}
type NativeNodeBuildResult<H, S> =
    Result<(HostedGraphNodeControl<H>, NativeGraphNode<H, S>), HostedGraphNodeBuildError<H>>;
fn prepare_native_graph_node<H: HostHandlers, S: NativeSample, FA>(
    mut instance: PluginInstance<H>,
    options: HostedGraphNodeOptions,
    audio_handler: FA,
    profile: NativeProfile,
) -> NativeNodeBuildResult<H, S>
where
    FA: for<'a> FnOnce(
        &'a <H as HostHandlers>::Shared<'a>,
        &<H as HostHandlers>::MainThread<'a>,
    ) -> <H as HostHandlers>::AudioProcessor<'a>,
{
    let NativeProfile {
        packed_ports,
        parameter_mappings,
        output_mappings,
        transport_enabled,
        note_input,
        note_output,
        gesture_output,
        modulation_input,
        gesture_input,
        note_identity_input,
        note_identity_output,
        note_expression_input,
        note_expression_output,
        scoped_note_input,
        scoped_note_output,
        scoped_parameter_input,
        scoped_parameter_output,
        note_end_output,
        modulation_output,
    } = profile;
    let config = options.configuration;
    if ((note_identity_input || note_expression_input || scoped_note_input)
        && !note_input.is_some_and(|route| route.dialect == crate::HostedGraphNoteDialect::Clap))
        || ((note_identity_output || note_expression_output || scoped_note_output)
            && !note_output
                .is_some_and(|route| route.dialect == crate::HostedGraphNoteDialect::Clap))
        || ((scoped_parameter_input || scoped_parameter_output || note_end_output)
            && !note_input
                .is_some_and(|route| route.dialect == crate::HostedGraphNoteDialect::Clap))
        || (scoped_parameter_input && parameter_mappings.is_none())
        || (scoped_parameter_output && output_mappings.is_none())
        || ((gesture_output || modulation_output) && output_mappings.is_none())
        || ((modulation_input || gesture_input) && parameter_mappings.is_none())
        || config.maximum_frames() > 65536
        || (parameter_mappings.is_none() && note_input.is_none() && config.maximum_events() != 0)
        || config.maximum_events() > 4096
        || (output_mappings.is_none()
            && note_output.is_none()
            && !note_end_output
            && config.maximum_output_events() != 0)
        || config.maximum_output_events() > 4096
        || !config.auxiliary_buses().is_empty()
    {
        return Err(HostedGraphNodeBuildError {
            reason: HostedGraphNodeBuildFailure::Configuration,
            instance,
        });
    }
    let ports = match HostedAudioConfiguration::inspect(&mut instance) {
        Ok(ports) => ports,
        Err(error) => {
            return Err(HostedGraphNodeBuildError {
                reason: HostedGraphNodeBuildFailure::Metadata(error),
                instance,
            });
        }
    };
    for (ports, channels) in [
        (&ports.inputs, config.layout().input_channels()),
        (&ports.outputs, config.layout().output_channels()),
    ] {
        if (packed_ports
            && ports
                .iter()
                .map(|port| port.channels as usize)
                .sum::<usize>()
                != channels)
            || (!packed_ports
                && ((channels == 0 && !ports.is_empty())
                    || (channels != 0
                        && (ports.len() != 1
                            || ports[0].channels as usize != channels
                            || !ports[0].flags.contains(AudioPortFlags::IS_MAIN)))))
        {
            return Err(HostedGraphNodeBuildError {
                reason: HostedGraphNodeBuildFailure::Configuration,
                instance,
            });
        }
    }
    if S::IS_F64
        && ports
            .inputs
            .iter()
            .chain(&ports.outputs)
            .any(|port| !port.flags.contains(AudioPortFlags::SUPPORTS_64BITS))
    {
        return Err(HostedGraphNodeBuildError {
            reason: HostedGraphNodeBuildFailure::Precision,
            instance,
        });
    }
    let note_input = if let Some(route) = note_input {
        match crate::graph_note_input::prepare(&mut instance, route) {
            Ok(route) => Some(route),
            Err(error) => {
                return Err(HostedGraphNodeBuildError {
                    reason: HostedGraphNodeBuildFailure::Notes(error),
                    instance,
                });
            }
        }
    } else {
        None
    };
    let parameters = if parameter_mappings.is_some() || note_input.is_some() {
        match crate::graph_parameters::ParameterInput::prepare(
            &mut instance,
            parameter_mappings,
            note_input.map(|route| crate::graph_note_input::InputProfile {
                route,
                identities: note_identity_input,
                expressions: note_expression_input,
                scoped: scoped_note_input,
            }),
            config.maximum_events(),
            modulation_input,
            gesture_input,
            scoped_parameter_input,
        ) {
            Ok(parameters) => Some(parameters),
            Err(error) => {
                return Err(HostedGraphNodeBuildError {
                    reason: HostedGraphNodeBuildFailure::Parameters(error),
                    instance,
                });
            }
        }
    } else {
        None
    };
    let note_output = if let Some(route) = note_output {
        match crate::graph_note_input::prepare_output(&mut instance, route) {
            Ok(route) => Some(route),
            Err(error) => {
                return Err(HostedGraphNodeBuildError {
                    reason: HostedGraphNodeBuildFailure::Notes(error),
                    instance,
                });
            }
        }
    } else {
        None
    };
    let parameter_output = if output_mappings.is_some() || note_output.is_some() || note_end_output
    {
        match crate::graph_parameter_output::ParameterOutput::prepare(
            &mut instance,
            output_mappings,
            note_output.map(|route| crate::graph_note_output::OutputProfile {
                route,
                identities: note_identity_output,
                expressions: note_expression_output,
                scoped: scoped_note_output,
            }),
            gesture_output,
            modulation_output,
            config.maximum_output_events(),
            crate::graph_parameter_output::InputEndpointOutputProfile {
                scoped_parameters: if scoped_parameter_output {
                    note_input.map(|route| route.port_index)
                } else {
                    None
                },
                completions: if note_end_output {
                    note_input.map(|route| route.port_index)
                } else {
                    None
                },
            },
        ) {
            Ok(output) => output,
            Err(error) => {
                return Err(HostedGraphNodeBuildError {
                    reason: HostedGraphNodeBuildFailure::Parameters(error),
                    instance,
                });
            }
        }
    } else {
        crate::graph_parameter_output::ParameterOutput::default()
    };
    let samples = |ports: &[crate::HostedAudioPort]| {
        ports
            .iter()
            .map(|port| vec![vec![S::default(); config.maximum_frames()]; port.channels as usize])
            .collect()
    };
    let inputs = samples(&ports.inputs);
    let outputs = samples(&ports.outputs);
    let input_ports =
        AudioPorts::with_capacity(config.layout().input_channels(), ports.inputs.len());
    let output_ports =
        AudioPorts::with_capacity(config.layout().output_channels(), ports.outputs.len());
    let (returned_tx, returned_rx) = RingBuffer::new(1);
    let counters = Arc::<Counters>::default();
    let stopped = match instance.activate(
        audio_handler,
        PluginAudioConfiguration {
            sample_rate: config.sample_rate(),
            min_frames_count: 1,
            max_frames_count: config.maximum_frames() as u32,
        },
    ) {
        Ok(stopped) => stopped,
        Err(error) => {
            return Err(HostedGraphNodeBuildError {
                reason: HostedGraphNodeBuildFailure::Activation(error),
                instance,
            });
        }
    };
    Ok((
        HostedGraphNodeControl {
            instance: Some(instance),
            returned: returned_rx,
            counters: counters.clone(),
            deactivated: false,
        },
        NativeGraphNode {
            options,
            main: std::thread::current().id(),
            owner: None,
            prepared: false,
            audio: Some(AudioToken::Stopped(stopped)),
            returned: returned_tx,
            counters,
            buffers: Buffers {
                inputs,
                outputs,
                input_ports,
                output_ports,
            },
            pending_reset: false,
            steady_time: Some(0),
            transport_enabled,
            parameters,
            parameter_output,
        },
    ))
}

struct Buffers<S: NativeSample> {
    inputs: Vec<Vec<Vec<S>>>,
    outputs: Vec<Vec<Vec<S>>>,
    input_ports: AudioPorts,
    output_ports: AudioPorts,
}
trait NativeSample: ravet_foundation::Sample + Send {
    const IS_F64: bool;
    fn call<H: HostHandlers>(
        buffers: &mut Buffers<Self>,
        started: &mut StartedPluginAudioProcessor<H>,
        frames: usize,
        steady_time: Option<u64>,
        transport: Option<&clack_host::events::event_types::TransportEvent>,
        input_events: &InputEvents,
        output_events: &mut OutputEvents,
    ) -> Result<clack_host::process::ProcessStatus, PluginInstanceError>;
}
macro_rules! native_sample {
    ($sample:ty, $input:ident, $output:ident, $is64:expr) => {
        impl NativeSample for $sample {
            const IS_F64: bool = $is64;
            fn call<H: HostHandlers>(
                buffers: &mut Buffers<Self>,
                started: &mut StartedPluginAudioProcessor<H>,
                frames: usize,
                steady_time: Option<u64>,
                transport: Option<&clack_host::events::event_types::TransportEvent>,
                input_events: &InputEvents,
                output_events: &mut OutputEvents,
            ) -> Result<clack_host::process::ProcessStatus, PluginInstanceError> {
                let input = buffers
                    .input_ports
                    .with_input_buffers(buffers.inputs.iter_mut().map(|port| {
                        AudioPortBuffer {
                            latency: 0,
                            channels: AudioPortBufferType::$input(
                                port.iter_mut()
                                    .map(|samples| InputChannel::variable(&mut samples[..frames])),
                            ),
                        }
                    }));
                let mut output =
                    buffers
                        .output_ports
                        .with_output_buffers(buffers.outputs.iter_mut().map(|port| {
                            AudioPortBuffer {
                                latency: 0,
                                channels: AudioPortBufferType::$output(
                                    port.iter_mut().map(|samples| &mut samples[..frames]),
                                ),
                            }
                        }));
                started.process(
                    &input,
                    &mut output,
                    input_events,
                    output_events,
                    steady_time,
                    transport,
                )
            }
        }
    };
}
native_sample!(f32, f32_input_only, f32_output_only, false);
native_sample!(f64, f64_input_only, f64_output_only, true);
macro_rules! node_processor {
    ($node:ident, $sample:ty) => {
        impl<H: HostHandlers> $node<H> {
            /// Explicit main-thread transport opt-in while stopped. Disabled by
            /// default; require known playing for any context and reject invalid/
            /// unrepresentable fields before native calls. Discontinuity has no
            /// native flag and does not reset history; see hosted_transport.
            pub fn set_transport_enabled(&mut self, enabled: bool) -> Result<(), PrepareError> {
                if std::thread::current().id() != self.inner.main
                    || !matches!(self.inner.audio, Some(AudioToken::Stopped(_)))
                {
                    return Err(PrepareError::UnsupportedLayout);
                }
                self.inner.transport_enabled = enabled;
                Ok(())
            }
        }
        impl<H: HostHandlers> Processor<$sample> for $node<H> {
            fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
                self.inner.prepare(config)
            }
            fn start_audio(&mut self) -> Result<(), ProcessError> {
                self.inner.start_audio()
            }
            fn stop_audio(&mut self) {
                self.inner.stop_audio();
            }
            fn reset(&mut self) {
                self.inner.reset();
            }
            fn deactivate(&mut self) {
                self.inner.deactivate();
            }
            fn latency_samples(&self) -> u32 {
                self.inner.latency_samples()
            }
            fn tail(&self) -> Tail {
                self.inner.tail()
            }
            fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, $sample>) -> PortableStatus {
                self.inner.process(block)
            }
        }
    };
}
node_processor!(HostedGraphNode, f32);
node_processor!(HostedGraphNode64, f64);
