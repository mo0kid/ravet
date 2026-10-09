"""MkDocs hooks: stage canonical sources without publishing a repository copy."""
import hashlib
import importlib.util
import os
from pathlib import Path
import re
import shutil
import subprocess
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("ravet_check_docs", ROOT / "scripts/check-docs.py")
checker = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(checker)
LINK = re.compile(r"!?\[[^\]\n]*\]\(\s*(<[^>\n]+>|[^\s)]+)(?:\s+[^)]+)?\)")
_api_fingerprint = None


def stage(root, destination):
    """Copy guides and explicitly linked evidence, rejecting escaping symlinks."""
    root = root.resolve()
    destination.mkdir(parents=True, exist_ok=True)
    features, packages = checker.load_catalogue(root)
    pending = set(root.glob("*.md")) | set((root / "docs").rglob("*.md"))
    content = {root / "docs/FEATURES.md": checker.render_catalogue(features, packages)}
    seen = set()
    while pending:
        source = pending.pop()
        resolved = source.resolve()
        if not resolved.is_relative_to(root):
            raise ValueError(f"documentation source escapes repository: {source}")
        relative = source.relative_to(root)
        if "target" in relative.parts or ".git" in relative.parts or "private" in relative.parts:
            raise ValueError(f"documentation link enters excluded directory: {relative}")
        if source in seen or not source.is_file():
            continue
        seen.add(source)
        output = destination / relative
        output.parent.mkdir(parents=True, exist_ok=True)
        if source.suffix == ".md":
            text = content.get(source, source.read_text(encoding="utf-8"))
            output.write_text(text, encoding="utf-8")
            for match in LINK.finditer(checker.prose_only(text)):
                link = urlsplit(match[1].strip("<>"))
                if not link.scheme and not link.netloc and link.path:
                    linked = Path(os.path.normpath(source.parent / unquote(link.path)))
                    if not linked.resolve().is_relative_to(root):
                        raise ValueError(f"documentation link escapes repository: {linked}")
                    if linked.is_file():
                        pending.add(linked)
        else:
            shutil.copyfile(source, output)
    # Remove stale evidence and pages after deleted links/files.
    expected = {p.relative_to(root) for p in seen}
    for path in destination.rglob("*"):
        if path.is_file() and path.relative_to(destination) not in expected:
            path.unlink()
    return features, packages


def on_pre_build(config):
    global _api_fingerprint
    features, packages = stage(ROOT, Path(config["docs_dir"]))
    if os.environ.get("RAVET_DOCS_API", "1") == "1":
        inputs = [ROOT / "Cargo.toml", ROOT / "Cargo.lock"]
        for directory in ("crates", "examples", "native", "tools"):
            inputs.extend(p for p in (ROOT / directory).rglob("*") if p.is_file())
        digest = hashlib.sha256()
        for path in sorted(inputs):
            digest.update(str(path.relative_to(ROOT)).encode())
            digest.update(path.read_bytes())
        fingerprint = digest.hexdigest()
        if fingerprint != _api_fingerprint:
            # Cargo retains HTML for removed packages and examples. Never copy
            # cached references from an older workspace into the portal.
            api_cache = ROOT / "target/doc"
            if api_cache.exists():
                shutil.rmtree(api_cache)
            env = dict(os.environ, RUSTDOCFLAGS="-D warnings", CARGO_TARGET_DIR=str(ROOT / "target"))
            subprocess.run(["cargo", "doc", "--workspace", "--no-deps", "--locked"], cwd=ROOT, env=env, check=True)
            _api_fingerprint = fingerprint
    config["nav"] = [
        {"Start": [{"Overview": "docs/README.md"}, {"Getting started": "docs/getting-started.md"},
                   {"Project starters": "docs/project-starters.md"}, {"Architecture": "docs/architecture.md"}]},
        {"Guides": [{p.stem.replace("-", " ").capitalize(): str(p.relative_to(ROOT))}
                    for p in sorted((ROOT / "docs").glob("*.md"))
                    if p.name not in {"README.md", "FEATURES.md", "WORK_STATUS.md", "getting-started.md", "project-starters.md", "architecture.md", "maintenance.md", "documentation-site.md"}]},
        {"Features": "docs/FEATURES.md"},
        {"API": "api-reference.md"},
        {"Development": [{"Contributing": "CONTRIBUTING.md"}, {"Documentation system": "docs/documentation-site.md"},
                         {"Maintenance": "docs/maintenance.md"}, {"Work status": "docs/WORK_STATUS.md"},
                         {"Decisions": "docs/decisions/README.md"}]},
    ]
    staged = Path(config["docs_dir"])
    api_text = "# API reference\n\nRustdoc is generated from this checkout with warnings denied. Each crate has its own API search.\n\n"
    if os.environ.get("RAVET_DOCS_API", "1") == "1":
        for name, description, _ in packages:
            crate = name.replace("-", "_")
            if (ROOT / "target/doc" / crate / "index.html").exists():
                api_text += f'<p><a href="../api/{crate}/index.html">{name}</a> — {description}</p>\n'
    else:
        api_text += "API generation is disabled in this preview. Restart without `--no-api` to build and include Rustdoc.\n"
    (staged / "api-reference.md").write_text(api_text, encoding="utf-8")


def on_post_build(config):
    output = Path(config["site_dir"]) / "api"
    if output.exists():
        shutil.rmtree(output)
    if os.environ.get("RAVET_DOCS_API", "1") == "1":
        shutil.copytree(ROOT / "target/doc", output)


def on_serve(server, config, builder):
    # Canonical inputs are watched in mkdocs.yml. Watching our generated staging
    # directory would make every rebuild trigger another rebuild indefinitely.
    server.unwatch(config["docs_dir"])
    return server
