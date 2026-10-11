//! Explicit SVG import image consumer. --export is headless; native requires live-ui.
#[path = "../svg_workspace/model.rs"]
mod model;
#[cfg(all(target_os = "macos", feature = "live-ui"))]
mod live {
    use super::model::{ASSETS, Model, base64};
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
        w: &StandaloneWindow,
        m: &Rc<RefCell<Model>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let end = Instant::now() + Duration::from_secs(5);
        while Instant::now() < end {
            w.pump(0.02);
            let expected = m.borrow().snapshot.clone();
            if w.eval(&format!("(()=>{{if(typeof ravetSvgReport!=='function')return false;let r=ravetSvgReport(),e={expected};return !!r&&r.revision===e.revision&&r.source===e.source&&r.fit===e.fit&&r.size===e.size&&r.straight===e.straight&&r.maxPmaError<=1;}})()"))?{return Ok(());}
        }
        Err("Owned SVG consumer failed to settle actual Rust pixels".into())
    }
    pub fn run(fixture: bool) -> Result<(), Box<dyn std::error::Error>> {
        let w = StandaloneWindow::new(StandaloneOptions {
            title: "RAVET SVG image import".into(),
            width: 900.,
            height: 640.,
            hidden: fixture,
            ..Default::default()
        })?;
        let model = Rc::new(RefCell::new(Model::new()));
        let drops = Rc::new(Cell::new(0));
        let editor = w.embed(
            include_str!("../svg_workspace/index.html"),
            Bridge {
                model: model.clone(),
                drops: drops.clone(),
            },
        )?;
        if !fixture {
            while w.pump(0.02) {}
            drop(editor);
            return Ok(());
        }
        settled(&w, &model)?;
        for source in ["scene", "logo", "integer"] {
            for fit in ["contain", "stretch"] {
                for size in ["128x64", "96x48", "160x48"] {
                    for (id, value) in [("source", source), ("fit", fit), ("size", size)] {
                        assert!(w.eval(&format!("(()=>{{const n=document.getElementById('{id}');n.value='{value}';n.dispatchEvent(new Event('change',{{bubbles:true}}));return true;}})()"))?);
                        for _ in 0..20 {
                            w.pump(0.02);
                        }
                    }
                    settled(&w, &model)?;
                    let svg = ASSETS.iter().find(|(name, _)| *name == source).unwrap().1;
                    assert!(w.eval(&format!("window.__svgOracle=null;ravetSvgOracle(atob({:?})).then(r=>window.__svgOracle=r);true",base64(svg.as_bytes())))?);
                    let end = Instant::now() + Duration::from_secs(5);
                    let mut passed = false;
                    while Instant::now() < end {
                        w.pump(0.02);
                        if w.eval("!!window.__svgOracle&&window.__svgOracle.pass")? {
                            passed = true;
                            break;
                        }
                    }
                    if !passed {
                        let _ = w.eval("console.error(JSON.stringify(window.__svgOracle));true");
                        return Err(format!(
                            "Independent WK SVG oracle failed {source}/{fit}/{size}"
                        )
                        .into());
                    }
                    println!(
                        "Actual WK Rustimport/transport/pixels and independent SVG oracle {source}/{fit}/{size} PASS"
                    );
                }
            }
        }
        let prior = model.borrow().snapshot.clone();
        assert!(w.eval(&format!("(()=>{{const before=ravetSvgReport(),e={prior};e.width=String(e.width);e.revision++;ravetUpdate(e);return JSON.stringify(ravetSvgReport())===JSON.stringify(before);}})()"))?);
        println!("Malformed frontend dimension retains full admitted image and revision PASS");
        assert!(
            w.eval("window.webkit.messageHandlers.ravet.postMessage({source:'unknown'});true")?
        );
        w.pump(0.1);
        assert_eq!(model.borrow().snapshot, prior);
        println!("Invalid source transition preserves admitted Rust image/model PASS");
        assert!(
            w.eval("document.getElementById('fit').focus();document.activeElement.id==='fit'")?
        );
        for (width, height) in [(640., 540.), (1200., 700.), (700., 1000.)] {
            assert!(w.set_size(width, height));
            w.pump(0.1);
            settled(&w, &model)?;
            let (cw, ch) = w.content_size().unwrap();
            assert!(w.eval(&format!("(()=>{{const r=ravetSvgReport();return r.focus==='fit'&&Math.abs(r.viewport[0]-{cw})<=1&&Math.abs(r.viewport[1]-{ch})<=1;}})()"))?);
            assert_eq!(model.borrow().snapshot, prior);
            println!("Actual measuredresize {cw}x{ch}/image/focus retained PASS");
        }
        assert!(w.open_settings());
        assert!(w.eval("document.getElementById('io-settings').open&&!document.getElementById('io-unavailable').hidden")?);
        assert!(w.eval("document.getElementById('close-settings').click();!document.getElementById('io-settings').open")?);
        assert!(w.request_close());
        drop(editor);
        assert_eq!(drops.get(), 1);
        drop(w);
        println!("Default settingspopup and closedparent/editor/context cleanup PASS");
        Ok(())
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--export"] {
        println!("{}", model::all_snapshots());
        return Ok(());
    }
    if !args.is_empty() && args != ["--fixture"] {
        return Err("Use --export or --fixture".into());
    }
    #[cfg(all(target_os = "macos", feature = "live-ui"))]
    {
        live::run(args == ["--fixture"])
    }
    #[cfg(not(all(target_os = "macos", feature = "live-ui")))]
    {
        Err("Native SVG consumer requires macOS/live-ui; --export is portable".into())
    }
}
