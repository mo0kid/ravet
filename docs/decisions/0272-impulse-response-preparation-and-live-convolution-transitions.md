# 0272: Source-rate impulse-response preparation and live convolution transitions

Status: accepted; implemented with offline numerical, allocation and concurrency tests only. No real-time deadline claim.

Follows [decision 0270](0270-native-convolution-schedules.md).

## Context

ADR 0270 left two follow-ons undecided. (a) Source-rate IR preparation: reading a response
from memory or a file, converting the sample rate, mapping channels, trimming and normalizing.
(b) Live transitions: preparing a new engine or bank on a worker, handing it to audio,
crossfading, and disposing of the old engine. `ravet-signal` has no runtime dependencies and
must keep it that way.

## Decision

### Placement (what owns which dependencies)

| Piece | Crate | Why |
|---|---|---|
| Memory preparation (`prepare_impulse_response`, request/plan/report types, `PreparedImpulseResponse::{bank32,bank64,engine32,engine64}`) | `ravet-signal` (always built) | Pure DSP; reuses `SincResampler::new_integer_rates`; no new dependency. |
| File preparation (`prepare_impulse_response_file`) | `ravet-audio-file`, existing default-disabled `resampled-playback` feature | That feature already owns the optional `ravet-signal` dependency and the `PlaybackReader` trait (WAV, AIFF and optional compressed readers). No manifest change. |
| Live transitions (`convolution_transition`, control/audio endpoints) | `ravet-graph`, new default-disabled `convolution-transitions` feature (`dep:ravet-signal`) | `ravet-graph` already owns `rtrb` and the `DynamicGraph` boxed-owner handoff/retirement pattern that this reuses. `ravet-signal` was already its dev-dependency, so `Cargo.lock` is unchanged. `ravet-device` also has both dependencies but would add `cpal` for consumers that have no device. `ravet-runtime` has no audio types. |

`ravet-signal` gains no dependency. The feature is off by default, so the default
`ravet-graph` build graph is unchanged.

### (a) Preparation

Planar f64 source at its own rate. `ImpulseResponseRequest` has no defaults: target rate,
`ResponseRateConversion`, `ResponseChannelMap`, `ResponseTrim`, `ResponseNormalization`,
`maximum_taps` (1..=262,144, the engine limit) and `maximum_working_bytes` (1..=256 MiB).
The fixed stage order is map, convert, trim, normalize.

- **Rate conversion.** `RequireEqualRates` makes an exact copy and refuses unequal rates.
  `Sinc { taps, gain, delay }` uses the existing integer-rate sinc profile (even taps
  16..=256, ratio 1/16..=16), also at equal rates. It feeds the source and at most `taps`
  zeros, giving `staged = floor((N+T-2)*out/in)+1` frames (the same formula as
  `prepare_file_playback`).
  - `ResponseRateGain::PreserveResponseGain` multiplies by `in/out`, so the frequency response
    and DC gain are preserved. `PreserveSampleAmplitude` applies gain 1.
  - `ResponseDelay::Retain` keeps the `(T-1)/2` source-sample delay.
    `RemoveWholeTargetFrames` drops `floor((T-1)*out/(2*in))` leading frames and reports the
    fractional remainder in [0,1).
- **Channel mapping.** `Identity`, or `Select(&[usize])` with 1..=64 entries. Duplicates and
  reorders are allowed. There is no mixing.
