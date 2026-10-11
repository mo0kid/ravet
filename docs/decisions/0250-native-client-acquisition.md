# 0250: Exact native client origin and balanced acquisition

Status: accepted contract; local implementation reviewed. Public source
adoption and subsequent qualification are tracked in WORK_STATUS.md.

`NativeProcessorClient` previously accepted callback-equivalent copied core tables
although companion contracts require the exact originating table. Canonical native
runtime acquisition (ADR0249) now supplies stable addresses and removes the reason
for that weaker policy.

Require exact core pointer identity for selected state and telemetry companions.
Keep whole metadata/precision/function preflight before acquisition, null/failure
invalidity and existing processing return codes. This is an explicit C++ behavior
change: copied core tables are no longer admitted merely because callbacks match.
Consumers acquire the canonical entry; no private-layout/type workaround or global
registry is introduced.

Guard CONTROL construction against violating foreign acquisition/query exceptions.
Retire every earlier acquired resource in reverse order and leave the client
invalid. Normal null creation preserves existing scoped retirement. Native ABI
callbacks/destruction remain nonthrowing obligations; a throwing destructor attempt
cannot guarantee foreign reclamation and is never replayed.

Actual existing Gain32/64 archives pass typed prepare/process/state/telemetry and
retirement in normal/fast fixtures. Byte-identical copied-core state and telemetry
admission fail. Public proxy tables prove balanced successful/null/throw acquisition
at each of parameter-cache, telemetry-cache and processor stages; a malformed late
companion makes no creator call. Receipts `target/w237-{configure,build-final,tests-final}.log`
pass2/2. The first fixture compilation used incorrect aggregate field assumptions;
its receipt is preserved, and corrected fixtures initialize the public v1 layouts.

This package is separate from ADR0247 raw short MIDI and ADR0240 AAX internal64;
those consumers require this exact-client policy after adoption. No SDK vendoring,
DAW/device/deadline or arbitrary foreign-code guarantee follows from offline proof.

Additional pre-acquisition groups prove a byte-identical synthetic copied core with identical creator counters makes zero creator/retirement calls, for state and telemetry-only admission; throwing state-cache/cache-schema/schema-core/telemetry-core metadata queries produce invalidity with no acquired resources. Normal/fast plus existing client all3 PASS at `target/w237-tests-query.log`.
