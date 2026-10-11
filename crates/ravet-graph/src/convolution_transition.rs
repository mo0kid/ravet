//! Live convolution transitions with worker-owned preparation and disposal.
//!
//! A worker (any non-audio thread owning the [`ConvolutionTransitionControl`])
//! prepares a new [`ravet_signal`] engine or bank, offers it through a bounded
//! single-producer/single-consumer handoff, and later drops returned engines. The
//! audio-owned [`ConvolutionTransition`] accepts at most one proposal per block
//! boundary, runs the old and new engines together for an explicit
//! [`CrossfadePolicy`], and hands the old engine back through a bounded return
//! queue. Process, reset and handoff on the audio side never allocate, lock,
//! deallocate or panic; the handoff and retirement mechanics follow
//! [`crate::DynamicGraph`] (`rtrb` queues of boxed owners plus one deferred
//! retirement slot). See [`ConvolutionTransition`] for exactly which thread frees
//! what at shutdown.
//!
//! [`TransitionStatus`] returned by each call is authoritative; the
//! [`TransitionReport`] queue is best-effort (bounded, with loss counts).
//!
//! Limits: the new engine starts from empty history, so for a response longer
//! than the fade its output still lacks the tail contributed by input from before
//! the transition; for stationary input the blend dips in level, by up to
//! `1 - (energy of the first fade-length taps / total energy)` at the end of the
//! fade, recovering as the new history fills. Engines with different latencies
//! are not aligned: under [`LatencyPolicy::Report`] the fade mixes both
//! latencies and the consumer applies any host latency change.
//!
//! ```
//! use ravet_graph::{
//!     CrossfadeLaw, CrossfadePolicy, LatencyPolicy, MidFadePolicy, TransitionOptions,
//!     convolution_transition,
//! };
//! use ravet_signal::{ConvolutionPartitioning, Convolver64};
//! let schedule = ConvolutionPartitioning::ZeroLatency { head: 16 };
//! let first = Box::new(Convolver64::new(&[1.0], schedule, 64, 1 << 20).unwrap());
//! let options = TransitionOptions {
//!     crossfade: CrossfadePolicy { frames: 4, law: CrossfadeLaw::EqualGain },
//!     mid_fade: MidFadePolicy::Refuse,
//!     latency: LatencyPolicy::RequireEqual,
//! };
//! let (mut control, mut audio) = convolution_transition(first, 64, options).unwrap();
//! // Worker: prepare and offer a replacement (allocation happens here).
//! let next = Box::new(Convolver64::new(&[0.5], schedule, 64, 1 << 20).unwrap());
//! let sequence = control.offer(next).unwrap();
//! // Audio: the next block starts a four-frame equal-gain fade.
//! let input = [1.0; 6];
//! let mut output = [0.0; 6];
//! let status = audio.process(&[&input], &mut [&mut output], 6).unwrap();
//! assert_eq!(status.started, Some(sequence));
//! assert_eq!(output, [0.875, 0.75, 0.625, 0.5, 0.5, 0.5]);
//! // Worker: the old engine comes back for disposal off the audio thread.
//! let returned = control.take_returned().unwrap();
//! assert_eq!(returned.sequence, 0);
//! ```
use ravet_signal::{
    ConvolutionBank32, ConvolutionBank64, ConvolutionProcessError, Convolver32, Convolver64,
};
use rtrb::{Consumer, Producer, PushError, RingBuffer};

/// Largest accepted crossfade length in frames.
pub const TRANSITION_MAXIMUM_FADE_FRAMES: usize = 1 << 20;
/// Returned-engine queue capacity (retired, refused and superseded engines).
pub const TRANSITION_RETURN_CAPACITY: usize = 4;
/// Transition report queue capacity.
pub const TRANSITION_REPORT_CAPACITY: usize = 16;

mod sealed {
    pub trait Sealed {}
}

/// Sealed native transition precision (`f32` or `f64`).
pub trait TransitionSample:
    sealed::Sealed
    + Copy
    + Default
    + Send
    + 'static
    + std::ops::Mul<Output = Self>
    + std::ops::Add<Output = Self>
{
    /// Round an f64 gain to this precision (`as` conversion, round to nearest).
    fn from_gain(gain: f64) -> Self;
}
impl sealed::Sealed for f32 {}
impl sealed::Sealed for f64 {}
impl TransitionSample for f32 {
    fn from_gain(gain: f64) -> Self {
        gain as f32
    }
}
impl TransitionSample for f64 {
    fn from_gain(gain: f64) -> Self {
        gain
    }
}

