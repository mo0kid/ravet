//! Explicit Rust color preparation consumed by a resizable ordinary HTML window.
//! --export prepares JSON without a window; native mode requires macOS/live-ui.
#[path = "../color_workspace/model.rs"]
mod model;
#[cfg(all(target_os = "macos", feature = "live-ui"))]
mod live {
    use super::model::{Model, Palette};
    use ravet_webview::{Callbacks, StandaloneOptions, StandaloneWindow};
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
        time::{Duration, Instant},
    };
    struct Bridge {
        model: Rc<RefCell<Model>>,
        drops: Rc<Cell<usize>>,
    }
    impl Callbacks for Bridge {
        fn message(&mut self, json: &str) {
            self.model.borrow_mut().message(json);
        }
        fn snapshot(&mut self) -> Option<String> {
            Some(self.model.borrow().snapshot.clone())
        }
    }
    impl Drop for Bridge {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
        }
    }
    fn settled(
        window: &StandaloneWindow,
        model: &Rc<RefCell<Model>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            window.pump(0.02);
            let expected = model.borrow().snapshot.clone();
            let script = format!(
                "(()=>{{if(typeof ravetColorReport!=='function')return false;const e={expected},r=ravetColorReport();return r.applied?.revision===e.revision&&r.applied.opacity===e.opacity&&r.applied.palette===e.palette&&JSON.stringify(r.numeric)===JSON.stringify(e.numeric)&&JSON.stringify(r.linear)===JSON.stringify(e.linear)&&r.opacity===String(e.opacity)&&r.palette===e.palette;}})()"
            );
            if window.eval(&script)? {
                return Ok(());
            }
        }
        Err("Owned color consumer failed to settle Rust-produced pixels".into())
    }
    pub fn run(fixture: bool) -> Result<(), Box<dyn std::error::Error>> {
        let window = StandaloneWindow::new(StandaloneOptions {
            title: "RAVET Color Preparation".into(),
            width: 1000.,
            height: 740.,
            hidden: fixture,
            ..StandaloneOptions::default()
        })?;
        let model = Rc::new(RefCell::new(Model::new()));
        let drops = Rc::new(Cell::new(0));
        let editor = window.embed(
            include_str!("../color_workspace/index.html"),
            Bridge {
                model: model.clone(),
                drops: drops.clone(),
            },
        )?;
        if !fixture {
            while window.pump(0.02) {}
            drop(editor);
            return Ok(());
        }
        settled(&window, &model)?;
        assert!(
            window
                .eval("ravetColorReport().linear[0]===188&&ravetColorReport().numeric[0]===128")?
        );
        println!("Actual WK Rust linear188 versus numeric128 image bytes PASS");
        for opacity in [0, 1, 64, 255, 128] {
            assert!(window.eval(&format!("(()=>{{const e=document.getElementById('opacity');e.value='{opacity}';e.dispatchEvent(new Event('input',{{bubbles:true}}));return true;}})()"))?);
            for _ in 0..100 {
                window.pump(0.02);
                if model.borrow().opacity == opacity {
                    break;
                }
            }
            assert_eq!(model.borrow().opacity, opacity);
            settled(&window, &model)?;
            println!(
                "Actual WK input transport opacity{opacity} and Rust-produced pixel arrays PASS"
            );
        }
        assert!(window.eval("(()=>{const e=document.getElementById('palette');e.value='amber';e.dispatchEvent(new Event('change',{bubbles:true}));return true;})()")?);
        for _ in 0..100 {
            window.pump(0.02);
            if model.borrow().palette == Palette::Amber {
                break;
            }
        }
        assert_eq!(model.borrow().palette, Palette::Amber);
        settled(&window, &model)?;
        println!("Actual WK palette transport and Rust-produced amber arrays PASS");
        let preserved = model.borrow().snapshot.clone();
        assert!(
            window.eval("window.webkit.messageHandlers.ravet.postMessage({opacity:256});true")?
        );
        window.pump(0.1);
        assert_eq!(model.borrow().snapshot, preserved);
        assert!(window.eval(
            "document.getElementById('palette').focus();document.activeElement.id==='palette'"
        )?);
        for (width, height) in [(700., 1000.), (1400., 650.), (640., 540.)] {
            assert!(window.set_size(width, height));
            window.pump(0.1);
            settled(&window, &model)?;
            let (w, h) = window.content_size().unwrap();
            assert!(window.eval(&format!("(()=>{{const r=ravetColorReport();return Math.abs(r.viewport[0]-{w})<=1&&Math.abs(r.viewport[1]-{h})<=1&&r.focus==='palette';}})()"))?);
            assert_eq!(model.borrow().snapshot, preserved);
            println!("Actual owned resize{w}x{h}, source/selected control/focus retained PASS");
        }
        assert!(window.open_settings());
        assert!(window.eval("document.getElementById('io-settings').open&&!document.getElementById('io-unavailable').hidden")?);
        assert!(window.eval("document.getElementById('close-settings').click();!document.getElementById('io-settings').open")?);
        assert!(window.request_close());
        assert!(window.state().unwrap().closed);
        drop(editor);
        assert_eq!(drops.get(), 1);
        drop(window);
        println!(
            "Default no-device popup and closed-parent/editor callback exactly-once cleanup PASS"
        );
        Ok(())
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "--export") {
        match args.as_slice() {
            [_, opacity, palette] => {
                let opacity: u8 = opacity.parse()?;
                let palette = model::Palette::parse(palette).ok_or("Use white or amber palette")?;
                let mut model = model::Model::new();
                assert!(model.message(&format!("{{\"opacity\":{opacity}}}")));
                assert!(model.message(&format!("{{\"palette\":\"{}\"}}", palette.name())));
                println!("{}", model.snapshot);
            }
            [_, all] if all == "all" => {
                print!("{{");
                let mut first = true;
                for palette in [model::Palette::White, model::Palette::Amber] {
                    for opacity in 0..=255 {
                        if !first {
                            print!(",");
                        }
                        first = false;
                        print!(
                            "\"{}:{opacity}\":{}",
                            palette.name(),
                            model::snapshot(opacity, palette, 0)
                        );
                    }
                }
                println!("}}");
            }
            _ => return Err("Use --export OPACITY white|amber or --export all".into()),
        }
        return Ok(());
    }
    if !args.is_empty() && args != ["--fixture"] {
        return Err("Use --fixture or --export".into());
    }
    #[cfg(all(target_os = "macos", feature = "live-ui"))]
    {
        live::run(args == ["--fixture"])
    }
    #[cfg(not(all(target_os = "macos", feature = "live-ui")))]
    {
        Err("Native color workspace requires macOS/live-ui; --export is portable".into())
    }
}
