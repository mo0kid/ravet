# 0067: Native VST3 gain sample precision

## Context

The native f64 gain ABI needs explicit VST3 setup/process selection. Advertising
64-bit support while narrowing samples or sharing handles would lose precision
or invalidate native lifecycle/state ownership.

## Decision

Own separate native f32/f64 DSP handles and prepare the selected setup precision.
Process requires that exact sample size and chooses matching native pointer APIs.
Shared atomic complete control state and conditional generation reflection stay
unchanged. Stop resets both handles; new setup restores state into its selected
handle. Per-channel output silence flags reflect actual native output values.

## Consequences and validation

Parameter/smoother resolution remains f32; audio and f64 metering are native f64.
This supports the reference only, not generic instruments/aux/events/editors.
Embedded AU/AAX adapters need separate rebuild/host evidence, and DAW precision
cannot be inferred from its plug-in discovery or earlier gain validation.

Contract acceptance covers six layouts/rates, exact f64 audio/aliases, event bypass,
malformed rejection, zero-frame edits, precision switching, mismatch silence and
both-precision output flags. Actual SDK validator exercises its dynamic 64-bit
suite. Offline/native SDK tests do not establish deadlines or a DAW's selection.
