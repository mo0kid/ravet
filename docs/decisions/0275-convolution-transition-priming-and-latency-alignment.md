# 0275: History priming and latency alignment for live convolution transitions

Status: Implemented; offline exact-reference, allocation and concurrency tests only. No real-time deadline claim.

Follows [decision 0272](0272-impulse-response-preparation-and-live-convolution-transitions.md).

## Context

ADR 0272 recorded two limits of live convolution transitions:

1. A newly handed-off engine starts with empty history. For a response longer than the
   fade, stationary input dips in level by up to the missing tail energy.
2. Engines with different latencies are not aligned. Under `LatencyPolicy::Report` the
   fade mixes misaligned signals.

The existing behaviour, public option struct and tests must stay unchanged unless a new
policy is selected. `ravet-signal` must gain no runtime dependency.

## Decision

### Placement and opt-in

The policies live in `ravet-graph`'s existing default-disabled `convolution-transitions`
feature. No crate gains a dependency and `Cargo.lock` is unchanged.

`TransitionOptions` is a public struct built with literals, so it is not extended. A new
constructor `convolution_transition_with_continuity(initial, maximum_frames, options,
TransitionContinuity { priming, alignment })` takes the new policies.
`convolution_transition` delegates to it with `TransitionContinuity::NONE`, which
allocates nothing extra and takes the same code paths as before.

Additions are made only where existing code cannot be broken:

- new `TransitionStatus` fields with default values;
- new `TransitionEvent` and `TransitionSetupFailure` variants;
- accessors.

`ReturnReason` gains no variant, because consumers match on it exhaustively. An
alignment refusal returns `ReturnReason::Refused` and is distinguished by its event and
status field. Construction checks every length, and the total history plus delay-line
storage (at most 256 MiB of samples), before allocating; every size computation,
including the delay-line length `D + maximum_frames`, is overflow-checked and refused
as `Storage`. Storage is reserved fallibly.

### (1) History priming

`PrimingPolicy::History { frames: N, frames_per_block: B }` keeps a planar history
ring of `N` frames per channel (1..=1,048,576) prepared at construction. Every
admitted block appends its input.

When a proposal is taken, the incoming engine processes the recorded history in order:
`min(N, frames seen since construction or reset)` frames, output discarded. Its state
then equals that of an engine that had processed the same input. With
`N >= tail_samples` (`taps - 1 + latency`), its output from the fade start matches a
never-switched engine with the new response.

Per-block work is bounded. At most `B` history frames per channel are processed at one
block boundary, in chunks of at most `maximum_frames`. Construction requires
`B >= N` or `B > maximum_frames`. Two schedules follow:

- **Single-block priming** (history <= `B`). The fade starts at the boundary where the
  proposal is taken. The cost of that block is one extra engine pass over up to `B`
  frames.
- **Delayed fade** (history > `B`). The old engine keeps playing alone. Each boundary
  consumes `B` ring frames while the block appends its own frames, so the lag shrinks
  by at least `B - b` for blocks of at most `b` frames. The fade starts at the first
  boundary whose remaining lag fits in `B`, after at most
  `ceil((g - B) / (B - b))` delayed blocks for an initial lag `g`. The requirement
  `B > maximum_frames` guarantees progress. Because the lag never grows, the ring
  never has to hold more than `N` frames.

The trade-off is explicit. A larger `B` costs more in one block but starts the fade
sooner. A smaller `B` spreads the cost over more blocks but delays the fade, and the
old response keeps playing meanwhile. An amortized schedule that starts the fade
before the engine catches up was rejected: the incoming engine would then lack history
during the fade, which is the defect being removed.

Priming progresses at every boundary, including zero-frame calls and boundaries
blocked by backpressure. A proposal being primed counts as an in-progress transition
for `MidFadePolicy`. When priming completes with a cut, that boundary takes no further
proposal, so at most one start happens per boundary.

`reset` forgets the history and resets the priming engine with zero lag. Its fade then
starts at the next boundary from empty history, consistent with the reset current
engine. The audio endpoint frees a priming engine when it is dropped, as it does its
other engines.

### (2) Latency alignment

`AlignmentPolicy::DelayLower { maximum_difference: D }` (0..=1,048,576) prepares two
planar delay lines of `D + maximum_frames` frames per channel. The current engine owns
one and the incoming engine the other; ownership swaps at each start. Each line
records its engine's raw output continuously, including priming output. Reads of
`delay + block <= capacity` therefore never meet overwritten data.

For a transition from latency `a` to `b` with fade length `L > 0`, the lower-latency
path is delayed by `d = |a - b|` for the whole fade, so the blend is aligned at
`max(a, b)`:

- If `b > a`, the old path is delayed. The output latency changes from `a` to `b` at
  the start frame, where the old path (gain near 1) repeats its last `d` frames. No
  delay remains after the fade.
- If `b < a`, the new path is delayed. The output latency stays `a` during the fade
  and changes to `b` at the frame where the fade completes, which can be mid-block. The
  delay is released while only the new path is audible, skipping `d` of its frames.
- A cut (`L = 0`) changes the latency at the start frame, with no delay (an unaligned
  jump).

This Release behaviour keeps the output latency equal to the current engine's latency
outside fades, so delays never accumulate across transitions. It places the
unavoidable discontinuity where only one path is audible. A Hold behaviour, which would
keep the higher latency after a decrease, is not decided here.

Each change is reported in `TransitionStatus::output_latency_change`, and as
`TransitionEvent::OutputLatencyChanged { previous_latency, latency }` with the absolute
frame. `output_latency_samples()` gives the current value. The consumer applies its
host's latency change.

