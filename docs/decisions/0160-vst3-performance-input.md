# 0160: Explicit VST3 performance input

Status: implemented; local code and format validation passed.

## Decision

Add optional external-SDK C++17 selected-bus performance translation independent
of a particular processor or SDK factory. Canonical native v1 records remain the
portable boundary; successful single-event mapping atomically replaces output
and count, while rejected/ignored events leave both unchanged.

Preserve SDK note IDs as full int32 bit identities in u32, with -1 alone absent.
Negative IDs include a documented plug-in-reserved range and must not be rejected
merely for their sign. Preserve unquantized f32 velocities (zero On remains On).
Finite cents within +/-12000 expand to portable semitone tuning after On/before
Off when nonzero; zero is the default. Length is an optional hint, not an inferred
release. SDK ppq/flags have no representation in this contract.

Poly pressure preserves channel/key/optional ID. Six standard expression types
require concrete ID and normalized finite 0..1 values. Volume multiplies by four,
including explicit mute; tuning uses240*(value-.5), and remaining values retain
f64. Unknown expressions reject unless the caller explicitly selects Ignore,
which still validates their scalar/ID. Other selected event types reject; no
variable foreign SysEx/text pointers are read or copied implicitly.

Complete list preflight reads each getter once and bounds raw/expanded events to
1024 and frames to 65536, validates global offset order including unselected
headers, and retains equal-offset/list/expansion ordering. Private stack scratch
is 56KiB plus local candidates. No visitor runs until every raw event validates.
Visitor failure cannot roll back its own prior effects, so DSP/state/host output
must wait for successful private staging. Normal framework mapping uses no heap,
locks or I/O; foreign callbacks retain consumer obligations.

## Validation and limits

Normal and fast-math fixtures exercise all standard types, exact ID bits,
physical scalars/signed zero/subnormal volume, cents/velocity bounds, malformed
foreign NaN/infinity, unsupported/unknown/selected policies, atomic failed outputs,
late list rejection, capacity/expansion/getter/order/zero-frame bounds and10000
full list cycles without C++ new/delete. Native codec tests independently validate
portable record acceptance. Gain has no performance input bus and does not use
this helper. SDK controller advertisement, instrument factory/voice application,
output delivery, SysEx/text/int expression contracts and DAW/device/deadline/other
platform validation remain separate work.
