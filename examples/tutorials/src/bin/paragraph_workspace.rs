//! Exact-font mixed-bidi paragraphs in an ordinary standalone window.
#[cfg(all(target_os = "macos", feature = "live-ui"))]
#[path = "../paragraph_workspace/live.rs"]
mod live;
#[path = "../paragraph_workspace/model.rs"]
mod model;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    #[cfg(all(target_os = "macos", feature = "live-ui"))]
    if args.first().is_some_and(|a| a == "--browser-fixture") {
        return live::browser_fixture(std::path::Path::new(
            args.get(1).ok_or("Missing browser fixture path")?,
        ));
    }
    if args.iter().any(|a| a == "--export") {
        let mut m = model::Model::new(
            model::authored_fonts()?,
            "Authored local fonts + Noto Arabic",
        )?;
        print!("[");
        let mut count = 0;
        for sample in 0..8 {
            for raster in [false, true] {
                for rtl in [false, true] {
                    for width in (80..=640).step_by(80) {
                        m.change(model::State {
                            sample,
                            raster,
                            width,
                            rtl,
                            ..m.state()
                        })?;
                        if count > 0 {
                            print!(",");
                        }
                        print!("{}", m.snapshot);
                        count += 1;
                    }
                }
            }
        }
        // Additional real Rust states exercise size/alignment in the browser
        // without multiplying the entire image corpus.
        for size in 0..3 {
            for alignment in 0..3 {
                for raster in [false, true] {
                    m.change(model::State {
                        sample: 0,
                        width: 320,
                        size,
                        alignment,
                        raster,
                        rtl: false,
                    })?;
                    print!(",{}", m.snapshot);
                }
            }
        }
        println!("]");
        return Ok(());
    }
    #[cfg(all(target_os = "macos", feature = "live-ui"))]
    {
        live::run(args.iter().any(|a| a == "--fixture"))
    }
    #[cfg(not(all(target_os = "macos", feature = "live-ui")))]
    {
        Err("Use --export or enable live-ui on macOS".into())
    }
}
