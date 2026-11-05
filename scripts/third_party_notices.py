"""Third-party notices for a RAVET product library (Python 3.11+; no global install).

``write(root, package, directory, target=None)`` writes into ``directory``:

- ``THIRD-PARTY-NOTICES.txt``: every third-party crate linked into ``package``
  for ``target`` (a Rust target triple; the host by default), as resolved from
  the lock file, with its version, declared licence, repository and the full
  text of every licence and notice file in its archive. Build-only
  dependencies are excluded, and so are RAVET's own crates;
- ``RAVET-LICENSE.txt``: RAVET's licence (the GPL route; commercial licensees
  follow their agreement);
- the Rust standard library notice: the toolchain's ``COPYRIGHT-library.html``
  (the file the Rust project provides for binaries linking the standard
  library) as ``RUST-STANDARD-LIBRARY-COPYRIGHT.html``, or a short text
  pointer when the toolchain has no ``rust-docs`` component.

The bundle scripts place these files in every bundle; see
docs/dependency-notices.md. This is notice material, not legal advice.
"""
import json
import pathlib
import re
import shutil
import subprocess

NOTICE_FILE = re.compile(r"^(LICEN[CS]E|COPYING|COPYRIGHT|NOTICE|UNLICENSE)([-._].*)?$", re.IGNORECASE)
STANDARD_LIBRARY = "RUST-STANDARD-LIBRARY-COPYRIGHT"
STANDARD_LIBRARY_POINTER = """\
The Rust standard library, linked into this product, is distributed under the
terms of both the MIT licence and the Apache License (Version 2.0), with
portions covered by various BSD-like licences. See
https://github.com/rust-lang/rust/blob/master/COPYRIGHT and the
COPYRIGHT-library.html file of the Rust toolchain used for the build.
"""


def _default_run(root):
    def run(arguments, *, capture=False):
        return subprocess.run([str(a) for a in arguments], cwd=root, check=True,
                              text=True, stdout=subprocess.PIPE if capture else None).stdout
    return run


def linked_packages(run, package, target=None, manifest_path=None):
    """(name, version) of every normal dependency of ``package`` for ``target``."""
    command = ["cargo", "tree", "--locked", "-p", package, "-e", "normal",
               "--prefix", "none", "--format", "{p}"]
    if target:
        command += ["--target", target]
    if manifest_path:
        command += ["--manifest-path", manifest_path]
    packages = set()
    for line in (run(command, capture=True) or "").splitlines():
        fields = line.split()
        if len(fields) >= 2 and fields[1].startswith("v"):
            packages.add((fields[0], fields[1][1:]))
    return sorted(packages)


def notice_files(directory):
    return sorted(p for p in directory.iterdir() if p.is_file() and NOTICE_FILE.match(p.name))


def render(root, package, target=None, *, manifest_path=None, run=None):
    """Notice text, and the crates whose archive holds no licence file."""
    root = pathlib.Path(root)
    run = run or _default_run(root)
    command = ["cargo", "metadata", "--locked", "--format-version", "1"]
    if manifest_path:
        command += ["--manifest-path", manifest_path]
    metadata = json.loads(run(command, capture=True) or "{}")
    ravet = json.loads(run(["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1",
                            "--manifest-path", root / "Cargo.toml"], capture=True) or "{}")
    first_party = {p["name"] for p in ravet.get("packages", [])}
    members = set(metadata.get("workspace_members", []))
    known = {(p.get("name"), p.get("version")): p for p in metadata.get("packages", [])}
    sections, missing = [], []
    for name, version in linked_packages(run, package, target, manifest_path):
        record = known.get((name, version))
        if record is None or record.get("id") in members or name in first_party:
            continue
        files = notice_files(pathlib.Path(record["manifest_path"]).parent)
        header = [f"{name} {version}",
                  f"Licence: {record.get('license') or record.get('license_file') or 'not declared'}"]
        if record.get("repository"):
            header.append(f"Source: {record['repository']}")
        body = [f"--- {file.name} ---\n{file.read_text(errors='replace').rstrip()}\n" for file in files]
        if not files:
            missing.append(name)
            body = ["(No licence file in the published archive; see the declared licence and source.)\n"]
        sections.append("\n".join(header) + "\n\n" + "\n".join(body))
    rule = "\n" + "=" * 78 + "\n\n"
    text = (f"Third-party notices for {package}" + (f" ({target})" if target else "") + "\n\n"
            "This product includes the following third-party software, linked into its\n"
            "library. Each component remains under its own licence, reproduced below.\n"
            "RAVET itself is covered by RAVET-LICENSE.txt, or by your commercial agreement.\n"
            + rule + (rule.join(sections) if sections else "(No third-party crates.)\n") + rule
            + f"Rust standard library\n\nSee {STANDARD_LIBRARY}.html (or .txt) beside this file.\n")
    return text, missing


def standard_library_notice(run):
    """The toolchain's notice for binaries linking the standard library, if installed."""
    sysroot = (run(["rustc", "--print", "sysroot"], capture=True) or "").strip()
    notice = pathlib.Path(sysroot) / "share" / "doc" / "rust" / "COPYRIGHT-library.html"
    return notice if sysroot and notice.is_file() else None


def write(root, package, directory, target=None, *, manifest_path=None, run=None):
    """Write the notices into ``directory``; returns the third-party notices file."""
    root, directory = pathlib.Path(root), pathlib.Path(directory)
    run = run or _default_run(root)
    text, _ = render(root, package, target, manifest_path=manifest_path, run=run)
    directory.mkdir(parents=True, exist_ok=True)
    notices = directory / "THIRD-PARTY-NOTICES.txt"
    notices.write_text(text)
    shutil.copy2(root / "LICENSE", directory / "RAVET-LICENSE.txt")
    standard = standard_library_notice(run)
    if standard:
        shutil.copy2(standard, directory / f"{STANDARD_LIBRARY}.html")
    else:
        (directory / f"{STANDARD_LIBRARY}.txt").write_text(STANDARD_LIBRARY_POINTER)
    return notices


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("package")
    parser.add_argument("directory", type=pathlib.Path)
    parser.add_argument("--target", help="Rust target triple (default: the host)")
    parser.add_argument("--manifest-path", type=pathlib.Path)
    args = parser.parse_args()
    print(write(pathlib.Path(__file__).resolve().parents[1], args.package, args.directory,
                args.target, manifest_path=args.manifest_path))
