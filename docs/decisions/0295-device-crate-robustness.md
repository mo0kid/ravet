# 0295: Device crate robustness

**Date:** 5 November 2026  
**Status:** Implemented for `ravet-device`, `ravet-midi-device` and
`ravet-core-midi` (prototype features). Unit and integration tests on macOS
arm64 and Arch Linux x86_64; Clippy on macOS arm64/x86_64, Linux x86_64 and
Windows x64 (cross-compiled, Clippy only). No real device was opened.

## Context

[Decision 0287](0287-export-robustness-and-notices.md) and
[decision 0292](0292-webview-robustness-and-system-notices.md) reviewed the
plug-in export layer and the web view/standalone window, but left out the
device crates that standalone applications use. Their code runs on CPAL audio
callbacks, graph and clock workers, and native MIDI callbacks. A panic there
either permanently disables DSP (inside the output adapters' unwind
containment) or, outside containment, ends the process: the CoreAudio render
callback and the CoreMIDI shim call Rust through `extern "C"` functions, where
an unwind aborts. On ALSA and WASAPI an uncaught panic ends CPAL's callback
thread and the stream goes silent without a counter.

The review covered every non-test `panic!`, `unwrap`, `expect`,
`unreachable!` and `assert!`, indexing and arithmetic on callback paths, and
every `unsafe` block, `unsafe impl` and native C entry point. Each site was
classified as (a) unreachable, (b) a real risk, or (c) acceptable on a control
path.

## Decision

- **Callback paths have no panic edge.** No reachable panic was found: every
  callback-path site depended on an invariant that holds by construction. All
  57 such sites were still converted to fallible code, because a violation
  would cost DSP or the process. Following 0287, a failed invariant silences
  the sub-block and counts a processor fault, or reads as input underflow. It
  no longer permanently disables DSP. Five sites were outside any unwind
  containment and would have aborted or killed the callback thread: the
  reclaimable output, clocked input and clocked output guards, plain input
  capture, and the UMP batch delivery used by CoreMIDI. Regression tests break
  the ownership invariants directly and check for silence, counted faults and
  no allocation.
- **The clocked converter cannot spin.** A converter step that makes no
  progress, which cannot happen with nonempty input and output, ends the block
  as underflow instead of asserting. A refused clock observation uses the
  nominal rate. Channel alignment is a debug assertion.
- **Recovery passes selections, not flags.** The recovery sessions now hand the
  selected device(s) to the opener, which removes seven `expect`/`unreachable!`
  sites. `reclaimable_clocked_recovery` does the same for its four sites.
- **Control-thread invariants stay, with comments.** 31 sites remain, all on
  control or retirement-worker threads. 29 are unreachable (class a):
  `Option` owners that are `Some` until a consuming method, and sole-owner
  extraction after retirement. A panic in the retirement worker surfaces as
  `WorkerPanicked`. Two are class (c): the spawn-failure path relies on
  `std::thread::Builder::spawn` dropping the unstarted closure, as std does.
- **No exception or unwind crosses CoreMIDI.** All nine `ravet_cm_*` entry
  points are `noexcept`. An allocation failure inside one now terminates
  deterministically, as in 0292. The Rust `receive`/`release` callbacks
  contain no panic path.
- **Every unsafe block states why it is sound.** All three crates deny
  `clippy::undocumented_unsafe_blocks`. The CoreMIDI word-slice block was
  previously undocumented under the lint's rules, and the context cast now
  names the pointer's origin. Clippy runs for aarch64/x86_64 macOS, Linux
  x86_64 and Windows x64. Only the CoreAudio and CoreMIDI blocks are
  target-specific; both are macOS-only.

## Findings

