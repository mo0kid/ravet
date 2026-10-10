use std::{env, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-changed=native/DirectoryPicker.mm");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        _ => panic!("Unsupported macOS architecture"),
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
                "native/DirectoryPicker.mm",
                "-o"
            ])
            .arg(out.join("picker.o"))
            .status()
            .expect("Directory picker compiler")
            .success()
    );
    assert!(
        Command::new("xcrun")
            .args(["ar", "rcs"])
            .arg(out.join("libfoundry_picker.a"))
            .arg(out.join("picker.o"))
            .status()
            .expect("Directory picker archive")
            .success()
    );
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=foundry_picker");
    println!("cargo:rustc-link-lib=framework=Cocoa");
}
