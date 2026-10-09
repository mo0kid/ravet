# 0130: Explicit parameter gesture input

Status: portable-to-native and native-to-portable CLAP input implemented;
synthetic offline adapter/ABI evidence only.

Complete the raw boundaries from decision 0121 using opt-in
`with_parameter_gesture_input` on the three CLAP bridges, their event translator,
and the hosted graph builder. Native-to-portable default ignores boundaries;
portable-to-native default rejects them. Both policies retain prior behavior.
Native input IDs must be valid. Hosted input requires an explicit pre-activation
writable parameter map, translates graph identities to native identities, and
rejects disabled/unknown boundaries before any native call. Gesture/value/note/
modulation events share bounded input capacity. Process offsets/order are retained;
flush timestamps carry no sample meaning and normalize to zero on CLAP ingress.
Portable empty-block boundaries require offset zero through existing event checks.

Events carry no scalar value. No ownership/pairing, automatic begin/end recovery,
recording or saved-state edits are imposed; repeated ends remain representable.
Generated/hosted output uses the existing separately selected output policies.
Native owned begin/end structures contain no foreign pointers; they remain within
the existing owner-scoped prepared event buffer and its Send safety contract.

Translator tests exercise mixed begin/value/end order, disabled filtering, flush,
raw repeated ends, complete traversal/timing/capacity and 10000 heap-instrumented
rounds. Linked native f32/f64 fixtures echo mapped boundaries and readonly base
telemetry, preserving audio/base values, reject valid-prefix/unknown or disabled
input before mutation, and measure 1000 prepared blocks without heap actions.
An explicit missing-map test rejects before activation. No third-party receiver,
GUI automation recording, external DAW or native deadline claim is added.
