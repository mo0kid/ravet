"""Build a native AUv2 from a trusted local ravet_plugin_v1 product library."""
import hashlib
import json
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import third_party_notices
import tomllib


def run(*args, capture=False):
    command = [str(arg) for arg in args]
    if capture:
        return subprocess.check_output(command, text=True)
    subprocess.run(command, check=True)


def fourcc(value):
    text = value.to_bytes(4, "big").decode("ascii")
    if any(ord(c) < 32 or ord(c) > 126 for c in text):
        raise ValueError("AU identity must contain four printable ASCII characters")
    return text


def component_plist(product):
    """Generate all component identity fields from the loaded table."""
    version = [int(part) for part in product["version"].split(".")]
    if len(version) != 3 or any(part < 0 or part > 255 for part in version):
        raise ValueError("AU version needs three components in 0..255")
    return {
        "CFBundleExecutable": "ravet_au",
        "CFBundleIdentifier": product["plugin_id"] + ".auv2",
        "CFBundleName": product["name"],
        "CFBundlePackageType": "BNDL",
        "CFBundleShortVersionString": product["version"],
        "CFBundleVersion": product["version"],
        "NSHighResolutionCapable": True,
        "AudioComponents": [{
            "name": product["vendor"] + ": " + product["name"],
            "description": product["name"],
            "factoryFunction": "RavetAUFactory",
            "type": fourcc(product["au_type"]),
            "subtype": fourcc(product["au_subtype"]),
            "manufacturer": fourcc(product["au_manufacturer"]),
            "version": (version[0] << 16) | (version[1] << 8) | version[2],
            # Sandboxed/real-time safety is not inferred from offline validation.
            "sandboxSafe": False,
        }],
    }


def build(root, args):
    if sys.platform != "darwin":
        raise ValueError("AUv2 builds require macOS and Xcode")
    sdk = args.audio_unit_sdk.expanduser().resolve()
    if not (sdk / "include/AudioUnitSDK/AUBase.h").is_file() or not (sdk / "src/AudioUnitSDK").is_dir():
        raise ValueError("use an external Apple AudioUnitSDK source checkout (include/AudioUnitSDK, src/AudioUnitSDK)")
    if args.mock and args.product_library:
        raise ValueError("choose --mock or --product-library")
    manifest = args.manifest_path
    product_package = None
    if not args.mock and not args.product_library:
        manifest = (manifest or root / "examples" / (args.plugin + "-plugin") / "Cargo.toml").resolve()
        with manifest.open("rb") as stream:
            package = tomllib.load(stream)
        name = package["package"]["name"]
        product_package = name
        run("cargo", "build", "--manifest-path", manifest, "-p", name, "--release", "--locked")
        metadata = json.loads(run("cargo", "metadata", "--manifest-path", manifest, "--no-deps", "--format-version", "1", capture=True))
        library_name = package.get("lib", {}).get("name", name.replace("-", "_"))
        library = Path(metadata["target_directory"]) / "release" / ("lib" + library_name + ".dylib")
    elif args.product_library:
        library = args.product_library.expanduser().resolve()
    else:
        library = None
    build_dir = (args.build_dir or root / "target/au-native" / ("mock-" + args.mock if args.mock else args.plugin)).resolve()
    source = root / "adapters/auv2"
    common = ["cmake", "-S", source, "-B", build_dir, "-DCMAKE_BUILD_TYPE=Release"]
    run(*common, "-DRAVET_PRODUCT_LIBRARY=")
    targets = ["ravet_au_metadata", "ravet_au_plugin_contracts"]
    if args.validate:
        targets.append("ravet_au_delay_contracts")
    if args.mock:
        target = {"effect": "ravet_au_mock", "instrument": "ravet_au_mock_instrument", "midi": "ravet_au_mock_midi"}[args.mock]
        targets.append(target)
    run("cmake", "--build", build_dir, "--target", *targets, "--parallel", "2")
    if args.mock:
        library = build_dir / ("lib" + target + ".dylib")
    product = json.loads(run(build_dir / "ravet_au_metadata", library, capture=True))
    suffix = hashlib.sha256(product["plugin_id"].encode()).hexdigest()[:16]
    plist = component_plist(product)
    plist_path = build_dir / "Product-Info.plist"
    plist_path.write_bytes(plistlib.dumps(plist))
    staged = build_dir / "product" / ("libravet_" + suffix + ".dylib")
    staged.parent.mkdir(exist_ok=True)
    shutil.copy2(library, staged)
    run("install_name_tool", "-id", "@rpath/" + staged.name, staged)
    run("codesign", "--force", "--sign", "-", staged)
    run(*common, f"-DRAVET_PRODUCT_LIBRARY={staged}", f"-DRAVET_AUDIOUNIT_SDK={sdk}",
        f"-DRAVET_AU_PLIST={plist_path}", f"-DRAVET_AU_CLASS_SUFFIX={suffix}",
        f"-DRAVET_AU_BUNDLE_ID={plist['CFBundleIdentifier']}")
    run("cmake", "--build", build_dir, "--target", "ravet_au", "--parallel", "2")
    component = build_dir / "AU/ravet_au.component"
    frameworks = component / "Contents/Frameworks"
    frameworks.mkdir(exist_ok=True)
    shutil.copy2(staged, frameworks / staged.name)
    notices = component / "Contents/Resources/notices"
    notices.mkdir(parents=True, exist_ok=True)
    shutil.copy2(root / "LICENSE", notices / "RAVET-LICENSE.txt")
    shutil.copy2(sdk / "LICENSE.txt", notices / "Apple-AudioUnitSDK-LICENSE.txt")
    if product_package:
        third_party_notices.write(root, product_package, notices, manifest_path=manifest,
                                  run=lambda arguments, capture=False: run(*arguments, capture=capture))
    elif not args.mock:
        print("note: --product-library given; add its third-party notices to "
              f"{notices} yourself", file=sys.stderr)
    run("codesign", "--force", "--sign", "-", component)
    if args.validate:
        run(build_dir / "ravet_au_plugin_contracts", component, *([] if args.mock else ["real"]))
        if product["plugin_id"] == "org.ravet.delay":
            run(build_dir / "ravet_au_delay_contracts", component)
    print(component)
    return component
