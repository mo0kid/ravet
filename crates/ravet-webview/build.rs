use std::{env, path::PathBuf, process::Command};
fn run(command: &mut Command) {
    assert!(
        command
            .status()
            .expect("native build tool could not start")
            .success(),
        "native build command failed"
    );
}
fn main() {
    println!("cargo:rerun-if-changed=../ravet-surface/native/RavetWebView.mm");
    println!("cargo:rerun-if-changed=../ravet-surface/native/RavetWebView.h");
    println!("cargo:rerun-if-changed=../ravet-surface/native/RavetAccessibility.inc");
    println!("cargo:rerun-if-changed=../ravet-surface/native/RavetObjcNames.h");
    println!("cargo:rerun-if-changed=native/StandaloneWindow.mm");
    println!("cargo:rerun-if-changed=native/StandaloneWindowWindows.cpp");
    println!("cargo:rerun-if-changed=native/StandaloneWindowLinux.cpp");
    println!("cargo:rerun-if-changed=../ravet-surface/native/RavetWebViewWindows.cpp");
    println!("cargo:rerun-if-changed=../ravet-surface/native/RavetWebViewLinux.cpp");
    match env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("macos") => {}
        Ok("windows") => return windows(),
        Ok("linux") => return linux(),
        _ => return,
    }
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        _ => panic!("unsupported macOS architecture"),
    };
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    // Distinct Objective-C class names per build tree (see RavetObjcNames.h):
    // FNV-1a of OUT_DIR, which differs between an app and any plugin it hosts.
    let hash = out
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |h, &b| {
            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
        });
    let suffix = format!("-DRAVET_OBJC_SUFFIX=cargo_{hash:016x}");
    run(Command::new("xcrun")
        .args([
            "--sdk",
            "macosx",
            "clang++",
            suffix.as_str(),
            "-std=c++17",
            "-fobjc-arc",
            "-mmacosx-version-min=12.0",
            "-arch",
            arch,
            "-c",
            "../ravet-surface/native/RavetWebView.mm",
            "-o",
        ])
        .arg(out.join("webview.o")));
    run(Command::new("xcrun")
        .args([
            "--sdk",
            "macosx",
            "clang++",
            "-std=c++17",
            "-fobjc-arc",
            "-mmacosx-version-min=12.0",
            "-arch",
            arch,
            "-c",
            "native/StandaloneWindow.mm",
            "-o",
        ])
        .arg(out.join("window.o")));
    run(Command::new("xcrun")
        .args(["ar", "rcs"])
        .arg(out.join("libravet_webview.a"))
        .arg(out.join("webview.o"))
        .arg(out.join("window.o")));
    if env::var_os("CARGO_FEATURE_SYSTEM_FONTS").is_some() {
        println!("cargo:rerun-if-changed=native/SystemFonts.mm");
        println!("cargo:rerun-if-changed=native/SystemFonts.h");
        let postscript = if env::var_os("CARGO_FEATURE_POSTSCRIPT_FONTS").is_some() {
            "-DRAVET_POSTSCRIPT_FONTS=1"
        } else {
            "-DRAVET_POSTSCRIPT_FONTS=0"
        };
        run(Command::new("xcrun")
            .arg("--sdk")
            .arg("macosx")
            .arg("clang++")
            .arg(postscript)
            .args([
                "-std=c++17",
                "-fobjc-arc",
                "-fobjc-arc-exceptions",
                "-mmacosx-version-min=12.0",
                "-arch",
                arch,
                "-c",
                "native/SystemFonts.mm",
                "-o",
            ])
            .arg(out.join("fonts.o")));
        run(Command::new("xcrun")
            .args(["ar", "rcs"])
            .arg(out.join("libravet_webview.a"))
            .arg(out.join("fonts.o")));
        println!("cargo:rustc-link-lib=framework=CoreText");
    }
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=ravet_webview");
    println!("cargo:rustc-link-lib=framework=Cocoa");
    println!("cargo:rustc-link-lib=framework=WebKit");
    println!("cargo:rustc-link-lib=c++");
}

/// Win32/WebView2 backend. `RAVET_WEBVIEW2_SDK` names an extracted
/// Microsoft.Web.WebView2 NuGet package; consumers ship its x64
/// `WebView2Loader.dll` beside the binary that links this crate.
fn windows() {
    println!("cargo:rerun-if-env-changed=RAVET_WEBVIEW2_SDK");
    let sdk = PathBuf::from(
        env::var_os("RAVET_WEBVIEW2_SDK")
            .expect("set RAVET_WEBVIEW2_SDK to the extracted Microsoft.Web.WebView2 NuGet package"),
    );
    let include = sdk.join("build/native/include");
    assert!(
        include.join("WebView2.h").exists(),
        "RAVET_WEBVIEW2_SDK has no build/native/include/WebView2.h"
    );
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .include(include)
        .define("NOMINMAX", None)
        .define("UNICODE", None)
        .define("_UNICODE", None)
        .file("../ravet-surface/native/RavetWebViewWindows.cpp")
        .file("native/StandaloneWindowWindows.cpp");
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu") {
        build.include("../ravet-surface/native/mingw");
    } else {
        build.flag("/EHsc").flag("/utf-8");
    }
    build.compile("ravet_webview");
    for library in ["ole32", "user32", "shell32", "shlwapi", "uuid", "gdi32"] {
        println!("cargo:rustc-link-lib={library}");
    }
}

/// GTK 3/WebKitGTK 4.1 backend, X11 only. Set
/// `RAVET_WEB_LINUX_SOFTWARE_RENDERING=1` for WebKit's non-accelerated painting.
fn linux() {
    println!("cargo:rerun-if-env-changed=RAVET_WEB_LINUX_SOFTWARE_RENDERING");
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .flag_if_supported("-Wno-misleading-indentation")
        .flag_if_supported("-Wno-deprecated-declarations");
    // Probe without emitting link flags: the static library must precede the
    // shared GTK/WebKit libraries on the linker command line.
    let mut libraries = Vec::new();
    for (package, version) in [("webkit2gtk-4.1", "2.40"), ("json-glib-1.0", "1.0")] {
        let library = pkg_config::Config::new()
            .atleast_version(version)
            .cargo_metadata(false)
            .probe(package)
            .unwrap_or_else(|error| panic!("{package} development files are required: {error}"));
        build.includes(&library.include_paths);
        libraries.push(library);
    }
    let software = env::var("RAVET_WEB_LINUX_SOFTWARE_RENDERING").is_ok_and(|v| v == "1");
    build
        .define(
            "RAVET_WEB_LINUX_SOFTWARE_RENDERING",
            if software { "1" } else { "0" },
        )
        .file("../ravet-surface/native/RavetWebViewLinux.cpp")
        .file("native/StandaloneWindowLinux.cpp")
        .compile("ravet_webview");
    for library in &libraries {
        for path in &library.link_paths {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
        for name in &library.libs {
            println!("cargo:rustc-link-lib={name}");
        }
    }
    println!("cargo:rustc-link-lib=dl");
}
