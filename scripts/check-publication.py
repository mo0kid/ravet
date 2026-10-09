#!/usr/bin/env python3
"""Reject known proprietary consumer paths in tracked files and Git history."""
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
PRIVATE_PREFIXES = ("examples/shine/", "tools/shine-reference/", "private/", "ravet-shine-port/")
PRIVATE_FILES = {"scripts/compare-shine.py"}


def private(path):
    return path in PRIVATE_FILES or path.startswith(PRIVATE_PREFIXES)


def find_private_paths(root):
    tracked = subprocess.check_output(["git", "ls-files", "-z"], cwd=root).decode().split("\0")
    # Object enumeration reports only one path per object. A later rename can
    # hide the proprietary path when the same blob is reachable under both
    # names. Enumerate commit changes, including the root and deleted paths.
    history = subprocess.check_output([
        "git", "log", "--all", "--full-history", "--root", "--no-renames",
        "--format=", "--name-only", "-z",
    ], cwd=root).decode().split("\0")
    paths = {path for path in tracked if private(path)}
    for entry in history:
        path = entry.strip("\n")
        if private(path):
            paths.add(path)
    return paths


def main():
    paths = find_private_paths(ROOT)
    if paths:
        print("PUBLICATION BLOCKED: private consumer paths remain tracked or reachable in Git history:", file=sys.stderr)
        for path in sorted(paths):
            print(f"  {path}", file=sys.stderr)
        print("Keep the remote private. Review docs/publication.md before publishing clean history.", file=sys.stderr)
        return 1
    print("Publication path check passed. Review file contents and dependency provenance separately.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
