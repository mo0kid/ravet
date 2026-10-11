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
//! Limits under the default [`convolution_transition`] constructor: the new
//! engine starts from empty history, so for a response longer than the fade its
//! output still lacks the tail contributed by input from before the transition;
//! for stationary input the blend dips in level, by up to
//! `1 - (energy of the first fade-length taps / total energy)` at the end of the
//! fade, recovering as the new history fills. Engines with different latencies
//! are not aligned: under [`LatencyPolicy::Report`] the fade mixes both
//! latencies and the consumer applies any host latency change.
//!
//! [`convolution_transition_with_continuity`] adds two explicit, optional
//! policies in a [`TransitionContinuity`]: [`PrimingPolicy::History`] feeds the
//! incoming engine the most recent input from a prepared history ring before its
//! fade starts (immediately, or over several blocks with a delayed fade), and
//! [`AlignmentPolicy::DelayLower`] delays the lower-latency path through a
//! prepared delay line during the fade, reports each output-latency change and
//! refuses transitions whose latency difference exceeds a configured maximum.
//! With [`TransitionContinuity::NONE`] the behaviour is exactly the default.
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
/// Largest accepted [`PrimingPolicy::History`] length in frames.
pub const TRANSITION_MAXIMUM_PRIMING_FRAMES: usize = 1 << 20;
/// Largest accepted [`AlignmentPolicy::DelayLower`] maximum difference in frames.
pub const TRANSITION_MAXIMUM_ALIGNMENT_FRAMES: usize = 1 << 20;
/// Largest accepted continuity storage (history ring plus both delay lines) in
/// bytes of samples, checked before any allocation.
pub const TRANSITION_MAXIMUM_CONTINUITY_BYTES: usize = 256 << 20;

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

/// Optional history priming of the incoming engine (see
/// [`convolution_transition_with_continuity`]).
///
/// Under [`PrimingPolicy::History`] the audio endpoint records the most recent
/// `frames` input frames of every channel in a ring prepared at construction.
/// When a proposal is taken, the incoming engine first processes the recorded
/// history (at most `frames`, fewer if less input has been seen since
/// construction or the last [`ConvolutionTransition::reset`]), output discarded,
/// so that when its fade starts its state equals that of an engine that had been
/// running on the same input. With `frames` at least the incoming engine's
/// `tail_samples` (`taps - 1 + latency`), its output from the fade start matches a
/// never-switched engine with the same response to rounding, and the level dip of
/// the default policy disappears.
///
/// Work is bounded per block: at most `frames_per_block` history frames per
/// channel are processed at one block boundary, in addition to normal rendering.
/// If the recorded history fits (`history <= frames_per_block`), priming completes
/// and the fade starts at the same boundary. Otherwise the fade is **delayed**: the
/// old engine keeps playing alone while the incoming engine catches up through the
/// ring (consuming `frames_per_block` frames per boundary while each block adds its
/// own frames), and the fade starts at the first boundary at which the remaining
/// lag is at most `frames_per_block`. For blocks of at most `b` frames, a lag `g`
/// shrinks by at least `frames_per_block - b` per block, so the fade starts after
/// at most `ceil((g - frames_per_block) / (frames_per_block - b))` delayed blocks.
/// A proposal being primed counts as an in-progress transition for
/// [`MidFadePolicy`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimingPolicy {
    /// No priming: the incoming engine starts from its prepared (empty) history.
    None,
    /// Prime from a prepared history ring.
    History {
        /// History ring length per channel, 1..=[`TRANSITION_MAXIMUM_PRIMING_FRAMES`].
        frames: usize,
        /// Largest number of history frames per channel processed at one block
        /// boundary, at least 1. Must be at least `frames` (single-block priming) or
        /// greater than the transition's `maximum_frames` (so a delayed fade always
        /// catches up).
        frames_per_block: usize,
    },
}

