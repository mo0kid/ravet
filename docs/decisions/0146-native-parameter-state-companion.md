# 0146: Native global schemas and state companion

Status: reusable schema/state companion and thin public Gain native facades
implemented and validated with Rust/C/C++ and rebuilt native bundles. Generic SDK registration/live
publication/editor infrastructure remains separate work.

Provide an optional companion table for the exact native processor facade type,
sample precision and immutable bounded Foundation parameter schema. A consumer
trait supplies stable product/version, complete snapshots and atomic restore;
portable DSP does not gain a native-backend dependency. Gain-native defines thin
facades around both portable precisions and exposes paired base/companion symbols.

Use canonical fixed global f32 metadata with full u32 IDs, linear/log scale,
continuous/discrete ranges, defaults, automatable/bypass/read-only roles and bounded
UTF-8 name/unit fields. Reject oversized/embedded-NUL text instead of truncating.
Roles are metadata declarations, not implicit event filtering or state permissions.
Pure range mapping follows Foundation clamp/snap behavior without hidden UI policy.

State calls require exclusive stopped ownership. Revalidate complete provider
snapshots against declared IDs/ranges and reorder by IDs before native output;
validate complete native state before invoking consumer restore. Default persisted
records use the existing Foundation little-endian product/version/count/ID-value
codec. Explicit consumer decoding can validate/migrate legacy records within the
8208-byte cap; Gain retains its complete-v2 and legacy-v1 decoding. Save/snapshot
preflight destinations and preserve tails. Consumer restore must reject atomically;
no arbitrary user-code rollback or live audio publication is inferred.

Metadata acquisition rejects invalid bounds/version/text or provider panic without
creating a companion. Snapshot/restore panics poison the shared processor owner
until fresh stopped preparation. Keep foreign ranges, table/handle pairing and
loaded-library lifetime explicit. SDK controllers still need format registration,
scoped flags, formatting/groups/programs and live control/audio publication policy.
