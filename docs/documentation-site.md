# Live documentation site

The local portal combines all current Markdown guides, the generated feature
catalogue, decision records, validation history and workspace Rustdoc. MkDocs
Material supplies responsive navigation, browser search, light/dark themes,
heading links and copyable code blocks. Assets and fonts are served locally;
there is no analytics service or external search dependency.

## Install and run

Requires Python 3.11 or newer. The complete portal also requires the workspace's
Rust toolchain and native build prerequisites described in [Getting started](getting-started.md).
From the repository root:

```sh
python3 -m venv target/docs-venv
target/docs-venv/bin/python -m pip install -r scripts/docs/requirements.txt
target/docs-venv/bin/python scripts/docs.py serve
```

Open `http://127.0.0.1:8000`. Saving guides, feature records, Rust sources or
manifests rebuilds the preview and refreshes the browser. Rustdoc is rebuilt when
workspace inputs change, with warnings denied. Rustdoc search remains separate
from guide search. A failed build reports the error in the terminal; resolve it
before treating the preview as current. Native build requirements still apply.
The watcher covers guides, crates, examples, native/tool sources, the site tooling,
manifests and entry guides. Restart after changing hook Python code.

For guide editing without a Rust build:

```sh
target/docs-venv/bin/python scripts/docs.py serve --no-api
```

This mode explicitly marks the API as unavailable and excludes older Rustdoc.
Use `--address 127.0.0.1:8001` to select another port. Keep generated output under
`target/`; never edit staged Markdown or generated HTML.

## Build a distributable site

```sh
python3 scripts/check-docs.py --write
target/docs-venv/bin/python scripts/docs.py build
python3 -m http.server 8000 --directory target/docs-site --bind 127.0.0.1
```

The strict build checks canonical documentation first, builds the portal, then
clears obsolete cached API HTML before generating and copying workspace Rustdoc into `target/docs-site/api/`. Serve the complete
`target/docs-site/` directory with any static HTTP host. Paths are relative so
hosting under a project subdirectory works. Guide search requires HTTP rather
than opening HTML with a `file:` URL. Rustdoc has its own navigation and styling.
The documentation CI workflow uploads a combined `ravet-documentation-site`
artifact alongside the existing API artifact. No public deployment is enabled;
publication still requires the [publication review](publication.md).

## Sources and update rules

[The site configuration](../mkdocs.yml) defines presentation and watched paths.
[The hook](../scripts/docs/site.py) stages canonical guides and explicitly linked
source/test evidence. It does not copy the repository wholesale or its Git
history. Linked directories remain repository context rather than browsable
source trees. File evidence can be downloaded; Rustdoc supplies API browsing.
[The command](../scripts/docs.py) provides strict builds and live previews.
[Dependencies](../scripts/docs/requirements.txt) are pinned for isolated tooling;
they do not become Cargo runtime dependencies. MkDocs uses the BSD licence and
Material uses MIT; dependency upgrades need review and a rebuilt site.

The preview regenerates its own catalogue from current registry/manifests without
modifying tracked Markdown. Commit the canonical catalogue by running the checker
with `--write`; strict builds reject a stale tracked catalogue. The site cannot
infer missing feature records or certify prose, compatibility or real-time claims.
Continue the [maintenance review](maintenance.md) with every implementation change.