/// Optional latency alignment between the outgoing and incoming engines (see
/// [`convolution_transition_with_continuity`]).
///
/// Under [`AlignmentPolicy::DelayLower`] the audio endpoint keeps two delay lines
/// of `maximum_difference + maximum_frames` frames per channel, prepared at
/// construction; each records its engine's raw output. When a transition from
/// latency `a` to latency `b` starts, the lower-latency path is delayed by
/// `|a - b|` frames for the whole fade, so both paths are aligned at `max(a, b)`:
///
/// - `b > a`: the old path is delayed from the fade start; the output latency
///   changes from `a` to `b` at the start frame (the old path repeats its last
///   `b - a` output frames while its gain is near one).
/// - `b < a`: the new path is delayed during the fade; the output latency stays
///   `a` and changes to `b` at the frame where the fade completes (the delay is
///   released while only the new path is audible, skipping `a - b` of its output
///   frames).
/// - a cut (`frames == 0`) changes the output latency at the start frame, with
///   no delay (an unaligned jump).
///
/// Without [`PrimingPolicy::History`] the incoming engine has no output from
/// before the fade start, so when `b < a` its delayed path reads zeros for the
/// first `a - b` fade frames: it stays silent while its gain rises, and if
/// `a - b` is at least the fade length it contributes nothing before the
/// release, deepening the level dip. Priming fills those frames.
///
/// Each change is reported through [`TransitionStatus::output_latency_change`]
/// and [`TransitionEvent::OutputLatencyChanged`], and
/// [`ConvolutionTransition::output_latency_samples`] gives the current value.
/// Outside fades no delay is applied. A proposal whose latency differs from the
/// current engine's by more than `maximum_difference` is refused at the block
/// boundary where it would start (only the audio endpoint knows the current
/// engine): it is returned unused as [`ReturnReason::Refused`] with
/// [`TransitionEvent::AlignmentRefused`], through a reserved returned slot; if
/// none is free it stays where it is and the boundary reports
/// `retirement_blocked`. Under [`LatencyPolicy::RequireEqual`] no difference can
/// arise and the policy has no effect.
///
/// Cost: every block copies each engine's output through its line, proportional
/// to the block. The boundary that takes a proposal forgets the incoming line and
/// [`ConvolutionTransition::reset`] forgets both in constant time: each line
/// tracks how many frames were written since it was forgotten and reads older
/// positions as zero, so no clear proportional to the prepared size
/// (`channels * (maximum_difference + maximum_frames)` samples) is performed.
/// No allocation is performed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignmentPolicy {
    /// No alignment (the default behaviour): the fade mixes both latencies.
    None,
    /// Delay the lower-latency path by the latency difference during the fade.
    DelayLower {
        /// Largest accepted latency difference and delay in frames,
        /// 0..=[`TRANSITION_MAXIMUM_ALIGNMENT_FRAMES`].
        maximum_difference: usize,
    },
}

/// Optional continuity policies for [`convolution_transition_with_continuity`].
/// [`TransitionContinuity::NONE`] (also the [`Default`]) gives exactly the
/// behaviour of [`convolution_transition`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransitionContinuity {
    /// History priming of the incoming engine.
    pub priming: PrimingPolicy,
    /// Latency alignment during the fade.
    pub alignment: AlignmentPolicy,
}
impl TransitionContinuity {
    /// No priming and no alignment.
    pub const NONE: Self = Self {
        priming: PrimingPolicy::None,
        alignment: AlignmentPolicy::None,
    };
}
impl Default for TransitionContinuity {
    fn default() -> Self {
        Self::NONE
    }
}

