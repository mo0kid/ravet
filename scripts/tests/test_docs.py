"""Regression tests for documentation drift checks; temporary fixtures only."""

import importlib.util
import pathlib
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "check-docs.py"
SPEC = importlib.util.spec_from_file_location("check_docs", SCRIPT)
docs = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(docs)

REGISTRY = '''schema_version = 1

[[feature]]
id = "example"
title = "Example feature"
status = "implemented"
summary = "Example behavior."
limits = "Provisional."
sources = ["crates/example/src/lib.rs"]
tests = ["crates/example/tests/contract.rs"]
docs = ["docs/guide.md"]
'''


class DocumentationChecks(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = pathlib.Path(self.directory.name)
        self.write("Cargo.toml", '[workspace]\nmembers = ["crates/example"]\n')
        self.write("crates/example/Cargo.toml", '[package]\nname = "example"\ndescription = "Example crate"\n')
        self.write("crates/example/src/lib.rs", "// fixture\n")
        self.write("crates/example/tests/contract.rs", "// fixture\n")
        self.write("docs/guide.md", "# Example\n")
        self.write("docs/getting-started.md", "# Getting started\n")
        self.write("docs/WORK_STATUS.md", "# Work status\n")
        self.write("docs/maintenance.md", "# Maintenance\n")
        self.write("docs/features.toml", REGISTRY)

    def write(self, name, content):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    def registry(self, before, after):
        self.write("docs/features.toml", REGISTRY.replace(before, after))

    def test_missing_or_stale_catalogue_fails_without_writing(self):
        errors = docs.check(self.root)
        self.assertTrue(any("is stale" in error for error in errors))
        self.assertFalse((self.root / "docs/FEATURES.md").exists())
        self.assertEqual(docs.check(self.root, write=True), [])
        generated = (self.root / "docs/FEATURES.md").read_text(encoding="utf-8")
        self.registry("Example behavior.", "Changed behavior.")
        self.assertTrue(any("is stale" in error for error in docs.check(self.root)))
        self.assertEqual((self.root / "docs/FEATURES.md").read_text(encoding="utf-8"), generated)

    def test_generation_is_deterministic_and_updates_package_metadata(self):
        self.assertEqual(docs.check(self.root, write=True), [])
        target = self.root / "docs/FEATURES.md"
        initial = target.read_bytes()
        self.assertEqual(docs.check(self.root, write=True), [])
        self.assertEqual(initial, target.read_bytes())
        self.write("crates/example/Cargo.toml", '[package]\nname = "example"\ndescription = "Changed role"\n')
        self.assertTrue(any("is stale" in error for error in docs.check(self.root)))
        self.assertEqual(docs.check(self.root, write=True), [])
        self.assertIn("Changed role", target.read_text(encoding="utf-8"))

    def test_deleted_evidence_fails(self):
        (self.root / "crates/example/tests/contract.rs").unlink()
        with self.assertRaisesRegex(ValueError, "missing or invalid evidence"):
            docs.load_catalogue(self.root)

    def test_duplicate_feature_id_fails(self):
        self.write("docs/features.toml", REGISTRY + REGISTRY.split("\n", 1)[1])
        with self.assertRaisesRegex(ValueError, "duplicate feature ID"):
            docs.load_catalogue(self.root)

    def test_unknown_status_fails(self):
        self.registry('status = "implemented"', 'status = "ready"')
        with self.assertRaisesRegex(ValueError, "unknown status"):
            docs.load_catalogue(self.root)

    def test_working_feature_needs_test_evidence(self):
        self.registry('tests = ["crates/example/tests/contract.rs"]', 'tests = []')
        with self.assertRaisesRegex(ValueError, "require source and test evidence"):
            docs.load_catalogue(self.root)

    def test_planned_feature_can_reference_placeholder_without_tests(self):
        self.write("docs/features.toml", REGISTRY.replace('status = "implemented"', 'status = "planned"').replace(
            'tests = ["crates/example/tests/contract.rs"]', 'tests = []'
        ))
        self.assertEqual(docs.check(self.root, write=True), [])

    def test_new_workspace_package_needs_feature_coverage(self):
        self.write("Cargo.toml", '[workspace]\nmembers = ["crates/*"]\n')
        self.write("crates/new/Cargo.toml", '[package]\nname = "new"\ndescription = "New crate"\n')
        with self.assertRaisesRegex(ValueError, "lacks a feature source reference: crates/new"):
            docs.load_catalogue(self.root)

    def test_excluded_workspace_package_does_not_need_coverage(self):
        self.write("Cargo.toml", '[workspace]\nmembers = ["crates/*"]\nexclude = ["crates/new"]\n')
        self.write("crates/new/Cargo.toml", '[package]\nname = "new"\ndescription = "New crate"\n')
        self.assertEqual(docs.check(self.root, write=True), [])

    def test_repository_escape_and_generated_evidence_are_rejected(self):
        for path in ("../outside.rs", "/tmp/outside.rs", "target/report.json"):
            with self.subTest(path=path):
                self.registry("crates/example/src/lib.rs", path)
                with self.assertRaisesRegex(ValueError, "invalid repository evidence"):
                    docs.load_catalogue(self.root)

    def test_invalid_schema_and_unknown_fields_fail(self):
        self.registry("schema_version = 1", "schema_version = 2")
        with self.assertRaisesRegex(ValueError, "expected schema_version"):
            docs.load_catalogue(self.root)
        self.write("docs/features.toml", REGISTRY + 'typo = "value"\n')
        with self.assertRaisesRegex(ValueError, "must contain exactly"):
            docs.load_catalogue(self.root)

    def test_local_links_include_code_labels_images_and_encoded_spaces(self):
        self.write("docs/with spaces.md", "# Spaces\n")
        self.write("README.md", '''[Guide](docs/guide.md#example)
[`Code label`](docs/missing.md)
![Image](docs/missing.svg)
[Spaces](docs/with%20spaces.md)
[Title](docs/guide.md "optional title")
[Angle](<docs/with spaces.md>)
[Web](https://example.com/missing)
[Section](#missing-anchor)
`[Inline example](docs/ignored.md)`
```md
[Example](docs/ignored.md)
```
~~~md
[Example](docs/ignored.md)
~~~
<!-- [Comment](docs/ignored.md) -->
''')
        self.assertEqual(docs.check_links(self.root), [
            "README.md: broken local link docs/missing.md",
            "README.md: broken local link docs/missing.svg",
        ])


if __name__ == "__main__":
    unittest.main()
