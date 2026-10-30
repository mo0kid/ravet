# Versioning and compatibility policy

**Status:** Accepted on 23 October 2026 ([decision 0280](decisions/0280-versioning-and-compatibility.md)).
It applies from 0.1.0 onward. Existing 0.0.1 formats and parameter numbering
are not covered; the 0.1.0 export layer brings them into line.

This policy states what RAVET consumers can rely on between releases. It covers
two areas: the Rust and native APIs, and the data that products save or expose
to hosts. The [feature catalogue](FEATURES.md) status of each capability decides
which rules apply to it.

## Releases and versions

- All workspace packages share one version and are released together under one
  immutable Git tag, `vMAJOR.MINOR.PATCH`. Released tags are never moved, and
  the public history is never rewritten.
- During 0.x, Cargo's rules for leading-zero versions apply. A **minor** release
  (0.1 → 0.2) may contain breaking changes. A **patch** release (0.1.0 → 0.1.1)
  contains only compatible fixes and additions.
- Raising the minimum supported Rust version (`rust-version`) happens only in a
  minor release, and the changelog states it.
- Every release has a changelog entry. Breaking changes are listed first, each
  with migration steps.

## API stability by feature status

| Feature status | Guarantee |
|---|---|
| `implemented` | Breaking changes only in a minor release. Where practical, the old API is first marked `#[deprecated(since = "…", note = "…")]` for at least one minor release before removal. |
| `prototype` | May change in any minor release without a deprecation period. The changelog still lists the change. |
| `planned`, `excluded` | No API is promised. |

A patch release never breaks an API, whatever its status. Native C ABIs and
headers follow the same rules as the Rust API they expose. ABI version numbers
change whenever the layout or calling contract changes.

## Saved state and presets

- Every state or preset format written by RAVET carries an explicit format
  version.
- A newer release reads state written by any earlier release from 0.1.0 onward,
  for formats of `implemented` features. Older releases need not read newer
  state, and must reject it rather than misread it.
- `prototype` formats may change between minor releases. They still reject
  versions they do not understand, and never silently reinterpret them.
- Products that must read state written by other frameworks supply import hooks.
  RAVET does not guess at foreign formats.

## Host-visible identities

These values are stored in host sessions. Changing them breaks existing sessions,
so RAVET treats them as product data, not implementation details:

- **Parameter IDs** come from stable keys the product supplies, never from
  declaration order. RAVET never renumbers or reassigns a released parameter ID.
  Removing a parameter retires its ID permanently.
- **Plug-in identities** (CLAP ID, VST3 class IDs, AU type/subtype/manufacturer,
  AAX product and plug-in IDs, LV2 URIs) are derived deterministically from
  product settings. The derivation does not change within a major version. If it
  ever must change, the release provides a way to keep the old identities.
- **Bus layouts and parameter ranges** of a released product are the product's
  responsibility. RAVET provides validation that warns when they change.

## Out of scope

This policy does not promise support for any platform, host or device; the
support matrix covers that. It does not cover private consumer products, and it
does not grant any licence; see [LICENSING.md](../LICENSING.md).