| Location | Sites | Class | Action |
|---|---|---|---|
| `ravet-device` `lib.rs` `PreparedOutput::render` | 3 `expect` | a (audio callback, contained) | Fault the sub-block; shared `interleave_outputs` helper |
| `lib.rs` `into_processor` | 1 `expect` | a (control) | Comment |
| `input.rs` `PreparedCapture::capture` | 1 `expect` | a (input callback, **uncontained**) | Count an invalid buffer |
| `input.rs` `PreparedDuplex` render/deactivate | 6 `expect` | a (audio callback) | Underflow or fault; deactivate if owned |
| `input.rs` `into_parts` | 2 `expect` | a (control) | Comment |
| `clocked.rs` `convert`/`render` | 9 (`expect`, `assert!`, `assert_eq!`) | a (audio callback) | Nominal-rate fallback, break on no progress, debug assertion |
| `clocked_input.rs` relay and `pump` | 3 `expect` | a (audio/worker) | Silence the missing sample; zero published frames |
| `file_output.rs`, `buffered_file_output.rs` | 2 `expect` | a (audio callback) | Fault `LayoutMismatch` |
| `buffered_file_duplex.rs` processing | 8 `expect` | a (audio callback) | Fault; segment receives output count |
| `buffered_file_duplex.rs` accessors, lifecycle | 9 `expect` | a (owner invariant) | Field comment |
| `graph_output.rs` worker pump and `RingOutput` | 6 `expect` | a (worker/audio) | `GraphStatus::Fault`, zero frames or `LayoutMismatch`; saturating underflow |
| `graph_events.rs` `next_batch`, batch view | 3 `unreachable!`/`expect`, 2 `expect` | a (worker/audio) | Report no batch; batch carries its header |
| `reclaimable.rs` `DataGuard::render` | 1 `expect` | a (audio callback, **uncontained**) | Silence |
| `reclaimable.rs` retirement and spawn failure | 4 (`unreachable!`, `expect`) | a / c (control worker) | Comment |
| `reclaimable.rs` `play`/`pause`/`stop` | 3 `expect` | a (control) | Field comment |
| `reclaimable_clocked.rs` input/output guards | 2 `expect` | a (callbacks, **uncontained**) | Drop input or silence |
| `reclaimable_clocked.rs` retirement and spawn failure | 6 (`unreachable!`, `expect`) | a / c (control worker) | Comment |
| `reclaimable_clocked.rs` `play`/`pause`/`stop` | 6 `expect` | a (control) | Field comment |
| `recovery.rs` sessions | 7 (`expect`, `unreachable!`) | a (control) | Pass the selection to the opener |
| `reclaimable_clocked_recovery.rs` | 4 (`unreachable!`, `expect`) | a (control) | Selection passing; `match` in `request_reopen` |
| `sample_rate/coreaudio.rs` | 4 unsafe blocks, 2 extern blocks | sound | Existing SAFETY comments; lint enforced |
| `reclaimable.rs`, `reclaimable_clocked.rs` | 2 `unsafe impl Sync`, 3 unsafe blocks | sound | Existing SAFETY comments; lint enforced |
| `ravet-midi-device` `ump.rs` `push_words` | 1 `expect` | a (MIDI callback, **uncontained** under CoreMIDI) | Refuse the suffix as `Malformed` |
| `ravet-midi-device` `lib.rs` ring indexing, `ump.rs` subtraction | 3 | a | Comment |
| `ravet-core-midi` `macos/ffi.rs` | 12 unsafe blocks, 1 extern block | sound | SAFETY comments (one added, one rewritten); lint enforced |
| `ravet-core-midi` `native/core-midi.mm` | 9 C entry points | b (exception at the C ABI) | `noexcept` |

Counts per crate: `ravet-device` had 88 non-test panic sites. 57 on callback or
worker paths were made fallible, and 31 control-path sites are documented
(29 a, 2 c). It also has 7 unsafe blocks/impls and 2 extern blocks.
`ravet-midi-device` had 1 panic site, now fallible, plus 3 commented indexing
or arithmetic sites, and no unsafe code. `ravet-core-midi` had no Rust panic
sites, and has 12 unsafe blocks, 1 extern block and 9 native entry points.
Callback-path indexing and arithmetic are in bounds by construction. Three
subtractions became saturating, and several slice reads now use `get`.

## Consequences

The only behavioural change is in cases that cannot occur today. An internal
invariant failure in a device callback now counts a processor fault and keeps
the stream running, instead of permanently disabling DSP or aborting. A
process that runs out of memory inside a CoreMIDI shim call terminates. The
existing processor-panic containment and `panic = "abort"` limits are
unchanged. Foreign `Processor::prepare`/`deactivate` panics remain exceptional.
Panics inside CPAL, midir, CoreAudio and CoreMIDI are outside this review.

Windows Clippy ran without the `vorbis-recording` and `mp3-recording`
features: their C encoders in `ravet-audio-file` do not cross-compile under
cargo-xwin. Those features add no code to these three crates. No test opened a
device, and Windows runtime behaviour is unverified.
