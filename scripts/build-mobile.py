#!/usr/bin/env python3
"""Build public AUv3/mobile fixtures; never install, launch or provision devices."""
import argparse
import json
import os
import pathlib
import shutil
import subprocess
import sys
import zipfile
import xml.etree.ElementTree as ET


def run(arguments, *, cwd=None, env=None):
    subprocess.run([str(value) for value in arguments], cwd=cwd, env=env, check=True)


def validate_choices(parser, args):
    """Reject incompatible deployment choices before invoking build tools."""
    if args.platform == "android":
        if args.deployment_target is not None:
            parser.error("--deployment-target is an Apple build option")
        if args.android_min_api < 26:
            parser.error("AAudio fixtures require --android-min-api >= 26")
        if args.android_target_sdk < args.android_min_api:
            parser.error("Android target SDK must be >= minimum API")
        if args.android_compile_sdk < max(36, args.android_target_sdk):
            parser.error("Android compile SDK must be >= 36 and >= target SDK")
    else:
        minimum = 12 if args.platform == "macos" else 17
        version = args.deployment_target or f"{minimum}.0"
        parts = version.split(".")
        if not 1 <= len(parts) <= 3 or any(not part.isascii() or not part.isdigit() for part in parts):
            parser.error("Deployment target must be a numeric Apple version")
        if int(parts[0]) < minimum:
            parser.error(f"{args.platform} fixtures require deployment target >= {minimum}")
        args.deployment_target = version
    if any(not part.isascii() or not part.isdigit() for part in args.android_build_tools.split(".")):
        parser.error("Android build-tools version must be numeric")


