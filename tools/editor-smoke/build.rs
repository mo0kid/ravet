use std::{env, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-changed=src/probe.mm");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        _ => panic!("unsupported macOS architecture"),
    };
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    assert!(
        Command::new("xcrun")
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
                "src/probe.mm",
                "-o"
            ])
            .arg(out.join("probe.o"))
            .status()
            .expect("Xcode required")
            .success()
    );
    assert!(
        Command::new("xcrun")
            .args(["ar", "rcs"])
            .arg(out.join("libeditor_probe.a"))
            .arg(out.join("probe.o"))
            .status()
            .unwrap()
            .success()
    );
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=editor_probe");
    println!("cargo:rustc-link-lib=framework=Cocoa");
    println!("cargo:rustc-link-lib=framework=WebKit");
    println!("cargo:rustc-link-lib=c++");
}
