//! Explicit native companion for a consumer-authored custom editor.
#[cfg(target_os = "macos")]
#[path = "../native_semantics/model.rs"]
mod model;
#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use ravet_webview::{StandaloneOptions, StandaloneWindow};
    use std::{cell::RefCell, rc::Rc};
    let window = StandaloneWindow::new(StandaloneOptions {
        title: "RAVET native semantic companion".into(),
        width: 700.,
        height: 560.,
        ..Default::default()
    })?;
    let state = Rc::new(RefCell::new(model::Model::new()));
    let editor = window.embed(
        include_str!("../native_semantics/index.html"),
        model::Bridge(state.clone()),
    )?;
    let mut published = None;
    while window.pump(0.05) {
        window.eval("typeof ravetCompanionGeometry==='function'&&ravetCompanionGeometry()")?;
        let current = {
            let m = state.borrow();
            m.geometry.map(|geometry| (m.revision, geometry))
        };
        if current.is_some() && current != published {
            model::publish(&editor, &state)?;
            published = current;
        }
    }
    Ok(())
}
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("The native semantic companion example requires macOS.");
}