/// Sealed set of transitionable engines: [`Convolver32`], [`Convolver64`],
/// [`ConvolutionBank32`] and [`ConvolutionBank64`]. A single engine is one channel.
pub trait TransitionEngine: sealed::Sealed + Send + 'static {
    /// Native sample precision.
    type Sample: TransitionSample;
    /// Channel count (1 for a single engine).
    fn transition_channels(&self) -> usize;
    /// Prepared maximum frames per call.
    fn transition_maximum_frames(&self) -> usize;
    /// Exact algorithmic latency in frames (shared by every bank channel).
    fn transition_latency(&self) -> usize;
    /// Largest channel tail in frames, including latency.
    fn transition_tail(&self) -> usize;
    /// Process one channel prefix with the engine's own admission.
    fn transition_process(
        &mut self,
        channel: usize,
        input: &[Self::Sample],
        output: &mut [Self::Sample],
        frames: usize,
    ) -> Result<(), ConvolutionProcessError>;
    /// Clear all history.
    fn transition_reset(&mut self);
}

macro_rules! engine {
    ($engine:ty, $sample:ty) => {
        impl sealed::Sealed for $engine {}
        impl TransitionEngine for $engine {
            type Sample = $sample;
            fn transition_channels(&self) -> usize {
                1
            }
            fn transition_maximum_frames(&self) -> usize {
                self.maximum_frames()
            }
            fn transition_latency(&self) -> usize {
                self.info().latency_samples
            }
            fn transition_tail(&self) -> usize {
                self.info().tail_samples
            }
            fn transition_process(
                &mut self,
                channel: usize,
                input: &[$sample],
                output: &mut [$sample],
                frames: usize,
            ) -> Result<(), ConvolutionProcessError> {
                if channel != 0 {
                    return Err(ConvolutionProcessError::InvalidChannel);
                }
                self.process(input, output, frames)
            }
            fn transition_reset(&mut self) {
                self.reset();
            }
        }
    };
}
macro_rules! bank {
    ($bank:ty, $sample:ty) => {
        impl sealed::Sealed for $bank {}
        impl TransitionEngine for $bank {
            type Sample = $sample;
            fn transition_channels(&self) -> usize {
                self.channels()
            }
            fn transition_maximum_frames(&self) -> usize {
                self.maximum_frames()
            }
            fn transition_latency(&self) -> usize {
                self.engine(0).map_or(0, |e| e.info().latency_samples)
            }
            fn transition_tail(&self) -> usize {
                (0..self.channels())
                    .filter_map(|c| self.engine(c))
                    .map(|e| e.info().tail_samples)
                    .max()
                    .unwrap_or(0)
            }
            fn transition_process(
                &mut self,
                channel: usize,
                input: &[$sample],
                output: &mut [$sample],
                frames: usize,
            ) -> Result<(), ConvolutionProcessError> {
                self.engine_mut(channel)
                    .ok_or(ConvolutionProcessError::InvalidChannel)?
                    .process(input, output, frames)
            }
            fn transition_reset(&mut self) {
                self.reset();
            }
        }
    };
}
engine!(Convolver32, f32);
engine!(Convolver64, f64);
bank!(ConvolutionBank32, f32);
bank!(ConvolutionBank64, f64);

/// Crossfade gain law. For fade position `k` in `0..frames` (frames since the
/// fade started) let `t = (k + 1) as f64 / frames as f64`; the last faded frame
/// has `t == 1`, so its gains are `(0, 1)` to rounding (`cos(pi/2)` is about
/// 6.1e-17 in f64). Gains are computed in f64 once per frame and rounded once to
/// the sample type; in f32 the two rounded gains can differ from their f64 sum
/// by one unit in the last place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrossfadeLaw {
    /// `old = 1 - t`, `new = t` (gains sum to one; suited to correlated responses).
    EqualGain,
    /// `old = cos(t * pi / 2)`, `new = sin(t * pi / 2)` (squared gains sum to one;
    /// suited to uncorrelated responses).
    EqualPower,
}
impl CrossfadeLaw {
    /// `(old, new)` f64 gains at fade position `position` of `frames` (`frames > 0`).
    pub fn gains(self, position: usize, frames: usize) -> (f64, f64) {
        let t = (position + 1) as f64 / frames as f64;
        match self {
            Self::EqualGain => (1. - t, t),
            Self::EqualPower => {
                let angle = t * std::f64::consts::FRAC_PI_2;
                (angle.cos(), angle.sin())
            }
        }
    }
}

