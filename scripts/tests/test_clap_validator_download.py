"""Check both pinned archive layouts and reject corruption before installation."""
import hashlib
import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest import mock
import zipfile

SPEC = importlib.util.spec_from_file_location(
    "clap_validator_download", Path(__file__).resolve().parents[1] / "fetch-clap-validator.py"
)
download = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(download)


def archive_for(platform):
    contents = io.BytesIO()
    with zipfile.ZipFile(contents, "w") as zipped:
        if platform == "win32":
            zipped.writestr("clap-validator.exe", b"test executable")
        else:
            nested = io.BytesIO()
            with tarfile.open(fileobj=nested, mode="w:gz") as tar:
                member = tarfile.TarInfo("binaries/clap-validator")
                member.size = len(b"test executable")
                tar.addfile(member, io.BytesIO(b"test executable"))
            zipped.writestr(
                "clap-validator-0.3.2-127-g152b982-macos-universal.tar.gz", nested.getvalue()
            )
        # No archive-controlled path may be written outside the destination.
        zipped.writestr("../unexpected", b"must not be extracted")
    return contents.getvalue()


class ValidatorDownloadTests(unittest.TestCase):
    def test_both_archive_layouts_install_only_the_executable(self):
        for platform, (asset, _, binary) in download.ASSETS.items():
            with self.subTest(platform=platform), tempfile.TemporaryDirectory() as temporary:
                archive = archive_for(platform)
                pin = (asset, hashlib.sha256(archive).hexdigest(), binary)
                destination = Path(temporary) / "validator"
                with mock.patch.dict(download.ASSETS, {platform: pin}), mock.patch.object(
                    download.urllib.request, "urlopen", return_value=io.BytesIO(archive)
                ) as fetch:
                    executable = download.install(destination, platform)
                self.assertEqual(executable.read_bytes(), b"test executable")
                self.assertEqual(list(destination.iterdir()), [executable])
                self.assertEqual(list(Path(temporary).iterdir()), [destination])
                fetch.assert_called_once_with(download.RELEASE + asset, timeout=60)

    def test_bad_checksum_is_rejected_before_archive_read_or_file_write(self):
        with tempfile.TemporaryDirectory() as temporary, mock.patch.object(
            download.urllib.request, "urlopen", return_value=io.BytesIO(b"corrupt archive")
        ), mock.patch.object(download.zipfile, "ZipFile") as unzip:
            destination = Path(temporary) / "validator"
            with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                download.install(destination, "win32")
            unzip.assert_not_called()
            self.assertFalse(destination.exists())

    def test_unsupported_platform_does_not_download(self):
        with mock.patch.object(download.urllib.request, "urlopen") as fetch:
            with self.assertRaisesRegex(ValueError, "macOS and Windows only"):
                download.install(Path("unused"), "linux")
            fetch.assert_not_called()


if __name__ == "__main__":
    unittest.main()
