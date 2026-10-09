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
guides, topic pages, API reference, tutorial walkthroughs, search index and
coverage manifest. The publisher supplies `write_page(path, title, description,
body, scripts)` to apply its site shell. The default API build runs Cargo with
all features and warnings denied; an immediately generated all-features Rustdoc
directory can be supplied through `api_dir`.

The existing website publisher reads these modules through its `--source`
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

Run the documentation checker and the hosted-navigation regression checks:

```sh
python3 scripts/check-docs.py
python3 -m unittest discover -s scripts/tests -p 'test_hosted_docs.py' -v
```

The website tests validate the complete generated output, including links,
fragments, API bookmarks, tutorial source hashes and external Rust references.
