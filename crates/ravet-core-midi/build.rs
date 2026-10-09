fn main() {
    println!("cargo:rerun-if-changed=native/core-midi.mm");
    #[cfg(target_os = "macos")]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new()
            .cpp(true)
            .flag("-std=c++17")
            .flag("-fblocks")
            .file("native/core-midi.mm")
            .compile("ravet_core_midi");
        println!("cargo:rustc-link-lib=framework=CoreMIDI");
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
    }
}
