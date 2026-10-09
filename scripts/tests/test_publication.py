"""Publication boundary checks against disposable local Git repositories."""
import importlib.util
import pathlib
import subprocess
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "check-publication.py"
SPEC = importlib.util.spec_from_file_location("publication", SCRIPT)
publication = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(publication)


class PublicationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = pathlib.Path(self.temporary.name)
        self.git("init", "--quiet")

    def git(self, *arguments):
        return subprocess.check_output([
            "git", "-c", "user.name=Publication test", "-c",
            "user.email=publication@example.invalid", *arguments,
        ], cwd=self.root, stderr=subprocess.STDOUT)

    def write(self, path, contents="synthetic fixture only\n"):
        file = self.root / path
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_text(contents)

    def commit(self):
        self.git("add", "--all")
        self.git("commit", "--quiet", "-m", "Synthetic publication fixture")

    def test_clean_history_passes(self):
        self.write("crates/example/src/lib.rs")
        self.commit()
        self.assertEqual(publication.find_private_paths(self.root), set())

    def test_deleted_private_path_still_blocks_publication(self):
        path = "examples/shine/src/fixture.rs"
        self.write(path)
        self.commit()
        (self.root / path).unlink()
        self.commit()
        self.assertIn(path, publication.find_private_paths(self.root))

    def test_rename_of_identical_blob_cannot_hide_private_path(self):
        path = "tools/shine-reference/fixture.cpp"
        self.write(path)
        self.commit()
        self.git("mv", path, "fixture.cpp")
        self.commit()
        self.assertIn(path, publication.find_private_paths(self.root))

    def test_staged_private_path_blocks_before_first_commit(self):
        path = "private/fixture.txt"
        self.write("README.md")
        self.commit()
        self.write(path)
        self.git("add", path)
        self.assertIn(path, publication.find_private_paths(self.root))
