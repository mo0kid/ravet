# 0219 — Initialized AAX transport capture and native forwarding

Status: implemented prototype; normal public native Gain22/22, Performance4/4,
stock Gain14/14 and eight Python pipeline tests pass with actual packaged
exports/SDK0x20801345980/registered algorithms.

The checked native AAX model borrows only its initialized public versioned SDK
transport proxy. Stop/join precedes attachment and retirement; the AUDIO callback
collects independent getters after complete audio/MIDI/capacity preflight and
before applyState, queue consumption or native processing. No SDK source copying,
private-layout access or unversioned MIDI-node transport is needed.

Transport errors affect field availability independently. Successful malformed
or native-invalid projection rejects whole capture, retaining prior snapshot and
accepted intent/history, with original returned SDK statuses separate from local
faults and thrown attempts. Stopped diagnostics describe the latest collection
attempt and require stop/join; process status remains independently atomic.

Preserve full exact signed-i64 raw ticks/samples and loop endpoints. Native f64 PPQ
uses explicitly rounded full-range conversion/division, not an artificial ±2^53
raw cutoff. Validate positive tick divisor and exact raw loop order, then reject
collapsed/nonincreasing native loop endpoints. Query native samples only after
known playing=true, matching the SDK output-buffer position availability.
Configured loop and DETAILS placeholders do not imply actual looping, recording,
discontinuity or pre-roll availability. Independently queried tuples cannot claim
simultaneous host coherence.

Default reusable templates retain CaptureTransport=false; explicit native Gain/
Performance reference aliases opt in without changing stock wrapper regression,
identity, schema, state or MIDI semantics. Trusted foreign typed outputs/ranges,
fixed stack requirements, SDK callbacks and caller scheduling remain obligations.

Strict normal/fast-math collector and authoritative Rust acceptance, heap-counted
maximum capture cycles, initialized SDK proxy lifetime/status forwarding and
independent Gain smoothing after queued-restore late failure pass target-only.
Fresh isolated eight CMake contracts, actual registered Gain and Performance
callbacks, and self-contained Performance package exports/version/algorithm pass.
Normal public native Gain22/22, Performance4/4 and stock Gain14/14 packages
and eight Python pipeline tests pass. Local evidence
is not Pro Tools/production signing/other-platform/deadline acceptance.
