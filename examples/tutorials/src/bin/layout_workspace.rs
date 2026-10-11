//! Explicit Rust grid geometry consumed by a resizable ordinary HTML workspace.
//! Native mode requires macOS and the default `live-ui` feature. `--fixture`
//! validates only owned hidden windows and opens no audio/MIDI devices.
#[cfg(all(target_os = "macos", feature = "live-ui"))]
use ravet_surface::layout;
#[cfg(all(target_os = "macos", feature = "live-ui"))]
use ravet_webview::{Callbacks, StandaloneOptions, StandaloneWindow};
#[cfg(all(target_os = "macos", feature = "live-ui"))]
#[path = "../layout_workspace/geometry.rs"]
mod geometry;
#[cfg(all(target_os = "macos", feature = "live-ui"))]
mod live {
    use crate::geometry;
    use crate::{Callbacks, StandaloneOptions, StandaloneWindow};
    use std::{
        cell::RefCell,
        rc::Rc,
        time::{Duration, Instant},
    };
    struct Bridge {
        state: Rc<RefCell<String>>,
    }
    impl Callbacks for Bridge {
        fn message(&mut self, _json: &str) {} // No devices are opened by this layout consumer.
        fn snapshot(&mut self) -> Option<String> {
            Some(self.state.borrow().clone())
        }
    }
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
    fn fixture() -> Result<(), Box<dyn std::error::Error>> {
        for aspect in [false, true] {
            let window = StandaloneWindow::new(StandaloneOptions {
                title: "RAVET layout fixture".into(),
                width: 1100.,
                height: 780.,
                hidden: true,
                lock_aspect_ratio: aspect,
                ..StandaloneOptions::default()
            })?;
            let initial = window
                .content_size()
                .ok_or("No content measurement before embedding")?;
            assert!(
                initial.0.is_finite() && initial.1.is_finite() && initial.0 > 0. && initial.1 > 0.
            );
            let state = Rc::new(RefCell::new(geometry::state(1100., 780.)));
            let editor = window.embed(
                include_str!("../layout_workspace/index.html"),
                Bridge {
                    state: state.clone(),
                },
            )?;
            let mut previous = None;
            settled(&window, &state, &mut previous)?;
            for (width, height) in [(1400., 650.), (700., 1000.), (320., 240.), (1100., 780.)] {
                assert!(window.eval(
                    "window.__fixturePreviousPanels=JSON.stringify(ravetLayoutReport().panels);true"
                )?);
                assert!(window.set_size(width, height));
                settled(&window, &state, &mut previous)?;
                if aspect {
                    let measured = previous.unwrap();
                    assert!((measured.0 - measured.1 * 1100. / 780.).abs() <= 1.);
                }
            }
            for index in 0..24 {
                assert!(
                    window.set_size(900. + f64::from(index) * 7., 650. + f64::from(index) * 3.)
                );
            }
            settled(&window, &state, &mut previous)?;
            assert!(window.open_settings());
            assert!(window.eval("document.getElementById('io-settings').open && !document.getElementById('io-unavailable').hidden && document.getElementById('start').hidden && document.getElementById('stop').hidden")?);
            assert!(window.eval("document.getElementById('close-settings').click(); !document.getElementById('io-settings').open")?);
            println!(
                "aspect={aspect}; default Audio/MIDI popup open/close and no-device state PASS"
            );
            let measured = window.content_size();
            assert!(!window.set_size(f64::NAN, 500.));
            assert!(!window.set_size(500., f64::INFINITY));
            assert!(!window.set_size(319., 500.));
            assert!(!window.set_size(500., 239.));
            assert_eq!(window.content_size(), measured);
            drop(editor);
            assert_eq!(window.content_size(), measured);
            drop(window);
            println!("editor-before-parent teardown PASS");
        }
        Ok(())
    }
    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        if std::env::args().skip(1).any(|arg| arg == "--fixture") {
            return fixture();
        }
        let window = StandaloneWindow::new(StandaloneOptions {
            title: "RAVET Layout Workspace".into(),
            width: 1100.,
            height: 780.,
            ..StandaloneOptions::default()
        })?;
        let policy = ravet_webview::ContentConstraints::new(640., 480., 4096., 4096.)
            .ok_or("Invalid workspace content policy")?;
        if !window.set_content_constraints(Some(policy)) {
            return Err("Could not install workspace content constraints".into());
        }
        let state = Rc::new(RefCell::new(geometry::state(1100., 780.)));
        let editor = window.embed(
            include_str!("../layout_workspace/index.html"),
            Bridge {
                state: state.clone(),
            },
        )?;
        let mut previous = None;
        while window.pump(0.02) {
            refresh(&window, &state, &mut previous);
        }
        drop(editor);
        Ok(())
    }
}
#[cfg(all(target_os = "macos", feature = "live-ui"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    live::run()
}
#[cfg(not(all(target_os = "macos", feature = "live-ui")))]
fn main() {
    eprintln!("The layout workspace UI requires macOS and the live-ui feature.");
}
