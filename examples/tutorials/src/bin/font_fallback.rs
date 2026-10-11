//! Local font fallback prepared in Rust and consumed by an ordinary standalone window.
#[path = "../font_fallback/model.rs"]
mod model;
#[cfg(all(target_os = "macos", feature = "live-ui"))]
mod live {
    use super::model::Model;
    use ravet_webview::{Callbacks, StandaloneOptions, StandaloneWindow};
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
        time::{Duration, Instant},
    };
    struct Bridge {
        model: Rc<RefCell<Model>>,
        drops: Rc<Cell<usize>>,
        fixture: bool,
    }
    impl Callbacks for Bridge {
        fn message(&mut self, json: &str) {
            if self.fixture && json.len() <= 32768 && json.starts_with("{\"fontOracleReceipt\":") {
                println!("WK oracle receipt {json}");
                return;
            }
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
        let end = Instant::now() + Duration::from_secs(5);
        while Instant::now() < end {
            window.pump(0.02);
            let expected = model.borrow().snapshot.clone();
            if window.eval(&format!("(()=>{{let r=typeof ravetFontReport==='function'&&ravetFontReport(),e={expected};return !!r&&r.exact&&r.revision===e.revision&&r.sample===e.sample&&r.order===e.order&&r.size===e.size&&r.mode===e.mode&&r.payload===e.payload&&r.source===e.source;}})()"))? {return Ok(());}
        }
        Err("Owned font consumer did not settle actual Rust rendering".into())
    }
    pub fn run(fixture: bool) -> Result<(), Box<dyn std::error::Error>> {
        let window = StandaloneWindow::new(StandaloneOptions {
            title: "RAVET local font fallback".into(),
            width: 900.,
            height: 640.,
            hidden: fixture,
            ..Default::default()
        })?;
        let model = Rc::new(RefCell::new(Model::new()?));
        let drops = Rc::new(Cell::new(0));
        let editor = window.embed(
            include_str!("../font_fallback/index.html"),
            Bridge {
                model: model.clone(),
                drops: drops.clone(),
                fixture,
            },
        )?;
        if !fixture {
            while window.pump(0.02) {}
            drop(editor);
            return Ok(());
        }
        settled(&window, &model)?;
        let mut groups = 0;
        for (sample, _, _, _) in super::model::SAMPLES {
            for order in ["explicit", "noto"] {
                for size in ["14", "20", "28"] {
                    for mode in ["svg", "raster"] {
                        for (id, value) in [
                            ("sample", sample),
                            ("order", order),
                            ("size", size),
                            ("mode", mode),
                        ] {
                            assert!(window.eval(&format!("(()=>{{let n=document.getElementById('{id}');n.value='{value}';n.dispatchEvent(new Event('change',{{bubbles:true}}));return true;}})()"))?);
                            window.pump(0.05);
                        }
                        settled(&window, &model)?;
                        groups += 1;
                        println!(
                            "Actual shared Rust/WK selected font/render {sample}/{order}/{size}/{mode} PASS"
                        );
                    }
                }
            }
        }
        let primary = super::model::base64(include_bytes!(
            "../../../../crates/ravet-surface/tests/fixtures/fallback/RavetFallbackPrimary.ttf"
        ));
        let complement = super::model::base64(include_bytes!(
            "../../../../crates/ravet-surface/tests/fixtures/fallback/RavetFallbackComplement.ttf"
        ));
        assert!(window.eval(&format!("window.fontOracleBytes=[['RavetOraclePrimary','{primary}'],['RavetOracleComplement','{complement}']];{};true",include_str!("../font_fallback/oracle.js")))?);
        let mut geometry_models = Vec::new();
        for sample in 0..super::model::SAMPLES.len() {
            for order in 0..2 {
                for size in 0..3 {
                    for raster in [false, true] {
                        geometry_models.push(super::model::prepare_for(
                            &model.borrow(),
                            sample,
                            order,
                            size,
                            raster,
                        )?);
                    }
                }
            }
        }
        assert!(window.eval(&format!("window.fontGeometryReceipt=null;window.fontGeometryError=null;fontAllGeometry([{}]).then(r=>{{window.webkit.messageHandlers.ravet.postMessage({{fontOracleReceipt:r}});window.fontGeometryReceipt=r;}}).catch(e=>window.fontGeometryError=String(e));true",geometry_models.join(",")))?);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !window.eval("!!window.fontGeometryReceipt||!!window.fontGeometryError")? {
            if Instant::now() >= deadline {
                return Err("Owned browser geometry oracle did not complete".into());
            }
            window.pump(0.02);
        }
        assert!(window.eval("!fontGeometryError&&fontGeometryReceipt.pass&&fontGeometryReceipt.widths.length===7&&fontGeometryReceipt.pixels.length===30")?);
        groups += 37;
        println!(
            "Actual WK CSS advances6 + NFC policy observation1 and SVG/CPU geometry30 PASS (width .02, mean1/max80/changed512 limits)"
        );
        let prior = model.borrow().snapshot.clone();
        assert!(window.eval(&format!("(()=>{{let before=JSON.stringify(ravetFontReport()),bad={prior};bad.size='14';bad.revision++;ravetUpdate(bad);return JSON.stringify(ravetFontReport())===before;}})()"))?);
        groups += 1;
        assert!(
            window
                .eval("window.webkit.messageHandlers.ravet.postMessage({sample:'unknown'});true")?
        );
        window.pump(0.1);
        assert_eq!(model.borrow().snapshot, prior);
        groups += 1;
        assert!(window.eval(
            "document.getElementById('order').focus();document.activeElement.id==='order'"
        )?);
        for (width, height) in [(640., 540.), (1200., 700.), (700., 1000.)] {
            assert!(window.set_size(width, height));
            window.pump(0.1);
            settled(&window, &model)?;
            let (width, height) = window.content_size().unwrap();
            assert!(window.eval(&format!("(()=>{{let r=ravetFontReport();return r.focus==='order'&&Math.abs(r.viewport[0]-{width})<=1&&Math.abs(r.viewport[1]-{height})<=1;}})()"))?);
            assert_eq!(model.borrow().snapshot, prior);
            groups += 1;
            println!("Measured own window/font source/focus resize {width}x{height} PASS");
        }
        assert!(window.open_settings());
        assert!(window.eval("document.getElementById('io-settings').open&&!document.getElementById('io-unavailable').hidden")?);
        assert!(window.eval("document.getElementById('close-settings').click();!document.getElementById('io-settings').open")?);
        groups += 1;
        assert!(window.request_close());
        drop(editor);
        assert_eq!(drops.get(), 1);
        drop(window);
        groups += 1;
        println!("{groups} shared font consumer groups PASS");
        Ok(())
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--export"] {
        let mut model = model::Model::new()?;
        let mut values = Vec::new();
        for sample in 0..model::SAMPLES.len() {
            for order in 0..2 {
                for size in 0..3 {
                    for raster in [false, true] {
                        for (id, value) in [
                            ("sample", model::SAMPLES[sample].0),
                            ("order", if order == 0 { "explicit" } else { "noto" }),
                            ("size", ["14", "20", "28"][size]),
                            ("mode", if raster { "raster" } else { "svg" }),
                        ] {
                            if !model.message(&format!("{{\"{id}\":\"{value}\"}}")) {
                                return Err("Headless model transition rejected".into());
                            }
                        }
                        values.push(model.snapshot.clone());
                    }
                }
            }
        }
        println!("[{}]", values.join(","));
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
        Err("Native font consumer requires macOS/live-ui; --export is portable".into())
    }
}
