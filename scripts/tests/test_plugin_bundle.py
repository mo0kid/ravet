"""Unified bundler contracts: discovery, selection, one product build and vendor hooks."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import plugin_bundle as bundle
import plugin_sdks as sdks


class Runner:
    def __init__(self, target):
        self.target, self.calls, self.environment = target, [], {}

    def run(self, arguments, **kwargs):
        args = list(map(str, arguments))
        self.calls.append(args)
        if args[:2] == ["cargo", "metadata"]:
            return json.dumps({"target_directory": str(self.target), "packages": [
                {"name": "my-product", "targets": [{"name": "custom_library", "crate_types": ["cdylib", "rlib"]}]}]})
        return "note: native-static-libs: -lSystem -lc\n"


class BundleTests(unittest.TestCase):
    def setUp(self):
        # Whole-bundle tests use a fake project; notice generation is covered below.
        stub = patch.object(bundle.third_party_notices, "write", lambda *a, **k: None)
        stub.start()
        self.addCleanup(stub.stop)

    def test_notices_ship_with_every_format(self):
        artifact = Path("/out/my-product")
        expected = {
            ("clap", "darwin"): Path("/out/my-product.clap/Contents/Resources/notices"),
            ("clap", "linux"): Path("/out/my-product notices"),
            ("clap", "win32"): Path("/out/my-product notices"),
            ("vst3", "win32"): Path("/out/my-product.vst3/Contents/Resources/notices"),
            ("au", "darwin"): Path("/out/my-product.component/Contents/Resources/notices"),
            ("aax", "darwin"): Path("/out/my-product.aaxplugin/Contents/Resources/notices"),
            ("lv2", "linux"): Path("/out/my-product.lv2"),
        }
        for (format, platform), directory in expected.items():
            path = artifact.with_suffix("." + bundle.SUFFIX[format])
            self.assertEqual(bundle.notices_directory(path, format, platform), directory, (format, platform))
        calls = []
        with tempfile.TemporaryDirectory() as temporary:
            clap = Path(temporary) / "my-product.clap"
            written = bundle.write_notices(clap, "clap", "linux", "my-product", None, Runner(Path(temporary)), "windows",
                                           writer=lambda *a, **k: calls.append((a, k)))
            self.assertEqual(written, Path(temporary) / "my-product notices")
            self.assertEqual(calls[0][0][1:4], ("my-product", written, bundle.windows_cross.TRIPLE))
            # An adapter that already wrote notices is left alone.
            written.mkdir()
            (written / "THIRD-PARTY-NOTICES.txt").write_text("x")
            bundle.write_notices(clap, "clap", "linux", "my-product", None, Runner(Path(temporary)), None,
                                 writer=lambda *a, **k: calls.append((a, k)))
            self.assertEqual(len(calls), 1)

    def test_universal_notices_cover_both_target_dependency_graphs(self):
        calls = []
        runner = Runner(Path("/target"))
        with tempfile.TemporaryDirectory() as temporary:
            artifact = Path(temporary) / "product.lv2"
            artifact.mkdir()
            (artifact / "THIRD-PARTY-NOTICES.txt").write_text("host-only adapter notices")
            bundle.write_notices(artifact, "lv2", "darwin", "my-product", None, runner, None,
                                 arch="universal", writer=lambda *a, **k: calls.append(a))
        self.assertEqual(calls[0][3], ("aarch64-apple-darwin", "x86_64-apple-darwin"))
        def tree(arguments, **kwargs):
            return "shared v1.0.0\n" + ("intel v2.0.0\n" if arguments[-1].startswith("x86") else "arm v3.0.0\n")
        self.assertEqual(bundle.third_party_notices.linked_packages(tree, "my-product", calls[0][3]),
                         [("arm", "3.0.0"), ("intel", "2.0.0"), ("shared", "1.0.0")])

    def test_argument_selection(self):
        args = bundle.parser().parse_args(["--package", "my-product", "--formats", "lv2,clap", "--release"])
        self.assertEqual(args.formats, ["lv2", "clap"])
        self.assertTrue(args.release)
        self.assertEqual(bundle.parser().parse_args(["--package", "my-product"]).formats, ["clap"])
        for selection in ("", "clap,clap", "vst2", "clap,unknown", "clap,"):
            with self.assertRaises(Exception):
                bundle.formats(selection)

    def test_macos_arch_defaults_and_platform_rejection(self):
        args = bundle.parser().parse_args(["--package", "my-product"])
        with patch.object(bundle.sys, "platform", "darwin"), patch.object(bundle.host_platform, "machine", return_value="x86_64"):
            self.assertEqual(bundle.macos_arch(args), "x86_64")
        args.arch = "universal"
        self.assertEqual(bundle.cmake_arch(args.arch), ["-DCMAKE_OSX_ARCHITECTURES=arm64;x86_64"])
        for platform, target in (("linux", None), ("win32", None), ("darwin", "windows")):
            args.target = target
            with self.assertRaisesRegex(ValueError, "--arch requires macOS"):
                bundle.preflight(args, {}, platform)

    def test_macos_rust_targets_and_universal_lipo(self):
        for arch in ("arm64", "x86_64", "universal"):
            with tempfile.TemporaryDirectory() as temporary:
                args = bundle.parser().parse_args(["--package", "my-product", "--arch", arch,
                                                  "--formats", "clap,aax", "--release"])
                runner = Runner(Path(temporary))
                with patch.object(bundle.sys, "platform", "darwin"):
                    _, library, archive, _ = bundle.build_product(args, runner)
                builds = [c for c in runner.calls if c[:2] == ["cargo", "rustc"]]
                triples = (["aarch64-apple-darwin", "x86_64-apple-darwin"] if arch == "universal" else
                           ["aarch64-apple-darwin" if arch == "arm64" else "x86_64-apple-darwin"])
                self.assertEqual([c[c.index("--target") + 1] for c in builds], triples)
                self.assertTrue(all(c.index("--target") < c.index("--") for c in builds))
                self.assertTrue(all("cdylib,staticlib" in c for c in builds))
                lipos = [c for c in runner.calls if c[0] == "lipo"]
                self.assertEqual(len(lipos), 2 if arch == "universal" else 0)
                if arch == "universal":
                    self.assertEqual([Path(c[-1]) for c in lipos], [library, archive])
                    self.assertIn("universal-apple-darwin", str(library))

    def test_universal_validation_selects_both_slices(self):
        runner = Runner(Path("/target"))
        bundle.validate("clap", Path("/out/product.clap"), runner, arch="universal",
                        clap_validator=Path("/tools/validator"))
        self.assertEqual([c[:3] for c in runner.calls],
                         [["arch", "-arm64", "/tools/validator"], ["arch", "-x86_64", "/tools/validator"]])

    def test_auval_registers_exact_x86_bundle_and_cleans_arm64_installation(self):
        import plistlib
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifact = root / "product.component"
            (artifact / "Contents").mkdir(parents=True)
            (artifact / "Contents/Info.plist").write_bytes(plistlib.dumps({"AudioComponents": [
                {"type": "aufx", "subtype": "Test", "manufacturer": "RvET"}]}))
            runner = Runner(root)
            with patch.object(bundle.shutil, "copyfile", side_effect=lambda a, b: Path(b).write_bytes(b"tool")) as copy, \
                 patch.object(bundle.Path, "home", return_value=root / "home"), \
                 patch.object(bundle.shutil, "copytree"):
                bundle.validate("au", artifact, runner, build=root, arch="universal")
            copy.assert_called_once_with("/usr/bin/auvaltool", root / "ravet-auvaltool")
            commands = [c for c in runner.calls if "/usr/bin/env" in c]
            self.assertEqual([c[:3] for c in commands],
                             [["arch", "-x86_64", "/usr/bin/env"]])
            self.assertIn(["arch", "-arm64", "auval", "-v", "aufx", "Test", "RvET"], runner.calls)
            self.assertFalse((root / "home/Library/Audio/Plug-Ins/Components/ravet-validation-product.component").exists())
            for command in commands:
                self.assertIn("RAVET_AUVAL_BUNDLE=" + str(artifact), command)
                self.assertIn("DYLD_INSERT_LIBRARIES=" + str(root / "libravet_auval_registration.dylib"), command)
                self.assertEqual(command[-4:], ["-v", "aufx", "Test", "RvET"])
            self.assertEqual(len([c for c in runner.calls if "ravet_au_plugin_contracts" in c[-3]]), 2)

    def test_sdk_layers_and_relative_paths(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            user = root / "user.toml"
            user.write_text('[sdks]\nvst3="user-vst3"\nlv2="user-lv2"\n[signing]\ncommand=["user-sign"]\n')
            project = root / "product"
            project.mkdir()
            (project / "ravet-sdks.toml").write_text('[sdks]\nvst3="project-vst3"\naax="project-aax"\n[signing]\ncommand=["project-sign"]\n')
            paths, command = sdks.discover(project, {"aax": root / "explicit"}, environment={"RAVET_VST3_SDK": str(root / "env")}, user_file=user)
            self.assertEqual(paths, {"vst3": root / "env", "lv2": root / "user-lv2", "aax": root / "explicit"})
            self.assertEqual(command, ["project-sign"])
            paths, _ = sdks.discover(project, environment={}, user_file=user)
            self.assertEqual(paths["vst3"], project / "project-vst3")
            override = root / "override.toml"
            override.write_text('[sdks]\naax="override-aax"\n')
            paths, command = sdks.discover(project, settings=override, environment={}, user_file=user)
            self.assertEqual(paths["vst3"], root / "user-vst3")
            self.assertEqual(paths["aax"], root / "override-aax")
            self.assertEqual(command, ["user-sign"])

    def test_missing_bad_and_unknown_settings(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            with self.assertRaisesRegex(ValueError, "RAVET_AAX_SDK"):
                sdks.required({}, "aax")
            with self.assertRaisesRegex(ValueError, "does not exist"):
                sdks.discover(root, settings=root / "missing", environment={}, user_file=root / "user")
            for text in ('[sdks]\nunknown="foo"', '[sdks]\nvst3=1', '[signing]\ncommand="sh -c anything"'):
                (root / "settings").write_text(text)
                with self.assertRaises(ValueError):
                    sdks.read_settings(root / "settings")

    def test_user_config_locations(self):
        env = {"HOME": "/home/vendor"}
        self.assertEqual(sdks.user_settings(env, "darwin"), Path("/home/vendor/Library/Application Support/ravet/ravet-sdks.toml"))
        self.assertEqual(sdks.user_settings(env, "linux"), Path("/home/vendor/.config/ravet/ravet-sdks.toml"))
        self.assertEqual(sdks.user_settings(dict(env, XDG_CONFIG_HOME="/config"), "darwin"), Path("/config/ravet/ravet-sdks.toml"))

    def test_product_compiled_once_with_both_library_types(self):
        with tempfile.TemporaryDirectory() as temporary:
            args = bundle.parser().parse_args(["--package", "my-product", "--formats", "clap,aax", "--release"])
            runner = Runner(Path(temporary))
            target, library, archive, libraries = bundle.build_product(args, runner)
            builds = [c for c in runner.calls if c[:2] == ["cargo", "rustc"]]
            self.assertEqual(len(builds), 1)
            self.assertIn("cdylib,staticlib", builds[0])
            self.assertIn("--locked", builds[0])
            self.assertIn("custom_library", library.name)
            self.assertIn("custom_library", archive.name)
            self.assertEqual(libraries, "-lSystem -lc")

    def test_cross_product_uses_xwin_static_crt(self):
        args = bundle.parser().parse_args(["--package", "my-product", "--target", "windows"])
        runner = Runner(Path("/target"))
        _, library, _, _ = bundle.build_product(args, runner)
        self.assertEqual(runner.calls[1][:3], ["cargo", "xwin", "rustc"])
        self.assertEqual(library, Path("/target/x86_64-pc-windows-msvc/debug/custom_library.dll"))
        self.assertIn("+crt-static", runner.environment["CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS"])

    def test_signing_hook_is_real_argument_command(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            script = root / "vendor hook.py"
            script.write_text("import json, os, pathlib, sys\npathlib.Path(sys.argv[1]).write_text(json.dumps([sys.argv[2:], os.environ['RAVET_BUNDLE_FORMAT']]))\n")
            log = root / "log.json"
            artifact = root / "a bundle;$(touch not-executed).aaxplugin"
            runner = bundle.Runner(root)
            bundle.sign(artifact, "aax", runner, [sys.executable, str(script), str(log)], platform="darwin")
            self.assertEqual(json.loads(log.read_text()), [[str(artifact)], "aax"])
            self.assertFalse((root / "not-executed").exists())
            with self.assertRaises(subprocess.CalledProcessError):
                bundle.sign(artifact, "aax", runner, [sys.executable, "-c", "raise SystemExit(7)"])

    def test_selected_format_only(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            library = root / "product.dylib"
            library.write_bytes(b"fake")
            args = bundle.parser().parse_args(["--package", "my-product", "--formats", "clap", "--manifest-path", str(root / "Cargo.toml")])
            with patch.object(bundle.plugin_sdks, "discover", return_value=({}, None)), \
                 patch.object(bundle, "build_product", return_value=(root, library, None, "")) as build, \
                 patch.object(bundle, "product_metadata", return_value={"name":"My product", "plugin_id":"com.vendor.product", "version":"1.2.3"}), \
                 patch.object(bundle, "sign") as sign, \
                 patch.object(bundle.generic_vst3_bundle, "build_generic") as vst3:
                artifacts = bundle.bundle(args)
            self.assertEqual(len(artifacts), 1)
            self.assertEqual(artifacts[0].suffix, ".clap")
            build.assert_called_once()
            sign.assert_called_once()
            vst3.assert_not_called()

    def test_all_formats_receive_the_same_prebuilt_product(self):
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            source = root / "source"
            source.mkdir()
            library, archive = root / "custom.dylib", root / "custom.a"
            library.write_bytes(b"fake")
            args = bundle.parser().parse_args(["--package", "my-product", "--manifest-path", str(root / "Cargo.toml"),
                                              "--formats", "clap,vst3,au,aax,lv2", "--arch", "universal"])
            paths = {name: root for name in sdks.ENVIRON}
            product = {"name":"My product", "plugin_id":"com.vendor.product", "version":"1.2.3"}
            with patch.object(bundle.plugin_sdks, "discover", return_value=(paths, None)), \
                 patch.object(bundle, "preflight"), \
                 patch.object(bundle, "macos_arch", return_value="universal"), \
                 patch.object(bundle, "build_product", return_value=(root, library, archive, "system-libs")) as build, \
                 patch.object(bundle, "product_metadata", return_value=product), \
                 patch.object(bundle, "sign") as signer, \
                 patch.object(bundle.generic_vst3_bundle, "build_generic", return_value=[source]) as vst3, \
                 patch.object(bundle.aax_plugin, "build", return_value=SimpleNamespace(build=root, artifact=source)) as aax, \
                 patch.object(bundle.lv2_generic, "prepare_generic", return_value=SimpleNamespace()) as lv2_prepare, \
                 patch.object(bundle.lv2_generic, "build_generic", return_value=(root, root)) as lv2, \
                 patch.object(bundle.lv2_generic, "package_generic", return_value=source), \
                 patch.object(bundle.au_native_bundle, "build", return_value=source) as au:
                artifacts = bundle.bundle(args)
            self.assertEqual([p.suffix for p in artifacts], [".clap", ".vst3", ".component", ".aaxplugin", ".lv2"])
            build.assert_called_once()
            self.assertEqual([call.args[1] for call in signer.call_args_list], list(bundle.FORMATS))
            self.assertEqual(vst3.call_args.kwargs["product_library"], library)
            self.assertEqual(vst3.call_args.kwargs["bundle_name"], "my-product")
            self.assertEqual(aax.call_args.kwargs["product_library"], archive)
            self.assertEqual(aax.call_args.args[2].name, "my-product")
            self.assertEqual(aax.call_args.kwargs["link_libraries"], "system-libs")
            self.assertEqual(lv2.call_args.kwargs["product_library"], library)
            self.assertEqual(au.call_args.args[1].product_library, library)
            expected = "-DCMAKE_OSX_ARCHITECTURES=arm64;x86_64"
            for builder in (vst3, aax, lv2):
                self.assertIn(expected, builder.call_args.kwargs["cmake_options"])
                self.assertIn("macos-universal", str(builder.call_args.kwargs["target_directory"]))
            self.assertIn(expected, au.call_args.args[1].cmake_options)
            self.assertIn("-DRAVET_LV2_LILV=OFF", lv2.call_args.kwargs["cmake_options"])

    def test_webview2_sdk_notices_are_copied(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            sdk = root / "sdk"
            sdk.mkdir()
            (sdk / "LICENSE.txt").write_text("BSD")
            (sdk / "NOTICE.txt").write_text("notice")
            copied = bundle.webview2_notices(root / "notices", sdk)
            self.assertEqual([p.name for p in copied], ["WEBVIEW2-SDK-LICENSE.txt", "WEBVIEW2-SDK-NOTICE.txt"])
            self.assertEqual((root / "notices/WEBVIEW2-SDK-LICENSE.txt").read_text(), "BSD")
            with self.assertRaises(ValueError):
                bundle.webview2_notices(root / "other", root / "missing")
            self.assertIn("webview2", sdks.ENVIRON)

    def test_clap_metadata_and_cross_platform_replacement(self):
        import plistlib
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            library = root / "libmy_product.dylib"
            library.write_bytes(b"product")
            artifact = root / "product.clap"
            product = {"name": "Product title", "plugin_id": "com.vendor.product", "version": "2.3.4"}
            bundle.clap_bundle(library, artifact, product, "darwin")
            metadata = plistlib.loads((artifact / "Contents/Info.plist").read_bytes())
            self.assertEqual(metadata["CFBundleExecutable"], "my_product")
            self.assertEqual(metadata["CFBundleIdentifier"], product["plugin_id"])
            bundle.clap_bundle(library, artifact, product, "linux")
            self.assertTrue(artifact.is_file())
            self.assertEqual(artifact.read_bytes(), b"product")

    def test_au_discovery_retries_only_discovery_and_always_removes_install(self):
        import plistlib
        from unittest.mock import Mock
        for failure in ("didn't find the component", "render test failed"):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                artifact = root / "source.component"
                (artifact / "Contents").mkdir(parents=True)
                (artifact / "Contents/Info.plist").write_bytes(plistlib.dumps({"AudioComponents": [
                    {"type": "aumu", "subtype": "Test", "manufacturer": "RvTs"}]}))
                error = subprocess.CalledProcessError(2, ["auval"], output=failure)
                runner = Mock()
                # actual-component fixture succeeds; auval initially fails.
                runner.run.side_effect = [None, error, "AU VALIDATION SUCCEEDED."]
                # The registrar-retry path is the arm64 slice; pin it so the test
                # does not take the x86_64 private-validator path on Intel hosts.
                with patch.object(Path, "home", return_value=root / "home"), patch.object(bundle.time, "sleep") as sleep, \
                        patch.object(bundle.host_platform, "machine", return_value="arm64"):
                    if failure == "render test failed":
                        with self.assertRaises(subprocess.CalledProcessError):
                            bundle.validate("au", artifact, runner, build=root / "build")
                        self.assertEqual(runner.run.call_count, 2)
                        sleep.assert_not_called()
                    else:
                        bundle.validate("au", artifact, runner, build=root / "build")
                        self.assertEqual(runner.run.call_count, 3)
                        sleep.assert_called_once_with(2)
                installed = root / "home/Library/Audio/Plug-Ins/Components/ravet-validation-source.component"
                self.assertFalse(installed.exists())

    def test_cross_validator_uses_wine_and_target_executable(self):
        runner = Runner(Path("/target"))
        with patch("shutil.which", return_value="/usr/bin/wine"):
            bundle.validate("clap", Path("/plugins/product.clap"), runner, cross="windows",
                            clap_validator=Path("/tools/clap-validator.exe"))
        self.assertEqual(runner.calls, [["/usr/bin/wine", "/tools/clap-validator.exe", "validate", "/plugins/product.clap"]])

    def test_platform_and_sdk_rejection_before_cargo(self):
        args = bundle.parser().parse_args(["--package", "my-product", "--formats", "au"])
        with self.assertRaisesRegex(ValueError, "AU requires"):
            bundle.preflight(args, {}, "linux")
        args.formats = ["vst3"]
        with self.assertRaisesRegex(ValueError, "RAVET_VST3_SDK"):
            bundle.preflight(args, {}, "darwin")
        args.formats = ["clap"]
        args.sign_command = "shell string"
        with self.assertRaisesRegex(ValueError, "argument array"):
            bundle.preflight(args, {}, "darwin")
