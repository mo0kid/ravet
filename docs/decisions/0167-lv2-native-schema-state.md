# 0167: LV2 native schema, processing and complete state

Status: prototype; local actual-binary contracts passed.

The mono/stereo public LV2 references previously called a bespoke Gain ABI and
constructed hard-coded parameter events. They now use the reusable external-SDK
C++17 `NativeLv2Processor<Parameters, MaximumChannels>` profile. Consumer-specific
factories still select the product's native tables, URI, port metadata and channel
count. The helper creates immutable native parameter metadata on control, prepares
one f32 input/output main bus, and translates connected physical control ports to
block-start events using the schema's clamp/snap rules. Read-only schema fields are
rejected by this input-only control profile.

A whole audio/control preflight precedes consuming state or rendering. Missing,
misaligned, overflowing or mutually overlapping output ranges reject; input/output
aliases are supported. Zero-frame blocks permit null sample ports. Oversized blocks
clear at most the supported 65,536-frame prefix. Host storage validity, disjoint
control/metadata/sample ranges, serialized callbacks and ordinary IEEE compiler
semantics remain caller obligations. Generic consumer backend failures after native
state application cannot promise rollback; the public Gain consumer's validated
profile has no additional semantic rejection path. Contained native panics disable
this instance until recreation.

LV2 `state:interface` saves the complete canonical native state as portable POD
`atom:Chunk` under the consumer's `urn:ravet:gain:state` key. Instantiation optionally
caches `urid:map` identifiers; hosts without it may process, but state calls return
`LV2_STATE_ERR_NO_FEATURE`. Saving uses the native coherent publication, so it may
overlap audio without borrowing the DSP. A contended one-attempt read returns an
error to the host; the framework does not retry or block. Restoring uses the LV2
Instantiation threading class: it must be serialized with every other instance
callback. No `state:threadSafeRestore` feature is advertised.

Restore validates the whole supplied native product/version/schema before
publication. A missing property restores schema defaults as required by LV2.
Malformed types, flags and payloads preserve the old state; host store failures
propagate. The host also persists input controls independently. After an explicit
restore, the helper captures the currently connected canonical controls and holds
restored values until a control changes. Changed controls take precedence for
that parameter at the next successful block, including zero-frame blocks. Controls
reapplied by the host during recall therefore become authoritative. If controls are
unavailable at restore, the first valid connected controls win. Repeated unchanged
ports and already accepted publications do not restart smoothing.

The actual built mono/stereo module passed previous audio/lifecycle/alias contracts,
complete state/default/malformed restore and port-fault checks, 4,000 normal
run/save cycles without C++ heap operations or URI mapper callbacks, and concurrent
saves during 20,000 audio blocks with coherent complete state. The C++ heap probe
does not measure Rust allocation or audio deadlines. Atom events, MIDI/instruments,
custom UI, programs, f64, auxiliary buses, DAW discovery/recall, Linux runtime and
production support remain unvalidated or outside this profile.
