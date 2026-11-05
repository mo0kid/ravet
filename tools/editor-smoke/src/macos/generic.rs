//! Probe for products exported through the generic plug-in layer
//! (`ravet-gain-plugin`): editor client, editor protocol and CLAP gestures.
use super::*;
use clack_host::events::UnknownEvent;

const GAIN: ClapId = ClapId::new(1);

/// Event kinds of a flushed output buffer: B(egin), V(alue), E(nd).
fn kinds(events: &EventBuffer) -> String {
    events
        .into_iter()
        .map(|event: &UnknownEvent| {
            if event.as_event::<ParamGestureBeginEvent>().is_some() {
                'B'
            } else if event.as_event::<ParamGestureEndEvent>().is_some() {
                'E'
            } else if event.as_event::<ParamValueEvent>().is_some() {
                'V'
            } else {
                '?'
            }
        })
        .collect()
}

pub fn run(path: PathBuf, screenshot: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    // SAFETY: the caller chooses a trusted, workspace-built bundle.
    let entry = unsafe { PluginEntry::load(&path)? };
    let mut instance = ravet_clap_smoke::instance(&entry, c"org.ravet.gain-generic")?;
    let handle = instance.plugin_handle();
    let gui = handle
        .get_extension::<PluginGui>()
        .ok_or("missing GUI extension")?;
    let params = handle
        .get_extension::<PluginParams>()
        .ok_or("missing params")?;
    let state = handle
        .get_extension::<PluginState>()
        .ok_or("missing state")?;
    let config = GuiConfiguration {
        api_type: GuiApiType::COCOA,
        is_floating: false,
    };
    let mut output = EventBuffer::with_capacity(16);
    let mut flush = |instance: &mut PluginInstance<_>, input: &[ParamValueEvent]| {
        output.clear();
        params.flush(
            &mut instance.inactive_plugin_handle().unwrap(),
            &InputEvents::from_buffer(&input),
            &mut OutputEvents::from_buffer(&mut output),
        );
        kinds(&output)
    };
    let gain = |instance: &mut PluginInstance<_>| params.get_value(&instance.plugin_handle(), GAIN);

    // SAFETY: called on the main thread; the probe owns its parent window.
    let parent = Parent(unsafe { editor_probe_create() });
    for cycle in 0..2 {
        // Each cycle starts from the default gain.
        assert_eq!(
            flush(
                &mut instance,
                &[ParamValueEvent::new(0, GAIN, Pckn::match_all(), 1.0)]
            ),
            ""
        );
        assert!(gui.is_api_supported(&instance.plugin_handle(), config));
        gui.create(&instance.plugin_handle(), config)?;
        assert_eq!(
            gui.get_size(&instance.plugin_handle()),
            Some(GuiSize {
                width: 480,
                height: 320
            })
        );
        assert_eq!(
            gui.adjust_size(
                &instance.plugin_handle(),
                GuiSize {
                    width: 100,
                    height: 5000
                }
            ),
            Some(GuiSize {
                width: 480,
                height: 1280
            })
        );
        // SAFETY: the probe keeps the parent view alive until after destroy.
        unsafe {
            gui.set_parent(
                &instance.plugin_handle(),
                Window::from_cocoa_nsview(editor_probe_view(parent.0)),
            )?;
        }
        gui.show(&instance.plugin_handle())?;

        // info and the first snapshot arrive after the client's `ready`.
        parent.wait_for(
            "window.ravetPlugin?.info?.product?.name==='RAVET Gain (generic)'&&\
             ravetPlugin.info.parameters.length===2&&ravetPlugin.value('gain')===1&&\
             document.getElementById('status').textContent==='Ready'&&\
             location.href==='ravet://editor/index.html'&&\
             getComputedStyle(document.documentElement).getPropertyValue('--ravet-asset').trim()==='loaded'",
        );

        // A complete edit: cached at once, then begin/value/end to the host.
        assert!(parent.eval("ravetPlugin.set('gain',2);true"));
        parent.wait_for("ravetPlugin.value('gain')===2");
        assert_eq!(gain(&mut instance), Some(2.0));
        assert_eq!(flush(&mut instance, &[]), "BVE");

        // A continuous gesture with two values.
        assert!(parent.eval(
            "ravetPlugin.begin('gain');ravetPlugin.update('gain',1.5);\
             ravetPlugin.update('gain',3);ravetPlugin.end('gain');true"
        ));
        parent.wait_for("ravetPlugin.value('gain')===3");
        assert_eq!(flush(&mut instance, &[]), "BVVE");
        assert_eq!(gain(&mut instance), Some(3.0));

        // Out-of-range values clamp; unknown keys and unbalanced gestures do nothing.
        assert!(parent.eval(
            "ravetPlugin.set('gain',99);ravetPlugin.set('nope',1);\
             ravetPlugin.update('gain',0.1);ravetPlugin.end('gain');true"
        ));
        parent.wait_for("ravetPlugin.value('gain')===4");
        assert_eq!(flush(&mut instance, &[]), "BVE");

        // Host changes reach the page, with no events sent back to the host.
        assert_eq!(
            flush(
                &mut instance,
                &[ParamValueEvent::new(0, GAIN, Pckn::match_all(), 0.5)]
            ),
            ""
        );
        parent.wait_for(
            "ravetPlugin.value('gain')===0.5&&\
             document.getElementById('gain-value').textContent==='0.50 x'",
        );

        // Loading state updates the page.
        let mut saved = Vec::new();
        state.save(&instance.plugin_handle(), &mut saved)?;
        flush(
            &mut instance,
            &[ParamValueEvent::new(0, GAIN, Pckn::match_all(), 2.5)],
        );
        parent.wait_for("ravetPlugin.value('gain')===2.5");
        state.load(&instance.plugin_handle(), &mut saved.as_slice())?;
        parent.wait_for("ravetPlugin.value('gain')===0.5");

        if cycle == 0
            && let Some(ref path) = screenshot
        {
            let path = CString::new(path.to_str().expect("UTF-8 screenshot path")).unwrap();
            // SAFETY: the probe owns the parent and the NUL-terminated path.
            assert!(unsafe { editor_probe_snapshot(parent.0, path.as_ptr()) });
        }

        // Closing the editor mid-gesture still ends the gesture for the host.
        assert!(parent.eval("ravetPlugin.begin('gain');true"));
        // SAFETY: pump the main run loop so the message is delivered.
        unsafe { editor_probe_pump(0.15) };
        gui.destroy(&instance.plugin_handle());
        unsafe { editor_probe_pump(0.15) };
        assert_eq!(flush(&mut instance, &[]), "BE");
        assert_eq!(unsafe { editor_probe_children(parent.0) }, 0);
    }
    println!(
        "RAVET generic editor probe: two create/info/edit/gesture/host-change/state-load/close cycles passed."
    );
    Ok(())
}