- **Trim.** Leading and trailing thresholds are independent and optional. Each is `Absolute(a>=0)`
  or `RelativeToPeak(0<=r<1)` of the linked post-conversion peak. Trimming is linked across
  channels (a frame is kept when any channel's |x| is greater than the threshold). Removed
  frame counts and resolved thresholds are reported. Nothing above the threshold gives
  `BelowThreshold`.
- **Normalization.** `None`, `Peak{target}` (gain = target/linked peak) or
  `Energy{target}` (gain = sqrt(target / largest per-channel sum of squares)). One linked gain
  is applied and reported.
  - The energy sum is formed after multiplying by an exact power of two `s` near `1/peak`
    (exponent clamped so `s` stays normal). The gain is `sqrt(target/sum)*s`, which is
    bitwise the direct formula whenever nothing over- or underflows.
  - Huge finite responses (1e200, `f64::MAX`) are therefore not zeroed, and subnormal-scale
    responses are not refused as silent.
  - An all-zero response gives `Silent`. An overflowing gain or nonfinite product gives
    `NonFinite`. A gain below `f64::MIN_POSITIVE` gives the new `UnrepresentableGain`.
  - An all-zero response is accepted only without normalization.
- **Bounds and refusal.** `ImpulseResponseRequest::plan` checks all policies and computes the
  exact frame counts and declared bytes (result + staging + 1026*T*8 sinc table) before any
  allocation. A test shows that every policy refusal performs zero allocations. Data-dependent
  refusals (`InvalidSource` for nonfinite or unequal input, `BelowThreshold`, `TooLong` after
  trim, `Silent`, `NonFinite`) return no partial result.
- **File path.** The reader must be at frame 0. Channels must be 1..=64 and the rate
  1..=768,000. The options bound the source frames and the read batch. Before any read, the
  plan for the declared (or caller-bounded) duration plus source and scratch bytes must fit
  the same budget. f32 is widened exactly. Reader errors, contract violations, nonfinite
  samples and declared-length mismatches are refused with `source_frames_read`. There is no
  rollback or seek.

### (b) Live transitions

- **Engines.** A sealed `TransitionEngine` covers `Convolver32/64` and `ConvolutionBank32/64`.
- **Construction.** `convolution_transition(initial, maximum_frames, options)` runs on
  CONTROL. It creates:
  - a 1-slot pending `rtrb` queue;
  - a 4-slot returned queue of `ReturnedEngine{reason, sequence, engine}`;
  - a 16-slot report queue;
  - one channel of scratch.
  It refuses `MaximumFrames`, `FadeFrames` and `Allocation`, returning the identical initial box.
- **Offers.** `ConvolutionTransitionControl` is `Send` and owned by a worker. Its `offer`
  checks channels, maximum frames and latency (under `RequireEqual`). It returns the identical
  box on refusal (`Channels`, `MaximumFrames`, `Latency`, `Full`, `AudioGone`) and a sequence on
  success. `take_returned` and `reclaim` drop engines on that thread; `take_report` reads reports.
- **Audio side.** `ConvolutionTransition::process(input, output, frames)`:
  1. Admits the whole call (maximum, channels, prefixes) before any change.
  2. At the block boundary, flushes a deferred retirement and applies the mid-fade policy, or
     starts at most one proposal.
  3. Renders.

  While fading, the new engine renders into the output and the old one into scratch for
  `min(frames, L-pos)` frames. Then `y = S(g_old)*y_old + S(g_new)*y_new` with
  `t=(k+1)/L`. `EqualGain` uses (1-t, t); `EqualPower` uses (cos, sin)(t*pi/2). Gains are f64,
  rounded once. `L=0` is an explicit cut. The new engine starts from its prepared (zero)
  history. When the fade completes, the old engine is pushed to the returned queue, or held
  in the single deferred slot if the queue is full. If both are full (not reachable under the
  start rule), it stays as a completed, silent, unprocessed outgoing owner and is retried at
  each boundary. A new start waits while the deferred slot is occupied (`retirement_blocked`).
  Refused and superseded engines are returned only into a queue slot reserved first
  (`rtrb::Producer::write_chunk_uninit(1)`); without a slot the proposal stays where it is.
  Engine process errors (unreachable after admission and offer checks) silence that channel.
  The audio path has no `debug_assert`, `expect`, `unwrap`, panic, leak or free. Gains are
  computed once per frame into scratch allocated at construction.
- **Mid-fade policy.**
  - `Refuse`: a proposal seen during a transition is returned unused (`Refused`). If no
    returned slot is free, it is marked refused in the handoff and returned as soon as a slot
    frees, even after the fade. It never starts.
  - `QueueOne`: the proposal stays in the 1-slot handoff; further offers get `Full`. It starts
    at the first boundary after the fade.
  - `LatestWins`: during a transition each new proposal moves to one waiting slot and any
    older waiting proposal is returned as `Superseded`. At the first post-fade boundary, a
    proposal still in the handoff supersedes the waiting one and the newest starts. Without a
    returned slot nothing starts (`retirement_blocked`), so a stale engine is never started in
    place of a newer one.

  Refused and superseded engines never process audio.
- **Latency.** `LatencyPolicy::RequireEqual` refuses offers with a different latency.
  `Report` accepts them and emits `TransitionEvent::Started{previous_latency, latency,
  fade_frames}` with the absolute frame. `Completed` carries the end frame. Accessors expose
  current and outgoing latency and tail. Lost reports are counted in `lost_before`. The host
  latency change is the consumer's job, and during a fade the output mixes both latencies.
- **Reset.** Clears current and outgoing histories. The fade position, waiting proposal and
  handoff are unchanged.
- **Status and reports.** Per-call `TransitionStatus` is authoritative; reports are
  best-effort. `offer` returning `Ok` means queued, not "will play".
- **Shutdown.** Both endpoints are constructed and dropped outside the callback.
  - Dropping the audio endpoint frees its current, outgoing, waiting and deferred engines on
    the dropping thread.
  - The rings, with any queued engines, are freed by the endpoint dropped last.
  - With control dropped first, audio keeps processing: retirements move into the unread
    returned queue and then the deferred slot or held outgoing, and nothing is freed.
  - A test measures zero audio-thread allocations and frees after the control drop, and
    frees on the dropping thread for both drop orders.

## Consequences and limits

- Offline evidence only, no real-time deadline claim. Fades cost two engines plus a cos/sin
  per frame per channel (EqualPower).
- There is no history transfer into the new engine. For a response longer than the fade,
  stationary input dips in level by up to `1 - (energy of first L taps / total energy)` at the
  end of the fade, recovering as the history fills.
- There is no automatic latency alignment between engines of different latency.
- The existing `SincResampler` table and history are infallible (bounded and budgeted)
  allocations. Builder scratch for banks and engines is reserved fallibly.
- The audio endpoint and its engines must be constructed and dropped off audio, as with
  `DynamicGraph`.
- The transition is not a Foundation `Processor`. Graph or host consumers wrap it and apply
  reported latency changes themselves.
- Not included: true-stereo matrices, mixing channel maps, history migration, three-way fades,
  automatic file format detection, and resampling beyond the existing sinc profile.

## Tests

- `ravet-signal/tests/impulse_response.rs` (10; R3 added energy overflow (1e200,
  `f64::MAX`), subnormal/f32-subnormal underflow and typed gain refusals):
  - analytic Gaussian-burst time and continuous-spectrum references for 6 rate pairs at 64 and
    256 taps;
  - length and delay formulas, with bitwise rate-gain and delay relations;
  - exact channel mapping;
  - exact linked trim offsets;
  - bitwise normalization gains;
  - bank and engine construction;
  - every refusal with zero allocations.
- `ravet-audio-file/tests/impulse_file.rs` (4): WAV result bitwise equal to the memory path;
  AIFF exact copy; refusals before reads with an untouched reader; read-time refusals with
  committed frames.
- `ravet-graph/tests/convolution_transition.rs` (15; R3 added latest-wins after the fade,
  refuse under backpressure, zero-frame calls and shutdown with engines in flight):
  - blend bit-exact against an independently computed reference for f32 and f64, both laws,
    three partition pairs and five segmentations (64, 1, 7, two random);
  - law endpoints;
  - cut;
  - latency refuse and report;
  - all three mid-fade policies against references;
  - refused and superseded engines unprocessed;
  - refused calls and offers leave state unchanged;
  - reset;
  - zero audio-side allocations or frees through handoff, fade, retirement, deferral and reset,
    with frees seen in control reclaim;
  - concurrent worker/audio stress over 3 seeds x 3 policies.
- Module doctests in `impulse_response.rs` and `convolution_transition.rs`.
