"""Check the AudioUnitSDK fetcher: verify before opening, extract only listed members."""
import hashlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import zipfile

SPEC = importlib.util.spec_from_file_location(
    "audio_unit_sdk_download", Path(__file__).resolve().parents[1] / "fetch-audio-unit-sdk.py"
)
download = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(download)

TOP = "AudioUnitSDK-AudioUnitSDK-1.4.0/"


def archive(scope_header=b"Expected<AudioBufferList&> GetBufferListOrError();"):
    contents = io.BytesIO()
    with zipfile.ZipFile(contents, "w") as zipped:
        zipped.writestr(TOP + "include/AudioUnitSDK/AUScopeElement.h", scope_header)
        zipped.writestr(TOP + "src/AudioUnitSDK/AUBase.cpp", b"source")
        zipped.writestr(TOP + "LICENSE.txt", b"licence")
        zipped.writestr(TOP + "demos/Demo.cpp", b"not kept")
        zipped.writestr(TOP + "../unexpected", b"must not be extracted")
        zipped.writestr("../escape", b"must not be extracted")
    return contents.getvalue()


def pinned(data):
    return mock.patch.object(download, "SHA256", hashlib.sha256(data).hexdigest())


class AudioUnitSdkDownloadTests(unittest.TestCase):
    def test_wrong_hash_is_rejected_before_opening(self):
        with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
            download.selected_members(b"not the archive")

    def test_installs_only_listed_members_without_the_top_directory(self):
        data = archive()
        response = mock.MagicMock()
        response.__enter__.return_value.read.return_value = data
        with tempfile.TemporaryDirectory() as temporary, pinned(data), \
                mock.patch.object(download.urllib.request, "urlopen", return_value=response):
            root = download.install(Path(temporary) / "sdk")
            files = sorted(p.relative_to(root).as_posix() for p in root.rglob("*") if p.is_file())
            self.assertEqual(files, ["LICENSE.txt", "include/AudioUnitSDK/AUScopeElement.h",
                                     "src/AudioUnitSDK/AUBase.cpp"])
            self.assertFalse((Path(temporary) / "unexpected").exists())
            self.assertFalse((Path(temporary) / "escape").exists())

    def test_sdk_older_than_1_4_is_rejected(self):
        data = archive(scope_header=b"AudioBufferList& GetBufferList();")
        with pinned(data), self.assertRaisesRegex(ValueError, "older than 1.4"):
            download.selected_members(data)

    def test_missing_sources_are_rejected(self):
        contents = io.BytesIO()
        with zipfile.ZipFile(contents, "w") as zipped:
            zipped.writestr(TOP + "LICENSE.txt", b"licence")
        data = contents.getvalue()
        with pinned(data), self.assertRaisesRegex(ValueError, "AUScopeElement.h"):
            download.selected_members(data)


if __name__ == "__main__":
    unittest.main()
