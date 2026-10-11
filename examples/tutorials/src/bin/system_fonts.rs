//! Exact copied system fonts consumed through the existing Rust text renderers.
#[cfg(all(target_os = "macos", feature = "live-ui"))]
#[path = "../system_fonts/browser.rs"]
mod browser;
#[cfg(target_os = "macos")]
#[path = "../system_fonts/first_party.rs"]
mod first_party;
#[cfg(target_os = "macos")]
#[path = "../system_fonts/glyphs.rs"]
mod glyphs;
#[cfg(all(target_os = "macos", feature = "live-ui"))]
#[path = "../system_fonts/live.rs"]
mod live;
#[cfg(target_os = "macos")]
#[path = "../system_fonts/model.rs"]
mod model;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    #[cfg(target_os = "macos")]
    {
        if args == ["--glyph-oracle"] {
            return glyphs::run();
        }
        if args == ["--export-first-party"] {
            first_party::run();
            return Ok(());
        }
        #[cfg(feature = "live-ui")]
        {
            if args.is_empty() {
                return live::run(false);
            }
            if args == ["--fixture"] {
                return live::run(true);
            }
            if args.len() == 2 && args[0] == "--browser-fixture" {
                return browser::run(&args[1]);
            }
        }
    }
    let _ = args;
    Err("system_fonts requires macOS; native window modes also require live-ui".into())
}
