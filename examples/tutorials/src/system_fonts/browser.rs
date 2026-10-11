use ravet_webview::{Callbacks, StandaloneOptions, StandaloneWindow};
use std::time::{Duration, Instant};
struct Bridge;
impl Callbacks for Bridge {
    fn message(&mut self, _: &str) {}
    fn snapshot(&mut self) -> Option<String> {
        None
    }
}
pub fn run(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let html = std::fs::read_to_string(path)?;
    let window = StandaloneWindow::new(StandaloneOptions {
        title: "RAVET first-party system-font browser proof".into(),
        hidden: true,
        ..Default::default()
    })?;
    let editor = window.embed(&html, Bridge)?;
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        window.pump(0.02);
        if window.eval("typeof fixtureAll==='function'&&!!ravetSystemFontReport()")? {
            break;
        }
        if Instant::now() >= end {
            return Err("first-party fixture load timeout".into());
        }
    }
    assert!(window.eval("(()=>{window.firstPartyReceipt=fixtureAll();return firstPartyReceipt.groups===39&&!firstPartyReceipt.systemFontBytes;})()")?);
    assert!(window.request_close());
    drop(editor);
    drop(window);
    println!(
        "39 actual owned WK first-party Rust-image/invalid-snapshot groups PASS; no systemfont bytes/browser font substitution"
    );
    Ok(())
}
