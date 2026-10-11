# 0269: asynchronous AUv3 host ownership and joined main-bus rendering

Status: prototype with in-process evidence; actual out-of-process (IPC) rendering unvalidated. See the [AUv3 hosting guide](../au-hosting.md).

The existing AUAudioUnit subclass is an exporter, not a foreign-component host.
Use an optional caller-linked authored macOS provider and safe Rust owners, with
explicit discovery/selection and AUAudioUnit's async factory. Never manufacture an
AUAudioUnit from an undocumented AudioComponent instance property or private layout.
No Apple SDK sources are copied. The provider is default-disabled and process-lifetime.

Foreign completion captures independently owned native state, never a borrowed
Rust catalogue or callback. Deliver/retire on main, bound pending16 and total
instance/reservations256, and retain cancelled work until actual completion.
Poll is nonblocking; applications service their run loop explicitly. Preserve
NSError's domain/signed64 code separately from native32 OSStatus and local status,
including attempted/returned distinctions. Required mode is checked against
isLoadedInProcess rather than request options or auxiliary service presence.

Prepare one main planar f32 mono/stereo profile stopped, with cached public render
and pull blocks plus fixed prepared scratch. Use an exclusive lock-free AUDIO CAS,
complete shape/byte/alias/known metadata preflight, source capture and whole successful
output publication. Preserve native failure codes; safe admitted outputs silence on
render refusal/exception, while post-DSP buffer corruption does not imply rollback.
Repeated whole-block pulls use fresh copies of captured source. No SRC/subframe
scheduler or precise i64-to-f64 timeline representation is invented.

Main geometry/foreign lifecycle excludes all admitted audio children. Safe HRTB
scopes join on normal return/unwind/forget. Main facade retirement can be accepted
into a bounded deferred queue without releasing immutable protected identities,
scratch or units until final joined end; cancellation still awaits real completion.
Fail-closed end/cleanup retains ownership rather than retrying unproved retirement.

Actual public Gain async/native and safe Rust rendering passed in-process;
synthetic SDK lifetime/failure/reentry bounds and eight ownership compile-fail tests
provide scoped evidence. One reviewed reversible exact-path registration of the
ad-hoc signed public Gain extension (non-/Applications location, no Team ID, no
sandbox entitlement) produced no PlugInKit or AudioComponent match, and the
out-of-process consumer reported UNAVAILABLE; cleanup restored the empty
before-state. Actual out-of-process observation therefore remains unvalidated, and
any further registration, signing or installation attempt needs separate explicit
approval. Option requests cannot turn refusal into support. This foundation
is not complete AU hosting: parameters/state/programs/editors/events/transport,
precision/aux/graph/recovery and exporter mappings remain explicit follow-ons.
