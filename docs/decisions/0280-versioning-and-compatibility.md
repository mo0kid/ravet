# 0280: Versioning and compatibility policy

**Date:** 23 October 2026  
**Status:** Accepted policy. It applies from 0.1.0; enforcement work is tracked in
the [0.1.0 release plan](../releases/0.1.0.md).

## Context

Consumers of the 0.0.1 preview could not tell what would stay stable. They pinned
revisions, re-implemented host-visible behaviour to keep existing sessions loading,
and found that RAVET numbered parameters by declaration order. 0.1.0 is distributed
by immutable Git tag ([decision 0279](0279-release-0-1-0-scope-and-distribution.md)),
so each tag needs a stated meaning.

## Decision

The owner accepted the [versioning and compatibility policy](../compatibility.md):

- All packages share one version and one immutable tag. In 0.x, minor releases may
  break APIs and patch releases may not. Raising the minimum Rust version requires
  a minor release.
- API guarantees follow feature status: implemented APIs are deprecated for at least
  one minor release where practical; prototype APIs may change in any minor release.
- Every saved state or preset format carries a version. Newer releases read state
  from earlier releases from 0.1.0 onward, and reject state they do not understand.
- Parameter IDs come from stable keys supplied by the product, never from
  declaration order, and are never reassigned. Plug-in identities are derived
  deterministically and do not change within a major version.

## Consequences

- Every release needs a changelog with breaking changes and migration steps first.
- The generic export layer must use caller-keyed parameter IDs and versioned state.
  Existing reference formats are brought into line before 0.1.0.
- Changes to identity derivation, ABI layout or state formats need explicit version
  bumps and tests that load earlier releases' state.