/// Explicit crossfade between old and new engines. Both engines process the same
/// input for `frames` frames starting at the block boundary where the transition
/// starts; each output sample is `S(old) * y_old + S(new) * y_new` evaluated in
/// the native sample type. `frames == 0` is an explicit cut: the old engine is
/// retired at that boundary without running. The new engine starts from its own
/// prepared (zero) history; there is no history transfer between engines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrossfadePolicy {
    /// Fade length in frames, 0..=1,048,576.
    pub frames: usize,
    /// Gain law.
    pub law: CrossfadeLaw,
}

/// What happens to a proposal while a fade is in progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidFadePolicy {
    /// A proposal that reaches the audio side while a transition is in progress is
    /// returned unused ([`ReturnReason::Refused`]) at a block boundary; the running
    /// fade is unaffected. If the returned queue is full at that boundary, the
    /// proposal stays in the handoff marked refused (`retirement_blocked`) and is
    /// returned as soon as a slot frees, even after the fade; it never starts.
    Refuse,
    /// Queue depth 1: the proposal stays in the one-slot handoff until the fade
    /// completes and starts at the following block boundary. Further offers are
    /// refused with [`TransitionOfferFailure::Full`] and stay with the caller.
    QueueOne,
    /// Latest wins: during a transition the audio side moves each new proposal
    /// into one waiting slot, returning any older waiting proposal unused
    /// ([`ReturnReason::Superseded`]). At the first block boundary after the fade,
    /// a proposal still in the handoff supersedes the waiting one, and the newest
    /// proposal starts. Superseding needs a free returned slot; otherwise the
    /// boundary reports `retirement_blocked` and nothing starts (the stale waiting
    /// proposal is never started in place of a newer one).
    LatestWins,
}

/// Whether engines with a different algorithmic latency are accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LatencyPolicy {
    /// Refuse offers whose latency differs from the initial engine's latency.
    RequireEqual,
    /// Accept and report each change through [`TransitionEvent::Started`]. The
    /// consumer applies its host's latency-change mechanism; during the fade the
    /// output mixes both latencies.
    Report,
}

/// Fixed transition configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransitionOptions {
    /// Crossfade length and law.
    pub crossfade: CrossfadePolicy,
    /// Mid-fade proposal behaviour.
    pub mid_fade: MidFadePolicy,
    /// Latency-change behaviour.
    pub latency: LatencyPolicy,
}

/// Why an engine was returned to the control owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReturnReason {
    /// Replaced; its fade (or cut) completed.
    Retired,
    /// Refused mid-fade under [`MidFadePolicy::Refuse`]; never processed audio.
    Refused,
    /// Replaced while waiting under [`MidFadePolicy::LatestWins`]; never processed audio.
    Superseded,
}

/// An engine handed back for reuse or disposal on the control thread.
pub struct ReturnedEngine<E> {
    /// Reason for the return.
    pub reason: ReturnReason,
    /// Offer sequence (0 for the initial engine).
    pub sequence: u64,
    /// The returned engine.
    pub engine: Box<E>,
}
impl<E> std::fmt::Debug for ReturnedEngine<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReturnedEngine")
            .field("reason", &self.reason)
            .field("sequence", &self.sequence)
            .finish_non_exhaustive()
    }
}

/// Audio-side transition event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionEvent {
    /// A proposal became current at a block boundary.
    Started {
        /// Latency of the engine being faded out.
        previous_latency: usize,
        /// Latency of the new current engine.
        latency: usize,
        /// Configured fade frames (0 for a cut).
        fade_frames: usize,
    },
    /// The fade into this sequence completed; the old engine was handed back.
    Completed,
    /// The proposal was refused mid-fade.
    Refused,
    /// The waiting proposal was superseded.
    Superseded,
}

