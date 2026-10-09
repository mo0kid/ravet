import importlib.util
from pathlib import Path
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("ravet_new_project", ROOT / "scripts/new-project.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ProjectTemplates(unittest.TestCase):
    def test_independent_manifest_and_existing_destination_are_preserved(self):
        with tempfile.TemporaryDirectory(prefix="ravet-project-fixture-") as temporary:
            destination = Path(temporary) / "consumer"
            MODULE.create_project(destination, "public-offline-fixture", "GPL-3.0-only")
            manifest = tomllib.loads((destination / "Cargo.toml").read_text())
            self.assertEqual(manifest["workspace"], {})
            self.assertFalse(manifest["package"]["publish"])
            self.assertEqual(manifest["package"]["license"], "GPL-3.0-only")
            self.assertEqual(Path(manifest["dependencies"]["ravet"]["path"]), ROOT / "crates/ravet")
            sentinel = destination / "owner.txt"
            sentinel.write_text("preserve existing consumer")
            with self.assertRaises(FileExistsError):
                MODULE.create_project(destination, "another", "GPL-3.0-only")
            self.assertEqual(sentinel.read_text(), "preserve existing consumer")

    def test_invalid_scope_names_and_licence_fail_before_creation(self):
        with tempfile.TemporaryDirectory(prefix="ravet-project-fixture-") as temporary:
            destination = Path(temporary) / "consumer"
            for name, license_id in [("../escape", "GPL-3.0-only"), ("X", "GPL-3.0-only"),
                                     ("x", ""), ("x", "line\nbreak")]:
                with self.assertRaises(ValueError):
                    MODULE.create_project(destination, name, license_id)
                self.assertFalse(destination.exists())
            with self.assertRaises(ValueError):
                MODULE.create_project(ROOT / "target/generated-consumer", "x", "GPL-3.0-only")
