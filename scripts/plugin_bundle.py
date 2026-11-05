"""Build one trusted export_plugin! product, then assemble selected format bundles."""
import argparse
import json
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
from types import SimpleNamespace

import aax_plugin
from clap_bundle import package_product as clap_bundle
import au_native_bundle
import generic_vst3_bundle
import lv2_generic
import plugin_sdks
import third_party_notices
import windows_cross

ROOT = Path(__file__).resolve().parents[1]
FORMATS = ("clap", "vst3", "au", "aax", "lv2")
SUFFIX = dict(zip(FORMATS, ("clap", "vst3", "component", "aaxplugin", "lv2")))


class Runner(aax_plugin.Runner):
    """All commands share the product working directory and cross environment."""

    def run(self, arguments, *, capture=False, capture_errors=False):
        try:
            result = super().run(arguments, capture=capture, capture_errors=capture_errors)
        except subprocess.CalledProcessError as error:
            if error.stderr:
                print(error.stderr, file=sys.stderr, end="")
            raise
        if capture_errors and result:
            print(result, file=sys.stderr, end="")
        return result


def formats(value):
    selected = value.split(",")
    if not selected or any(item not in FORMATS for item in selected) or len(set(selected)) != len(selected):
        raise argparse.ArgumentTypeError("select distinct formats from " + ",".join(FORMATS))
    return selected


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--package", required=True, help="Cargo package with exactly one cdylib target")
    result.add_argument("--manifest-path", type=Path, default=Path("Cargo.toml"))
    result.add_argument("--formats", type=formats, default=["clap"])
    result.add_argument("--target", choices=["windows"], help="cross-build x86_64 Windows using cargo-xwin")
    result.add_argument("--release", action="store_true")
    result.add_argument("--validate", action="store_true", help="validate final signed bundles; fail if a validator is missing")
    result.add_argument("--offline", action="store_true")
    result.add_argument("--clap-validator", type=Path, help="target-native clap-validator executable")
    result.add_argument("--output-dir", type=Path)
    result.add_argument("--settings", type=Path, help="project SDK settings override (user settings still apply)")
    result.add_argument("--sign-command", type=json.loads, help='JSON argument array; bundle path is appended')
    result.add_argument("--check", action="store_true", help="check format/platform/SDK selection without building or resolving Cargo")
    for name in plugin_sdks.ENVIRON:
        result.add_argument("--" + name.replace("_", "-") + "-sdk", type=Path)
    return result


def preflight(args, paths, platform=sys.platform):
    if args.target and platform == "win32":
        raise ValueError("--target windows cross-compiles from macOS/Linux; omit it on Windows")
    if "au" in args.formats and (platform != "darwin" or args.target):
        raise ValueError("AU requires a native macOS build")
    if "aax" in args.formats and platform == "linux" and not args.target:
        raise ValueError("AAX requires macOS or Windows")
    if args.sign_command is not None and (not isinstance(args.sign_command, list) or not args.sign_command or
                                          any(not isinstance(arg, str) or not arg for arg in args.sign_command)):
        raise ValueError("--sign-command must be a nonempty JSON argument array")
    for format in args.formats:
        if format == "clap":
            continue
        name = "audio_unit" if format == "au" else format
        sdk = plugin_sdks.required(paths, name)
        if format == "vst3":
            files = ("LICENSE.txt", "CMakeLists.txt", "pluginterfaces/vst/ivstaudioprocessor.h")
        elif format == "aax":
            files = aax_plugin.SDK_FILES
        elif format == "au":
            files = ("include/AudioUnitSDK/AUBase.h", "src/AudioUnitSDK", "LICENSE.txt")
        else:
            # Reuse the SDK's official/flat include-layout checks.
            lv2_generic.prepare_generic(ROOT, sdk, crate=args.package, cross=args.target, platform=platform)
            continue
        for file in files:
            if not (sdk / file).exists():
                raise ValueError(f"{name} SDK at {sdk} is missing {file}")


def build_product(args, runner):
    metadata = json.loads(runner.run(["cargo", "metadata", "--manifest-path", args.manifest_path,
                                      "--no-deps", "--format-version", "1"], capture=True))
    packages = [p for p in metadata["packages"] if p["name"] == args.package]
    if len(packages) != 1:
        raise ValueError(f"Cargo package {args.package!r} not found")
    libraries = [t for t in packages[0]["targets"] if "cdylib" in t["crate_types"]]
    if len(libraries) != 1:
        raise ValueError("product must declare exactly one cdylib target")
    target = Path(metadata["target_directory"])
    command = windows_cross.cargo("rustc") if args.target else ["cargo", "rustc"]
    command += ["--manifest-path", args.manifest_path, "-p", args.package, "--lib", "--locked", "--jobs", "2",
                "--crate-type", "cdylib,staticlib" if "aax" in args.formats else "cdylib"]
    if args.release:
        command.append("--release")
    if args.offline:
        command.append("--offline")
    command += ["--", "--print", "native-static-libs"]
    if args.target:
        windows_cross.static_crt(runner.environment)
    notes = runner.run(command, capture_errors=True)
    profile = "release" if args.release else "debug"
    platform = "win32" if args.target else sys.platform
    directory = windows_cross.output_directory(target, profile) if args.target else target / profile
    stem = libraries[0]["name"].replace("-", "_")
    library = directory / lv2_generic.library_name(stem, platform)
    archive = directory / (stem + ".lib" if platform == "win32" else "lib" + stem + ".a")
    return target, library, archive, aax_plugin.native_libraries(notes or "")


