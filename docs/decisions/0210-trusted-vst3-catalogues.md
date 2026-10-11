# 0210: Explicit trusted VST3 catalogue ownership

Status: prototype; owned Rust ABI/cache and external-SDK native loader implemented;
public native CMake/catalogue/provider composition and complete shared Rust gates
pass locally. Public Rust/native probe and all three native CTest executables,
including controlled context faults, pass after the fixes. Full instance hosting
is separate.

Keep SDK implementation outside Cargo and the repository. An explicitly linked
C ABI provider owns a selected module and publishes bounded whole fixed records.
Rust retains the provider lifetime, rejects malformed records and owns immutable
metadata without exposing raw factories or handles. Unsafe construction/load
make executable lifetime and module trust obligations explicit; thread-restricted
owners preserve platform main-thread retirement.

Read raw factory count/status/metadata instead of SDK convenience collectors
that reserve unchecked counts or ignore factory-info errors. Bound enumeration
to256, require whole canonical narrow/Unicode strings and unique opaque IDs, and
retain the highest supported interface's failure. Own host application lifetime
through module termination. Reserve exact live factory identity before setters
to prevent singleton/context overwrites, with no bookkeeping lock held during
foreign code. Accept and preserve the SDK's optional NotImplemented context
result; never infer delivery success from it. Failed-native clearing consumes
ownership, while non-consuming provider rejection recovers the exact owner.
Take ownership of pointer-valid unexpected query outputs before checking status,
so failed queries release their acquired reference. Map caught C++ exceptions to
InternalError rather than retaining a stale caller diagnostic. Authored controlled
factory/module fixtures separate optional install/clear, exceptional setter, late
metadata failure and final host probes from actual loader evidence.

Catalogue publication is only the first hosting boundary. Component/controller
lifetime (including non-discardable classes), checked activation/audio, callbacks,
complete state, editors and graph integration require subsequent contracts and
real executable evidence. Arbitrary native effects, crashes and allocations are
not contained. See [the guide](../vst3-hosting.md) for bounds, usage and evidence.
