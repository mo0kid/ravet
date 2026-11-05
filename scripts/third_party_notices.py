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
  pointer when the toolchain has no ``rust-docs`` component;
- for Windows products that link ``ravet-webview``: the WebView2 SDK's
  ``LICENSE.txt`` and ``NOTICE.txt`` (from ``RAVET_WEBVIEW2_SDK``), which cover
  the ``WebView2Loader.dll`` shipped beside the product;
- for Linux products that link ``ravet-webview`` or ``alsa-sys``:
  ``SYSTEM-LIBRARIES-LGPL.txt``, naming the LGPL system libraries the product
  loads (GTK 3, WebKitGTK, JSON-GLib, GLib; ALSA), followed by the LGPL 2.1 text
  from the build machine when it has one.

The bundle scripts place these files in every bundle; see
docs/dependency-notices.md. This is notice material, not legal advice.
"""
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys

NOTICE_FILE = re.compile(r"^(LICEN[CS]E|COPYING|COPYRIGHT|NOTICE|UNLICENSE)([-._].*)?$", re.IGNORECASE)
STANDARD_LIBRARY = "RUST-STANDARD-LIBRARY-COPYRIGHT"
STANDARD_LIBRARY_POINTER = """\
The Rust standard library, linked into this product, is distributed under the
terms of both the MIT licence and the Apache License (Version 2.0), with
portions covered by various BSD-like licences. See
https://github.com/rust-lang/rust/blob/master/COPYRIGHT and the
COPYRIGHT-library.html file of the Rust toolchain used for the build.
"""


MIT_TEXT = """\
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
"""


def missing_licence_body(record):
    """Notice for a crate whose archive has no licence file."""
    licence = record.get("license") or ""
    if re.search(r"\bMIT\b", licence):
        holders = ", ".join(a.split(" <")[0] for a in record.get("authors") or []) \
            or f"the {record.get('name')} contributors"
        return ("(The published archive has no licence file. Used here under the MIT option of its\n"
                "declared licence; the notice below is reconstructed from the crate's manifest. See\n"
                "the upstream repository for the authoritative copyright notice.)\n\n"
                f"--- MIT licence ---\nCopyright (c) {holders}\n\n{MIT_TEXT}")
    return "(No licence file in the published archive; see the declared licence and source.)\n"


WEBVIEW2_SDK = "RAVET_WEBVIEW2_SDK"
# Where common distributions install the LGPL 2.1 text (Arch, Debian/Ubuntu, Fedora).
LGPL_TEXTS = ("/usr/share/licenses/spdx/LGPL-2.1-or-later.txt", "/usr/share/common-licenses/LGPL-2.1",
              "/usr/share/licenses/glib2/COPYING")
LINUX_WEB_LIBRARIES = ("GTK 3 and GDK 3", "WebKitGTK 4.1 and JavaScriptCoreGTK (some parts under BSD-style licences; see that package's licence file)", "JSON-GLib 1.0", "GLib, GObject and GIO")
LINUX_LGPL_NOTICE = """\
System libraries used by this product (Linux)

This product does not include the libraries below. It loads them dynamically,
as shared libraries, from the operating system installed on the user's
machine. Each is free software under the GNU Lesser General Public License,
version 2.1 or (at your option) any later version.

{libraries}

You may replace these libraries with modified versions. Nothing in this
product's licence prohibits modifying them, or reverse engineering this
product to the extent needed to debug such modifications. Their source code is
available from your distribution and from each project.
"""


def _target_os(target):
    if target:
        return "windows" if "windows" in target else "linux" if "linux" in target else "darwin" \
            if "apple" in target else "other"
    return {"win32": "windows", "linux": "linux", "darwin": "darwin"}.get(sys.platform, "other")


def system_notices(directory, names, target=None, environ=None, lgpl_texts=LGPL_TEXTS):
    """Notices for native components outside the Cargo graph; returns any reminders."""
    environ = os.environ if environ is None else environ
    names, system, reminders = set(names), _target_os(target), []
    if system == "windows" and "ravet-webview" in names:
        sdk = environ.get(WEBVIEW2_SDK)
        for source, published in (("LICENSE.txt", "WebView2-SDK-LICENSE.txt"), ("NOTICE.txt", "WebView2-SDK-NOTICE.txt")):
            path = pathlib.Path(sdk) / source if sdk else None
            if path and path.is_file():
                shutil.copy2(path, directory / published)
            else:
                reminders.append(f"{published}: set {WEBVIEW2_SDK} to the extracted WebView2 NuGet package "
                                 f"and ship its {source} with WebView2Loader.dll")
    if system == "linux":
        libraries = list(LINUX_WEB_LIBRARIES) if "ravet-webview" in names else []
        if "alsa-sys" in names:
            libraries.append("ALSA library (libasound)")
        if libraries:
            text = LINUX_LGPL_NOTICE.format(libraries="\n".join(f"- {name}" for name in libraries))
            lgpl = next((pathlib.Path(p) for p in lgpl_texts if pathlib.Path(p).is_file()), None)
            if lgpl:
                text += "\n" + "=" * 78 + "\n\n" + lgpl.read_text(errors="replace")
            else:
                text += "\nThe licence text: https://www.gnu.org/licenses/old-licenses/lgpl-2.1.txt\n"
                reminders.append("SYSTEM-LIBRARIES-LGPL.txt: no LGPL 2.1 text on this machine; add it")
            (directory / "SYSTEM-LIBRARIES-LGPL.txt").write_text(text)
    return reminders


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
            body = [missing_licence_body(record)]
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
    text, missing = render(root, package, target, manifest_path=manifest_path, run=run)
    for name in missing:
        print(f"notice reminder: {name} has no licence file in its archive; check its notice in "
              f"{directory / 'THIRD-PARTY-NOTICES.txt'}", file=sys.stderr)
    directory.mkdir(parents=True, exist_ok=True)
    notices = directory / "THIRD-PARTY-NOTICES.txt"
    notices.write_text(text)
    shutil.copy2(root / "LICENSE", directory / "RAVET-LICENSE.txt")
    standard = standard_library_notice(run)
    if standard:
        shutil.copy2(standard, directory / f"{STANDARD_LIBRARY}.html")
    else:
        (directory / f"{STANDARD_LIBRARY}.txt").write_text(STANDARD_LIBRARY_POINTER)
    names = [name for name, _ in linked_packages(run, package, target, manifest_path)]
    for reminder in system_notices(directory, names, target):
        print(f"notice reminder: {reminder}", file=sys.stderr)
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