def android_apk(root, target, build, sdk, java, ndk, args):
    sdk, java = sdk.resolve(), java.resolve()
    platforms = sorted((sdk / "platforms").glob(f"android-{args.android_compile_sdk}/android.jar"))
    build_tools = list((sdk / "build-tools").glob(f"{args.android_build_tools}/aapt2"))
    if not platforms or not build_tools:
        raise SystemExit(f"APK packaging requires external Android platform {args.android_compile_sdk} and build-tools {args.android_build_tools}")
    tools = build_tools[-1].parent
    env = dict(os.environ, JAVA_HOME=str(java), PATH=str(java / "bin") + os.pathsep + os.environ["PATH"])
    source = root / "examples/gain-mobile/android/app/src/main"
    classes = build / "java"
    if classes.exists():
        shutil.rmtree(classes)
    classes.mkdir(parents=True)
    run([java / "bin/javac", "-Xlint:all", "-Werror", "-classpath", platforms[0], "-d", classes,
         source / "java/org/ravet/gain/MainActivity.java"])
    dex = build / "dex"
    dex.mkdir(exist_ok=True)
    run([tools / "d8", "--min-api", str(args.android_min_api), "--lib", platforms[0], "--output", dex,
         *sorted(classes.rglob("*.class"))], env=env)
    unsigned = build / "unsigned.apk"
    ET.register_namespace("android", "http://schemas.android.com/apk/res/android")
    manifest = ET.parse(source / "AndroidManifest.xml")
    manifest.getroot().set("package", "org.ravet.gain")
    packaged_manifest = build / "AndroidManifest.xml"
    manifest.write(packaged_manifest, encoding="utf-8", xml_declaration=True)
    run([tools / "aapt2", "link", "-I", platforms[0], "--manifest", packaged_manifest,
         "--min-sdk-version", str(args.android_min_api), "--target-sdk-version", str(args.android_target_sdk), "--version-code", "1", "--version-name", "0.0.1", "-o", unsigned])
    with zipfile.ZipFile(unsigned, "a") as archive:
        archive.write(dex / "classes.dex", "classes.dex")
        archive.write(build / "libravet_gain_mobile.so", "lib/arm64-v8a/libravet_gain_mobile.so")
        archive.write(root / "LICENSE", "assets/RAVET-LICENSE.txt")
        for notice in sorted((root / "native/common/notices").glob("*.txt")):
            archive.write(notice, "assets/" + notice.name)
        for name in ("NOTICE", "NOTICE.toolchain"):
            archive.write(ndk / name, "assets/NDK-" + name + ".txt")
    aligned = build / "aligned.apk"
    run([tools / "zipalign", "-P", "16", "-f", "4", unsigned, aligned])
    keystore = target / "mobile/debug.keystore"
    if not keystore.exists():
        run([java / "bin/keytool", "-genkeypair", "-keystore", keystore, "-storepass", "android", "-keypass", "android",
             "-alias", "ravet-debug", "-keyalg", "RSA", "-keysize", "2048", "-validity", "3650", "-dname", "CN=RAVET development fixture"])
    artifact = build / "ravet-gain-debug.apk"
    run([tools / "apksigner", "sign", "--ks", keystore, "--ks-key-alias", "ravet-debug", "--ks-pass", "pass:android",
         "--key-pass", "pass:android", "--out", artifact, aligned], env=env)
    run([tools / "apksigner", "verify", artifact], env=env)
    run([tools / "zipalign", "-c", "-P", "16", "4", artifact])
    return artifact


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("platform", choices=("macos", "ios", "ios-sim", "android"))
    parser.add_argument("--ndk", type=pathlib.Path, help="External Android NDK r29 root")
    parser.add_argument("--android-sdk", type=pathlib.Path, help="Also package a debug APK using an external Android SDK")
    parser.add_argument("--java-home", type=pathlib.Path, help="JDK 17 root for APK packaging")
    parser.add_argument("--embed-contracts", action="store_true", help="Include explicit iOS simulator native contract runner")
    parser.add_argument("--deployment-target", help="Apple minimum OS (defaults: macOS 12.0, iOS/iPadOS 17.0)")
    parser.add_argument("--android-min-api", type=int, default=26, help="Minimum Android API, >= 26")
    parser.add_argument("--android-compile-sdk", type=int, default=36, help="Installed Android compile SDK, >= 36")
    parser.add_argument("--android-target-sdk", type=int, default=36, help="Android target SDK, between minimum and compile SDK")
    parser.add_argument("--android-build-tools", default="36.0.0", help="Installed Android build-tools version")
    args = parser.parse_args()
    validate_choices(parser, args)
    if args.platform != "android" and sys.platform != "darwin":
        parser.error("Apple fixtures require macOS and Xcode")
    if args.embed_contracts and args.platform != "ios-sim":
        parser.error("--embed-contracts is limited to the simulator fixture")
    root = pathlib.Path(__file__).resolve().parents[1]
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"], cwd=root))
    target = pathlib.Path(metadata["target_directory"])
    env = dict(os.environ, CARGO_BUILD_JOBS=os.environ.get("CARGO_BUILD_JOBS", "2"))
    if args.platform != "android":
        env["MACOSX_DEPLOYMENT_TARGET" if args.platform == "macos" else "IPHONEOS_DEPLOYMENT_TARGET"] = args.deployment_target
    cargo_target = {"ios": "aarch64-apple-ios", "ios-sim": "aarch64-apple-ios-sim", "android": "aarch64-linux-android"}.get(args.platform)
    cargo = ["cargo", "build", "-p", "ravet-gain-native", "--release", "--locked"]
    if cargo_target:
        cargo.extend(["--target", cargo_target])
    if args.platform == "android":
        if not args.ndk:
            parser.error("Android requires --ndk (external SDKs are never downloaded automatically)")
        ndk = args.ndk.resolve()
        toolchain = ndk / "build/cmake/android.toolchain.cmake"
        prebuilt = {"darwin": "darwin-x86_64", "linux": "linux-x86_64"}.get(sys.platform)
        if not prebuilt or not toolchain.is_file():
            parser.error("Supply an external macOS/Linux Android NDK")
        linker = ndk / "toolchains/llvm/prebuilt" / prebuilt / f"bin/aarch64-linux-android{args.android_min_api}-clang"
        if not linker.is_file():
            parser.error(f"NDK has no matching arm64 API-{args.android_min_api} compiler")
        env["CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER"] = str(linker)
    run(cargo, cwd=root, env=env)
    library = target / cargo_target / "release/libravet_gain_native.a" if cargo_target else target / "release/libravet_gain_native.a"
    build = target / ("mobile/android" if args.platform == "android" else f"auv3/{args.platform}")
    cmake = ["cmake", "-S", root / ("examples/gain-mobile/android" if args.platform == "android" else "examples/gain-auv3"),
             "-B", build, "-DCMAKE_BUILD_TYPE=Release", f"-DRAVET_GAIN_LIBRARY={library}"]
    if args.platform == "android":
        cmake.extend([f"-DCMAKE_TOOLCHAIN_FILE={toolchain}", "-DANDROID_ABI=arm64-v8a", f"-DANDROID_PLATFORM=android-{args.android_min_api}", "-DANDROID_STL=c++_static"])
    elif cargo_target:
        cmake.extend(["-DCMAKE_SYSTEM_NAME=iOS", f"-DCMAKE_OSX_SYSROOT={'iphoneos' if args.platform == 'ios' else 'iphonesimulator'}",
                      "-DCMAKE_OSX_ARCHITECTURES=arm64", f"-DCMAKE_OSX_DEPLOYMENT_TARGET={args.deployment_target}",
                      f"-DRAVET_EMBED_AUV3_CONTRACTS={'ON' if args.embed_contracts else 'OFF'}"])
    else:
        cmake.append(f"-DCMAKE_OSX_DEPLOYMENT_TARGET={args.deployment_target}")
    run(cmake)
    run(["cmake", "--build", build, "--parallel", "2"])
    if args.platform == "android":
        artifact = build / "libravet_gain_mobile.so"
        if args.android_sdk:
            if not args.java_home:
                parser.error("APK packaging requires --java-home for JDK 17")
            artifact = android_apk(root, target, build, args.android_sdk, args.java_home, ndk, args)
    else:
        if args.platform == "macos":
            run(["ctest", "--test-dir", build, "--output-on-failure"])
            artifact = build / "RavetGainContainer.app"
            extension = artifact / "Contents/PlugIns/RavetGainExtension.appex"
            extension_resources = extension / "Contents/Resources"
        else:
            artifact = build / "RavetGainMobile.app"
            extension = artifact / "PlugIns/RavetGainExtension.appex"
            extension_resources = extension
        extension_resources.mkdir(parents=True, exist_ok=True)
        shutil.copy2(root / "LICENSE", extension_resources / "RAVET-LICENSE.txt")
        for notice in sorted((root / "native/common/notices").glob("*.txt")):
            shutil.copy2(notice, extension_resources / notice.name)
            if args.platform != "macos":
                shutil.copy2(notice, artifact / notice.name)
        if args.platform != "ios":
            sign = ["codesign", "--force", "--sign", "-"]
            if args.platform == "macos":
                run([*sign, "--entitlements", root / "examples/gain-auv3/Extension.entitlements", extension])
            else:
                run([*sign, extension])
            run([*sign, artifact])
            run(["codesign", "--verify", "--deep", "--strict", artifact])
    print(artifact)


if __name__ == "__main__":
    main()
