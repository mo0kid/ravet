# 0124: Live documentation portal

Status: implemented locally; CI artifact build configured, public deployment absent.

## Decision

Use MkDocs Material around canonical repository Markdown and generated feature
records, with workspace Rustdoc copied into the same static output. Keep Python
site tooling in an isolated environment under `target/`; it is not a framework
runtime dependency. Stage only guides and explicitly linked evidence, preserving
repository-relative paths and rejecting links that escape the repository.

## Reasons and consequences

Existing guides remain readable in the checkout and need no duplicate site
content. Live previews regenerate the feature catalogue without editing tracked
files; strict builds still require the canonical catalogue to be current.
Guide navigation includes every top-level guide automatically. Rustdoc retains
its own search and layout and is rebuilt when workspace input content changes.
API generation clears cached HTML so removed workspace packages cannot leak into
the site. Guide-only previews explicitly omit APIs. Static artifacts can be hosted beneath
a path prefix without configuring a remote. Public publication remains a separate
review because existing history and publication boundaries matter.

This is an authoring and reference system, not automated proof that feature
semantics match code. Review, tests, Rustdoc and the feature registry remain the
sources of evidence. CI configuration is separate from executed local validation.
