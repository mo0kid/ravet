"""Package a prebuilt CLAP product; shared by generic and reference commands."""
import plistlib
import shutil


def package_product(library, artifact, product, platform):
    if artifact.exists() or artifact.is_symlink():
        if artifact.is_dir() and not artifact.is_symlink():
            shutil.rmtree(artifact)
        else:
            artifact.unlink()
    if platform != "darwin":
        artifact.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(library, artifact)
        return artifact
    contents = artifact / "Contents"
    executable = contents / "MacOS" / library.stem.removeprefix("lib")
    executable.parent.mkdir(parents=True)
    shutil.copy2(library, executable)
    (contents / "Info.plist").write_bytes(plistlib.dumps({
        "CFBundleExecutable": executable.name, "CFBundleIdentifier": product["plugin_id"],
        "CFBundleName": product["name"], "CFBundlePackageType": "BNDL",
        "CFBundleVersion": product["version"], "CFBundleShortVersionString": product["version"],
    }))
    return artifact

