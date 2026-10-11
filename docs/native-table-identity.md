# Canonical native table identity

Native processor, parameter, cached parameter, audio-state, complete-state and
telemetry v1 exports acquire immutable tables at runtime on CONTROL. Repeated
acquisition of the same concrete family/precision in the same loaded library
returns its canonical address. Companion chains return those same originating
tables; foreign adapters continue requiring exact pointer identity rather than
structural equivalence or guessed export names. Retain the library through every
borrower and retire handles/caches only after callbacks have stopped.

`NativeProcessor::<P, S>::entry()` is no longer const-callable. Const evaluation
and inlined references to associated constant table values could promote distinct
copies into export/companion codegen units. A native raw-MIDI reference exposed
this defect when its exported processor pointer differed from its telemetry
companion's processor pointer. Acquiring the table at runtime fixes identity;
table contents, C ABI versions, DSP algorithms and provenance checks stay intact.
Metadata validation remains on the existing acquisition path and can allocate
where its previous contract permits; table canonicalization adds no heap registry,
locking, leaked owner or per-consumer layout workaround. Do not call acquisition
from AUDIO merely because address lookup itself is small.

All six native implementations keep their constant table *values* private and
return their address through a deliberately out-of-line runtime function. Callers
that previously used a const expression must move acquisition to runtime before
registration/creation. Unsupported precisions and invalid companion metadata
still return null. A Rustdoc compile-fail example records the const-callability
change.

The optional native identity fixture compares exported and transitive companion
addresses repeatedly for Gain, Performance, BusRouting, NoteRouting and Delay8/16,
in both precisions, then creates/prepares/processes real native owners and uses
matching audio-state/telemetry caches. The fixture needs current canonical archives:

```sh
cargo build -p ravet-gain-native -p ravet-performance-native -p ravet-bus-routing-native --release --locked
cmake -S tools/native-validation -B target/native-table-identity \
  -DRAVET_GAIN_LIBRARY="$PWD/target/release/libravet_gain_native.a" \
  -DRAVET_PERFORMANCE_LIBRARY="$PWD/target/release/libravet_performance_native.a" \
  -DRAVET_BUS_LIBRARY="$PWD/target/release/libravet_bus_routing_native.a"
cmake --build target/native-table-identity --parallel 1
ctest --test-dir target/native-table-identity -R table_identity --output-on-failure
```

A Gain archive built with `native-editor` also requires the existing explicit
`RAVET_VALIDATE_WEB_EDITOR=ON` CMake option to link its native GUI frameworks.
Normal and fast-math fixtures are local cross-codegen/table/processing evidence;
they establish no DAW, device, platform or deadline support. Earlier format
receipts remain historical observations of the archives actually tested; affected
current consumers need requalification against archives containing this fix.

See [ADR 0249](decisions/0249-native-canonical-tables.md).
