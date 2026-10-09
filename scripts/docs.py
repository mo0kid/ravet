#!/usr/bin/env python3
"""Build or serve the documentation portal. Requires scripts/docs/requirements.txt."""
import argparse
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("build", "serve"))
    parser.add_argument("--no-api", action="store_true", help="guides-only preview; omit Cargo/Rustdoc")
    parser.add_argument("--address", default="127.0.0.1:8000", help="live server address")
    args = parser.parse_args()
    if args.command == "build":
        subprocess.run([sys.executable, "scripts/check-docs.py"], cwd=ROOT, check=True)
    env = dict(os.environ, RAVET_DOCS_API="0" if args.no_api else "1")
    (ROOT / "target/docs-source").mkdir(parents=True, exist_ok=True)
    command = [sys.executable, "-m", "mkdocs", args.command, "--config-file", str(ROOT / "mkdocs.yml")]
    command += ["--strict"] if args.command == "build" else ["--dev-addr", args.address]
    return subprocess.call(command, cwd=ROOT, env=env)


if __name__ == "__main__":
    sys.exit(main())
