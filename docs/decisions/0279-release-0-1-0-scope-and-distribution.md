# 0279: 0.1.0 format scope and tagged Git distribution

**Date:** 23 October 2026  
**Status:** Accepted plan; not implemented. The criteria are tracked in the
[0.1.0 release plan](../releases/0.1.0.md).

## Context

RAVET 0.0.1 is a source preview. Every plug-in export is a gain-based prototype,
and all 38 packages are `publish = false`. Private consumer products built on
the preview each wrote their own VST3, AU, AAX, CLAP or LV2 wrapper, standalone
device glue and denormal guard, because RAVET provided no supported export layer
or standalone runner.

The consumers also depended on RAVET in incompatible ways: relative paths,
absolute paths, Git revisions and `[patch]` overrides that keep a single copy of
the framework. Several pinned a revision from before the history cleanup. That
revision is not in the new public repository (see the
[publication guide](../publication.md)), so fresh checkouts of those consumers
cannot fetch it. Those manifests also carried version 0.1.0, while the cleaned
preview reset the workspace to 0.0.1.

## Decision

### Format and platform scope

0.1.0 targets five export formats on three desktop platforms, where the format
exists:

| Format | macOS | Windows | Linux |
|---|---|---|---|
| VST3 | Yes | Yes | Yes |
| CLAP | Yes | Yes | Yes |
| LV2 | Yes | Yes | Yes |
| AU (AUv2) | Yes | — | — |
| AAX | Yes | Yes | — |

AU exists only on macOS and AAX has no Linux host, so these are 12 combinations.
Each claimed combination needs host evidence before release, as described in the
release plan. A combination without that evidence ships as unsupported. It is not
dropped silently. AUv3, iOS, Android and embedded targets stay outside the
0.1.0 support claim and retain their existing prototype limits.

### Distribution through immutable Git tags

0.1.0 is distributed as an annotated `v0.1.0` tag on the public repository.
It is not published to crates.io. Consumers depend on the umbrella `ravet`
crate by tag and select components through Cargo features:

```toml
ravet = { git = "https://github.com/mo0kid/ravet", tag = "v0.1.0", features = ["clap", "webview"] }
```

Reasons:

- crates.io versions can be yanked but never deleted. The API is still
  provisional, and 38 packages would multiply every breaking change.
- Each published crate must build from its own package. RAVET's native C++,
  Objective-C and CMake parts make that harder. The AAX SDK cannot be
  redistributed, so AAX support can never build from crates.io alone.
- The commercial agreement, contributor terms and dependency-notice audit are
  unfinished.
- An immutable tag solves the consumers' actual problem of revisions that
  disappear and versions that change meaning.

The public history is not rewritten again, and released tags are never moved.

### Staying ready for crates.io

The workspace stays ready to publish without publishing. Internal dependencies
carry version requirements, and packages carry complete metadata
(`repository`, `documentation`, `readme`, `keywords`, `categories`) and a stated
`rust-version`. A later release can enable publication by changing `publish`,
after the export API has stabilized across releases and licensing is settled.
Reserving crate names with placeholder packages is not part of this plan.

## Consequences

- The generic export layer for the five formats is the core 0.1.0 work package.
  The [release plan](../releases/0.1.0.md) lists its requirements.
- The `release-support` feature record tracks this plan and keeps its `planned`
  status until the criteria are met.
- The version stays 0.0.1 until the release commit. The release changes the
  workspace version and the tag together.
- Windows and Linux host evidence needs real machines or virtual machines and
  hosted CI capacity. Offline tests and cross-compilation do not establish support.
- AAX runtime evidence needs Avid developer access and Pro Tools. Distributing
  AAX binaries additionally requires PACE signing, which remains the consumer's
  responsibility.
