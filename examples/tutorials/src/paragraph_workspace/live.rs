use super::model::{self, Model};
use ravet_surface::text::{TextFont, TextVariation};
use ravet_webview::{Callbacks, StandaloneOptions, StandaloneWindow, fonts::SystemFontCatalog};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};
struct Bridge {
    model: Rc<RefCell<Model>>,
    messages: Rc<Cell<usize>>,
    drops: Rc<Cell<usize>>,
}
impl Callbacks for Bridge {
    fn message(&mut self, json: &str) {
        self.messages.set(self.messages.get() + 1);
        if let Ok(mut model) = self.model.try_borrow_mut() {
            model.message(json);
        }
    }
    fn snapshot(&mut self) -> Option<String> {
        self.model.try_borrow().ok().map(|m| m.snapshot.clone())
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
fn system_fonts() -> Result<([TextFont; 3], String), Box<dyn std::error::Error>> {
    let catalog = SystemFontCatalog::discover()?;
    let selection = catalog
        .catalog()
        .styles("Arial")
        .into_iter()
        .find(|s| catalog.catalog().descriptor(*s).unwrap().style() == "Regular")
        .ok_or("Exact Arial Regular is absent; no implicit family/style substitution")?;
    let loaded = catalog.load(selection)?;
    let name = format!(
        "{} {} with explicit local alternatives",
        loaded.descriptor().family(),
        loaded.descriptor().style()
    );
    let settings = loaded
        .variations()
        .iter()
        .map(|v| TextVariation {
            tag: v.tag,
            value: v.value,
        })
        .collect::<Vec<_>>();
    let selected = TextFont::from_bytes(loaded.bytes(), 0)?.with_variations(&settings)?;
    let mut fonts = model::authored_fonts()?;
    fonts[0] = selected;
    // Loaded source bytes and selected instance own their resources after native catalogue retirement.
    drop(loaded);
    drop(catalog);
    Ok((fonts, name))
}
fn settle(
    window: &StandaloneWindow,
    model: &Rc<RefCell<Model>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        window.pump(0.02);
        let snapshot = model.borrow().snapshot.clone();
        if window.eval(&format!("(()=>{{const r=ravetParagraphReport?.(),e={snapshot};return !!r&&r.exact&&!r.busy&&r.revision===e.revision&&r.delivery===e.delivery&&!r.recovery&&r.sample===e.sample&&r.width===e.width&&r.direction===e.direction&&r.alignment===e.alignment&&r.mode===e.mode&&r.payload===e.payload&&r.source===e.source&&r.pixelHash===e.pixelHash&&r.diagnostic===e.diagnostic&&JSON.stringify(r.geometry)===JSON.stringify(e.geometry);}})()"))?{return Ok(());}
    }
    Err("Exact Rust paragraph image/model did not settle in owned validation window".into())
}
fn send(
    window: &StandaloneWindow,
    model: &Rc<RefCell<Model>>,
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
            return Err("Owned paragraph message callback did not acknowledge".into());
        }
        window.pump(0.02);
    }
    settle(window, model)
}
pub fn run(fixture: bool) -> Result<(), Box<dyn std::error::Error>> {
    let (fonts, name) = system_fonts()?;
    let model = Rc::new(RefCell::new(Model::new(fonts, &name)?));
    let messages = Rc::new(Cell::new(0));
    let drops = Rc::new(Cell::new(0));
    let window = StandaloneWindow::new(StandaloneOptions {
        title: "RAVET paragraph workspace".into(),
        width: 900.,
        height: 640.,
        hidden: fixture,
        ..Default::default()
    })?;
    let editor = window.embed(
        include_str!("index.html"),
        Bridge {
            model: model.clone(),
            messages: messages.clone(),
            drops: drops.clone(),
        },
    )?;
    let mut size = window
        .content_size()
        .ok_or("Live content dimensions unavailable")?;
    model.borrow_mut().resize(size.0)?;
    if !fixture {
        while window.pump(0.02) {
            if let Some(next) = window.content_size()
                && next != size
            {
                size = next;
                let _ = model.borrow_mut().resize(next.0);
            }
        }
        drop(editor);
        return Ok(());
    }
    settle(&window, &model)?;
    let mut groups = 1;
    for sample in 0..8 {
        for mode in 0..2 {
            send(&window, &model, &messages, "sample", sample)?;
            send(&window, &model, &messages, "mode", mode)?;
            assert_eq!(model.borrow().state().sample, sample);
            assert_eq!(model.borrow().state().raster, mode == 1);
            assert_eq!(model.borrow().diagnostic(), None);
            groups += 1;
        }
    }
    for (key, values) in [
        ("size", vec![0, 1, 2]),
        ("width", vec![80, 160, 320, 640]),
        ("direction", vec![1, 0]),
        ("alignment", vec![1, 2, 0]),
    ] {
        for value in values {
            send(&window, &model, &messages, key, value)?;
            groups += 1;
        }
    }
    send(&window, &model, &messages, "sample", 0)?;
    let payload = model.borrow().payload().to_owned();
    let previous = model.borrow().state();
    send(&window, &model, &messages, "sample", 8)?;
    assert_eq!(model.borrow().state(), previous);
    assert_eq!(model.borrow().payload(), payload);
    assert_eq!(model.borrow().diagnostic(), Some(model::Error::Capacity));
    groups += 1;
    model.borrow_mut().change(previous)?;
    settle(&window, &model)?;
    let old = model.borrow().snapshot.clone();
    let count = messages.get();
    assert!(
        window
            .eval("window.webkit.messageHandlers.ravet.postMessage({sample:1,revision:0});true")?
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while messages.get() == count {
        if Instant::now() >= deadline {
            return Err("Stale native command not delivered".into());
        }
        window.pump(0.02);
    }
    assert_eq!(model.borrow().snapshot, old);
    groups += 1;
    assert!(
        window
            .eval("document.getElementById('size').focus();document.activeElement.id==='size'")?
    );
    for (width, height) in [(340., 480.), (520., 560.), (800., 640.)] {
        assert!(window.set_size(width, height));
        window.pump(0.1);
        let (w, h) = window.content_size().ok_or("Measured resize failed")?;
        model.borrow_mut().resize(w)?;
        settle(&window, &model)?;
        assert_eq!(
            model.borrow().state().width,
            (w - 40.).floor().clamp(80., 640.) as usize
        );
        assert!(window.eval(&format!("(()=>{{const r=ravetParagraphReport();return r.focus==='size'&&Math.abs(r.viewport[0]-{w})<=1&&Math.abs(r.viewport[1]-{h})<=1;}})()"))?);
        groups += 1;
    }
    assert!(window.open_settings());
    assert!(window.eval("document.getElementById('io-settings').open&&!document.getElementById('io-unavailable').hidden")?);
    assert!(window.eval("document.getElementById('close-settings').click();!document.getElementById('io-settings').open")?);
    groups += 1;
    // Scripted actual native button default activation goes through the shared Rust callback.
    let old_width = model.borrow().state().width;
    assert!(window.eval("document.getElementById('narrower').click();true")?);
    let deadline = Instant::now() + Duration::from_secs(5);
    while model.borrow().state().width == old_width {
        if Instant::now() >= deadline {
            return Err("Actual button did not change Rust layout".into());
        }
        window.pump(0.02);
    }
    settle(&window, &model)?;
    assert_eq!(model.borrow().state().width, (old_width - 80).max(80));
    groups += 1;
    // The native bridge caches a successfully evaluated JSON script even if
    // the async consumer rejects its image. Reproduce that exact cache boundary.
    let old_payload = model.borrow().payload().to_owned();
    assert!(window.eval("window.paragraphOriginalUpdate=window.ravetUpdate;window.ravetUpdate=m=>{window.ravetUpdate=window.paragraphOriginalUpdate;return window.paragraphOriginalUpdate({...m,payload:'AAAA'});};ravetParagraphCommand('width',320)")?);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        window.pump(0.02);
        if window.eval("ravetParagraphReport().recovery&&!ravetParagraphReport().busy")? {
            break;
        }
        if Instant::now() >= deadline {
            return Err("Cached image rejection was not observed".into());
        }
    }
    assert!(window.eval(&format!(
        "ravetParagraphReport().payload==='{}'",
        old_payload
    ))?);
    let revision = model.borrow().revision();
    let delivery = model.borrow().delivery();
    // Native's unchanged-JSON cache does not silently retry the rejected update.
    window.pump(0.1);
    assert!(window.eval("ravetParagraphReport().recovery")?);
    assert!(window.eval("ravetParagraphRefresh()")?);
    settle(&window, &model)?;
    assert_eq!(model.borrow().revision(), revision);
    assert_eq!(model.borrow().delivery(), delivery + 1);
    groups += 1;
    let geometry = model.borrow().geometry().to_owned();
    assert!(window.eval("ravetParagraphCommand('width',80)")?);
    let deadline = Instant::now() + Duration::from_secs(5);
    while model.borrow().state().width != 80 {
        window.pump(0.02);
        if Instant::now() >= deadline {
            return Err("Recovered normal command not delivered".into());
        }
    }
    settle(&window, &model)?;
    assert_ne!(model.borrow().geometry(), geometry);
    groups += 1;
    model.borrow_mut().fixture_delivery_near_limit();
    let old_state = model.borrow().state();
    let old_revision = model.borrow().revision();
    let old_payload = model.borrow().payload().to_owned();
    assert!(window.eval("window.ravetUpdate=m=>{window.ravetUpdate=window.paragraphOriginalUpdate;return window.paragraphOriginalUpdate({...m,payload:'AAAA'});};ravetParagraphRefresh()")?);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        window.pump(0.02);
        if window.eval("ravetParagraphReport().recovery&&!ravetParagraphReport().refreshPending")? {
            break;
        }
        if Instant::now() >= deadline {
            return Err("Missed MAX delivery rejection not observed".into());
        }
    }
    assert_eq!(model.borrow().delivery(), u32::MAX);
    assert!(window.eval("ravetParagraphRefresh()")?);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        window.pump(0.02);
        if window.eval("ravetParagraphReport().terminal&&!ravetParagraphReport().busy&&!ravetParagraphReport().refreshPending&&document.getElementById('refresh').disabled")? {break;}
        if Instant::now() >= deadline {
            return Err("MAX terminal acknowledgment not observed".into());
        }
    }
    assert!(window.eval(&format!("ravetParagraphReport().payload==='{}'&&ravetParagraphReport().status.includes('exhausted')",old_payload))?);
    assert_eq!(model.borrow().state(), old_state);
    assert_eq!(model.borrow().revision(), old_revision);
    groups += 1;
    let previous_messages = messages.get();
    assert!(window.eval(&format!("window.webkit.messageHandlers.ravet.postMessage({{width:160,revision:{old_revision}}});true"))?);
    let deadline = Instant::now() + Duration::from_secs(5);
    while messages.get() == previous_messages {
        window.pump(0.02);
        if Instant::now() >= deadline {
            return Err("Terminal edit refusal callback not delivered".into());
        }
    }
    assert_eq!(model.borrow().state(), old_state);
    assert_eq!(model.borrow().revision(), old_revision);
    assert!(window.eval("!ravetParagraphRefresh()&&!ravetParagraphCommand('width',160)")?);
    groups += 1;
    if let Ok(path) = std::env::var("RAVET_PARAGRAPH_SNAPSHOT") {
        assert!(window.snapshot(std::path::Path::new(&path))?);
    }
    assert!(window.request_close());
    assert!(window.state().is_some_and(|s| s.closed));
    drop(editor);
    assert_eq!(drops.get(), 1);
    drop(window);
    groups += 1;
    println!(
        "{groups} actual owned native paragraph consumer groups PASS; exact selected Arial/whole Rust layout+SVG/PNG,measured resize/default settings/close; no trusted OS-key or AX claim"
    );
    Ok(())
}

