//! Coalesced native audio configuration notifications, deferred to scheduling.
use crate::{ParameterMainRequests, ParameterSharedRequests};
use clack_extensions::{
    audio_ports::{AudioPortRescanFlags, HostAudioPorts, HostAudioPortsImpl},
    latency::{HostLatency, HostLatencyImpl},
    params::{HostParams, HostParamsImplMainThread, ParamClearFlags, ParamRescanFlags},
    tail::{HostTail, HostTailImpl},
};
use clack_host::{
    host::{AudioProcessorHandler, HostExtensions, HostHandlers, MainThreadHandler},
    utils::ClapId,
};
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};
const LATENCY: u32 = 1 << 30;
const TAIL: u32 = 1 << 31;
/// Independent coalesced changes; consumers own deactivation, metadata re-query,
/// prepared graph rebuilding and retirement. Tail must be queried on audio owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingAudioChanges {
    pub ports: AudioPortRescanFlags,
    pub latency: bool,
    pub tail: bool,
}
/// One prepared mailbox per instance. Atomic publication/drain has no framework
/// heap, locks or plugin reentry. Clone only during preparation. Notifications
/// merge; a snapshot is not a lossless ordered configuration history.
#[derive(Clone, Default)]
pub struct AudioConfigurationRequests(Arc<AtomicU32>);
impl AudioConfigurationRequests {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn take(&self) -> PendingAudioChanges {
        let bits = self.0.swap(0, Ordering::AcqRel);
        PendingAudioChanges {
            ports: AudioPortRescanFlags::from_bits_truncate(bits),
            latency: bits & LATENCY != 0,
            tail: bits & TAIL != 0,
        }
    }
    pub fn defer(&self, changes: PendingAudioChanges) {
        let bits = (changes.ports.bits() & AudioPortRescanFlags::all().bits())
            | if changes.latency { LATENCY } else { 0 }
            | if changes.tail { TAIL } else { 0 };
        self.0.fetch_or(bits, Ordering::Release);
    }
}
impl AudioProcessorHandler<'_> for AudioConfigurationRequests {}
impl HostTailImpl for AudioConfigurationRequests {
    fn changed(&mut self) {
        self.0.fetch_or(TAIL, Ordering::Release);
    }
}
/// Owner-thread parameter mailbox combined with audio notification publication.
/// Prepare and retain both supplied mailboxes; pass a clone of this wrapper to the
/// main handler factory, and a configuration-mailbox clone to audio activation.
#[derive(Clone)]
pub struct ProcessingMainRequests {
    parameters: ParameterMainRequests,
    audio: AudioConfigurationRequests,
}
impl ProcessingMainRequests {
    pub fn new(parameters: ParameterMainRequests, audio: AudioConfigurationRequests) -> Self {
        Self { parameters, audio }
    }
}
impl MainThreadHandler<'_> for ProcessingMainRequests {}
impl HostParamsImplMainThread for ProcessingMainRequests {
    fn rescan(&self, flags: ParamRescanFlags) {
        self.parameters.rescan(flags);
    }
    fn clear(&self, id: ClapId, flags: ParamClearFlags) {
        self.parameters.clear(id, flags);
    }
}
impl HostAudioPortsImpl for ProcessingMainRequests {
    fn is_rescan_flag_supported(&self, flag: AudioPortRescanFlags) -> bool {
        !flag.is_empty() && AudioPortRescanFlags::all().contains(flag)
    }
    fn rescan(&self, flags: AudioPortRescanFlags) {
        self.audio.0.fetch_or(
            flags.bits() & AudioPortRescanFlags::all().bits(),
            Ordering::Release,
        );
    }
}
impl HostLatencyImpl for ProcessingMainRequests {
    fn changed(&self) {
        self.audio.0.fetch_or(LATENCY, Ordering::Release);
    }
}
/// Explicit parameter/audio-port/latency/tail-aware handlers. All advertised
/// changes require consumer service. Non-name port changes need deactivation;
/// latency changes require a new timing query after activation. This creates no
/// native scheduler and cannot update a running graph automatically.
pub struct ProcessingHost;
impl HostHandlers for ProcessingHost {
    type Shared<'a> = ParameterSharedRequests;
    type MainThread<'a> = ProcessingMainRequests;
    type AudioProcessor<'a> = AudioConfigurationRequests;
    fn declare_extensions(builder: &mut HostExtensions<Self>, _: &Self::Shared<'_>) {
        builder
            .register::<HostParams>()
            .register::<HostAudioPorts>()
            .register::<HostLatency>()
            .register::<HostTail>();
    }
}