/// One audio-side event delivered to the control owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransitionReport {
    /// Offer sequence the event refers to.
    pub sequence: u64,
    /// Event.
    pub event: TransitionEvent,
    /// Absolute audio frame (frames accepted before it, saturating) at which the
    /// event took effect.
    pub frame: u64,
    /// Reports dropped because the report queue was full since the previous
    /// delivered report (saturating).
    pub lost_before: u64,
}

/// Per-block audio-side status.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransitionStatus {
    /// Sequence that became current at this block boundary.
    pub started: Option<u64>,
    /// Sequence whose fade (or cut) completed in this block.
    pub completed: Option<u64>,
    /// A fade remains in progress after this block.
    pub fading: bool,
    /// A proposal could not be taken because returned-engine storage is full;
    /// the control owner must take or reclaim returned engines.
    pub retirement_blocked: bool,
}

/// Why an offer was refused. The caller keeps the engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionOfferFailure {
    /// Channel count differs from the initial engine.
    Channels,
    /// Engine maximum frames is below the transition maximum.
    MaximumFrames,
    /// Latency differs under [`LatencyPolicy::RequireEqual`].
    Latency,
    /// The one-slot handoff is occupied.
    Full,
    /// The audio endpoint has been dropped.
    AudioGone,
}

/// Refused offer with ownership of the unchanged engine.
pub struct TransitionOfferError<E> {
    /// Refusal reason.
    pub reason: TransitionOfferFailure,
    /// The engine, unchanged.
    pub engine: Box<E>,
}
impl<E> std::fmt::Debug for TransitionOfferError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransitionOfferError")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}

/// Why construction was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionSetupFailure {
    /// Maximum frames is 0 or exceeds the initial engine's prepared maximum.
    MaximumFrames,
    /// Crossfade frames exceed [`TRANSITION_MAXIMUM_FADE_FRAMES`].
    FadeFrames,
    /// Scratch reservation failed.
    Allocation,
}

/// Refused construction with ownership of the unchanged initial engine.
pub struct TransitionSetupError<E> {
    /// Refusal reason.
    pub reason: TransitionSetupFailure,
    /// The initial engine, unchanged.
    pub engine: Box<E>,
}
impl<E> std::fmt::Debug for TransitionSetupError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransitionSetupError")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}

struct Proposal<E> {
    sequence: u64,
    engine: Box<E>,
}

/// Unique control/worker endpoint. Prepare engines, offer them, and take or drop
/// returned engines here, never on the audio thread. `Send` when the engine is.
pub struct ConvolutionTransitionControl<E: TransitionEngine> {
    pending: Producer<Proposal<E>>,
    returned: Consumer<ReturnedEngine<E>>,
    reports: Consumer<TransitionReport>,
    channels: usize,
    maximum_frames: usize,
    latency: usize,
    options: TransitionOptions,
    next_sequence: u64,
}

