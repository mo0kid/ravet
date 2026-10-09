# 0098: Serialized native gain meter snapshots

Status: implemented public reference DSP ABI, not native editor delivery.

## Decision

Expose the existing complete f32/f64 gain meter snapshots through fixed-layout
C records under the same exclusive serialized handle ownership as processing.
Keep native f64 peak/RMS, counts/diagnostics and zero reserved words; widen f32
readings. Null rejection leaves caller storage unchanged, success copies all
four readings. Prepare/reset clear readings while empty/rejected blocks retain.
No implicit reset or allocation. No concurrent control-thread read of live DSP.
Adapters must provide a safe processing-to-control handoff for native editors;
no pointer-sharing/host-process assumption is introduced by this ABI.

## Evidence

Public C++ contracts link the actual Rust static library and verify layouts,
mono/stereo/rates, exact f64 precision and large finite RMS, sample counts,
nonfinite diagnostics, complete retention/rejection/reset and null outputs.
No native editor delivery, broad platform or callback deadline claim.
