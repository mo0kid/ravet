# Maintaining RAVET documentation

Documentation changes travel with the code they describe. The author of a change
owns its documentation update, and the reviewer checks that feature status,
examples, and support claims match the resulting behavior.

## Sources of truth

| Information | Maintain here | Derived or linked views |
|---|---|---|
| Public APIs and exact contract semantics | Rustdoc beside code, supported by tests | Cargo API reference |
| Capability status, scope, limits, source, tests | [features.toml](features.toml) | Generated [FEATURES.md](FEATURES.md) |
| Package names, descriptions, membership | Cargo manifests | Catalogue workspace table |
| Setup and usage commands | [Getting started](getting-started.md) | README links |
| Crate boundaries and ownership | [Architecture](architecture.md) | Decision and API links |
| Significant choices and rationale | [Decision records](decisions/README.md) | Architecture and feature references |
| Executed validation and work packages | [WORK_STATUS.md](WORK_STATUS.md) | Links from guides |
| Licence policy | [LICENSING.md](../LICENSING.md) and [LICENSE](../LICENSE) | README summary and distribution decision |

The root README remains a short project entry point. The handoff preserves
planning history; historical snapshots are not current feature promises.
When current summaries disagree with code, correct the summaries and review the
underlying contract rather than treating the old prose as evidence of support.

## Feature records

Each `[[feature]]` record has a stable `id`, a human-readable `title`, `status`,
`summary`, `limits`, and repository-relative `sources`, `tests`, and `docs` lists.
Paths name existing files; they do not point into generated `target/` output.
Every record links at least one guide or decision. Implemented and prototype
records also require source and test evidence. Each workspace package must be
represented by at least one source reference, including placeholders.

| Status | Meaning |
|---|---|
| `implemented` | Working behavior within the stated scope; provisional APIs and support limits still apply |
| `prototype` | Working integration with further feasibility or compatibility validation needed |
| `planned` | Intended work; no usable implementation claimed |
| `deferred` | Outside the current milestone; revisit when product needs justify it |
| `excluded` | Explicitly outside project scope |

For a new feature, add a record following an adjacent example, connect the source,
tests, and guide, and describe the user-visible behavior and limitations. Split a
record when capabilities need different statuses. For removals, retain the stable
ID when useful for explaining the scope change; update status and references so
the catalogue cannot continue advertising removed behavior.

Regenerate and verify from the workspace root:

```sh
python3 scripts/check-docs.py --write
python3 scripts/check-docs.py
```

Generation is deterministic and adds no timestamp. The check command makes no
changes and exits nonzero for invalid metadata, missing evidence files, missing
package coverage, a stale catalogue, or broken local inline Markdown links in
root Markdown files and Markdown under `docs/`.
Links in code fences and inline code are ignored. Markdown anchors, reference
links, external URLs, and semantic accuracy of prose require review. This check
cannot infer new features or behavioral changes from Rust source automatically.

## Review a code change

1. Compare user-visible behavior, API semantics, defaults, limits, dependencies,
   and commands with the affected guides and Rustdoc.
2. Update the feature registry and regenerate the catalogue when capability or
   package metadata changes. Check source and test references after refactors.
3. Add or update decision records for significant choices. Record new executed
   validation in work status, including the environment and remaining limits.
4. Run the documentation checker. Test Rustdoc examples and build with warnings
   denied when changing API documentation or examples.

Pure refactors can leave explanatory prose unchanged after this review; state
that conclusion in the change description. The pull request template prompts
this review, and AGENTS.md applies the same rule to future coding sessions.

## Continuous checks and reference artifacts

[The documentation workflow](../.github/workflows/documentation.yml) runs on
pushes, pull requests, and manual dispatch. It checks the registry, generated
catalogue, local links, and checker regression tests. It also tests Rustdoc
examples and builds API HTML with warnings denied on a macOS runner. The
`ravet-api-reference` artifact contains the resulting API reference; download it
and open `ravet/index.html` locally. Actions are pinned to commits from the
official [checkout](https://github.com/actions/checkout),
[setup-python](https://github.com/actions/setup-python), and
[upload-artifact](https://github.com/actions/upload-artifact) repositories.

The workflow becomes active when this repository is pushed to GitHub with
Actions enabled. Repository administrators can then make the documentation jobs
required checks. The [live portal](documentation-site.md) builds the guides and API reference into a
combined static artifact, with strict link validation and a local preview watcher.
Public deployment remains unconfigured. CI configuration alone is not platform support
evidence or an executed validation result.