impl<E: TransitionEngine> std::fmt::Debug for ConvolutionTransitionControl<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConvolutionTransitionControl")
            .field("channels", &self.channels)
            .field("maximum_frames", &self.maximum_frames)
            .field("options", &self.options)
            .field("next_sequence", &self.next_sequence)
            .finish_non_exhaustive()
    }
}
impl<E: TransitionEngine> std::fmt::Debug for ConvolutionTransition<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConvolutionTransition")
            .field("current_sequence", &self.current_sequence)
            .field("fade_position", &self.fade_position())
            .field("waiting_sequence", &self.waiting_sequence())
            .field("frames_processed", &self.frame)
            .finish_non_exhaustive()
    }
}
impl<E: TransitionEngine> ConvolutionTransitionControl<E> {
    /// Offer an already-prepared engine. Channel count, maximum frames and (under
    /// [`LatencyPolicy::RequireEqual`]) latency are checked here; a refused engine
    /// is returned unchanged. Success returns the proposal's sequence (1, 2, ...).
    /// The engine should have fresh history; it is not reset on handoff.
    /// `Ok` means "queued in the handoff", not "will play": a queued proposal can
    /// later be refused or superseded, and if the audio endpoint is dropped after
    /// the abandonment check it is freed with the queue.
    pub fn offer(&mut self, engine: Box<E>) -> Result<u64, TransitionOfferError<E>> {
        let reason = if self.pending.is_abandoned() {
            Some(TransitionOfferFailure::AudioGone)
        } else if engine.transition_channels() != self.channels {
            Some(TransitionOfferFailure::Channels)
        } else if engine.transition_maximum_frames() < self.maximum_frames {
            Some(TransitionOfferFailure::MaximumFrames)
        } else if self.options.latency == LatencyPolicy::RequireEqual
            && engine.transition_latency() != self.latency
        {
            Some(TransitionOfferFailure::Latency)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(TransitionOfferError { reason, engine });
        }
        let sequence = self.next_sequence;
        match self.pending.push(Proposal { sequence, engine }) {
            Ok(()) => {
                self.next_sequence += 1;
                Ok(sequence)
            }
            Err(PushError::Full(p)) => Err(TransitionOfferError {
                reason: TransitionOfferFailure::Full,
                engine: p.engine,
            }),
        }
    }
    /// Whether the one-slot handoff currently holds an untaken proposal.
    pub fn has_pending(&self) -> bool {
        self.pending.slots() == 0
    }
    /// Take one returned engine for disposal, or for reuse after calling the
    /// engine's own `reset` (retired engines keep their history).
    pub fn take_returned(&mut self) -> Option<ReturnedEngine<E>> {
        self.returned.pop().ok()
    }
    /// Drop up to [`TRANSITION_RETURN_CAPACITY`] returned engines on this thread.
    pub fn reclaim(&mut self) -> usize {
        let mut count = 0;
        for _ in 0..TRANSITION_RETURN_CAPACITY {
            match self.take_returned() {
                Some(returned) => {
                    drop(returned);
                    count += 1;
                }
                None => break,
            }
        }
        count
    }
    /// Take one audio-side report.
    pub fn take_report(&mut self) -> Option<TransitionReport> {
        self.reports.pop().ok()
    }
    /// Fixed configuration.
    pub fn options(&self) -> TransitionOptions {
        self.options
    }
}

/// Audio-owned transition. Between construction and drop, `process` and `reset`
/// perform no allocation, deallocation, locking, I/O or panic: every engine handed
/// back first has queue capacity reserved (or the one deferred slot), and a
/// completed fade whose engine has nowhere to go keeps that engine as a silent,
/// unprocessed outgoing owner until a later block boundary can return it.
///
/// Drop behaviour (construct and drop this endpoint outside the audio callback):
/// dropping it frees its current, outgoing, waiting and deferred engines on the
/// dropping thread. The three queues, including any engines still queued in the
/// handoff or returned queue, are freed by whichever endpoint is dropped last. If
/// the control endpoint is dropped first, `process` continues: retirements still
/// move into the (now unread) returned queue and, once it is full, the deferred
/// slot; nothing is freed until this endpoint is dropped.
pub struct ConvolutionTransition<E: TransitionEngine> {
    current: Box<E>,
    current_sequence: u64,
    outgoing: Option<(u64, Box<E>)>,
    fade_position: usize,
    waiting: Option<Proposal<E>>,
    deferred: Option<ReturnedEngine<E>>,
    pending: Consumer<Proposal<E>>,
    returned: Producer<ReturnedEngine<E>>,
    reports: Producer<TransitionReport>,
    scratch: Vec<E::Sample>,
    gains: Vec<f64>,
    refuse_head: bool,
    channels: usize,
    maximum_frames: usize,
    options: TransitionOptions,
    frame: u64,
    lost_reports: u64,
}

