#[path = "../../../../../../examples/tutorials/src/postscript_fonts/model.rs"]
mod model;
use model::Model;
use ravet_webview::{Callbacks, StandaloneOptions, StandaloneWindow, fonts::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};
struct Bridge {
    model: Rc<RefCell<Model<Source>>>,
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
    model: &Rc<RefCell<Model<Source>>>,
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
    model: &Rc<RefCell<Model<Source>>>,
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

use model::FontSource;
use ravet_surface::text::*;
use std::ffi::{CString, c_void};
use std::ptr::NonNull;
unsafe extern "C" {
    fn ravet_cff_fixture_register(path: *const std::ffi::c_char) -> *mut c_void;
    fn ravet_cff_fixture_unregister(raw: *mut c_void) -> bool;
}
struct Registration(Option<NonNull<c_void>>);
impl Drop for Registration {
    fn drop(&mut self) {
        if let Some(raw) = self.0.take() {
            let ok = unsafe { ravet_cff_fixture_unregister(raw.as_ptr()) };
            eprintln!("fixture process unregister={ok}");
        }
    }
}
struct Source {
    catalog: SystemFontCatalog,
    user: f32,
}
impl FontSource for Source {
    fn catalog(&self) -> &FontCatalog {
        self.catalog.catalog()
    }
    fn load(&self, selection: FontSelection) -> Result<TextFont, FontError> {
        let loaded = self.catalog.load(selection)?;
        let base =
            TextFont::from_bytes(loaded.bytes(), 0).map_err(|_| FontError::UnsupportedFont)?;
        if base.outline_format() != Ok(TextOutlineFormat::Cff2) {
            return Err(FontError::UnsupportedFont);
        }
        base.with_variations(&[TextVariation {
            tag: *b"wght",
            value: self.user,
        }])
        .map_err(|_| FontError::UnsupportedFont)
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = CString::new(
        std::env::args()
            .nth(1)
            .ok_or("first-party authored CFF2 path required")?,
    )?;
    let raw = NonNull::new(unsafe { ravet_cff_fixture_register(path.as_ptr()) })
        .ok_or("process registration refused")?;
    let mut registration = Registration(Some(raw));
    let mut groups = 0;
    for user in [100., 400., 650., 900.] {
        let catalog = SystemFontCatalog::discover()?;
        let initial = catalog
            .catalog()
            .styles("Ravet Analytic Variable Postscript")
            .into_iter()
            .find(|s| catalog.catalog().descriptor(*s).unwrap().style() == "Regular")
            .ok_or("exact first-party authored CFF2 style absent")?;
        let model = Rc::new(RefCell::new(Model::new(Source { catalog, user }, initial)?));
        assert_eq!(model.borrow().selected(), initial);
        assert!(!model.borrow().payload().is_empty());
        let drops = Rc::new(Cell::new(0));
        let messages = Rc::new(Cell::new(0));
        let window = StandaloneWindow::new(StandaloneOptions {
            title: "RAVET authored CFF2 coordinates".into(),
            width: 900.,
            height: 640.,
            hidden: true,
            ..Default::default()
        })?;
        let editor = window.embed(
            include_str!("../../../../../../examples/tutorials/src/postscript_fonts/index.html"),
            Bridge {
                model: model.clone(),
                drops: drops.clone(),
                messages: messages.clone(),
            },
        )?;
        settle(&window, &model)?;
        for mode in [0, 1] {
            send(&window, &model, &messages, "mode", mode)?;
            assert_eq!(model.borrow().diagnostic(), None);
            assert!(
                model
                    .borrow()
                    .snapshot
                    .contains(&format!("\"user\":{user}"))
            );
            groups += 1;
            eprintln!("actual shared CFF2 native copied-source/user={user}/mode={mode} PASS");
        }
        assert!(window.open_settings());
        assert!(window.eval("document.getElementById('io-settings').open&&!document.getElementById('io-unavailable').hidden")?);
        assert!(window.eval("document.getElementById('close-settings').click();!document.getElementById('io-settings').open")?);
        groups += 1;
        assert!(window.request_close());
        drop(editor);
        assert_eq!(drops.get(), 1);
        drop(window);
        drop(model);
        groups += 1;
    }
    let raw = registration.0.take().unwrap();
    assert!(unsafe { ravet_cff_fixture_unregister(raw.as_ptr()) });
    eprintln!("fixture process unregister=true");
    println!("{groups} actual normal shared native CFF2 consumer groups PASS");
    Ok(())
}
