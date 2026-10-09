"""Documentation staging boundary and catalogue freshness regression tests."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("docs_site", Path(__file__).resolve().parents[1] / "docs/site.py")
site = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(site)


class SiteStaging(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.output = self.root / "target/stage"
        self.write("Cargo.toml", '[workspace]\nmembers = ["crates/demo"]\n')
        self.write("crates/demo/Cargo.toml", '[package]\nname = "demo"\ndescription = "Demo"\n')
        self.write("crates/demo/src/lib.rs", "// evidence\n")
        self.write("crates/demo/tests/check.rs", "// test\n")
        self.write("docs/README.md", "# Guide\n[Source](../crates/demo/src/lib.rs)\n")
        self.write("docs/FEATURES.md", "stale\n")
        self.write("docs/features.toml", '''schema_version = 1
[[feature]]
id = "demo"
title = "Demo"
status = "implemented"
summary = "Current behavior."
limits = "Limited."
sources = ["crates/demo/src/lib.rs"]
tests = ["crates/demo/tests/check.rs"]
docs = ["docs/README.md"]
''')

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def test_current_catalogue_without_mutating_checkout_or_copying_unlinked_files(self):
        self.write("unlinked-secret.txt", "not documentation")
        site.stage(self.root, self.output)
        self.assertIn("Current behavior.", (self.output / "docs/FEATURES.md").read_text())
        self.assertEqual((self.root / "docs/FEATURES.md").read_text(), "stale\n")
        self.assertTrue((self.output / "crates/demo/src/lib.rs").exists())
        self.assertFalse((self.output / "unlinked-secret.txt").exists())

    def test_deleted_page_and_unlinked_evidence_are_removed(self):
        self.write("docs/old.md", "[Old](../old.txt)")
        self.write("old.txt", "old")
        site.stage(self.root, self.output)
        (self.root / "docs/old.md").unlink()
        site.stage(self.root, self.output)
        self.assertFalse((self.output / "docs/old.md").exists())
        self.assertFalse((self.output / "old.txt").exists())

    def test_symlink_outside_repository_is_rejected(self):
        with tempfile.TemporaryDirectory() as external:
            secret = Path(external) / "secret.txt"
            secret.write_text("private")
            (self.root / "linked.txt").symlink_to(secret)
            self.write("docs/README.md", "[Link](../linked.txt)")
            with self.assertRaisesRegex(ValueError, "escapes repository"):
                site.stage(self.root, self.output)

    def test_generated_directory_is_rejected(self):
        self.write("target/secret.txt", "generated")
        self.write("docs/README.md", "[Link](../target/secret.txt)")
        with self.assertRaisesRegex(ValueError, "excluded directory"):
            site.stage(self.root, self.output)

    def test_live_server_watches_canonical_inputs_without_generated_rebuild_loop(self):
        from unittest.mock import Mock
        server = Mock()
        config = {"docs_dir": str(self.output)}
        self.assertIs(site.on_serve(server, config, builder=None), server)
        server.unwatch.assert_called_once_with(str(self.output))

    def test_guide_only_build_removes_stale_api(self):
        from unittest.mock import patch
        api = self.output / "api"
        api.mkdir(parents=True)
        (api / "index.html").write_text("stale API")
        with patch.dict(site.os.environ, {"RAVET_DOCS_API": "0"}):
            site.on_post_build({"site_dir": str(self.output)})
        self.assertFalse(api.exists())