/// Why an engine was returned to the control owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReturnReason {
    /// Replaced; its fade (or cut) completed.
    Retired,
    /// Refused mid-fade under [`MidFadePolicy::Refuse`], or refused at its start
    /// boundary by [`AlignmentPolicy::DelayLower`] (reported as
    /// [`TransitionEvent::AlignmentRefused`]); never processed audio.
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
    /// Priming of this proposal began under [`PrimingPolicy::History`]
    /// (`Started` follows when its fade starts, possibly at the same boundary).
    PrimingStarted {
        /// Recorded history frames the incoming engine will process.
        history_frames: usize,
    },
    /// The proposal was refused at its start boundary because its latency differs
    /// from the current engine's by more than the
    /// [`AlignmentPolicy::DelayLower`] maximum.
    AlignmentRefused {
        /// Latency of the current engine.
        previous_latency: usize,
        /// Latency of the refused engine.
        latency: usize,
    },
    /// The aligned output latency changed at this frame (only under
    /// [`AlignmentPolicy::DelayLower`]).
    OutputLatencyChanged {
        /// Output latency before the change.
        previous_latency: usize,
        /// Output latency from this frame.
        latency: usize,
    },
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
    /// An incoming engine is still being primed after this block (delayed fade
    /// under [`PrimingPolicy::History`]).
    pub priming: bool,
    /// History frames per channel fed to the incoming engine at this boundary.
    pub primed_frames: usize,
    /// Sequence refused at this boundary by [`AlignmentPolicy::DelayLower`].
    pub alignment_refused: Option<u64>,
    /// `(previous, new)` output latency when it changed within this block (only
    /// under [`AlignmentPolicy::DelayLower`]).
    pub output_latency_change: Option<(usize, usize)>,
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
    /// Invalid [`PrimingPolicy::History`] lengths.
    Priming,
    /// [`AlignmentPolicy::DelayLower`] maximum exceeds
    /// [`TRANSITION_MAXIMUM_ALIGNMENT_FRAMES`].
    Alignment,
    /// Continuity storage would exceed [`TRANSITION_MAXIMUM_CONTINUITY_BYTES`], or
    /// a continuity size computation would overflow.
    Storage,
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
    /// The engine should have fresh history; it is not reset on handoff. Under
    /// [`AlignmentPolicy::DelayLower`] the latency difference is checked by the
    /// audio endpoint at the start boundary instead, against the engine current
    /// at that time.
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
/// dropping it frees its current, outgoing, priming, waiting and deferred engines
/// on the dropping thread. The three queues, including any engines still queued in the
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
    continuity: TransitionContinuity,
    /// Planar input history ring, `channels * history_frames` (empty when unused).
    history: Vec<E::Sample>,
    history_frames: usize,
    history_write: usize,
    history_recorded: usize,
    /// Proposal being primed and its remaining lag behind the live input.
    incoming: Option<(Proposal<E>, usize)>,
    /// Two planar delay lines, `channels * line_capacity` each (empty when unused).
    lines: [Vec<E::Sample>; 2],
    line_capacity: usize,
    line_write: [usize; 2],
    /// Frames written to each line since it was last forgotten (saturating at
    /// `line_capacity`); older positions read as zero, so forgetting a line is
    /// constant time instead of a clear proportional to its prepared size.
    line_valid: [usize; 2],
    current_line: usize,
    current_delay: usize,
    outgoing_delay: usize,
    output_latency: usize,
}

/// Write `block` into one channel of a delay line at `write`, then replace each
/// frame with the line value `delay` frames earlier (`first` for frames before
/// `split`, `rest` after), or zero when that frame precedes the `valid` frames
/// written since the line was last forgotten. Requires
/// `delay + block.len() <= line.len()`.
fn delay_through<S: Copy + Default>(
    line: &mut [S],
    write: usize,
    valid: usize,
    block: &mut [S],
    split: usize,
    first: usize,
    rest: usize,
) {
    let capacity = line.len();
    if capacity == 0 {
        return;
    }
    for (i, value) in block.iter().enumerate() {
        line[(write + i) % capacity] = *value;
    }
    for (i, value) in block.iter_mut().enumerate() {
        let delay = if i < split { first } else { rest };
        if delay != 0 {
            *value = if valid + i < delay {
                S::default()
            } else {
                line[(write + i + capacity - delay) % capacity]
            };
        }
    }
}

/// Fallibly reserve and zero-fill `len` samples.
fn zeroed<S: Copy + Default>(len: usize) -> Option<Vec<S>> {
    let mut v = Vec::new();
    v.try_reserve_exact(len).ok()?;
    v.resize(len, S::default());
    Some(v)
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
    convolution_transition_with_continuity(
        initial,
        maximum_frames,
        options,
        TransitionContinuity::NONE,
    )
}

