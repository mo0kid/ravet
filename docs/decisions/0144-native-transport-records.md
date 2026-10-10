# 0144: Native block-start transport records

Status: canonical v1 native transport conversion implemented and validated with
Rust/C/C++ and full workspace checks. No new SDK/DAW timeline mapping is claimed.

Use a separate native C record with independent availability and boolean flags.
Unknown playing and absent details differ from explicit false/all-false details.
Retain exact i64 samples, f64 beats/tempo/increment/loop/bar starts, i32 bar numbers
and nonzero u16 signatures. Use no fixed-point representation, extrapolation,
rounding or implicit reset. Empty callbacks use the same snapshot conversion.

Check portable semantic dependencies before encoding: finite beats and positive
finite tempo; finite increment requires tempo; bar metadata requires beats; active
strictly increasing loop bounds require their matching timeline; signatures are
nonzero without power-of-two restrictions. Active loops may omit unavailable
bounds. Decoding rejects unknown bits, reserved/nonzero unused fields, inconsistent
availability and invalid dependencies rather than guessing missing context.
Preserve used negative-zero bits while requiring positive-zero unused doubles.

Keep conversion heap-free and independent of callback ownership. Consumers select
records, decode before invoking DSP and retain foreign-memory/fault-silence policy.
The prepared audio adapter still accepts exact typed Transport directly without
implicitly opting into this codec. SDK-specific fixed-point conversions and host
metadata support remain independent implementation/evidence.
