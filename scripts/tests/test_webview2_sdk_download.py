"""Check the WebView2 SDK fetcher: verify before opening, extract only listed members."""
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import zipfile

SPEC = importlib.util.spec_from_file_location(
    "webview2_sdk_download", Path(__file__).resolve().parents[1] / "fetch-webview2-sdk.py"
)
download = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(download)


def archive():
    contents = io.BytesIO()
    with zipfile.ZipFile(contents, "w") as zipped:
        zipped.writestr("build/native/include/WebView2.h", b"header")
        zipped.writestr("build/native/x64/WebView2LoaderStatic.lib", b"lib")
        zipped.writestr("LICENSE.txt", b"licence")
        zipped.writestr("NOTICE.txt", b"notice")
        zipped.writestr("build/native/arm64/WebView2LoaderStatic.lib", b"not kept")
        zipped.writestr("../unexpected", b"must not be extracted")
    return contents.getvalue()


class WebView2DownloadTests(unittest.TestCase):
    def test_wrong_hash_is_rejected_before_opening(self):
        with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
            download.selected_members(b"not the package")

    def test_installs_only_listed_members(self):
        data = archive()
        response = mock.MagicMock()
        response.__enter__.return_value.read.return_value = data
        with tempfile.TemporaryDirectory() as temporary, \
                mock.patch.object(download, "SHA256", __import__("hashlib").sha256(data).hexdigest()), \
                mock.patch.object(download.urllib.request, "urlopen", return_value=response):
            root = download.install(Path(temporary) / "sdk")
            files = sorted(p.relative_to(root).as_posix() for p in root.rglob("*") if p.is_file())
            self.assertEqual(files, ["LICENSE.txt", "NOTICE.txt", "build/native/include/WebView2.h",
                                     "build/native/x64/WebView2LoaderStatic.lib"])
            self.assertFalse((Path(temporary) / "unexpected").exists())

    def test_missing_header_is_rejected(self):
        contents = io.BytesIO()
        with zipfile.ZipFile(contents, "w") as zipped:
            zipped.writestr("LICENSE.txt", b"licence")
        data = contents.getvalue()
        with mock.patch.object(download, "SHA256", __import__("hashlib").sha256(data).hexdigest()):
            with self.assertRaisesRegex(ValueError, "WebView2.h"):
                download.selected_members(data)


if __name__ == "__main__":
    unittest.main()