/// [`convolution_transition`] with explicit [`TransitionContinuity`] policies
/// (CONTROL). Additionally prepares, before returning, a history ring of
/// `channels * frames` samples for [`PrimingPolicy::History`] and two delay lines
/// of `channels * (maximum_difference + maximum_frames)` samples for
/// [`AlignmentPolicy::DelayLower`]. All lengths and the total
/// ([`TRANSITION_MAXIMUM_CONTINUITY_BYTES`]) are checked before any allocation,
/// storage is reserved fallibly, and a refusal returns the initial engine
/// unchanged. [`TransitionContinuity::NONE`] allocates nothing extra and behaves
/// exactly like [`convolution_transition`].
///
/// ```
/// use ravet_graph::{
///     AlignmentPolicy, CrossfadeLaw, CrossfadePolicy, LatencyPolicy, MidFadePolicy,
///     PrimingPolicy, TransitionContinuity, TransitionOptions,
///     convolution_transition_with_continuity,
/// };
/// use ravet_signal::{ConvolutionPartitioning, Convolver64};
/// let zero = ConvolutionPartitioning::ZeroLatency { head: 16 };
/// let delayed = ConvolutionPartitioning::UniformDelayed { partition: 16 };
/// let first = Box::new(Convolver64::new(&[1.0, 1.0], zero, 64, 1 << 20).unwrap());
/// let options = TransitionOptions {
///     crossfade: CrossfadePolicy { frames: 8, law: CrossfadeLaw::EqualGain },
///     mid_fade: MidFadePolicy::Refuse,
///     latency: LatencyPolicy::Report,
/// };
/// let continuity = TransitionContinuity {
///     priming: PrimingPolicy::History { frames: 64, frames_per_block: 64 },
///     alignment: AlignmentPolicy::DelayLower { maximum_difference: 32 },
/// };
/// let (mut control, mut audio) =
///     convolution_transition_with_continuity(first, 64, options, continuity).unwrap();
/// let input = [1.0; 32];
/// let mut output = [0.0; 32];
/// audio.process(&[&input], &mut [&mut output], 32).unwrap();
/// // Worker: a 16-frame-latency replacement with the same response.
/// let next = Box::new(Convolver64::new(&[1.0, 1.0], delayed, 64, 1 << 20).unwrap());
/// control.offer(next).unwrap();
/// let status = audio.process(&[&input], &mut [&mut output], 32).unwrap();
/// // Primed from the recorded input, the old path delayed by 16 frames: the
/// // fade blends identical steady-state signals (to FFT rounding).
/// assert_eq!(status.primed_frames, 32);
/// assert_eq!(status.output_latency_change, Some((0, 16)));
/// assert_eq!(audio.output_latency_samples(), Some(16));
/// assert!(output.iter().all(|y| (y - 2.0).abs() < 1e-12));
/// ```
#[allow(clippy::type_complexity)]
pub fn convolution_transition_with_continuity<E: TransitionEngine>(
    initial: Box<E>,
    maximum_frames: usize,
    options: TransitionOptions,
    continuity: TransitionContinuity,
) -> Result<(ConvolutionTransitionControl<E>, ConvolutionTransition<E>), TransitionSetupError<E>> {
    let refuse = |reason, engine| Err(TransitionSetupError { reason, engine });
    if maximum_frames == 0 || maximum_frames > initial.transition_maximum_frames() {
        return refuse(TransitionSetupFailure::MaximumFrames, initial);
    }
    if options.crossfade.frames > TRANSITION_MAXIMUM_FADE_FRAMES {
        return refuse(TransitionSetupFailure::FadeFrames, initial);
    }
    let history_frames = match continuity.priming {
        PrimingPolicy::None => 0,
        PrimingPolicy::History {
            frames,
            frames_per_block,
        } => {
            if frames == 0
                || frames > TRANSITION_MAXIMUM_PRIMING_FRAMES
                || frames_per_block == 0
                || (frames_per_block < frames && frames_per_block <= maximum_frames)
            {
                return refuse(TransitionSetupFailure::Priming, initial);
            }
            frames
        }
    };
    let line_capacity = match continuity.alignment {
        AlignmentPolicy::None => 0,
        AlignmentPolicy::DelayLower { maximum_difference } => {
            if maximum_difference > TRANSITION_MAXIMUM_ALIGNMENT_FRAMES {
                return refuse(TransitionSetupFailure::Alignment, initial);
            }
            match maximum_difference.checked_add(maximum_frames) {
                Some(capacity) => capacity,
                None => return refuse(TransitionSetupFailure::Storage, initial),
            }
        }
    };
    let channels = initial.transition_channels();
    let history_len = channels.checked_mul(history_frames);
    let line_len = channels.checked_mul(line_capacity);
    let total = history_len
        .zip(line_len)
        .and_then(|(h, l)| l.checked_mul(2).and_then(|l| l.checked_add(h)))
        .and_then(|n| n.checked_mul(std::mem::size_of::<E::Sample>()));
    let (history_len, line_len) = match (total, history_len, line_len) {
        (Some(bytes), Some(h), Some(l)) if bytes <= TRANSITION_MAXIMUM_CONTINUITY_BYTES => (h, l),
        _ => return refuse(TransitionSetupFailure::Storage, initial),
    };
    let (Some(history), Some(line_a), Some(line_b)) =
        (zeroed(history_len), zeroed(line_len), zeroed(line_len))
    else {
        return refuse(TransitionSetupFailure::Allocation, initial);
    };
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
            continuity,
            history,
            history_frames,
            history_write: 0,
            history_recorded: 0,
            incoming: None,
            lines: [line_a, line_b],
            line_capacity,
            line_write: [0, 0],
            line_valid: [0, 0],
            current_line: 0,
            current_delay: 0,
            outgoing_delay: 0,
            output_latency: latency,
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
        if self.line_capacity > 0 {
            // The incoming line was forgotten (and possibly primed) by `take_next`.
            self.current_line = 1 - self.current_line;
            let (old_delay, new_delay) = if fade_frames == 0 {
                (0, 0)
            } else if latency >= previous_latency {
                (latency - previous_latency, 0)
            } else {
                (0, previous_latency - latency)
            };
            self.outgoing_delay = old_delay;
            self.current_delay = new_delay;
            if new_delay == 0 && latency != previous_latency {
                self.change_output_latency(latency, self.frame, status);
            }
        }
        if fade_frames == 0 {
            self.retire(old_sequence, old);
            self.report(proposal.sequence, TransitionEvent::Completed, self.frame);
            status.completed = Some(proposal.sequence);
        } else {
            self.outgoing = Some((old_sequence, old));
        }
    }
    fn change_output_latency(&mut self, latency: usize, frame: u64, status: &mut TransitionStatus) {
        let previous_latency = self.output_latency;
        self.output_latency = latency;
        status.output_latency_change = Some((previous_latency, latency));
        self.report(
            self.current_sequence,
            TransitionEvent::OutputLatencyChanged {
                previous_latency,
                latency,
            },
            frame,
        );
    }
    /// Take the next proposal (waiting first) and begin it: refuse it under the
    /// alignment maximum, otherwise prime it or start it directly.
    fn take_next(&mut self, status: &mut TransitionStatus) {
        let candidate = match &self.waiting {
            Some(p) => Some(p.engine.transition_latency()),
            None => self
                .pending
                .peek()
                .ok()
                .map(|p| p.engine.transition_latency()),
        };
        let Some(latency) = candidate else {
            return;
        };
        let previous_latency = self.current.transition_latency();
        if let AlignmentPolicy::DelayLower { maximum_difference } = self.continuity.alignment
            && latency.abs_diff(previous_latency) > maximum_difference
        {
            let Ok(chunk) = self.returned.write_chunk_uninit(1) else {
                status.retirement_blocked = true;
                return;
            };
            let proposal = match self.waiting.take() {
                Some(p) => Some(p),
                None => self.pending.pop().ok(),
            };
            if let Some(p) = proposal {
                let sequence = p.sequence;
                chunk.fill_from_iter(std::iter::once(ReturnedEngine {
                    reason: ReturnReason::Refused,
                    sequence,
                    engine: p.engine,
                }));
                status.alignment_refused = Some(sequence);
                self.report(
                    sequence,
                    TransitionEvent::AlignmentRefused {
                        previous_latency,
                        latency,
                    },
                    self.frame,
                );
            }
            return;
        }
        let next = match self.waiting.take() {
            Some(p) => Some(p),
            None => self.pending.pop().ok(),
        };
        let Some(p) = next else {
            return;
        };
        if self.line_capacity > 0 {
            let incoming = 1 - self.current_line;
            // Forget the incoming line (constant time; see `line_valid`).
            self.line_write[incoming] = 0;
            self.line_valid[incoming] = 0;
        }
        if self.history_frames == 0 {
            self.start(p, status);
            return;
        }
        let lag = self.history_recorded;
        self.report(
            p.sequence,
            TransitionEvent::PrimingStarted {
                history_frames: lag,
            },
            self.frame,
        );
        self.incoming = Some((p, lag));
        self.prime_step(status);
    }
    /// Feed at most `frames_per_block` recorded history frames per channel to the
    /// incoming engine (output discarded, or recorded in its delay line), then
    /// start its fade once it has caught up with the live input.
    fn prime_step(&mut self, status: &mut TransitionStatus) {
        let PrimingPolicy::History {
            frames_per_block, ..
        } = self.continuity.priming
        else {
            return;
        };
        let Some((proposal, lag)) = self.incoming.as_mut() else {
            return;
        };
        let ring = self.history_frames;
        let chunk = frames_per_block.min(*lag);
        let engine = &mut proposal.engine;
        let incoming_line = 1 - self.current_line;
        let capacity = self.line_capacity;
        let mut read = (self.history_write + ring - *lag) % ring;
        let mut remaining = chunk;
        while remaining > 0 {
            let n = remaining.min(ring - read).min(self.maximum_frames);
            let write = self.line_write[incoming_line];
            for channel in 0..self.channels {
                let source = &self.history[channel * ring + read..channel * ring + read + n];
                let scratch = &mut self.scratch[..n];
                if engine
                    .transition_process(channel, source, scratch, n)
                    .is_err()
                {
                    scratch.fill(E::Sample::default());
                }
                if capacity > 0 {
                    let line = &mut self.lines[incoming_line]
                        [channel * capacity..(channel + 1) * capacity];
                    for (i, value) in scratch.iter().enumerate() {
                        line[(write + i) % capacity] = *value;
                    }
                }
            }
            if capacity > 0 {
                self.line_write[incoming_line] = (write + n) % capacity;
                self.line_valid[incoming_line] = (self.line_valid[incoming_line] + n).min(capacity);
            }
            read = (read + n) % ring;
            remaining -= n;
        }
        *lag -= chunk;
        status.primed_frames = chunk;
        if *lag == 0
            && let Some((p, _)) = self.incoming.take()
        {
            self.start(p, status);
        }
    }
    /// Append this block's input to the history ring (last `history_frames` only).
    fn record_history(&mut self, input: &[&[E::Sample]], frames: usize) {
        let ring = self.history_frames;
        if ring == 0 {
            return;
        }
        let take = frames.min(ring);
        let skip = frames - take;
        for (channel, x) in input.iter().enumerate() {
            let history = &mut self.history[channel * ring..(channel + 1) * ring];
            for (i, value) in x[skip..frames].iter().enumerate() {
                history[(self.history_write + i) % ring] = *value;
            }
        }
        self.history_write = (self.history_write + take) % ring;
        self.history_recorded = (self.history_recorded + frames).min(ring);
        if let Some((_, lag)) = self.incoming.as_mut() {
            *lag = (*lag + frames).min(self.history_recorded);
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
        // Priming always progresses, so its lag stays within the history ring. A
        // completion that cuts (no fade) ends this boundary: at most one start.
        if self.incoming.is_some() {
            self.prime_step(status);
            if status.started.is_some() && self.outgoing.is_none() {
                return;
            }
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
        if self.outgoing.is_some() || self.incoming.is_some() {
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
        self.take_next(status);
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
        // Alignment: the current path is delayed by `current_delay` while fading,
        // released at the frame where the fade completes; the old path by
        // `outgoing_delay`. Both lines always record their engine's raw output.
        let capacity = self.line_capacity;
        let completes = faded > 0 && position + faded == length;
        let current_delay = self.current_delay;
        let current_rest = if completes { 0 } else { current_delay };
        let outgoing_delay = self.outgoing_delay;
        let current_write = self.line_write[self.current_line];
        let outgoing_write = self.line_write[1 - self.current_line];
        let current_valid = self.line_valid[self.current_line];
        let outgoing_valid = self.line_valid[1 - self.current_line];
        let [line_a, line_b] = &mut self.lines;
        let (current_line, outgoing_line) = if self.current_line == 0 {
            (line_a, line_b)
        } else {
            (line_b, line_a)
        };
        let current = &mut self.current;
        let scratch = &mut self.scratch;
        let mut outgoing = self.outgoing.as_mut().map(|(_, e)| e);
        for (channel, (x, y)) in input.iter().zip(output.iter_mut()).enumerate() {
            if current.transition_process(channel, x, y, frames).is_err() {
                // Unreachable after admission and offer checks; silence, never panic.
                y[..frames].fill(E::Sample::default());
            }
            if capacity > 0 {
                delay_through(
                    &mut current_line[channel * capacity..(channel + 1) * capacity],
                    current_write,
                    current_valid,
                    &mut y[..frames],
                    faded,
                    current_delay,
                    current_rest,
                );
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
            if capacity > 0 {
                delay_through(
                    &mut outgoing_line[channel * capacity..(channel + 1) * capacity],
                    outgoing_write,
                    outgoing_valid,
                    &mut scratch[..faded],
                    faded,
                    outgoing_delay,
                    outgoing_delay,
                );
            }
            for (i, (out, previous)) in y[..faded].iter_mut().zip(&scratch[..faded]).enumerate() {
                *out = E::Sample::from_gain(old_gains[i]) * *previous
                    + E::Sample::from_gain(new_gains[i]) * *out;
            }
        }
        if capacity > 0 {
            self.line_write[self.current_line] = (current_write + frames) % capacity;
            self.line_valid[self.current_line] = (current_valid + frames).min(capacity);
            if faded > 0 {
                self.line_write[1 - self.current_line] = (outgoing_write + faded) % capacity;
                self.line_valid[1 - self.current_line] = (outgoing_valid + faded).min(capacity);
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
            if capacity > 0 {
                self.outgoing_delay = 0;
                if self.current_delay > 0 {
                    self.current_delay = 0;
                    let latency = self.current.transition_latency();
                    self.change_output_latency(latency, at, &mut status);
                }
            }
        }
        self.record_history(input, frames);
        status.fading = self.outgoing.is_some() && self.fade_position < length;
        status.priming = self.incoming.is_some();
        self.frame = self.frame.saturating_add(frames as u64);
        Ok(status)
    }
    /// Clear the histories of the current and (if fading) outgoing engines. The
    /// fade position, waiting proposal and handoff are unchanged.
    ///
    /// With continuity policies, reset also forgets the recorded input history,
    /// forgets both delay lines in constant time (alignment delays and output
    /// latency unchanged; during a fade the delayed path, old or new, reads zeros
    /// for its delay after the reset) and resets a proposal being primed with zero remaining lag, so its
    /// fade starts at the next block boundary from empty history.
    pub fn reset(&mut self) {
        self.current.transition_reset();
        if let Some((_, old)) = &mut self.outgoing {
            old.transition_reset();
        }
        if let Some((proposal, lag)) = &mut self.incoming {
            proposal.engine.transition_reset();
            *lag = 0;
        }
        self.history_recorded = 0;
        // Forget both delay lines (constant time; see `line_valid`).
        self.line_valid = [0, 0];
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
    /// Fixed continuity policies.
    pub fn continuity(&self) -> TransitionContinuity {
        self.continuity
    }
    /// Aligned output latency in frames under [`AlignmentPolicy::DelayLower`]
    /// (the delay from input to output of the audible signal after the last
    /// processed frame), or `None` without alignment.
    pub fn output_latency_samples(&self) -> Option<usize> {
        (self.line_capacity > 0).then_some(self.output_latency)
    }
    /// Sequence being primed under [`PrimingPolicy::History`] (delayed fade).
    pub fn priming_sequence(&self) -> Option<u64> {
        self.incoming.as_ref().map(|(p, _)| p.sequence)
    }
    /// Remaining history frames the priming engine lags behind the live input.
    pub fn priming_lag(&self) -> Option<usize> {
        self.incoming.as_ref().map(|(_, lag)| *lag)
    }
}
