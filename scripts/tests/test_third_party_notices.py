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
        self.assertIn("published archive has no licence file", text)
        self.assertIn("Copyright (c) the bare contributors", text)
        self.assertIn("Permission is hereby granted", text)
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

    def test_missing_licence_file_without_mit_option_keeps_a_pointer(self):
        body = third_party_notices.missing_licence_body({"name": "x", "license": "Zlib OR Apache-2.0"})
        self.assertIn("No licence file", body)
        self.assertNotIn("Permission is hereby granted", body)
        body = third_party_notices.missing_licence_body(
            {"name": "y", "license": "Zlib OR Apache-2.0 OR MIT", "authors": ["A Person <a@example.invalid>"]})
        self.assertIn("Copyright (c) A Person\n", body)

    def test_windows_editor_ships_webview2_sdk_notices(self):
        sdk = self.root / "webview2"
        sdk.mkdir()
        (sdk / "LICENSE.txt").write_text("WebView2 BSD")
        (sdk / "NOTICE.txt").write_text("WebView2 third parties")
        out = self.root / "windows"
        out.mkdir()
        reminders = third_party_notices.system_notices(out, ["ravet-webview"], "x86_64-pc-windows-msvc",
                                                       environ={"RAVET_WEBVIEW2_SDK": str(sdk)})
        self.assertEqual(reminders, [])
        self.assertEqual((out / "WebView2-SDK-LICENSE.txt").read_text(), "WebView2 BSD")
        self.assertEqual((out / "WebView2-SDK-NOTICE.txt").read_text(), "WebView2 third parties")
        missing = self.root / "missing"
        missing.mkdir()
        reminders = third_party_notices.system_notices(missing, ["ravet-webview"], "x86_64-pc-windows-msvc", environ={})
        self.assertEqual(len(reminders), 2)
        self.assertEqual(list(missing.iterdir()), [])
        self.assertEqual(third_party_notices.system_notices(missing, ["serde_json"], "x86_64-pc-windows-msvc",
                                                            environ={}), [])

    def test_linux_lists_lgpl_system_libraries_only_when_linked(self):
        lgpl = self.root / "LGPL-2.1"
        lgpl.write_text("GNU LESSER GENERAL PUBLIC LICENSE Version 2.1")
        out = self.root / "linux"
        out.mkdir()
        reminders = third_party_notices.system_notices(out, ["ravet-webview", "alsa-sys"],
                                                       "x86_64-unknown-linux-gnu", lgpl_texts=(str(lgpl),))
        text = (out / "SYSTEM-LIBRARIES-LGPL.txt").read_text()
        self.assertEqual(reminders, [])
        for name in ("WebKitGTK 4.1", "GTK 3", "JSON-GLib", "ALSA library", "Version 2.1", "reverse engineering"):
            self.assertIn(name, text)
        audio = self.root / "audio"
        audio.mkdir()
        reminders = third_party_notices.system_notices(audio, ["alsa-sys"], "x86_64-unknown-linux-gnu",
                                                       lgpl_texts=())
        text = (audio / "SYSTEM-LIBRARIES-LGPL.txt").read_text()
        self.assertNotIn("WebKitGTK", text)
        self.assertIn("lgpl-2.1.txt", text)
        self.assertEqual(len(reminders), 1)
        none = self.root / "none"
        none.mkdir()
        third_party_notices.system_notices(none, ["ravet-webview"], "aarch64-apple-darwin")
        third_party_notices.system_notices(none, ["serde_json"], "x86_64-unknown-linux-gnu")
        self.assertEqual(list(none.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
