# Exact native client acquisition

`ravet::native::NativeProcessorClient` is an optional SDK-independent C++17 owner
for a native processor and optional state/telemetry companions. Construct, prepare,
query stopped state, deactivate and retire after callbacks stop and join on CONTROL.
Retain every table and its originating loaded library for the whole owner lifetime.

The state schema's `processor_entry()` and telemetry's `processor_entry()` must
return the **same core table address** supplied to the client. A byte-identical
copy with identical function pointers is rejected before any cache or processor
creation. This corrects the prior callback-equivalence acceptance policy; consumers
that copied a core table must acquire its canonical exported/runtime entry instead.
See [canonical native tables](native-table-identity.md) for runtime acquisition and
the separate loss of const-callability. Table pointer identity is a provenance
contract with trusted loaded code, not authentication of arbitrary foreign code.

The complete core and selected companion shapes, precision, counts and required
functions preflight before cache creation. Null creation leaves an invalid owner
and stops later acquisition. Already acquired resources remain owned until CONTROL
retirement. If a violating foreign acquisition/query throws a C++ exception, the
constructor leaves an invalid client and defensively retires acquired processor,
telemetry and parameter caches in reverse order. It does not turn such code into
a supported throwing native ABI. Native callbacks and destruction must not unwind;
a throwing retirement attempt is not retried and cannot promise external cleanup.
Original returned processing/status behavior is unchanged.

The normal/fast actual-archive fixture
[`client-companions.cpp`](../tools/native-validation/client-companions.cpp) consumes
genuine exported Gain32/64 tables, performs typed rendering/state/telemetry and
stopped lifecycle, rejects byte-identical copied cores for state and telemetry,
and proves balanced successful/null/throw acquisition with coherent public proxy
tables. Late malformed metadata invokes no resource creators. Configure
`tools/native-validation` with a current `RAVET_GAIN_LIBRARY` to include it.

Target-only evidence is recorded in ADR0250. Public rebuilt archives and current
format consumers require qualification after adoption. These local tests do not
establish DAW, device, deadline, other-platform or arbitrary foreign-allocation
support.

Additional pre-acquisition groups prove a byte-identical synthetic copied core with identical creator counters makes zero creator/retirement calls, for state and telemetry-only admission; throwing state-cache/cache-schema/schema-core/telemetry-core metadata queries produce invalidity with no acquired resources. Normal/fast plus existing client all3 PASS at `target/w237-tests-query.log`.
