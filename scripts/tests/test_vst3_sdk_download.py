"""Check the VST3 SDK fetcher: verify before opening, extract only listed members."""
import hashlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import zipfile

SPEC = importlib.util.spec_from_file_location(
    "vst3_sdk_download", Path(__file__).resolve().parents[1] / "fetch-vst3-sdk.py"
)
download = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(download)

ROOT = "VST_SDK/vst3sdk/"


def archive(omit=()):
    contents = io.BytesIO()
    with zipfile.ZipFile(contents, "w") as zipped:
        for name in download.REQUIRED:
            if name not in omit:
                zipped.writestr(ROOT + name, b"data")
        zipped.writestr(ROOT + "base/source/fobject.cpp", b"kept")
        zipped.writestr(ROOT + "vstgui4/vstgui/vstgui.h", b"not kept")
        zipped.writestr(ROOT + "doc/index.html", b"not kept")
        zipped.writestr("VST_SDK/VST3_Project_Generator/readme.txt", b"not kept")
        zipped.writestr(ROOT + "../unexpected", b"must not be extracted")
        link = zipfile.ZipInfo(ROOT + "public.sdk/link")
        link.external_attr = (0o120777 << 16)
        zipped.writestr(link, b"/etc/passwd")
    return contents.getvalue()


def pinned(data):
    return mock.patch.object(download, "SHA256", hashlib.sha256(data).hexdigest())


class Vst3SdkDownloadTests(unittest.TestCase):
    def test_wrong_hash_is_rejected_before_opening(self):
        with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
            download.selected_members(b"not the archive")

    def test_installs_only_listed_members_without_the_prefix(self):
        data = archive()
        response = mock.MagicMock()
        response.__enter__.return_value.read.return_value = data
        with tempfile.TemporaryDirectory() as temporary, pinned(data), \
                mock.patch.object(download.urllib.request, "urlopen", return_value=response):
            root = download.install(Path(temporary) / "sdk")
            files = sorted(p.relative_to(root).as_posix() for p in root.rglob("*") if p.is_file())
            self.assertEqual(files, sorted([*download.REQUIRED, "base/source/fobject.cpp"]))
            self.assertFalse((Path(temporary) / "unexpected").exists())

    def test_missing_sources_are_rejected(self):
        data = archive(omit=("pluginterfaces/vst/ivstaudioprocessor.h",))
        with pinned(data), self.assertRaisesRegex(ValueError, "ivstaudioprocessor.h"):
            download.selected_members(data)


if __name__ == "__main__":
    unittest.main()