def product_metadata(target, library, runner, cross):
    build = target / "plugin-metadata" / ("windows" if cross else "native")
    toolchain = windows_cross.prepare(runner, target, platform=sys.platform) if cross else []
    runner.run(["cmake", "-S", ROOT / "tools/plugin-metadata", "-B", build,
                "-DCMAKE_BUILD_TYPE=Release", *toolchain])
    runner.run(["cmake", "--build", build, "--config", "Release", "--parallel", "2"])
    executable_name = "ravet-product-metadata.exe" if cross or sys.platform == "win32" else "ravet-product-metadata"
    executable = next((p for p in sorted(build.rglob(executable_name)) if p.is_file()), None)
    if executable is None:
        raise ValueError("product metadata executable was not built")
    command = [executable, library]
    if cross:
        wine = shutil.which("wine")
        if not wine:
            raise ValueError("cross-build metadata needs Wine to read the Windows product table")
        command = [wine, *command]
    return json.loads(runner.run(command, capture=True))


def copy_artifact(source, destination):
    if destination.exists():
        if destination.is_dir():
            shutil.rmtree(destination)
        else:
            destination.unlink()
    if source.is_dir():
        shutil.copytree(source, destination)
    else:
        shutil.copy2(source, destination)
    return destination



def sign(artifact, format, runner, command=None, *, platform=sys.platform):
    """Vendor hook runs once per final bundle; default signs inner Mach-O images first."""
    if command:
        runner.environment.update(RAVET_BUNDLE_FORMAT=format, RAVET_BUNDLE_PATH=str(artifact))
        runner.run([*command, artifact])
    elif platform == "darwin":
        for file in sorted(artifact.rglob("*")) if artifact.is_dir() else []:
            if file.is_file():
                with file.open("rb") as source:
                    magic = source.read(4)
                if magic in (b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe", b"\xca\xfe\xba\xbe"):
                    runner.run(["codesign", "--force", "--sign", "-", file])
        runner.run(["codesign", "--force", "--sign", "-", artifact]) if format != "lv2" else None


def validate(format, artifact, runner, *, build=None, cross=None, clap_validator=None):
    prefix = []
    if cross:
        wine = shutil.which("wine")
        if not wine:
            raise ValueError("--validate on a Windows cross-build requires Wine")
        prefix = [wine]
    if format == "clap":
        executable = "clap-validator.exe" if cross or sys.platform == "win32" else "clap-validator"
        validator = clap_validator or shutil.which(executable)
        if not validator:
            candidates = [p for p in (ROOT / "target/tools").rglob(executable) if p.is_file()]
            validator = candidates[0] if candidates else None
        if not validator:
            raise ValueError("CLAP validation needs a target-native clap-validator: use --clap-validator, PATH or target/tools")
        runner.run([*prefix, validator, "validate", artifact])
    elif format == "vst3":
        candidates = sorted(build.rglob("validator.exe" if cross or sys.platform == "win32" else "validator"))
        validator = next((p for p in candidates if p.is_file()), None)
        if not validator:
            raise ValueError("Steinberg validator was not built")
        runner.run([*prefix, validator, artifact])
    elif format == "lv2":
        validator = shutil.which("lv2_validate")
        if not validator:
            raise ValueError("LV2 validation requires lv2_validate on PATH")
        runner.run([validator, artifact / "manifest.ttl", artifact / "plugin.ttl"])
    elif format == "aax":
        # Avid/PACE production validation remains vendor-owned.
        exe = "ravet_aax_check.exe" if cross or sys.platform == "win32" else "ravet_aax_check"
        candidates = [p for p in build.rglob(exe) if p.is_file()]
        if not candidates:
            raise ValueError("AAX synthetic bundle checker was not built")
        runner.run([*prefix, candidates[0], artifact])
    elif format == "au":
        runner.run([build / "ravet_au_plugin_contracts", artifact, "real"])
        # auval requires installation. Never replace an existing component.
        installed = Path.home() / "Library/Audio/Plug-Ins/Components" / ("ravet-validation-" + artifact.name)
        installed.parent.mkdir(parents=True, exist_ok=True)
        installed.mkdir()  # Exclusive reservation, including protection against symlinks.
        try:
            shutil.copytree(artifact, installed, dirs_exist_ok=True)
            with (artifact / "Contents/Info.plist").open("rb") as source:
                identity = plistlib.load(source)["AudioComponents"][0]
            runner.run(["auval", "-v", identity["type"], identity["subtype"], identity["manufacturer"]])
        finally:
            shutil.rmtree(installed)


def notices_directory(artifact, format, platform):
    """Where a format's bundle carries the licence notices.

    Inside macOS-style bundles (`Contents/Resources/notices`), at the root of an
    LV2 bundle, and beside a single-file `.clap` on Windows and Linux, which
    installers must carry with it.
    """
    artifact = Path(artifact)
    if format == "lv2":
        return artifact
    if format == "clap" and platform != "darwin":
        return artifact.parent / f"{artifact.stem} notices"
    return artifact / "Contents" / "Resources" / "notices"


def write_notices(artifact, format, platform, package, manifest_path, runner, cross, *, writer=None):
    """Ship RAVET's licence and the product's third-party notices with a bundle."""
    directory = notices_directory(artifact, format, platform)
    if (directory / "THIRD-PARTY-NOTICES.txt").exists():
        return directory
    (writer or third_party_notices.write)(ROOT, package, directory, windows_cross.TRIPLE if cross else None,
           manifest_path=manifest_path, run=runner.run)
    return directory


def bundle(args):
    args.manifest_path = args.manifest_path.expanduser().resolve()
    project = args.manifest_path.parent
    if args.clap_validator:
        args.clap_validator = args.clap_validator.expanduser().resolve()
    paths, settings_command = plugin_sdks.discover(project, {name: getattr(args, name + "_sdk")
                                                            for name in plugin_sdks.ENVIRON}, settings=args.settings)
    if args.sign_command is None:
        args.sign_command = settings_command
    preflight(args, paths)
    if args.target and not shutil.which("wine"):
        raise ValueError("--target windows requires Wine for product metadata")
    if args.check:
        return []
    runner = Runner(project)
    if args.offline:
        runner.environment["CARGO_NET_OFFLINE"] = "true"
    target, library, archive, links = build_product(args, runner)
    product = product_metadata(target, library, runner, args.target)
    profile = "release" if args.release else "debug"
    platform = "win32" if args.target else sys.platform
    output = (args.output_dir or target / "plugins" / ("windows" if args.target else "") / profile).resolve()
    output.mkdir(parents=True, exist_ok=True)
    # Filenames use the validated Cargo package, never arbitrary product display names.
    stem = args.package.replace("-", "_")
    artifacts = []
    for format in args.formats:
        artifact = output / f"{args.package}.{SUFFIX[format]}"
        build = None
        if format == "clap":
            clap_bundle(library, artifact, product, platform)
        elif format == "vst3":
            sources = generic_vst3_bundle.build_generic(ROOT, paths["vst3"], stem, profile,
                package=args.package, runner=runner, cross=args.target, product_library=library,
                target_directory=target, validate=args.validate, sign=False, run_validation=False, bundle_name=args.package)
            copy_artifact(sources[0], artifact)
            build = target / "vst3/generic" / stem / (f"windows-{profile}" if args.target else profile)
        elif format == "aax":
            selected = aax_plugin.product(None, package=args.package, name=args.package, identifier=product["plugin_id"] + ".aax")
            built = aax_plugin.build(ROOT, paths["aax"], selected, runner=runner, cross=args.target,
                product_library=archive, link_libraries=links, target_directory=target,
                profile=profile, validate=args.validate, version=product["version"])
            copy_artifact(built.artifact, artifact)
            build = built.build
        elif format == "lv2":
            context = lv2_generic.prepare_generic(ROOT, paths["lv2"], crate=args.package, runner=runner, cross=args.target)
            _, build = lv2_generic.build_generic(context, product_library=library, target_directory=target,
                                                 profile=profile, validate=args.validate)
            source = lv2_generic.package_generic(context, target, build, sign=False)
            copy_artifact(source, artifact)
        else:
            build = target / "au-native" / stem / profile
            options = SimpleNamespace(audio_unit_sdk=paths["audio_unit"], mock=None, product_library=library,
                                      manifest_path=args.manifest_path, plugin=stem, build_dir=build,
                                      validate=args.validate)
            copy_artifact(au_native_bundle.build(ROOT, options, runner=runner, sign=False, profile=profile), artifact)
        write_notices(artifact, format, platform, args.package, args.manifest_path, runner, args.target)
        sign(artifact, format, runner, args.sign_command, platform=platform)
        if args.validate:
            validate(format, artifact, runner, build=build, cross=args.target, clap_validator=args.clap_validator)
        artifacts.append(artifact)
    return artifacts


def main(argv=None):
    cli = parser()
    args = cli.parse_args(argv)
    try:
        for artifact in bundle(args):
            print(artifact)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        cli.exit(1, f"bundle-plugin: {error}\n")
