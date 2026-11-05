# 0289: Versioned observable-tree records and the 0.1.0 state inventory

**Date:** 5 November 2026  
**Status:** Implemented in `ravet-state`, `ravet-preset` and `ravet-foundation`;
offline tests on macOS arm64.

## Context

The [compatibility policy](../compatibility.md) requires every saved format to
carry a version, reject versions it does not understand, and identify parameters
by stable product IDs. An inventory of the supported surface
([upgrade notes](../releases/0.1.0-upgrade.md)) found that the plug-in envelope,
Foundation parameter records, native tables, reference examples and generic
adapters already met it. The exception was the observable-tree record
`RVETTREE` used by `ravet-state` and `.ravetpreset` files. It carried a record
version, but every unknown version failed as a generic `InvalidRecord`, and it
had no place for the product's own schema version. A product that changed its
tree layout could not tell old presets from current ones.

## Decision

- `RVETTREE` format 2 adds a nonzero u32 product schema version after the
  record format version. RAVET writes format 2 only.
- Decoding rejects record formats other than 1 and 2 with
  `StateError::UnsupportedVersion`, and schemas newer than the caller's current
  schema with `StateError::UnsupportedSchema`. Neither is reinterpreted.
- Format 1 (RAVET 0.0.1) still decodes, as schema 1, so existing preset files
  keep loading. The existing `encode`, `decode` and `restore` work at schema 1.
- Products migrate older schemas in their own code: `decode_schema` returns the
  saved schema, and `restore_migrating` / `load_into_migrating` pass an older
  tree to a hook and apply the result as one validated, undoable transaction.
- Foundation parameter records keep their layout; `parameter_record_version`
  lets a product read the version before choosing a schema to decode with.
- LV2 port indices remain in declaration order, because LV2 fixes ports by
  index per plug-in URI; symbols come from stable keys. The upgrade notes state
  that parameters must be appended to keep released LV2 indices.

## Consequences

- 0.0.1 cannot read presets written by 0.1.0; the policy allows this.
- `StateError` gained two variants and `Limits::bytes` must be at least 24.
- Tests pin the format and schema fields, rejection of newer formats and schemas
  without touching live state, a hand-built 0.0.1 record, and atomic migration.
