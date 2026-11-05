"""Windows cross-build helper: cargo-xwin environment, toolchain and static CRT."""
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
import windows_cross  # noqa: E402

XWIN_ENV = '''export CMAKE_GENERATOR="Ninja";
export CMAKE_TOOLCHAIN_FILE_x86_64_pc_windows_msvc="/cache/cargo-xwin/cmake/clang-cl/x86_64-pc-windows-msvc-toolchain.cmake";
export PATH="/home/u/.cargo/bin:/usr/bin:/cache/cargo-xwin";
export RUSTFLAGS="";
'''


class Runner:
    def __init__(self):
        self.calls, self.environment = [], {"PATH": "/usr/bin"}

    def run(self, arguments, *, capture=False, **_):
        self.calls.append([str(argument) for argument in arguments])
        return XWIN_ENV if capture else None


class WindowsCrossTests(unittest.TestCase):
    def test_parse_environment(self):
        parsed = windows_cross.parse_environment(XWIN_ENV)
        self.assertEqual(parsed["CMAKE_GENERATOR"], "Ninja")
        self.assertEqual(parsed["RUSTFLAGS"], "")
        self.assertTrue(parsed["PATH"].endswith("/cache/cargo-xwin"))

    def test_prepare_takes_only_path_and_toolchain(self):
        with tempfile.TemporaryDirectory() as temporary, mock.patch("shutil.which", return_value="/usr/bin/wine"):
            runner = Runner()
            arguments = windows_cross.prepare(runner, Path(temporary), platform="linux")
            self.assertEqual(runner.calls, [["cargo", "xwin", "env", "--target", windows_cross.TRIPLE]])
            environment = runner.environment
            tools = Path(temporary) / "windows-cross/host-tools"
            self.assertTrue(environment["PATH"].startswith(f"{tools}:"))
            self.assertIn("/cache/cargo-xwin", environment["PATH"])
            self.assertIn("CMAKE_TOOLCHAIN_FILE_x86_64_pc_windows_msvc", environment)
            self.assertNotIn("CMAKE_GENERATOR", environment, "Ninja is not required")
            self.assertNotIn("RUSTFLAGS", environment)
            self.assertEqual(environment[windows_cross.RUSTFLAGS], windows_cross.STATIC_CRT)
            self.assertTrue(os.access(tools / "attrib", os.X_OK))
            self.assertIn(f"-DCMAKE_TOOLCHAIN_FILE={SCRIPTS / 'cmake/windows-xwin.cmake'}", arguments)
            self.assertIn("-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded", arguments)
            self.assertIn("-DCMAKE_CROSSCOMPILING_EMULATOR=/usr/bin/wine", arguments)
            self.assertTrue(windows_cross.emulator_available(arguments))

    def test_without_wine_no_emulator(self):
        with mock.patch("shutil.which", return_value=None):
            arguments = windows_cross.toolchain_arguments(SCRIPTS)
        self.assertFalse(windows_cross.emulator_available(arguments))

    def test_windows_host_is_refused(self):
        runner = Runner()
        with self.assertRaises(ValueError):
            windows_cross.prepare(runner, Path("target"), platform="win32")
        self.assertEqual(runner.calls, [])

    def test_static_crt_is_added_once(self):
        environment = {windows_cross.RUSTFLAGS: "-C opt-level=3"}
        windows_cross.static_crt(environment)
        windows_cross.static_crt(environment)
        self.assertEqual(environment[windows_cross.RUSTFLAGS], f"-C opt-level=3 {windows_cross.STATIC_CRT}")

    def test_cargo_and_output(self):
        self.assertEqual(windows_cross.cargo("build", "-p", "x"),
                         ["cargo", "xwin", "build", "--target", windows_cross.TRIPLE, "-p", "x"])
        self.assertEqual(windows_cross.output_directory(Path("t")), Path("t") / windows_cross.TRIPLE / "release")

    def test_toolchain_file_restores_system_libraries(self):
        text = (SCRIPTS / "cmake/windows-xwin.cmake").read_text()
        self.assertIn("CMAKE_TOOLCHAIN_FILE_x86_64_pc_windows_msvc", text)
        for library in ("kernel32.lib", "user32.lib", "ole32.lib", "shell32.lib", "uuid.lib"):
            self.assertIn(library, text)


if __name__ == "__main__":
    unittest.main()
