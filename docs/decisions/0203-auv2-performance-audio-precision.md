# 0203: Explicit Performance AUv2 audio precision and checked lifecycle

Status: prototype; normal packaged Performance reference passes all 15 AU
validators and SDK 47/0, including actual f32/f64 and lifecycle fixtures, strict
normal/fast-math companion adoption and default32 helper regression. Actual Gain
component and four Python bundler regressions also pass. Live DAW validation is
separate.

The public Performance processor already has native32/native64 audio paths, but
the inherited Steinberg AU wrapper hardcodes sample32 in initialization, offline
mode changes and output pointer access. An f64 ASBD alone cannot repair those
paths. RAVET owns a focused adapter around protected SDK boundaries, with external
SDK implementation sources unchanged and unvendored.

The constructor creates explicit native32/native64 immutable schema and isolated
complete-state helpers. Their optional expectedBits defaults to32; explicit64
must match exact cache/parameter/state companion precision and originating table.
Normalized AU metadata, control, physical text and complete Level codec remainf32.
Audio precision is selected separately from the accepted output format and actual
processor sample-size support. Exported f64 symbol presence does not imply support.

Initialize places the SDK main-thread guard before any owned work, because stock
initialization virtually reenters Initialize on main. It preflights complete
mono/stereo planar formats with the matching companion.processor_entry() origin,
then retains stock initialization and checks an explicit selected setup:
stop/deactivate, typed HostProcessData preparation, setupProcessing, activation and
setProcessing. Stock initialization still performs intermediate sample32 setup;
only completed owned selected setup makes rendering ready. Render and caller
preflight use exact selected float/double pointers and byte/alignment contracts.

OfflineRender is an owned global0 UInt32 Boolean property with info/get/set.
Stopped changes defer to initialization. Serialized initialized changes rerun the
selected path instead of the stock sample32 setter; unchanged values are no-ops.
Successful changed setup resets transient voices while preserving complete Level
publication and already accepted pending timed notes/Level intent. Actual initialized
transitions in both directions verify queued-note/timed-Level delivery and exact
new-voice output, separate from failed setup clearing queues. Reset checks
processing transitions with precision and stock SDK selector behavior retained; cleanup
retires both maps and clears queued note metadata before stock best-effort cleanup.

A setup/activation failure marks the adapter unready, clears note/control transfer
queues and mapped profiles, and attempts stop/deactivation. A subsequent render
rejects with bounded supported caller-buffer silence. Recovery requires explicit
uninitialize/initialize. This fail-closed policy is not rollback of arbitrary
SDK allocations, notifications or earlier backend effects. Hosts serialize all
CONTROL transitions with audio/MIDI callbacks; no callback locks/deadlines or
all-backend allocation guarantees are added. Native Level state can be recalled
through stopped32→64→32 reinitialization because the public complete codec is the
same, while transient voice/history state is intentionally excluded.

Actual draft components execute the full mono/stereo timing/identity/control/
state/transport/fault suite in both audio precisions and200voice cycles each.
f64 analytic timing/phase/decay tests use1e-12 tolerance, including rejected alias
silence/tails without phase advancement and retained accepted timed notes.
Another actual component probe covers stopped/initialized offline changes,
worker/main initialization, reset and coherent complete Level recall32↔64.
Strict normal/optimized-fast-math helper fixtures exercise real native64 scalar,
metadata/text/codec acceptance, foreign nonfinite rejection, companion mismatches
and retained CF ownership; existing default32 synthetic/actual helper tests pass.
Those fast-math checks do not prove whole stock SDK wrapper fast-math support.
Arbitrary backend setup/activation failures are source-reviewed, not fault-injected
by the public processor. Existing Apple original ASBD size/reserved normalization
and unchanged-active-format no-op limits remain explicit.

The package remains a local macOS arm64 prototype. No DAW/auval discovery or
session recall, device/sandbox, other architecture/platform, auxiliary host routing,
MIDI output port, production support or real-time deadline follows from these
in-process checks. Default Gain packaging and distributed f32 defaults remain.