/// Explicit authored-state HTML fixture; the ordinary dynamic Rust model is separate.
pub fn browser_fixture(path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("Owned browser fixture HTML exceeds bound".into());
    }
    let html = std::str::from_utf8(&bytes)?;
    struct Quiet;
    impl Callbacks for Quiet {
        fn message(&mut self, json: &str) {
            if json.len() <= 4096 && json.contains("paragraphFixtureDiagnostic") {
                eprintln!("{json}");
            }
        }
        fn snapshot(&mut self) -> Option<String> {
            None
        }
    }
    let window = StandaloneWindow::new(StandaloneOptions {
        title: "RAVET authored paragraph transport validation".into(),
        hidden: true,
        ..Default::default()
    })?;
    let editor = window.embed(html, Quiet)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        window.pump(0.02);
        if window.eval("window.paragraphFixture?.ready===true&&paragraphFixture.error===null&&paragraphFixture.results.length===65&&ravetParagraphReport().exact&&!ravetParagraphReport().busy")? {
            assert!(window.request_close());drop(editor);drop(window);
            println!("65 authored Rust-prepared paragraph image/transport groups PASS in actual owned WKWebView; PNG pixels/source geometry and bounded admission; no native trusted-key/AX claim");return Ok(());
        }
        if window.eval("!!window.paragraphFixture?.error")? {
            return Err("Owned authored browser paragraph fixture failed".into());
        }
    }
    let _=window.eval("window.webkit.messageHandlers.ravet.postMessage({paragraphFixtureDiagnostic:JSON.stringify({groups:window.paragraphFixture?.results.length,ready:window.paragraphFixture?.ready,error:window.paragraphFixture?.error,frames:document.querySelectorAll('iframe').length})});true");
    window.pump(0.02);
    Err("Owned authored browser paragraph fixture did not finish".into())
}
