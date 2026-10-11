# 0268: Hosted VST3 32/64 aux buses and owned Graph banks

Status: prototype implemented; local macOS arm64 native, mock-provider and
actual Gain/Performance module evidence only.

## Context

Scoped VST3 instances lend AUDIO through a higher-ranked scope that cannot escape
and supports only main-bus f32. Canonical Graph owns `'static` boxed processors.
Erasing the scope lifetime, or moving an `Rc`/Instance onto AUDIO, would let a
worker unload a module or tear down native owners. `Processor::reset` assumed an
unconditional history clear that VST3 cannot guarantee.

## Decision

1. Add an exact-origin `RavetVst3HostedAudioProviderV1` companion (refresh,
   per-bus records, explicit complete profile, timing getters, process32 and
   process64 with the unchanged rich carriers). Existing tables/domains stay
   unchanged. The report distinguishes local refusal, attempted, returned and
   thrown foreign calls; getters never invent a tresult.
2. Profiles request every advertised bus explicitly in native index order; no
   default-active inference, no fallback precision, no layout substitution.
   Packed channels follow native index order per direction; inactive buses stay
   in native `ProcessData`.
3. Safe Rust adds `refresh_hosted_audio`, typed `prepare_hosted32/64` and
   `Audio::process_hosted::<S>` on the existing scoped capability.
4. A Graph bank consumes a MAIN `Library<'static>`. Instance's private parent is
   `Borrowed(&Library)` or `Retained(Rc<Library<'static>>)`; only the bank's
   private constructor creates the retained form. All children are prepared
   before the first lease begins; leases begin last.
5. A process-lifetime MAIN registry (64 banks x 128 children, nonwrapping
   generations, leaked deliberately) retains every bank before any node escapes.
   Each node has one preallocated return slot whose consumer stays in the
   registry. Node drop moves the bundle there; control drop only orphans.
   Reclamation ends all leases before any stop/retirement and never guesses that
   a lease ended.
6. Rich native lanes are explicit per-node batch rings with exact sequences and
   reserved return capacity; opaque SDK kinds are not forced through Foundation
   events.
7. `Processor::try_reset` (default delegates to `reset`) and
   `ProcessError::RebuildRequired`; hosted VST3 nodes refuse checked reset and
   remain faulted; Graph/wrappers propagate refusal without masking it.

## Consequences

Abandoned or forgotten nodes consume bounded registry capacity indefinitely; that
is preferred to unproved teardown. Rebuild is the only recovery from a faulted or
reset node. Portable event/transport mapping, a shared Sidechain VST3 reference
module and VST3 editor hosting remain separate packages. Rust allocation
evidence excludes foreign heaps; no DAW, third-party, deadline or other-platform
claim follows.
