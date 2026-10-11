#![allow(dead_code)]
#[path = "../../src/content_constraints.rs"]
mod content_constraints;
pub use content_constraints::ContentConstraints;
#[path = "../../src/macos.rs"]
mod macos;
pub use macos::{Callbacks, WebView, is_main_thread};
#[path = "../../src/standalone.rs"]
mod standalone;
pub use standalone::{StandaloneOptions, StandaloneWindow, StandaloneWindowState};
use std::{
    cell::Cell,
    ffi::{CString, c_char, c_void},
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    ptr::NonNull,
    rc::Rc,
};
unsafe extern "C" {
    fn ravet_lifecycle_fixture_create(title: *const c_char) -> *mut c_void;
    fn ravet_lifecycle_fixture_destroy(raw: *mut c_void);
    fn ravet_lifecycle_observe(
        raw: *mut c_void,
        context: *mut c_void,
        callback: extern "C" fn(*mut c_void),
    ) -> bool;
    fn ravet_lifecycle_stop_observing(raw: *mut c_void);
    fn ravet_lifecycle_veto(raw: *mut c_void, enabled: bool) -> bool;
    fn ravet_lifecycle_action(raw: *mut c_void, action: u32) -> bool;
    fn ravet_lifecycle_children(raw: *mut c_void) -> i64;
    fn ravet_lifecycle_child_hidden(raw: *mut c_void) -> i32;
    fn ravet_lifecycle_query(raw: *mut c_void, script: *const c_char) -> bool;
}
struct Native<'a> {
    raw: NonNull<c_void>,
    _window: PhantomData<&'a StandaloneWindow>,
}
impl<'a> Native<'a> {
    fn new(window: &'a StandaloneWindow, title: &str) -> Self {
        let title = CString::new(title).unwrap();
        let raw = NonNull::new(unsafe { ravet_lifecycle_fixture_create(title.as_ptr()) })
            .expect("Unique exact own-window fixture");
        let _ = window;
        Self {
            raw,
            _window: PhantomData,
        }
    }
    fn action(&self, action: u32) -> bool {
        unsafe { ravet_lifecycle_action(self.raw.as_ptr(), action) }
    }
    fn veto(&self, enabled: bool) -> bool {
        unsafe { ravet_lifecycle_veto(self.raw.as_ptr(), enabled) }
    }
    fn query(&self, script: &str) -> bool {
        let script = CString::new(script).unwrap();
        unsafe { ravet_lifecycle_query(self.raw.as_ptr(), script.as_ptr()) }
    }
    fn children(&self) -> i64 {
        unsafe { ravet_lifecycle_children(self.raw.as_ptr()) }
    }
    fn hidden(&self) -> i32 {
        unsafe { ravet_lifecycle_child_hidden(self.raw.as_ptr()) }
    }
}
impl Drop for Native<'_> {
    fn drop(&mut self) {
        unsafe { ravet_lifecycle_fixture_destroy(self.raw.as_ptr()) }
    }
}
struct Bridge(Rc<Cell<usize>>);
impl Callbacks for Bridge {
    fn message(&mut self, _: &str) {}
    fn snapshot(&mut self) -> Option<String> {
        Some("{\"io\":{\"audio\":false,\"input\":false,\"midi\":false}}".into())
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1)
    }
}
struct Probe<'a> {
    window: &'a StandaloneWindow,
    fake: Cell<bool>,
    synthetic: Cell<usize>,
    real: Cell<usize>,
    correct: Cell<bool>,
}
extern "C" fn closed(raw: *mut c_void) {
    // SAFETY: the synchronous exact-window observer cannot outlive its borrowed probe.
    let p = unsafe { &*raw.cast::<Probe<'_>>() };
    if catch_unwind(AssertUnwindSafe(|| {
        let state = p.window.state().expect("Readable retained native owner");
        if p.fake.get() {
            p.synthetic.set(p.synthetic.get() + 1);
            p.correct.set(p.correct.get() && !state.closed);
            return;
        }
        p.real.set(p.real.get() + 1);
        let policy = ContentConstraints::new(640., 480., 1200., 900.).unwrap();
        p.correct.set(
            p.correct.get()
                && p.real.get() <= 8
                && state.closed
                && !p.window.request_close()
                && !p.window.set_size(800., 600.)
                && !p.window.set_content_constraints(Some(policy))
                && !p.window.set_content_constraints(None)
                && !p.window.open_settings()
                && !p.window.eval("window.__effects++;true").unwrap(),
        );
    }))
    .is_err()
    {
        p.correct.set(false)
    }
}
struct Observer<'a, 'b> {
    native: &'a Native<'b>,
    _probe: &'a Probe<'b>,
}
impl<'a, 'b> Observer<'a, 'b> {
    fn new(native: &'a Native<'b>, probe: &'a Probe<'b>) -> Self {
        assert!(unsafe {
            ravet_lifecycle_observe(
                native.raw.as_ptr(),
                (probe as *const Probe<'b>).cast_mut().cast(),
                closed,
            )
        });
        Self {
            native,
            _probe: probe,
        }
    }
}
impl Drop for Observer<'_, '_> {
    fn drop(&mut self) {
        unsafe { ravet_lifecycle_stop_observing(self.native.raw.as_ptr()) }
    }
}
// This fixture-only object retains the native window independently, never a raw
// Rust/framework host. Its counter callback has no access to the dropped owner.
struct RetainedNative(NonNull<c_void>);
impl RetainedNative {
    fn new(title: &str) -> Self {
        let title = CString::new(title).unwrap();
        Self(NonNull::new(unsafe { ravet_lifecycle_fixture_create(title.as_ptr()) }).unwrap())
    }
}
impl Drop for RetainedNative {
    fn drop(&mut self) {
        unsafe { ravet_lifecycle_fixture_destroy(self.0.as_ptr()) }
    }
}
extern "C" fn counted_close(raw: *mut c_void) {
    // SAFETY: scoped synchronous native observer is removed before counter drop.
    let count = unsafe { &*raw.cast::<Cell<usize>>() };
    count.set(count.get().saturating_add(1));
}
struct CounterObserver<'a> {
    native: &'a RetainedNative,
    _count: &'a Cell<usize>,
}
impl<'a> CounterObserver<'a> {
    fn new(native: &'a RetainedNative, count: &'a Cell<usize>) -> Self {
        assert!(unsafe {
            ravet_lifecycle_observe(
                native.0.as_ptr(),
                (count as *const Cell<usize>).cast_mut().cast(),
                counted_close,
            )
        });
        Self {
            native,
            _count: count,
        }
    }
}
impl Drop for CounterObserver<'_> {
    fn drop(&mut self) {
        unsafe { ravet_lifecycle_stop_observing(self.native.0.as_ptr()) }
    }
}
const PAGE: &str = "<body><h1>Owned window lifecycle fixture</h1><script>window.__effects=0;window.ravetUpdate=()=>{};</script></body>";
fn ready(window: &StandaloneWindow, script: &str) -> Result<(), Box<dyn std::error::Error>> {
    for _ in 0..100 {
        window.pump(0.02);
        if window.eval(script)? {
            return Ok(());
        }
    }
    Err("Owned lifecycle fixture page readiness timed out".into())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/w212-native/refused.png"));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, b"owned fixture sentinel")?;
    for direct in [false, true] {
        let title = if direct {
            "RAVET lifecycle native action"
        } else {
            "RAVET lifecycle wrapper action"
        };
        let window = StandaloneWindow::new(StandaloneOptions {
            title: title.into(),
            hidden: true,
            ..StandaloneOptions::default()
        })?;
        assert_eq!(
            window.state(),
            Some(StandaloneWindowState {
                closed: false,
                visible: false,
                key: false,
                miniaturized: false
            })
        );
        assert!(!window.pump(0.));
        assert!(!window.state().unwrap().closed);
        let native = Native::new(&window, title);
        assert_eq!(native.children(), 0);
        let drops = Rc::new(Cell::new(0));
        let editor = window.embed(PAGE, Bridge(drops.clone()))?;
        ready(
            &window,
            "window.__effects===0&&typeof ravetOpenSettings==='function'&&!document.getElementById('io-unavailable').hidden",
        )?;
        assert_eq!(native.children(), 1);
        let policy = ContentConstraints::new(640., 480., 1600., 1200.).unwrap();
        assert!(window.set_content_constraints(Some(policy)));
        assert!(window.open_settings());
        assert!(window.eval("document.getElementById('io-settings').open&&!document.getElementById('io-unavailable').hidden")?);
        assert!(window.eval("document.getElementById('close-settings').click();!document.getElementById('io-settings').open")?);
        println!(
            "direct={direct}; hidden-live/visible distinction, embedded owner and default no-device popup PASS"
        );
        let probe = Probe {
            window: &window,
            fake: Cell::new(true),
            synthetic: Cell::new(0),
            real: Cell::new(0),
            correct: Cell::new(true),
        };
        let observer = Observer::new(&native, &probe);
        let size = window.content_size();
        assert!(native.action(2));
        assert!(!window.state().unwrap().closed);
        assert_eq!(window.content_size(), size);
        assert_eq!(probe.synthetic.get(), 1);
        assert!(probe.correct.get());
        probe.fake.set(false);
        println!(
            "direct={direct}; actual synthetic will-close notification cannot retire owner PASS"
        );
        assert!(native.veto(true));
        assert!(!window.request_close());
        assert!(!window.state().unwrap().closed);
        assert_eq!(probe.real.get(), 0);
        assert_eq!(window.content_size(), size);
        assert!(native.veto(false));
        println!(
            "direct={direct}; native should-close veto rejects without closure/geometry changes PASS"
        );
        if direct {
            assert!(native.action(1));
        } else {
            assert!(window.request_close());
        }
        assert_eq!(probe.real.get(), 1);
        assert!(probe.correct.get());
        let state = window.state().unwrap();
        assert!(state.closed && !state.visible);
        assert_eq!(drops.get(), 0);
        assert_eq!(native.children(), 1);
        for _ in 0..4 {
            assert!(window.request_close());
        }
        assert_eq!(probe.real.get(), 1);
        println!(
            "direct={direct}; admitted-before-notification, exact one native close, guarded reentry/idempotence PASS"
        );
        assert_eq!(window.content_size(), None);
        assert_eq!(window.content_constraints(), Some(policy));
        assert!(!window.set_size(800., 600.));
        assert!(!window.set_content_constraints(Some(policy)));
        assert!(!window.set_content_constraints(None));
        assert!(!window.open_settings());
        assert!(!window.eval("window.__effects++;true")?);
        assert!(!window.snapshot(&path)?);
        let rejected_drop = Rc::new(Cell::new(0));
        assert!(window.embed(PAGE, Bridge(rejected_drop.clone())).is_err());
        assert_eq!(rejected_drop.get(), 1);
        assert_eq!(native.children(), 1);
        let hidden = native.hidden();
        editor.set_visible(false);
        editor.set_visible(true);
        assert_eq!(native.hidden(), hidden);
        assert!(native.action(4));
        assert!(native.query("window.__effects===0&&!document.getElementById('io-settings').open"));
        assert_eq!(std::fs::read(&path)?, b"owned fixture sentinel");
        assert!(!window.pump(0.01));
        assert_eq!(window.state(), Some(state));
        println!(
            "direct={direct}; retired embed/resize/policy/JS/snapshot/popup/view-visibility no effects PASS"
        );
        drop(observer);
        drop(editor);
        assert_eq!(drops.get(), 1);
        assert_eq!(native.children(), 0);
        assert!(window.state().unwrap().closed);
        drop(native);
        drop(window);
        println!(
            "direct={direct}; observer removed, context exactly-once, child-before-parent cleanup PASS"
        );
    }
    for requested in [false, true] {
        let title = if requested {
            "RAVET destructor already closed"
        } else {
            "RAVET destructor veto bypass"
        };
        let window = StandaloneWindow::new(StandaloneOptions {
            title: title.into(),
            hidden: true,
            ..StandaloneOptions::default()
        })?;
        let native = RetainedNative::new(title);
        let count = Cell::new(0);
        let observer = CounterObserver::new(&native, &count);
        let drops = Rc::new(Cell::new(0));
        let editor = window.embed(PAGE, Bridge(drops.clone()))?;
        ready(&window, "window.__effects===0")?;
        if requested {
            assert!(window.request_close());
            assert_eq!(count.get(), 1);
        } else {
            assert!(unsafe { ravet_lifecycle_veto(native.0.as_ptr(), true) });
        }
        drop(editor);
        assert_eq!(drops.get(), 1);
        assert_eq!(unsafe { ravet_lifecycle_children(native.0.as_ptr()) }, 0);
        drop(window);
        assert_eq!(count.get(), 1);
        drop(observer);
        drop(native);
        println!(
            "requested={requested}; {} PASS",
            if requested {
                "already-closed owner destruction emits no second notification"
            } else {
                "owner destruction bypasses native veto and closes exactly once"
            }
        );
    }
    let window = StandaloneWindow::new(StandaloneOptions {
        title: "RAVET missing content fixture".into(),
        hidden: true,
        ..StandaloneOptions::default()
    })?;
    let native = Native::new(&window, "RAVET missing content fixture");
    assert!(native.action(5));
    assert!(!window.state().unwrap().closed);
    assert_eq!(window.content_size(), None);
    let drops = Rc::new(Cell::new(0));
    assert!(window.embed(PAGE, Bridge(drops.clone())).is_err());
    assert_eq!(drops.get(), 1);
    assert_eq!(native.children(), 0);
    assert!(window.request_close());
    drop(native);
    drop(window);
    println!(
        "unavailable native content rejects embedding without panic, leaked child or callback context PASS"
    );
    Ok(())
}