/// Construct the control and audio endpoints around a prepared initial engine
/// (CONTROL: allocates three bounded queues, one channel of sample scratch and
/// per-frame gain scratch).
/// `maximum_frames` (1..=initial maximum) bounds every audio call and every
/// later offer. A refusal returns the initial engine unchanged.
#[allow(clippy::type_complexity)]
pub fn convolution_transition<E: TransitionEngine>(
    initial: Box<E>,
    maximum_frames: usize,
    options: TransitionOptions,
) -> Result<(ConvolutionTransitionControl<E>, ConvolutionTransition<E>), TransitionSetupError<E>> {
    let refuse = |reason, engine| Err(TransitionSetupError { reason, engine });
    if maximum_frames == 0 || maximum_frames > initial.transition_maximum_frames() {
        return refuse(TransitionSetupFailure::MaximumFrames, initial);
    }
    if options.crossfade.frames > TRANSITION_MAXIMUM_FADE_FRAMES {
        return refuse(TransitionSetupFailure::FadeFrames, initial);
    }
    let mut scratch = Vec::new();
    if scratch.try_reserve_exact(maximum_frames).is_err() {
        return refuse(TransitionSetupFailure::Allocation, initial);
    }
    scratch.resize(maximum_frames, E::Sample::default());
    let mut gains = Vec::new();
    if gains.try_reserve_exact(2 * maximum_frames).is_err() {
        return refuse(TransitionSetupFailure::Allocation, initial);
    }
    gains.resize(2 * maximum_frames, 0.);
    let (pending_tx, pending_rx) = RingBuffer::new(1);
    let (returned_tx, returned_rx) = RingBuffer::new(TRANSITION_RETURN_CAPACITY);
    let (reports_tx, reports_rx) = RingBuffer::new(TRANSITION_REPORT_CAPACITY);
    let channels = initial.transition_channels();
    let latency = initial.transition_latency();
    Ok((
        ConvolutionTransitionControl {
            pending: pending_tx,
            returned: returned_rx,
            reports: reports_rx,
            channels,
            maximum_frames,
            latency,
            options,
            next_sequence: 1,
        },
        ConvolutionTransition {
            current: initial,
            current_sequence: 0,
            outgoing: None,
            fade_position: 0,
            waiting: None,
            deferred: None,
            pending: pending_rx,
            returned: returned_tx,
            reports: reports_tx,
            scratch,
            gains,
            refuse_head: false,
            channels,
            maximum_frames,
            options,
            frame: 0,
            lost_reports: 0,
        },
    ))
}

