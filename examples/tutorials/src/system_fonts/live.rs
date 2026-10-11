use super::model::Model;
use ravet_webview::{Callbacks, StandaloneOptions, StandaloneWindow, fonts::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};
struct Bridge {
    model: Rc<RefCell<Model<SystemFontCatalog>>>,
    drops: Rc<Cell<usize>>,
    messages: Rc<Cell<usize>>,
}
impl Callbacks for Bridge {
    fn message(&mut self, json: &str) {
        println!("command={json}");
        self.messages.set(self.messages.get() + 1);
        self.model.borrow_mut().message(json);
    }
    fn snapshot(&mut self) -> Option<String> {
        Some(self.model.borrow().snapshot.clone())
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1)
    }
}
fn settle(
    window: &StandaloneWindow,
    model: &Rc<RefCell<Model<SystemFontCatalog>>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        window.pump(0.02);
        let snapshot = model.borrow().snapshot.clone();
        if window.eval(&format!("(()=>{{const r=window.ravetSystemFontReport?.(),e={snapshot};return !!r&&r.exact&&r.revision===e.revision&&r.selected===e.selected&&r.payload===e.payload&&r.diagnostic===e.diagnostic;}})()"))?{return Ok(())}
    }
    Err("shared Rust font consumer did not settle".into())
}
fn send(
    window: &StandaloneWindow,
    model: &Rc<RefCell<Model<SystemFontCatalog>>>,
    messages: &Rc<Cell<usize>>,
    key: &str,
    value: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let prior = messages.get();
    let revision = model.borrow().revision();
    assert!(window.eval(&format!("window.webkit.messageHandlers.ravet.postMessage({{{key}:{value},revision:{revision}}});true"))?);
    let deadline = Instant::now() + Duration::from_secs(5);
    while messages.get() == prior {
        if Instant::now() >= deadline {
            return Err("native command callback did not run".into());
        }
        window.pump(0.02);
    }
    settle(window, model)
}
pub fn run(fixture: bool) -> Result<(), Box<dyn std::error::Error>> {
    let catalog = SystemFontCatalog::discover()?;
    let initial = catalog
        .catalog()
        .styles("Arial")
        .into_iter()
        .find(|s| catalog.catalog().descriptor(*s).unwrap().style() == "Regular")
        .ok_or("Arial Regular absent; no style substitution")?;
    let model = Rc::new(RefCell::new(Model::new(catalog, initial)?));
    let drops = Rc::new(Cell::new(0));
    let messages = Rc::new(Cell::new(0));
    let window = StandaloneWindow::new(StandaloneOptions {
        title: "RAVET system font selection".into(),
        width: 900.,
        height: 640.,
        hidden: fixture,
        ..Default::default()
    })?;
    let editor = window.embed(
        include_str!("index.html"),
        Bridge {
            model: model.clone(),
            drops: drops.clone(),
            messages: messages.clone(),
        },
    )?;
    if !fixture {
        while window.pump(0.02) {}
        drop(editor);
        return Ok(());
    }
    settle(&window, &model)?;
    let mut groups = 1;
    for (family, styles) in [
        ("Arial", vec!["Regular", "Bold", "Italic"]),
        ("Helvetica", vec!["Regular", "Bold"]),
        ("Skia", vec!["Regular", "Bold", "Black", "Extended"]),
    ] {
        let family_index = model
            .borrow()
            .catalog()
            .families()
            .iter()
            .position(|f| *f == family)
            .ok_or("required family absent")?;
        send(&window, &model, &messages, "familyPage", family_index / 4)?;
        send(&window, &model, &messages, "family", family_index)?;
        for style in styles {
            let selection = model
                .borrow()
                .catalog()
                .styles(family)
                .into_iter()
                .find(|s| model.borrow().catalog().descriptor(*s).unwrap().style() == style)
                .ok_or("required exact style absent")?;
            let style_index = model
                .borrow()
                .catalog()
                .styles(family)
                .iter()
                .position(|s| *s == selection)
                .unwrap();
            send(&window, &model, &messages, "stylePage", style_index / 4)?;
            let index = (0..model.borrow().catalog().entries().len())
                .find(|i| model.borrow().catalog().selection(*i) == Ok(selection))
                .unwrap();
            send(&window, &model, &messages, "font", index)?;
            println!(
                "attempt={family}/{style} diagnostic={:?}",
                model.borrow().diagnostic()
            );
            assert_eq!(model.borrow().selected(), selection);
            assert_eq!(model.borrow().diagnostic(), None);
            for mode in [0, 1] {
                send(&window, &model, &messages, "mode", mode)?;
                groups += 1;
                println!("Actual shared native style/render {family}/{style}/{mode} PASS")
            }
        }
    }
    // Preserve admitted font/render on an actual available but out-of-source named coordinate.
    let refused = model
        .borrow()
        .catalog()
        .styles("Skia")
        .into_iter()
        .find(|s| model.borrow().catalog().descriptor(*s).unwrap().style() == "Light")
        .ok_or("Skia Light diagnostic unavailable")?;
    let old = model.borrow().selected();
    let payload = model.borrow().payload().to_owned();
    assert_eq!(
        model.borrow_mut().select(refused),
        Err(FontError::UnsupportedFont)
    );
    settle(&window, &model)?;
    assert_eq!(model.borrow().selected(), old);
    assert_eq!(model.borrow().payload(), payload);
    groups += 1;
    for (family, style, error) in [
        ("Eurostile-Bold", "Regular", FontError::UnsupportedFont),
        ("Apple Color Emoji", "Regular", FontError::Capacity),
        ("Hiragino Sans", "W0", FontError::UnsupportedFont),
    ] {
        let selected = model
            .borrow()
            .catalog()
            .styles(family)
            .into_iter()
            .find(|s| model.borrow().catalog().descriptor(*s).unwrap().style() == style)
            .ok_or("required negative native style absent")?;
        assert_eq!(model.borrow_mut().select(selected), Err(error));
        settle(&window, &model)?;
        assert_eq!(model.borrow().selected(), old);
        assert_eq!(model.borrow().payload(), payload);
        groups += 1;
        println!(
            "Actual refused native style {family}/{style}/{error:?} prior font/image preserved PASS"
        );
    }
    let old_snapshot = model.borrow().snapshot.clone();
    assert!(
        window.eval(
            "window.webkit.messageHandlers.ravet.postMessage({font:99999,revision:0});true"
        )?
    );
    window.pump(0.1);
    assert_eq!(model.borrow().snapshot, old_snapshot);
    groups += 1;
    assert!(
        window
            .eval("document.getElementById('size').focus();document.activeElement.id==='size'")?
    );
    for (width, height) in [(640., 540.), (1200., 700.), (700., 1000.)] {
        assert!(window.set_size(width, height));
        window.pump(0.1);
        settle(&window, &model)?;
        let (w, h) = window.content_size().unwrap();
        assert!(window.eval(&format!("(()=>{{let r=ravetSystemFontReport();return r.focus==='size'&&Math.abs(r.viewport[0]-{w})<=1&&Math.abs(r.viewport[1]-{h})<=1;}})()"))?);
        groups += 1;
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
    println!("{groups} actual owned native system-font consumer groups PASS");
    Ok(())
}
