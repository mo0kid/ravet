# 0040 — Native-precision main-bus CLAP bridge and MIDI instrument

Date: 9 October 2026. Status: prototype, with local actual-binary validation.

Preserve the existing gain EffectBridge while adding a generic main-bus bridge
with native f32/f64 storage, 0..64 inputs and 1..64 outputs. Copies of all inputs
precede portable processing so legal host aliases cannot corrupt future reads.
Outputs have independent prepared storage. Shape, precision, full event stream
and block-start transport validate before DSP mutation. Maximum f64 scratch is
64 MiB. Dual-precision descriptors reject instead of selecting a buffer silently.
Auxiliary buses and event output remain separate pending work.

Optional raw MIDI translation uses one explicit input port. Validate status and
seven-bit payload up to status-defined length; ignore CLAP padding. Other port
MIDI and unsupported event kinds are ignored but every timestamp/count still
validates. Flush translates parameters only. This avoids claiming CLAP note-ID
matching that the portable voice allocator does not implement.

The new public sine reference exports stereo f32, a MIDI-only port, stable level
and a 16-byte level record. Atomic level/generation restoration resets notes at
synchronization; one-attempt audio reflection preserves later restores. Notes and
MIDI performance settings remain transient. A shared ABI fixture tests linked
and actual bundled entries, including active load and no callback heap work.
The external validator passes after replacing an arbitrary DSP rate cap with
actual envelope-capacity validation. This is local prototype evidence, not
production distribution or arbitrary plug-in hosting support.
