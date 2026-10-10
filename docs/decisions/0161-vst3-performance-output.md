# 0161: Explicit VST3 performance output

Status: implemented; local code and format validation passed.

## Decision

Add optional external-SDK C++17 canonical native performance conversion and
complete generated-stream delivery. Check all representability, scalar, unused
field, timing/order and capacity constraints before any mutable SDK host call.
Native source records remain untouched; failed single conversion preserves output.

Concrete notes and scoped Off/pressure with concrete channel/key preserve exact
widened f32 scalar values and optional int32-bit IDs. UINT32_MAX concrete identity
rejects because SDK reserves -1 as absent. Standard expressions require ID-only
scope: SDK cannot carry their channel/key matching fields. Volume/4 must remain
exact; integer IEEE exponent/significand checks reject subnormal rounding or
underflow and survive fast-math algebraic cancellation. Tuning/240+.5 is explicit
floating unit conversion and may round; other supported expressions retain f64.
Zero volume mutes and signed-zero used scalars survive. No epsilon is substituted.

Choke/end, wildcards or precision loss, MIDI/UMP/SysEx, parameters and text/int
expressions reject rather than change meaning. Note completion is not release.
SDK ppq, flags, optional length and note tuning metadata default to zero; nothing
is inferred from transport or voice state. Consumers advertise compatible event
buses/expression metadata and own voice policy independently.

Complete delivery bounds1024 records/65536 frames, preserves global offset/tie
order and zero-frame rules, and uses stack scratch of1024 SDK events. A host
refusal reports the exact accepted prefix and remaining suffix. DSP/history and
accepted host events cannot be rolled back; no replay/retry is attempted. Normal
framework work avoids heap actions, locks and I/O; host callbacks retain consumer
thread and retention obligations. Null host is valid only for an empty stream.

## Validation and limits

Normal/fast-math fixtures exercise every supported tag, both identity sign halves,
exact unquantized scalar/address mapping, input/output composition, normalized
expression units, signed zero, volume subnormal/exponent boundaries, malformed
foreign scalars and canonical fields, unsupported semantics, complete late output
rejection before any host call, maximum zero-frame lists, mutable-host isolation
and exact refusal counts.1000 full-capacity cycles perform no C++ new/delete.
Gain has no performance output and does not adopt this helper. SDK instrument
export, voice application, controller advertisement and DAW/device/deadline/other
platform evidence remain separate work.
