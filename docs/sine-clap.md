# MIDI sine CLAP reference

`ravet-sine-clap` exports the portable 16-voice sine instrument as
`org.ravet.sine` / RAVET Sine. It has no audio input, one stereo f32 output,
one MIDI input port and one automatable level control (stable ID 1, range 0..1,
default 0.2). MIDI note/bend/controller messages preserve sample timing. The
portable selected lower-zone RPN behavior and three bank-0 factory level
programs (0 Default=0.2, 1 Quiet=0.05, 2 Full=1) are available through raw MIDI
on channel 0; other channels/unavailable programs preserve level; the
export advertises MIDI only. It does not advertise CLAP note-ID/wildcard, MIDI2,
SysEx, per-note modulation, auxiliary audio, f64 or a custom editor. Hosts may
show their generic parameter window. Held notes have an infinite declared tail.
See [portable synthesis and expression](midi.md) for DSP/voice policy.

## Build and actual-binary checks

```sh
python3 scripts/bundle-clap.py --plugin sine --profile release
CARGO_BUILD_JOBS=2 cargo run -p ravet-sine-clap --example validate --locked -- \
  'target/clap/release/RAVET Sine.clap'
```

The helper builds with two Cargo workers. Its default `--plugin gain` retains
the existing gain path. Bundles stay under target; macOS uses development ad-hoc
signing. No global plug-in install or production signature is created. On Linux/
Windows the helper copies the matching native library; these paths need runtime
validation before support claims.

The development command loads only the trusted public reference built here.
It checks the actual bundle's descriptor/audio/MIDI ports, finite level codec,
inactive/active flush, note timing, completed release, sample-timed sostenuto/sustain takeover, program audio/state recall, stereo
output, invalid
whole-block silence, active state recall and three activation cycles. Its 3,024
processing blocks include 3,012 heap-instrumented callbacks with no Rust heap
operations. Loading arbitrary third-party code is outside this fixture.

The local CLAP validator 0.4.1 reports 34 passed, 10 skipped and zero failed on
macOS arm64. Skipped capabilities include f64, CLAP note wildcards, dynamic
port configuration and preset discovery. Validator rate testing exposed an
unnecessary 384 kHz DSP cap; positive finite rates now validate against actual
u32 envelope-count capacity, including a 768 kHz fixture and overflow rejection.
REAPER 7.82 on local macOS 27.0.1 arm64 also passed timed synthetic MIDI stereo
audio, generic parameter-window presence, saved level recall and two automation
levels with measured peaks approximately 0.04/0.02/0.04. The disposable test copy
used its already-running 96 kHz device rate with every hardware send muted; the
project did not override the device rate. No other host/platform/deadline claim
follows from these checks. See [host validation](host-validation.md).

## State and synchronization

Level is the sole persisted control. The deterministic record is exactly 16
bytes: `RVETSIN1` magic/version, little-endian f32 level, four zero reserved
bytes. Nonfinite/out-of-range, malformed, truncated and trailing records reject
before publication. Save snapshots one atomic word. Successful load publishes
level plus a 32-bit restore generation, rescans host parameter values and
requests processing. Audio applies a new generation without advancing time and
clears notes/controllers/history before processing the next callback. Notes,
envelopes and negotiated MIDI performance settings are transient, rather than
persisted product controls. Within a running instance the portable reset retains
negotiated zone/ranges as described in the MIDI guide.

Audio reflection uses one compare/exchange against its original snapshot so an
intervening restore wins. The 32-bit generation wraps after 2^32 restores; callers
must not leave an audio owner unsynchronized across that many restores. Tests
cover stale reflection, invalid-load preservation and active recall. Processing
faults clear host outputs and publish a numeric diagnostic rather than returning
an error that would make Clack log on the audio thread. State errors occur on the
host's main thread and may be logged by Clack.

See [decision 0040](decisions/0040-clap-main-bus-instrument.md) and
[work status](WORK_STATUS.md) for current evidence and remaining export work.

## Factory program requests and host refresh

Raw MIDI CC0/32 stages the pending bank; Program Change recalls the complete
matching level at its sample offset, retaining sounding voices, phase/envelopes
and negotiation. This reference has one global level and accepts requests only
on channel 0, both before and after lower-zone negotiation. Repeated requests
are accepted; unknown bank/number leaves values intact. Program changes travel
through process, not a parameter-only flush. Reset/restore clears pending MIDI
banks, keeping the published level. Accepted RPN6 configurations also clear
pending banks/requests; RPN0 range changes preserve them. No separate current program identity or host
program/preset-discovery menu is advertised.

Successful changed active level publication requests at most one deferred
main-thread callback until serviced. The callback rescans current parameter
values when the host supplies `HostParams`; audio never invokes the main-thread
rescan directly. Notifications also cover changed active level automation/flush,
and stale audio reflection after restore cannot publish or schedule a change.
Hosts must service their callback requests. Polling/save sees the same atomic
level, which remains the sole persisted control. Linked parameter-aware host
checks and actual-bundle audio/save queries validate these paths; no new DAW
program-recall evidence is claimed. See
[decision 0081](decisions/0081-sine-program-recall.md).
