"""Third-party notice generation from Cargo's resolved graph."""
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import third_party_notices


class NoticeTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        (self.root / "LICENSE").write_text("GPL fixture")
        self.registry = self.root / "registry"
        self.crates = {}
        for name, files in (("permissive", {"LICENSE-MIT": "MIT text", "LICENSE-APACHE": "Apache text",
                                            "README.md": "not a notice"}),
                            ("bare", {}), ("ravet-foundation", {"LICENSE": "first party"})):
            directory = self.registry / f"{name}-1.0.0"
            directory.mkdir(parents=True)
            for file, text in files.items():
                (directory / file).write_text(text)
            (directory / "Cargo.toml").write_text("")
            self.crates[name] = directory
        self.commands = []
        self.sysroot = self.root / "sysroot"

    def fake(self, arguments, *, capture=False):
        arguments = [str(a) for a in arguments]
        self.commands.append(arguments)
        if arguments[:2] == ["cargo", "tree"]:
            return "\n".join(f"{name} v1.0.0" for name in ("product", "permissive", "bare", "ravet-foundation")) \
                + "\npermissive v1.0.0\n"
        if arguments[:2] == ["cargo", "metadata"] and "--no-deps" in arguments:
            return json.dumps({"packages": [{"name": "ravet-foundation"}]})
        if arguments[:2] == ["cargo", "metadata"]:
            packages = [{"id": f"{n} 1.0.0", "name": n, "version": "1.0.0", "license": "MIT OR Apache-2.0",
                         "repository": f"https://example.invalid/{n}", "manifest_path": str(d / "Cargo.toml")}
                        for n, d in self.crates.items()]
            packages.append({"id": "product 1.0.0", "name": "product", "version": "1.0.0",
                             "manifest_path": str(self.root / "Cargo.toml")})
            return json.dumps({"packages": packages, "workspace_members": ["product 1.0.0"]})
        if arguments[:2] == ["rustc", "--print"]:
            return str(self.sysroot) + "\n"
        return ""

    def test_lists_linked_third_party_crates_with_their_licence_files(self):
        out = self.root / "out"
        notices = third_party_notices.write(self.root, "product", out, "x86_64-pc-windows-msvc", run=self.fake)
        text = notices.read_text()
        self.assertIn("permissive 1.0.0\nLicence: MIT OR Apache-2.0", text)
        self.assertEqual(text.count("permissive 1.0.0"), 1)
        self.assertIn("--- LICENSE-MIT ---\nMIT text", text)
        self.assertIn("--- LICENSE-APACHE ---\nApache text", text)
        self.assertNotIn("not a notice", text)
        self.assertIn("bare 1.0.0", text)
        self.assertIn("No licence file in the published archive", text)
        self.assertNotIn("ravet-foundation", text)
        self.assertNotIn("product 1.0.0", text)
        self.assertEqual((out / "RAVET-LICENSE.txt").read_text(), "GPL fixture")
        tree = next(c for c in self.commands if c[:2] == ["cargo", "tree"])
        self.assertEqual(tree[tree.index("--target") + 1], "x86_64-pc-windows-msvc")
        self.assertEqual(tree[tree.index("-e") + 1], "normal")

    def test_standard_library_notice_or_pointer(self):
        out = self.root / "pointer"
        third_party_notices.write(self.root, "product", out, run=self.fake)
        self.assertIn("MIT", (out / "RUST-STANDARD-LIBRARY-COPYRIGHT.txt").read_text())
        notice = self.sysroot / "share/doc/rust/COPYRIGHT-library.html"
        notice.parent.mkdir(parents=True)
        notice.write_text("<html>std</html>")
        out = self.root / "html"
        third_party_notices.write(self.root, "product", out, run=self.fake)
        self.assertEqual((out / "RUST-STANDARD-LIBRARY-COPYRIGHT.html").read_text(), "<html>std</html>")


if __name__ == "__main__":
    unittest.main()