The refusal check runs on the audio side because only the audio endpoint knows which
engine will be current when a proposal starts; earlier proposals may still be refused
or superseded. At the boundary where a proposal would be taken, if
`|latency - current latency| > D`:

- it is returned unused through a reserved returned slot (`ReturnReason::Refused`,
  `TransitionEvent::AlignmentRefused`, `TransitionStatus::alignment_refused`) before
  any priming or line change;
- without a free slot it stays in place, the boundary reports `retirement_blocked` and
  nothing starts.

## Consequences and limits

- `convolution_transition` is `convolution_transition_with_continuity` with
  `TransitionContinuity::NONE`, so the two cannot differ. That the default path keeps
  its ADR 0272 behaviour is guarded by the 15 unchanged ADR 0272 transition tests (the
  test file is byte-identical) and by a test that compares digests of output sample
  bits, the ADR 0272 status fields, offer results, reports and returned engines with
  values recorded from the public ADR 0272 build. The 108 scenarios cover f32 and f64,
  every mid-fade policy, both laws, a cut, three segmentations, a mid-fade reset,
  differing latencies and returned-storage backpressure.
- Per-block cost with priming is one copy of the input into the ring plus up to `B`
  frames of extra engine work at boundaries; a single-block prime can cost up to
  `frames` engine frames at one boundary, so `B > maximum_frames` spreads the load.
  With alignment it is one copy of each engine's output through its line,
  proportional to the block. Taking a proposal forgets one line and `reset` forgets
  both in constant time: each line counts the frames written since it was forgotten
  and reads older positions as zero, replacing a clear proportional to the prepared
  size (`channels * (D + maximum_frames)` samples). Nothing allocates, locks or
  panics. No deadline claim.
- The latency jump at a change repeats (increase) or skips (decrease) `d` frames. This
  is the documented discontinuity; there is no resampling or time-stretching.
- A delayed fade postpones the switch by a bounded number of blocks.
- Priming that covers less than the incoming tail improves the dip but does not remove
  it.
- Without priming, an aligned new path delayed by `d` reads zeros (its engine produced
  nothing before the start), so it is silent for the first `d` fade frames while its
  gain rises. If `d` is at least the fade length it contributes nothing before the
  release and the level dip is deeper. With priming it reads the primed output.
- After `reset` during a fade the delayed path, old or new, reads zeros for its delay.
- A proposal pending when priming completes is handled by the mid-fade policy at that
  boundary, as during a fade.
- Not included: Hold alignment, alignment beyond two engines, fractional latency,
  host-latency negotiation, and refusal at offer time.

## Tests

`ravet-graph/tests/convolution_transition_continuity.rs` (17 tests). The existing
`convolution_transition.rs` tests are unchanged.

- The default constructor matches digests recorded from the public ADR 0272 build in
  108 scenarios (output bits, statuses, offer results, reports, returned engines). The
  scenarios reach every ADR 0272 event and return reason, a full handoff and
  `retirement_blocked`.
- `convolution_transition_with_continuity(.., NONE)` and `convolution_transition` give
  the same output, statuses and reports. They share one code path, so this test checks
  the entry point only; it is not evidence against ADR 0272.
- **Priming, f32 and f64 banks.** Output is bit-exact against a fresh engine fed the
  recorded input, for segmentations 64, 1, 7 and two random. For a 4,096-tap response
  and a 256-frame fade, the windowed level dip is -7.50 dB without priming (-7.44 dB
  predicted from the missing tail energy) and 0 dB with priming. Error against a
  never-switched engine is zero when the history start is partition-aligned, and
  3.3e-16 (f64) or 1.75e-7 (f32) of peak for a 4,100-frame history.
- **Delayed fade.** The fade starts at the independently simulated boundary within the
  documented bound (29/29, 21/21 and 24/29 delayed blocks). Every block primes at most
  `B` frames. Output is bit-exact against the reference and equal to the never-switched
  engine.
- **Alignment, f32 and f64.** The tests cover four latency pairs (increase and decrease),
  both laws, fade and cut, with and without priming, over five segmentations. Error
  against an independent delayed-blend reference is zero. Each change is reported
  exactly once at the predicted frame and block. The default policy's misalignment
  error is 1.22 of peak during the fade.
- **Impulse onset.** On 148 impulses across three latency changes, the measured onset
  equals the reported output latency (worst difference 0). The default policy shows
  doubled onsets during a fade.
- **Refusals.**
  - Setup refusals return the identical engine with zero allocations.
  - An alignment refusal returns the identical, unprocessed engine.
  - Under backpressure, the refusal waits without starting anything.
  - A refused process call takes nothing.
  - Output stays bit-identical to a never-switched engine throughout.
- Mid-fade policies and `reset` during a delayed fade. One start per boundary when
  priming completes with a cut; this test fails without the guard.
- `LatestWins` during priming: later proposals supersede the waiting one in offer order,
  and only the newest starts after the primed fade.
- History shorter than requested, before the ring fills and after `reset`: only the
  recorded frames are primed, and output is bit-exact against a reference fed only that
  history.
- `reset` during aligned fades with the old or the new path delayed, against an
  independent reference in which both engines restart and the delayed path reads zeros
  for its delay.
- Zero audio-side allocations and frees across all mid-fade policies, single-block and
  delayed priming, alignment refusals, latency changes and reset.
- Concurrent worker/audio stress over 3 seeds x 3 policies with random latencies, some
  beyond `D`, in debug and release. Each engine returns exactly once, zero audio
  allocations, bounded peak, and output latency equal to the current latency at rest.
- A doctest on `convolution_transition_with_continuity`.
