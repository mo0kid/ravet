# 0001 — first portable processing contract

Status: original offline gain milestone, preserved as history. Precision/bus/
output-event/latency contracts are extended by [0017](0017-extended-processing.md).
Parameter/state and editor capabilities are recorded in their subsequent decisions;
the [catalogue](../FEATURES.md) is the current implementation view.

## Decision

Keep processor contracts in `ravet-foundation`, numerical components in
`ravet-signal`, and the reference product in `examples/gain` (`ravet-gain`).
The umbrella crate only re-exports modules. Portable crates retain
`unsafe_code = forbid` and have no external runtime dependencies.

Start with one planar f32 main bus: zero, one, or two input channels and one or
two output channels. Configuration validates a finite positive sample rate,
a nonzero maximum block size, and a maximum number of input events. A prepared
processor accepts any frame count from zero through that maximum.

`AudioBuffer` distinguishes in-place mutable storage from separate immutable
inputs and mutable outputs. Safe constructors check that every channel has
at least the requested frame count; backing capacities may differ. Accessors
expose only the active frames. No raw pointers or copies are needed. Hosts
that alias storage must use the in-place representation or adapter scratch
storage; constructing separate overlapping references is not legal Rust.
No-input instruments use separate buffers with an empty input slice.

The lifecycle is create -> prepare -> process/reset -> deactivate. Preparation
may replace an earlier configuration only when callbacks have stopped. Failed
preparation leaves the example processor inactive. Reset snaps smoothing to
the latest requested value without changing that value or configuration.
Deactivation preserves the requested parameter for the next preparation.
Processors are exclusively borrowed during processing and need not be Sync.

Events are borrowed, ordered by nondecreasing frame offset, and bounded by the
prepared capacity. Same-frame events keep slice order. Offsets are strictly
less than the frame count; zero-frame blocks accept only offset zero. Parameter
values are finite physical values, identified by permanent u32 IDs. Basic
note-on/off events validate MIDI channel/key and normalized velocity. Gain
ignores unknown parameter IDs and notes. Output events are deferred until an
instrument needs them.

Each processor validates a whole block before changing state or samples. An
invalid block returns a small fault enum and silences all active output frames;
no events in that block are applied. More events than capacity are rejected
before traversing the event slice. Processing while inactive also silences
outputs. These paths perform no allocation, formatting, logging, or locking.
A zero-frame block applies valid parameter events but advances no DSP time.

Gain automation takes effect before the sample at its offset. A linear ramp of
N samples starts from the current effective gain; the first sample advances
1/N of the distance and the Nth reaches the exact target. Retargeting starts
from the current value. Setting an unchanged target does not restart a ramp.
Zero-length smoothing is immediate. Progress counts samples, not blocks, so
partitioning a stream into different blocks preserves output. Smoothing time
in milliseconds is rounded to the nearest sample at prepare time.

## Scope and consequences

The contract covers one main bus and f32 only. Auxiliary buses, output events,
parameter descriptions/gestures, state serialization, latency/tails, and FFI
are future decisions. The gain example accepts matched mono/stereo effect
layouts; the foundation also supports no-input instrument layouts, exercised
by a small test processor. The surface crate remains a documented placeholder.

Allocation instrumentation is a development-only dependency in the gain
example; it does not enter the framework's runtime dependency graph. Its
thread-local counting is checked with intentional allocation/free probes and
concurrent worker activity. Offline tests do not establish device deadlines,
plug-in host compatibility, or editor feasibility.
