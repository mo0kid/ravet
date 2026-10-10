use std::{env, path::PathBuf, process::Command};
fn run(command: &mut Command) {
    assert!(
        command
            .status()
            .expect("native media build tool could not start")
            .success(),
        "native media build command failed"
    );
}
fn main() {
    println!("cargo:rerun-if-changed=native/Video.mm");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let architecture = match env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        _ => panic!("unsupported macOS architecture"),
    };
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    run(Command::new("xcrun")
        .args([
            "--sdk",
            "macosx",
            "clang++",
            "-std=c++17",
            "-fobjc-arc",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-mmacosx-version-min=12.0",
            "-arch",
            architecture,
            "-c",
            "native/Video.mm",
            "-o",
        ])
        .arg(output.join("video.o")));
    run(Command::new("xcrun")
        .args(["ar", "rcs"])
        .arg(output.join("libravet_media.a"))
        .arg(output.join("video.o")));
    println!("cargo:rustc-link-search=native={}", output.display());
    println!("cargo:rustc-link-lib=static=ravet_media");
    for framework in ["AVFoundation", "CoreGraphics", "CoreMedia", "Foundation"] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
    println!("cargo:rustc-link-lib=c++");
}
