# 0083: Complete gain reference programmes and native metadata

Status: portable catalogue implemented; native C ABI prototype validated.

## Decision

Reuse the complete immutable bank from 0080 for Unity/Silence/Bypass physical
controls [1,0]/[0,0]/[1,1], permanent IDs 1/2/3, MIDI bank 0/numbers 0/1/2.
Consumers propose/copy state; no live DSP/current identity is added. Complete
control persistence remains the existing version-two record. Selecting and
applying a program uses the consumer's restore/event smoothing and timing rules.

The native public header exposes count and copied repr(C) metadata: stable ID,
physical controls, MIDI address, zero reserved byte and complete NUL-terminated/
zero-padded 128-byte UTF8 name. It has no handle/dependent native SDK allocation.
Validate everything before writing; rejection/panic preserves output. Caller
pointer/ownership obligations remain auditable at the narrow ABI boundary.
Normal validation/copy has no heap, locks or I/O; exception handling is not a
real-time guarantee. Fixed name capacity applies to this reference catalogue,
not to every possible portable bank. Host menu/selection mapping is next work.

## Validation

Portable fixtures restore/persist/render all complete states in f32/f64. Native
Rust fixtures cover layout fields, null/index/atomic rejection and 10000 heap-
free reads of all entries. An actual public-header C++ client validates struct
size, copied names/values, rejection and complete codec/DSP renders in both
precisions; existing gain64 native regression passes. The first portable fixture
called the complete-state codec with scalar arguments; passing the complete
state fixes it. No raw MIDI handling, native host menu/DAW selection, program
identity persistence or generic SDK ABI is claimed.