impl<E: TransitionEngine> ConvolutionTransition<E> {
    fn report(&mut self, sequence: u64, event: TransitionEvent, frame: u64) {
        let report = TransitionReport {
            sequence,
            event,
            frame,
            lost_before: self.lost_reports,
        };
        if self.reports.push(report).is_ok() {
            self.lost_reports = 0;
        } else {
            self.lost_reports = self.lost_reports.saturating_add(1);
        }
    }
    /// Hand a replaced engine back through the returned queue or the deferred
    /// slot. When neither has room the engine stays (or is restored) as a
    /// completed, unprocessed outgoing owner and is retried at a later boundary.
    fn retire(&mut self, sequence: u64, engine: Box<E>) {
        let item = ReturnedEngine {
            reason: ReturnReason::Retired,
            sequence,
            engine,
        };
        if let Err(PushError::Full(item)) = self.returned.push(item) {
            if self.deferred.is_none() {
                self.deferred = Some(item);
            } else {
                self.outgoing = Some((item.sequence, item.engine));
                self.fade_position = self.options.crossfade.frames;
            }
        }
    }
    /// Return an unused proposal with `reason` only if a queue slot is reserved
    /// first; otherwise leave it in the handoff. Returns whether it was returned.
    fn return_pending(&mut self, reason: ReturnReason) -> bool {
        let Ok(chunk) = self.returned.write_chunk_uninit(1) else {
            return false;
        };
        let Ok(p) = self.pending.pop() else {
            return false;
        };
        let sequence = p.sequence;
        chunk.fill_from_iter(std::iter::once(ReturnedEngine {
            reason,
            sequence,
            engine: p.engine,
        }));
        let event = match reason {
            ReturnReason::Superseded => TransitionEvent::Superseded,
            _ => TransitionEvent::Refused,
        };
        self.report(sequence, event, self.frame);
        true
    }
    /// Move the pending proposal into the waiting slot, returning any older
    /// waiting proposal as superseded through a reserved queue slot.
    fn supersede_into_waiting(&mut self) -> bool {
        if self.waiting.is_none() {
            if let Ok(p) = self.pending.pop() {
                self.waiting = Some(p);
            }
            return true;
        }
        let Ok(chunk) = self.returned.write_chunk_uninit(1) else {
            return false;
        };
        let Ok(p) = self.pending.pop() else {
            return false;
        };
        let Some(old) = self.waiting.replace(p) else {
            return true;
        };
        let sequence = old.sequence;
        chunk.fill_from_iter(std::iter::once(ReturnedEngine {
            reason: ReturnReason::Superseded,
            sequence,
            engine: old.engine,
        }));
        self.report(sequence, TransitionEvent::Superseded, self.frame);
        true
    }
    fn start(&mut self, proposal: Proposal<E>, status: &mut TransitionStatus) {
        let previous_latency = self.current.transition_latency();
        let latency = proposal.engine.transition_latency();
        let old = std::mem::replace(&mut self.current, proposal.engine);
        let old_sequence = std::mem::replace(&mut self.current_sequence, proposal.sequence);
        let fade_frames = self.options.crossfade.frames;
        self.report(
            proposal.sequence,
            TransitionEvent::Started {
                previous_latency,
                latency,
                fade_frames,
            },
            self.frame,
        );
        status.started = Some(proposal.sequence);
        self.fade_position = 0;
        if fade_frames == 0 {
            self.retire(old_sequence, old);
            self.report(proposal.sequence, TransitionEvent::Completed, self.frame);
            status.completed = Some(proposal.sequence);
        } else {
            self.outgoing = Some((old_sequence, old));
        }
    }
    fn boundary(&mut self, status: &mut TransitionStatus) {
        let length = self.options.crossfade.frames;
        if let Some(item) = self.deferred.take()
            && let Err(PushError::Full(item)) = self.returned.push(item)
        {
            self.deferred = Some(item);
        }
        // A completed fade (or cut) whose engine had nowhere to go: retry.
        if self.fade_position >= length
            && let Some((sequence, old)) = self.outgoing.take()
        {
            self.retire(sequence, old);
        }
        let has_pending = !self.pending.is_empty();
        // Under Refuse, a proposal that arrived during a transition stays refused
        // even if retirement backpressure delayed its return past the fade.
        if self.refuse_head {
            if !has_pending || self.return_pending(ReturnReason::Refused) {
                self.refuse_head = false;
            } else {
                status.retirement_blocked = true;
                return;
            }
        }
        let has_pending = !self.pending.is_empty();
        if self.outgoing.is_some() {
            match self.options.mid_fade {
                MidFadePolicy::QueueOne => {}
                MidFadePolicy::Refuse => {
                    if has_pending && !self.return_pending(ReturnReason::Refused) {
                        self.refuse_head = true;
                        status.retirement_blocked = true;
                    }
                }
                MidFadePolicy::LatestWins => {
                    if has_pending && !self.supersede_into_waiting() {
                        status.retirement_blocked = true;
                    }
                }
            }
            return;
        }
        if self.deferred.is_some() {
            status.retirement_blocked = self.waiting.is_some() || has_pending;
            return;
        }
        // Latest wins: a newer proposal offered after the last mid-fade boundary
        // supersedes the older waiting one before anything starts.
        if self.options.mid_fade == MidFadePolicy::LatestWins
            && self.waiting.is_some()
            && has_pending
            && !self.supersede_into_waiting()
        {
            status.retirement_blocked = true;
            return;
        }
        let next = match self.waiting.take() {
            Some(p) => Some(p),
            None => self.pending.pop().ok(),
        };
        if let Some(p) = next {
            self.start(p, status);
        }
    }
    /// Process the first `frames` frames of every channel. Channel count, the
    /// maximum and every prefix are admitted before any state changes; a refused
    /// call takes no proposal and leaves histories, fade position and output
    /// unchanged. An admitted call (including `frames == 0`) first handles at most
    /// one proposal at this block boundary, then renders, blending while a fade
    /// is in progress. Per-frame fade gains are computed once per block.
    pub fn process(
        &mut self,
        input: &[&[E::Sample]],
        output: &mut [&mut [E::Sample]],
        frames: usize,
    ) -> Result<TransitionStatus, ConvolutionProcessError> {
        if frames > self.maximum_frames {
            return Err(ConvolutionProcessError::TooManyFrames);
        }
        if input.len() != self.channels || output.len() != self.channels {
            return Err(ConvolutionProcessError::ChannelMismatch);
        }
        if input.iter().any(|x| x.len() < frames) || output.iter().any(|x| x.len() < frames) {
            return Err(ConvolutionProcessError::PrefixTooShort);
        }
        let mut status = TransitionStatus::default();
        self.boundary(&mut status);
        let length = self.options.crossfade.frames;
        let position = self.fade_position;
        let faded = if self.outgoing.is_some() {
            frames.min(length.saturating_sub(position))
        } else {
            0
        };
        let (old_gains, new_gains) = self.gains.split_at_mut(self.maximum_frames);
        for i in 0..faded {
            let (g_old, g_new) = self.options.crossfade.law.gains(position + i, length);
            old_gains[i] = g_old;
            new_gains[i] = g_new;
        }
        let current = &mut self.current;
        let scratch = &mut self.scratch;
        let mut outgoing = self.outgoing.as_mut().map(|(_, e)| e);
        for (channel, (x, y)) in input.iter().zip(output.iter_mut()).enumerate() {
            if current.transition_process(channel, x, y, frames).is_err() {
                // Unreachable after admission and offer checks; silence, never panic.
                y[..frames].fill(E::Sample::default());
            }
            if faded == 0 {
                continue;
            }
            let Some(old) = outgoing.as_mut() else {
                continue;
            };
            if old.transition_process(channel, x, scratch, faded).is_err() {
                scratch[..faded].fill(E::Sample::default());
            }
            for (i, (out, previous)) in y[..faded].iter_mut().zip(&scratch[..faded]).enumerate() {
                *out = E::Sample::from_gain(old_gains[i]) * *previous
                    + E::Sample::from_gain(new_gains[i]) * *out;
            }
        }
        self.fade_position += faded;
        if faded > 0
            && self.fade_position == length
            && let Some((old_sequence, old)) = self.outgoing.take()
        {
            self.retire(old_sequence, old);
            let at = self.frame.saturating_add(faded as u64);
            self.report(self.current_sequence, TransitionEvent::Completed, at);
            status.completed = Some(self.current_sequence);
        }
        status.fading = self.outgoing.is_some() && self.fade_position < length;
        self.frame = self.frame.saturating_add(frames as u64);
        Ok(status)
    }
    /// Clear the histories of the current and (if fading) outgoing engines. The
    /// fade position, waiting proposal and handoff are unchanged.
    pub fn reset(&mut self) {
        self.current.transition_reset();
        if let Some((_, old)) = &mut self.outgoing {
            old.transition_reset();
        }
    }
    /// Current engine latency in frames.
    pub fn latency_samples(&self) -> usize {
        self.current.transition_latency()
    }
    /// Outgoing engine latency while a fade is in progress.
    pub fn outgoing_latency_samples(&self) -> Option<usize> {
        self.outgoing.as_ref().map(|(_, e)| e.transition_latency())
    }
    /// Largest tail of the current and outgoing engines.
    pub fn tail_samples(&self) -> usize {
        let old = self
            .outgoing
            .as_ref()
            .map_or(0, |(_, e)| e.transition_tail());
        self.current.transition_tail().max(old)
    }
    /// Sequence of the current engine (0 for the initial engine).
    pub fn current_sequence(&self) -> u64 {
        self.current_sequence
    }
    /// Frames into the running fade, if any.
    pub fn fade_position(&self) -> Option<usize> {
        self.outgoing.as_ref().map(|_| self.fade_position)
    }
    /// Sequence waiting under [`MidFadePolicy::LatestWins`].
    pub fn waiting_sequence(&self) -> Option<u64> {
        self.waiting.as_ref().map(|p| p.sequence)
    }
    /// Absolute frames accepted so far (saturating).
    pub fn frames_processed(&self) -> u64 {
        self.frame
    }
    /// Borrow the current engine for observations.
    pub fn current(&self) -> &E {
        &self.current
    }
    /// Channel count.
    pub fn channels(&self) -> usize {
        self.channels
    }
    /// Maximum frames per call.
    pub fn maximum_frames(&self) -> usize {
        self.maximum_frames
    }
    /// Fixed configuration.
    pub fn options(&self) -> TransitionOptions {
        self.options
    }
}
