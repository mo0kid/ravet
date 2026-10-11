//! Exact caller-selected native CFF/CFF2 fonts consumed through existing renderers.
#[cfg(all(target_os = "macos", feature = "live-ui"))]
#[path = "../postscript_fonts/browser.rs"]
mod browser;
#[cfg(target_os = "macos")]
#[path = "../postscript_fonts/export.rs"]
mod export;
#[cfg(target_os = "macos")]
#[path = "../postscript_fonts/first_party.rs"]
mod first_party;
#[cfg(target_os = "macos")]
#[path = "../postscript_fonts/glyphs.rs"]
mod glyphs;
#[cfg(all(target_os = "macos", feature = "live-ui"))]
#[path = "../postscript_fonts/live.rs"]
mod live;
#[cfg(target_os = "macos")]
#[path = "../postscript_fonts/model.rs"]
mod model;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    #[cfg(target_os = "macos")]
    {
        if args == ["--glyph-oracle"] {
            return glyphs::run();
        }
        if args == ["--export-first-party"] {
            export::run();
            return Ok(());
        }
        #[cfg(feature = "live-ui")]
        {
            if args.len() == 2 && args[0] == "--browser-fixture" {
                return browser::run(&args[1]);
            }
        }
    }
    #[cfg(all(target_os = "macos", feature = "live-ui"))]
    {
        live::run(&args)
    }
    #[cfg(not(all(target_os = "macos", feature = "live-ui")))]
    {
        let _ = args;
        Err("postscript_fonts requires macOS; window modes also require live-ui".into())
    }
}
