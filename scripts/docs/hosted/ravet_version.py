"""Record which RAVET revision produced a hosted documentation snapshot.

The publisher writes ``docs/version.json`` beside ``manifest.json``. Deployment
checks read it from the live site to confirm that the expected commit is being
served. This module uses only the standard library.
"""
import json
import os
from pathlib import Path
import re
import subprocess

VERSION_FILE = 'version.json'
_COMMIT = re.compile(r'[0-9a-f]{40}')


def source_revision(source, environ=None):
    """Return the 40-character commit for ``source``, or ``None`` if unknown.

    ``RAVET_DOCS_COMMIT`` takes precedence so CI can name the commit it checked
    out. Otherwise ``git rev-parse HEAD`` is used when ``source`` is a checkout.
    Malformed values are ignored rather than published.
    """
    environ = os.environ if environ is None else environ
    explicit = environ.get('RAVET_DOCS_COMMIT', '').strip().lower()
    if explicit:
        return explicit if _COMMIT.fullmatch(explicit) else None
    try:
        output = subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=source, capture_output=True,
                                text=True, check=True, timeout=30).stdout.strip().lower()
    except (OSError, subprocess.SubprocessError):
        return None
    return output if _COMMIT.fullmatch(output) else None


def version_record(manifest, commit):
    """Build the small public record served at ``/ravet/docs/version.json``."""
    return {
        'schema': 1,
        'repository': 'mo0kid/ravet',
        'commit': commit,
        'snapshot': manifest['snapshot'],
        'sourceHash': manifest['sourceHash'],
        'apiPages': manifest['apiPages'],
        'tutorialLines': manifest['tutorialLines'],
    }


def write_version(docdir, manifest, source, environ=None):
    """Write ``version.json`` into ``docdir`` and return the record."""
    record = version_record(manifest, source_revision(source, environ))
    Path(docdir, VERSION_FILE).write_text(json.dumps(record, indent=2) + '\n')
    return record
