#[path = "../../src/accessibility.rs"]
pub mod accessibility;
#[path = "../../src/macos.rs"]
mod macos;
pub use macos::{Callbacks, WebView, WebViewError, is_main_thread};
#[path = "../../src/content_constraints.rs"]
mod content_constraints;
pub use content_constraints::ContentConstraints;
#[path = "../../src/standalone.rs"]
mod standalone;
use accessibility::*;
pub use standalone::{
    StandaloneEditor, StandaloneOptions, StandaloneWindow, StandaloneWindowState,
};
use std::{cell::RefCell, rc::Rc};
unsafe extern "C" {
    fn w223_order_own(visible: bool) -> bool;
    fn w223_action(id: u64, kind: u32) -> bool;
    fn w223_gain() -> f64;
    fn w223_native_profile(gain: f64) -> bool;
    fn w223_settings_default() -> bool;
    fn w223_native_key_up() -> bool;
    fn w223_report_begin(gain: f64, revision: u64);
    fn w223_report_done() -> i32;
    fn w223_capture_receipt();
    fn w223_receipt() -> *const std::ffi::c_char;
    fn w223_frame_profile(rectangles: *const f64, count: usize) -> bool;
}
extern crate self as ravet_webview;
#[path = "../../../../examples/tutorials/src/native_semantics/model.rs"]
mod model;
use model::*;
fn visual(
    window: &StandaloneWindow,
    model: &Rc<RefCell<Model>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (gain, revision) = {
        let m = model.borrow();
        (m.gain, m.revision)
    };
    // SAFETY: fixture helpers touch only this process's explicitly named owner.
    unsafe { w223_report_begin(gain, u64::from(revision)) };
    for _ in 0..100 {
        window.pump(0.02);
        match unsafe { w223_report_done() } {
            1 => return Ok(()),
            -1 => {
                unsafe { w223_report_begin(gain, u64::from(revision)) };
            }
            _ => (),
        }
    }
    Err("Actual WK visual snapshot did not match Rust model".into())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = std::env::args().any(|s| s == "--fixture");
    let window = StandaloneWindow::new(StandaloneOptions {
        title: "RAVET native semantic companion".into(),
        width: 700.,
        height: 560.,
        hidden: fixture,
        ..Default::default()
    })?;
    let model = Rc::new(RefCell::new(Model::new()));
    let editor = window.embed(
        include_str!("../../../../examples/tutorials/src/native_semantics/index.html"),
        Bridge(model.clone()),
    )?;
    // Geometry is actual trusted-document layout; parsing cannot mutate source
    // model until all three rectangles pass reusable frame validation.
    for _ in 0..100 {
        window.eval("typeof ravetCompanionGeometry==='function'&&ravetCompanionGeometry()")?;
        window.pump(0.02);
        if model.borrow().geometry.is_some() {
            break;
        }
    }
    if model.borrow().geometry.is_none() {
        return Err("Consumer geometry unavailable".into());
    }
    publish(&editor, &model)?;
    if !fixture {
        while window.pump(0.05) {
            // Full snapshots are explicit publication. No semantic worker or
            // hidden callback timer is added to the reusable API.
            publish(&editor, &model)?;
        }
        return Ok(());
    }
    let mut groups = 0;
    macro_rules! check {
        ($condition:expr,$label:literal) => {
            if !$condition {
                return Err($label.into());
            }
            groups += 1;
            println!("{} PASS", $label);
        };
    }
    check!(
        unsafe { w223_settings_default() },
        "Actual default title-bar settings popup retained"
    );
    check!(
        unsafe { w223_order_own(true) && w223_native_profile(0.5) },
        "Owned native companion reads actual Rust initial model"
    );
    visual(&window, &model)?;
    check!(true, "Initial Rust model reaches actual WK visual control");
    check!(
        unsafe { w223_action(2, 1) }
            && model.borrow().gain == 0.6
            && model.borrow().native_actions == 1,
        "Native increment updates shared Rust model"
    );
    check!(
        unsafe { w223_gain() } == 0.5,
        "Native value remains previous admitted snapshot until publication"
    );
    publish(&editor, &model)?;
    visual(&window, &model)?;
    check!(
        unsafe { w223_native_profile(0.6) },
        "Rust acknowledgement reaches native values and actual WK visual"
    );
    check!(
        unsafe { w223_action(3, 0) } && model.borrow().gain == 0.5,
        "Native reset routes same Rust model"
    );
    publish(&editor, &model)?;
    visual(&window, &model)?;
    check!(
        unsafe { w223_native_profile(0.5) },
        "Reset acknowledgement matches native and WK values"
    );
    let before_exhaustion = {
        let m = model.borrow();
        (m.gain, m.revision, m.native_actions)
    };
    model.borrow_mut().revision = u32::MAX;
    check!(
        !unsafe { w223_action(2, 1) }
            && model.borrow().gain == before_exhaustion.0
            && model.borrow().native_actions == before_exhaustion.2,
        "Exhausted consumer model refuses native intent atomically"
    );
    model.borrow_mut().revision = before_exhaustion.1;
    model.borrow_mut().refuse = true;
    let revision = model.borrow().revision;
    check!(
        !unsafe { w223_action(2, 1) } && model.borrow().revision == revision,
        "Refused native intent preserves shared model"
    );
    model.borrow_mut().refuse = false;
    window.eval("document.getElementById('gain').focus()")?;
    window.pump(0.1);
    let before = model.borrow().messages;
    check!(
        unsafe { w223_native_key_up() },
        "Own AppKit key injection admitted without application activation"
    );
    for _ in 0..100 {
        window.pump(0.02);
        if model.borrow().messages > before {
            break;
        }
    }
    check!(
        model.borrow().messages == before + 1 && (model.borrow().gain - 0.6).abs() < 1e-12,
        "Own injected native key follows normal WK message into Rust model"
    );
    publish(&editor, &model)?;
    visual(&window, &model)?;
    check!(
        unsafe { w223_native_profile(model.borrow().gain) },
        "Keyboard model acknowledgement reaches native and WK views"
    );
    unsafe { w223_capture_receipt() };
    for _ in 0..100 {
        window.pump(0.02);
        let report = unsafe { w223_receipt() };
        if !report.is_null() {
            println!(
                "Actual own WK injected-key receipt {}",
                unsafe { std::ffi::CStr::from_ptr(report) }.to_str()?
            );
            break;
        }
    }
    for (w, h) in [(900., 640.), (640., 560.)] {
        check!(window.set_size(w, h), "Consumer measured resize admitted");
        window.pump(0.1);
        publish(&editor, &model)?;
        visual(&window, &model)?;
        check!(
            unsafe { w223_native_profile(model.borrow().gain) },
            "Actual resize retains semantic and visual model"
        );
        let frames = model.borrow().geometry.unwrap();
        let coordinates: Vec<f64> = frames
            .into_iter()
            .flat_map(|f| {
                let (x, y, w, h) = f.rectangle();
                [x, y, w, h]
            })
            .collect();
        check!(
            unsafe { w223_frame_profile(coordinates.as_ptr(), 3) },
            "Native screen frames follow actual WK consumer geometry"
        );
    }
    check!(
        window.open_settings(),
        "Default settings action remains independent of companion subtree"
    );
    check!(
        unsafe { w223_order_own(false) } && !unsafe { w223_action(2, 1) },
        "Hidden original owner refuses native actions"
    );
    check!(
        window.request_close() && !unsafe { w223_action(2, 1) },
        "Closed original owner refuses native actions"
    );
    check!(
        editor.set_accessibility(Some(&model.borrow().semantics()?))
            == Err(AccessibilityError::Unavailable),
        "Closed borrowed editor rejects semantic publication"
    );
    check!(
        editor.set_accessibility(None).is_ok(),
        "Original closed owner permits semantic cleanup"
    );
    drop(editor);
    drop(window);
    println!("{} shared Rust/native/WK consumer groups PASS", groups);
    Ok(())
}
