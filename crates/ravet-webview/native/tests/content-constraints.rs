#![allow(dead_code)]
#[path = "../../src/content_constraints.rs"]
mod content_constraints;
pub use content_constraints::ContentConstraints;
#[path = "../../src/macos.rs"]
mod macos;
pub use macos::{Callbacks, WebView, is_main_thread};
#[path = "../../src/standalone.rs"]
mod standalone;
pub use standalone::{StandaloneOptions, StandaloneWindow};
#[path = "../../../../examples/tutorials/src/layout_workspace/geometry.rs"]
mod geometry;
#[path = "../../../ravet-surface/src/layout.rs"]
mod layout;

use std::{
    cell::{Cell, RefCell},
    ffi::{CString, c_char, c_void},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};
unsafe extern "C" {
    fn ravet_constraints_observe(
        title: *const c_char,
        context: *mut c_void,
        callback: extern "C" fn(*mut c_void),
    ) -> *mut c_void;
    fn ravet_constraints_stop_observing(token: *mut c_void);
    fn ravet_constraints_user_properties(
        title: *const c_char,
        minw: *mut f64,
        minh: *mut f64,
        maxw: *mut f64,
        maxh: *mut f64,
    ) -> bool;
}
struct Bridge(Rc<RefCell<String>>);
impl Callbacks for Bridge {
    fn message(&mut self, _: &str) {}
    fn snapshot(&mut self) -> Option<String> {
        Some(self.0.borrow().clone())
    }
}
struct Probe<'a> {
    window: &'a StandaloneWindow,
    prior: Option<ContentConstraints>,
    calls: Cell<usize>,
    correct: Cell<bool>,
}
extern "C" fn reenter(raw: *mut c_void) {
    // SAFETY: synchronous own-window observer holds a borrow of this live probe.
    let probe = unsafe { &*raw.cast::<Probe<'_>>() };
    if catch_unwind(AssertUnwindSafe(|| {
        probe.calls.set(probe.calls.get() + 1);
        if probe.calls.get() > 32 {
            probe.correct.set(false);
            return;
        }
        let policy = ContentConstraints::new(640., 480., 1200., 900.).unwrap();
        let rejected = !probe.window.set_content_constraints(Some(policy))
            && !probe.window.set_size(800., 600.);
        probe.correct.set(
            probe.correct.get() && rejected && probe.window.content_constraints() == probe.prior,
        );
    }))
    .is_err()
    {
        probe.correct.set(false);
    }
}
struct Observe<'a> {
    token: *mut c_void,
    _probe: &'a Probe<'a>,
}
impl Drop for Observe<'_> {
    fn drop(&mut self) {
        unsafe {
            ravet_constraints_stop_observing(self.token);
        }
    }
}
fn properties(title: &CString) -> (f64, f64, f64, f64) {
    let (mut a, mut b, mut c, mut d) = (0., 0., 0., 0.);
    assert!(unsafe {
        ravet_constraints_user_properties(title.as_ptr(), &mut a, &mut b, &mut c, &mut d)
    });
    (a, b, c, d)
}
fn policy_size(window: &StandaloneWindow, policy: ContentConstraints, aspect: bool) {
    let (w, h) = window.content_size().unwrap();
    let (minw, minh) = policy.minimum_size();
    let (maxw, maxh) = policy.maximum_size();
    assert!(w >= minw - 1. && w <= maxw + 1. && h >= minh - 1. && h <= maxh + 1.);
    let ideal = policy
        .resolve(w, h, aspect.then_some((1100., 780.)))
        .unwrap();
    assert!((w - ideal.0).abs() <= 1. && (h - ideal.1).abs() <= 1.);
    assert_eq!(window.content_constraints(), Some(policy));
}
fn step(
    window: &StandaloneWindow,
    state: &Rc<RefCell<String>>,
    previous: &mut Option<(f64, f64)>,
    w: f64,
    h: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    assert!(
        window.eval(
            "window.__fixturePreviousPanels=JSON.stringify(ravetLayoutReport().panels);true"
        )?
    );
    assert!(window.set_size(w, h));
    settled(window, state, previous)
}
use std::time::{Duration, Instant};
fn refresh(
    window: &StandaloneWindow,
    state: &Rc<RefCell<String>>,
    previous: &mut Option<(f64, f64)>,
) -> bool {
    let Some(size) = window.content_size() else {
        return false;
    };
    if *previous != Some(size) {
        *state.borrow_mut() = geometry::state(size.0, size.1);
        *previous = Some(size);
    }
    true
}
fn settled(
    window: &StandaloneWindow,
    state: &Rc<RefCell<String>>,
    previous: &mut Option<(f64, f64)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        window.pump(0.02);
        refresh(window, state, previous);
        let (width, height) = previous.ok_or("No measured native content size")?;
        let (script, description) = match geometry::geometry(width, height) {
            Ok((mode, rects)) => {
                let mut script = format!(
                    "(()=>{{if(typeof ravetLayoutReport!=='function')return false;const r=ravetLayoutReport();if(Math.abs(r.viewport[0]-{width})>1||Math.abs(r.viewport[1]-{height})>1||!r.applied||r.applied[0]!=={width}||r.applied[1]!=={height}||r.mode!=='{mode}'||r.diagnostic)return false;"
                );
                for (index, rect) in rects.iter().enumerate() {
                    script.push_str(&format!("{{const p=r.panels[{index}];if(!p.visible||Math.abs(p.x-{})>.1||Math.abs(p.y-{})>.1||Math.abs(p.width-{})>.1||Math.abs(p.height-{})>.1)return false;}}",rect.x(),rect.y(),rect.width(),rect.height()));
                }
                script.push_str("return true;})()");
                (script, format!("{mode}; five actual WK panel bounds"))
            }
            Err(crate::layout::LayoutError::InsufficientSpace) => (
                format!(
                    "(()=>{{if(typeof ravetLayoutReport!=='function')return false;const r=ravetLayoutReport();return Math.abs(r.viewport[0]-{width})<=1&&Math.abs(r.viewport[1]-{height})<=1&&r.diagnostic==='Layout unavailable: InsufficientSpace'&&r.pending===null&&r.applied!==null&&JSON.stringify(r.panels)===window.__fixturePreviousPanels;}})()"
                ),
                "TooSmall diagnostic; exact previous five-panel presentation retained".into(),
            ),
            Err(_) => return Err("Invalid fixture layout geometry".into()),
        };
        if window.eval(&script)? {
            println!("measured {width}x{height}; {description} PASS");
            return Ok(());
        }
    }
    Err("Timed out waiting for measured native viewport and latest grid geometry".into())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for aspect in [false, true] {
        let title = CString::new(if aspect {
            "RAVET constraints fixture locked"
        } else {
            "RAVET constraints fixture free"
        })?;
        let window = StandaloneWindow::new(StandaloneOptions {
            title: title.to_str()?.into(),
            width: 1100.,
            height: 780.,
            hidden: true,
            lock_aspect_ratio: aspect,
            ..StandaloneOptions::default()
        })?;
        let original = properties(&title);
        let state = Rc::new(RefCell::new(geometry::state(1100., 780.)));
        let editor = window.embed(
            include_str!("../../../../examples/tutorials/src/layout_workspace/index.html"),
            Bridge(state.clone()),
        )?;
        let mut previous = None;
        settled(&window, &state, &mut previous)?;
        assert_eq!(window.content_constraints(), None);
        step(&window, &state, &mut previous, 320., 240.)?;
        let first = ContentConstraints::new(640.25, 480.5, 1400.75, 1000.125).unwrap();
        assert!(window.set_content_constraints(Some(first)));
        policy_size(&window, first, aspect);
        settled(&window, &state, &mut previous)?;
        assert_eq!(properties(&title), (640.25, 480.5, 1400.75, 1000.125));
        for (w, h) in [(320., 240.), (4096., 4096.), (800., 600.)] {
            step(&window, &state, &mut previous, w, h)?;
            policy_size(&window, first, aspect);
        }
        let before = window.content_size();
        let admitted = window.content_constraints();
        if aspect {
            let invalid = ContentConstraints::new(640., 480., 640., 480.).unwrap();
            assert!(!window.set_content_constraints(Some(invalid)));
            assert_eq!(window.content_constraints(), admitted);
            assert_eq!(window.content_size(), before);
            assert_eq!(properties(&title), (640.25, 480.5, 1400.75, 1000.125));
            println!("aspect incompatible proposal preserves Rust/native policy and geometry PASS");
        }
        let probe = Probe {
            window: &window,
            prior: window.content_constraints(),
            calls: Cell::new(0),
            correct: Cell::new(true),
        };
        let token = unsafe {
            ravet_constraints_observe(
                title.as_ptr(),
                (&probe as *const Probe<'_>).cast_mut().cast(),
                reenter,
            )
        };
        assert!(!token.is_null());
        let observer = Observe {
            token,
            _probe: &probe,
        };
        let next = ContentConstraints::new(900.25, 650.5, 1300.75, 950.125).unwrap();
        assert!(window.set_content_constraints(Some(next)));
        assert!(probe.calls.get() > 0 && probe.correct.get());
        drop(observer);
        policy_size(&window, next, aspect);
        assert_eq!(properties(&title), (900.25, 650.5, 1300.75, 950.125));
        settled(&window, &state, &mut previous)?;
        println!(
            "aspect={aspect}; {} actual synchronous own-window resize callbacks reject both policy/size reentry; admitted readback stable PASS",
            probe.calls.get()
        );
        let resize_probe = Probe {
            window: &window,
            prior: window.content_constraints(),
            calls: Cell::new(0),
            correct: Cell::new(true),
        };
        let token = unsafe {
            ravet_constraints_observe(
                title.as_ptr(),
                (&resize_probe as *const Probe<'_>).cast_mut().cast(),
                reenter,
            )
        };
        assert!(!token.is_null());
        let observer = Observe {
            token,
            _probe: &resize_probe,
        };
        assert!(window.set_size(1200., 800.));
        assert!(resize_probe.calls.get() > 0 && resize_probe.correct.get());
        drop(observer);
        policy_size(&window, next, aspect);
        settled(&window, &state, &mut previous)?;
        println!(
            "aspect={aspect}; {} actual synchronous resize-operation callbacks reject policy/size reentry PASS",
            resize_probe.calls.get()
        );
        let smaller = ContentConstraints::new(640., 480., 800., 650.).unwrap();
        assert!(window.set_content_constraints(Some(smaller)));
        policy_size(&window, smaller, aspect);
        settled(&window, &state, &mut previous)?;
        assert_eq!(properties(&title), (640., 480., 800., 650.));
        let before = window.content_size();
        assert!(window.set_content_constraints(None));
        assert_eq!(window.content_constraints(), None);
        assert_eq!(window.content_size(), before);
        assert_eq!(properties(&title), original);
        println!("original exact user min/max restored without size request PASS");
        step(&window, &state, &mut previous, 320., 240.)?;
        step(&window, &state, &mut previous, 1100., 780.)?;
        assert!(window.open_settings());
        assert!(window.eval("document.getElementById('io-settings').open&&!document.getElementById('io-unavailable').hidden")?);
        assert!(window.eval("document.getElementById('close-settings').click();!document.getElementById('io-settings').open")?);
        drop(editor);
        drop(window);
        println!("aspect={aspect}; default popup and editor-before-parent cleanup PASS");
    }
    Ok(())
}
