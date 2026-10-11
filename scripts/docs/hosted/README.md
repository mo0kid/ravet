# Hosted documentation content

This directory owns the documentation content and rendering tools used by the
hosted RAVET reference at `djwaudio.co/ravet/docs/`. Maintain framework prose,
the beginner introduction, practical guides, tutorial chapters and line notes
in this repository, alongside the Markdown guides, Rustdoc and example sources.
The website repository owns its HTML shell, styles, browser behavior and service
configuration. Formatting requests there do not change the documentation prose.

Install the pinned Python dependencies in an isolated environment:

```sh
python3 -m venv target/hosted-docs-venv
target/hosted-docs-venv/bin/python -m pip install -r scripts/docs/hosted/requirements.txt
```

`ravet_docs.build_docs(source, output, write_page, api_dir=None)` generates the
guides, topic pages, API reference, tutorial walkthroughs, search index,
coverage manifest and `docs/version.json`. The version record names the RAVET
commit, taken from `RAVET_DOCS_COMMIT` when set (CI sets it to the pushed
commit) or from `git rev-parse HEAD` in the source checkout; it is `null` when
neither is available. `ravet_version.py` uses only the standard library.
The publisher supplies `write_page(path, title, description,
body, scripts)` to apply its site shell. The default API build runs Cargo with
all features and warnings denied; an immediately generated all-features Rustdoc
directory can be supplied through `api_dir`.

The default API build documents the workspace twice. Rustdoc links to another
workspace crate only when that crate's output directory exists at render time,
and Cargo empties a crate's directory while documenting it, so a single parallel
run links cross-crate types differently from run to run. The second run, one
crate at a time with every directory present, makes those links complete and
the snapshot byte-reproducible for the same source, toolchain and date. Links
without a target are reduced to text. A directory passed through `api_dir`
skips both runs and carries no such guarantee.

The website publisher reads these modules through its `--source`
checkout. Its output is disposable and excluded from the website's Git history.
Keep the complete build output for local preview or deployment; never edit it as
the source of a documentation change. This publisher produces a static snapshot:
after editing guides or regenerating `docs/FEATURES.md`, rerun the website's
`scripts/build-ravet.py --source ../ravet` before refreshing its local preview.
A PHP or static HTTP server does not rebuild the snapshot automatically.
No core source pages, decision records,
internal handoffs, work logs or draft commercial agreements are published by
this generator. The selected teaching sources are rendered with exact line
numbers and recorded hashes.

The API index includes every crate, module, public symbol and indexed method.
Navigation branch IDs remain stable so the website can retain expansion state.
Code markup exposes syntax classes for the website's Ayu theme.

### Shared navigation

The complete navigation tree is written once, to `docs/nav-<hash>.json`, where
`<hash>` is the first 12 hex digits of the SHA-256 of its content.
`manifest.json` names the file under `navigation`. Because the name changes
whenever the content does, the server may cache it indefinitely. Each page's
`<nav aria-label="Documentation">` carries `data-nav-src` (the file's URL) and
`data-nav-current` (the page URL). The website's `docs-search.js` renders the
tree from the file. The format, built by `ravet_navigation.py`, is
`{"schema": 1, "nodes": [...]}`, where each node is either:

- a link: `h` (URL), `l` (label), optional `t` (tooltip) and `s` (code style);
- a branch: `id` (the stable `data-nav-id`), `m` (summary text or a link node),
  `c` (children), optional `o` (open by default) and `z` (lazy).

A branch opens when it is open by default or when its children, including a
child branch's link summary but not its own, lead to the current page.
`render_nodes(nodes, current, complete=True)` is the reference rendering; the
website script produces the same markup. Each page embeds
`render_nodes(nodes, current)` as a fallback for readers and crawlers without
JavaScript. That fallback includes the guide groups and tutorials, plus the open
path to the current page, such as an API page's crate, module and sibling items.
It omits other closed lazy branches: API crates, modules, item kinds, method
lists and guide topic lists. Keep IDs stable: saved expansion state is keyed by
them. Moving the full tree into this file reduced the snapshot from about
1.8 GB to 90 MB, and the median page from 856 KB to 30 KB.

## Automatic publication

`.github/workflows/hosted-docs.yml` publishes `/ravet/docs/` when a push to
`main` changes documentation inputs: guides, crates, examples, workspace tools,
this directory, Cargo manifests or the licence files. It can also be started
manually with `workflow_dispatch`. Its jobs are:

1. **Build** (macOS): checks out this commit and the website's `main` branch
   through the read-only `WEBSITE_READ_KEY` deploy key, runs the website's
   `scripts/build-ravet.py --source ravet`, then its `tests/ravet.test.js`.
   A newer push cancels an older build.
2. **Publish** (Ubuntu, `hosted-docs` environment): uploads only the generated
   `docs` directory with `publish-hosted-docs.sh` through a docs-only FTPS
   account, checks the live `version.json` commit and a few pages, and restores
   the previous snapshot automatically if that check fails. Publishing is queued,
   never cancelled, so a directory swap cannot be interrupted by a newer run.

A failed build or test publishes nothing. Choosing `rollback` in a manual run
swaps the live and previous snapshots without building.

`publish-hosted-docs.sh` expects the FTPS account's home to be a dedicated
release root outside `public_html`, with `public_html/ravet/docs` a symbolic
link to its `current/` directory. It uploads the snapshot to `next/`, writes
`version.json` last as a completion marker, then renames `current/` to
`previous/` and `next/` to `current/`. The two renames leave a window of
milliseconds with no live snapshot; there is no partially uploaded state. It
refuses symbolic links, hidden files and anything other than static page,
data, style, script, image and licence files, so `.php` and `.htaccess` are
never published. It also refuses a snapshot whose manifest names a navigation
file that is missing. The website's `ravet/.htaccess` refuses script requests
under `/ravet/docs/` on the server; the live check confirms that a `.php`
request returns 403 or 404. The script also runs against a `file:///` URL for local dry
runs.

One-time setup, with names only (values are never stored in either repository):

- RAVET repository secret `WEBSITE_READ_KEY`: private half of a read-only deploy
  key registered on `mo0kid/djwaudio.co`.
- RAVET environment `hosted-docs`, limited to the `main` branch, with secrets
  `DOCS_FTP_HOST`, `DOCS_FTP_USER` and `DOCS_FTP_PASSWORD` for a cPanel FTP
  account whose directory is the release root (for example `ravet-docs`).
- On the server, once: copy the live `public_html/ravet/docs` into
  `ravet-docs/current`, then replace `public_html/ravet/docs` with a symbolic
  link to that `current` directory. The website deployment excludes
  `ravet/docs` and never writes through the link.

A snapshot is about 90 MB in 2,105 files. The server holds up to three copies
during a publish.

Run the documentation checker and the hosted-navigation regression checks:

```sh
python3 scripts/check-docs.py
python3 -m unittest discover -s scripts/tests -p 'test_hosted_docs.py' -v
```

The website tests validate the complete generated output, including links,
fragments, API bookmarks, tutorial source hashes and external Rust references.
