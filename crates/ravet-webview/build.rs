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
    println!("cargo:rerun-if-changed=native/StandaloneWindow.mm");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        _ => panic!("unsupported macOS architecture"),
    };
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
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
