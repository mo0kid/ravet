# 0135: Explicit exported native CLAP performance ports

Status: optional exported adapter translation implemented; offline adapter
validation only, without a new instrument or DAW compatibility claim.

Keep existing MIDI/UMP selectors and default filtering intact. Add independently
selected native CLAP performance input/output ports bounded to 0..256. Translation
is explicit because port metadata and voice support are exporter responsibilities;
activating a bridge option must not silently change an instrument's advertised
capabilities or DSP behavior.

Native concrete On/Off preserves optional identity and checked f32 velocity;
legacy wildcard-ID notes remain ordinary portable notes. All standard expressions
retain wildcard channel/key/ID scope, f64 values and strict finite unit ranges.
Decode raw negative fields before Clack normalization and match all standard
expression discriminators, including Clack 0.2.0's omitted Expression type.
Unknown kinds and unrelated/wildcard ports filter. Wildcard Off channel/key,
choke/end and scoped parameters need independent portable contracts. Input flush
remains parameter-only; representable selected performance in zero-frame process
rejects. Whole input validates before processor calls, including ignored timing.

Generated notes use the native profile when selected; raw MIDI/UMP retain their
separate routes. Complete preflight rejects missing native routes, too-wide IDs
and native performance in parameter flush without host prefix or identity/scope
loss. Audio and DSP have already progressed. Host refusal preserves exact prefix
progress and never retries. Preparation/drop may allocate, processing does not.

Tests cover all seven expression types and scope, native ID endpoints, zero On,
legacy notes, malformed selected fields, unknown/unselected filtering, whole
input rejection, exact native output timing/values, host prefix refusal and
parameter flush rejection. Prepared translator/main-bus f32/f64/auxiliary f64
workloads measure no heap actions. Public gain/sine default behavior is unchanged;
no third-party expression receiver, DAW, physical MIDI or deadline claim follows.
