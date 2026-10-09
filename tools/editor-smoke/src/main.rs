//! Built-bundle editor probe; trusted RAVET bundles only, no DAW/device claim.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod macos {
    use clack_extensions::{gui::*, params::PluginParams, state::PluginState};
    use clack_host::events::{
        event_types::{ParamGestureBeginEvent, ParamGestureEndEvent, ParamValueEvent},
        io::EventBuffer,
    };
    use clack_host::prelude::*;
    use std::{
        ffi::{CString, c_char, c_void},
        path::PathBuf,
    };
    unsafe extern "C" {
        fn editor_probe_create() -> *mut c_void;
        fn editor_probe_view(parent: *mut c_void) -> *mut c_void;
        fn editor_probe_resize(parent: *mut c_void, width: f64, height: f64);
        fn editor_probe_children(parent: *mut c_void) -> usize;
        fn editor_probe_hidden(parent: *mut c_void) -> bool;
        fn editor_probe_eval(parent: *mut c_void, script: *const c_char) -> bool;
        fn editor_probe_snapshot(parent: *mut c_void, path: *const c_char) -> bool;
        fn editor_probe_pump(seconds: f64);
        fn editor_probe_destroy(parent: *mut c_void);
    }
    struct Parent(*mut c_void);
    impl Parent {
        fn eval(&self, script: &str) -> bool {
            let script = CString::new(script).unwrap();
            // SAFETY: this fixture owns a live parent and NUL-terminated script.
            unsafe { editor_probe_eval(self.0, script.as_ptr()) }
        }
        fn wait_for(&self, script: &str) {
            for _ in 0..80 {
                if self.eval(script) {
                    return;
                }
                unsafe {
                    editor_probe_pump(0.05);
                }
            }
            panic!("editor assertion timed out: {script}");
        }
    }
    impl Drop for Parent {
        fn drop(&mut self) {
            unsafe {
                editor_probe_destroy(self.0);
            }
        }
    }
    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let mut args = std::env::args_os().skip(1);
        let path = PathBuf::from(
            args.next()
                .ok_or("usage: ravet-editor-smoke <trusted CLAP bundle> [screenshot.png]")?,
        );
        let screenshot = args.next().map(PathBuf::from);
        if args.next().is_some() {
            return Err("too many arguments".into());
        }
        // SAFETY: caller chooses the trusted workspace-built bundle. Clack keeps
        // its library alive through all instance, GUI and processor lifetimes.
        let entry = unsafe { PluginEntry::load(&path)? };
        let mut instance = ravet_clap_smoke::instance(&entry, c"org.ravet.gain")?;
        let gui = instance
            .plugin_handle()
            .get_extension::<PluginGui>()
            .ok_or("missing GUI extension")?;
        let params = instance
            .plugin_handle()
            .get_extension::<PluginParams>()
            .ok_or("missing params")?;
        let state = instance
            .plugin_handle()
            .get_extension::<PluginState>()
            .ok_or("missing state")?;
        // SAFETY: called on process main thread, native parent owns its NSWindow.
        let parent = Parent(unsafe { editor_probe_create() });
        let config = GuiConfiguration {
            api_type: GuiApiType::COCOA,
            is_floating: false,
        };
        assert!(gui.is_api_supported(&instance.plugin_handle(), config));
        assert!(!gui.is_api_supported(
            &instance.plugin_handle(),
            GuiConfiguration {
                is_floating: true,
                ..config
            }
        ));
        let mut gestures = EventBuffer::with_capacity(16);
        for cycle in 0..3 {
            let initial = ravet_gain::state::encode_parameters(
                &ravet_gain::parameter_schema().state([1.0, 0.0])?,
            )?;
            state.load(&instance.plugin_handle(), &mut initial.as_slice())?;
            gui.create(&instance.plugin_handle(), config)?;
            assert_eq!(
                gui.get_size(&instance.plugin_handle()),
                Some(GuiSize {
                    width: 480,
                    height: 390
                })
            );
            assert!(gui.can_resize(&instance.plugin_handle()));
            assert_eq!(
                gui.adjust_size(
                    &instance.plugin_handle(),
                    GuiSize {
                        width: 479,
                        height: 390
                    }
                ),
                None
            );
            assert!(
                gui.set_size(
                    &instance.plugin_handle(),
                    GuiSize {
                        width: 479,
                        height: 390
                    }
                )
                .is_err()
            );
            assert_eq!(
                gui.adjust_size(
                    &instance.plugin_handle(),
                    GuiSize {
                        width: 2400,
                        height: 1800
                    }
                ),
                Some(GuiSize {
                    width: 1200,
                    height: 900
                })
            );
            assert!(
                gui.set_size(
                    &instance.plugin_handle(),
                    GuiSize {
                        width: 1201,
                        height: 900
                    }
                )
                .is_err()
            );
            assert_eq!(
                gui.get_size(&instance.plugin_handle()),
                Some(GuiSize {
                    width: 480,
                    height: 390
                })
            );
            assert!(gui.show(&instance.plugin_handle()).is_err()); // parent required
            // SAFETY: probe retains parent NSView until after GUI destruction.
            unsafe {
                gui.set_parent(
                    &instance.plugin_handle(),
                    Window::from_cocoa_nsview(editor_probe_view(parent.0)),
                )?;
            }
            gui.show(&instance.plugin_handle())?;
            parent.wait_for("document.getElementById('status')?.textContent==='Ready'");
            parent.wait_for(r#"(() => {
                if(!window.RavetControls) return false;
                const messages=[], post=m=>{messages.push(m);return true};
                const range=document.createElement('input');range.type='range';
                const number=document.createElement('input');number.type='number';
                const slider=new RavetControls.Slider(range,number,{id:71,minimum:0,maximum:4},post);
                if(!slider.update(1)) return false;
                range.dispatchEvent(new Event('pointerdown'));
                range.value='2';range.dispatchEvent(new Event('input'));
                if(slider.update(3)) return false;
                window.dispatchEvent(new Event('pointercancel'));
                if(messages.map(m=>m.type).join(',')!=='begin,value,end') return false;
                if(messages[1].value!==2 || !slider.update(3) || number.value!=='3') return false;
                if(!slider.dispose()) return false;
                range.dispatchEvent(new Event('change'));if(messages.length!==3) return false;
                const keys=[], keyboard=new RavetControls.Slider(range,null,{id:76,minimum:0,maximum:4},m=>keys.push(m));
                range.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight'}));
                for(const value of [1.1,1.2]) {
                    range.value=String(value);range.dispatchEvent(new Event('input'));range.dispatchEvent(new Event('change'));
                }
                range.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowUp'}));
                range.dispatchEvent(new KeyboardEvent('keyup',{key:'Shift'}));
                range.dispatchEvent(new KeyboardEvent('keyup',{key:'ArrowRight'}));
                if(!keyboard.active || keys.at(-1).type!=='value') return false;
                range.dispatchEvent(new KeyboardEvent('keyup',{key:'ArrowUp'}));
                if(keys.map(m=>m.type).join(',')!=='begin,value,value,end' || !keyboard.dispose()) return false;
                const meter=document.createElement('meter'), reading=document.createElement('span');
                const accessible=new RavetControls.AccessibleMeter(meter,reading,{label:'Synthetic output level'});
                if(!accessible.update({peak:2,rms:1.5,nonfinite:true}) || meter.value!==1 ||
                   !meter.getAttribute('aria-valuetext').includes('RMS 3.5 dB') ||
                   !reading.textContent.includes('invalid audio')) return false;
                const previous=reading.textContent;
                if(accessible.update({peak:NaN,rms:0,nonfinite:false}) || reading.textContent!==previous) return false;
                if(accessible.update({peak:1,rms:-1,nonfinite:false}) || reading.textContent!==previous) return false;
                accessible.dispose();
                if(accessible.update({peak:0,rms:0,nonfinite:false})) return false;
                const invalidFormat=new RavetControls.AccessibleMeter(meter,reading,{label:'Test',format:()=>'<unsafe-looking text>'});
                if(!invalidFormat.update({peak:0,rms:0,nonfinite:false}) || reading.textContent!=='<unsafe-looking text>' || reading.children.length!==0) return false;
                invalidFormat.dispose();
                let rejectEnd=true;
                const retry=new RavetControls.Slider(range,null,{id:77,minimum:0,maximum:4},m=>m.type!=='end'||!rejectEnd);
                if(!retry.begin() || retry.dispose() || !retry.active) return false;
                rejectEnd=false;if(!retry.dispose() || retry.active) return false;
                const denied=new RavetControls.Slider(range,null,{id:72,minimum:0,maximum:4},()=>false);
                if(denied.begin() || denied.active || !denied.dispose()) return false;
                const box=document.createElement('input');box.type='checkbox';
                const toggle=new RavetControls.Toggle(box,73,post);
                toggle.update(true);box.dispatchEvent(new Event('change'));toggle.dispose();
                if(messages.at(-1).id!==73 || messages.at(-1).value!==1) return false;
                const select=document.createElement('select');
                select.add(new Option('Low','0'));select.add(new Option('High','1.0'));
                const choice=new RavetControls.Choice(select,74,post);
                if(!choice.update(1) || select.value!=='1.0') return false;
                select.dispatchEvent(new Event('change'));choice.dispose();
                if(messages.at(-1).value!==1) return false;
                const button=document.createElement('button');
                const action=new RavetControls.ActionButton(button,'reset',post);
                button.click();action.dispose();button.click();
                if(messages.at(-1).type!=='action' || messages.at(-1).action!=='reset' || messages.length!==6) return false;
                try {new RavetControls.Slider(range,null,{id:75,minimum:4,maximum:0},post);return false} catch {}
                window.__ravetReusableControlsPassed=true;return true;
            })()"#);
            for size in [
                GuiSize {
                    width: 640,
                    height: 520,
                },
                GuiSize {
                    width: 1200,
                    height: 900,
                },
                GuiSize {
                    width: 480,
                    height: 390,
                },
            ] {
                unsafe {
                    editor_probe_resize(parent.0, size.width as f64, size.height as f64);
                }
                gui.set_size(&instance.plugin_handle(), size)?;
                assert_eq!(gui.get_size(&instance.plugin_handle()), Some(size));
                parent.wait_for(&format!("window.innerWidth==={}&&window.innerHeight==={}&&document.documentElement.scrollWidth<=window.innerWidth&&document.documentElement.scrollHeight<=window.innerHeight",size.width,size.height));
                if cycle == 2
                    && size.width == 640
                    && let Some(ref path) = screenshot
                {
                    let path = path.with_file_name("gain-editor-resized.png");
                    let path = CString::new(path.to_str().expect("UTF-8 screenshot path")).unwrap();
                    assert!(unsafe { editor_probe_snapshot(parent.0, path.as_ptr()) });
                }
            }

            assert!(parent.eval(
                "window.webkit.messageHandlers.ravet.postMessage({type:'edit',id:99,value:1});true"
            ));
            parent.wait_for("document.getElementById('status').dataset.error==='true'");
            assert!(parent.eval("document.getElementById('gain').value='0.5';document.getElementById('gain').dispatchEvent(new Event('change'));document.getElementById('bypass').checked=true;document.getElementById('bypass').dispatchEvent(new Event('change'));true"));
            unsafe {
                editor_probe_pump(0.15);
            }
            gestures.clear();
            params.flush(
                &mut instance.inactive_plugin_handle().unwrap(),
                &InputEvents::empty(),
                &mut OutputEvents::from_buffer(&mut gestures),
            );
            assert_eq!(gestures.len(), 6);
            assert_eq!(
                params.get_value(&instance.plugin_handle(), ClapId::new(1)),
                Some(0.5)
            );
            assert_eq!(
                params.get_value(&instance.plugin_handle(), ClapId::new(2)),
                Some(1.0)
            );
            parent.wait_for("document.getElementById('gain-number').value==='0.5'&&document.getElementById('bypass').checked&&document.getElementById('status').textContent==='Bypassed'");
            // Live values arrive before release, with exactly one begin/end.
            assert!(parent.eval("document.getElementById('gain').dispatchEvent(new Event('pointerdown'));document.getElementById('gain').value='0.75';document.getElementById('gain').dispatchEvent(new Event('input'));true"));
            unsafe {
                editor_probe_pump(0.15);
            }
            gestures.clear();
            params.flush(
                &mut instance.inactive_plugin_handle().unwrap(),
                &InputEvents::empty(),
                &mut OutputEvents::from_buffer(&mut gestures),
            );
            assert_eq!(gestures.len(), 2);
            assert!(
                gestures
                    .get(0)
                    .unwrap()
                    .as_event::<ParamGestureBeginEvent>()
                    .is_some()
            );
            assert!(
                gestures
                    .get(1)
                    .unwrap()
                    .as_event::<ParamValueEvent>()
                    .is_some()
            );
            assert_eq!(
                params.get_value(&instance.plugin_handle(), ClapId::new(1)),
                Some(0.75)
            );
            assert!(parent.eval("document.getElementById('gain').value='1.25';document.getElementById('gain').dispatchEvent(new Event('input'));document.getElementById('gain').dispatchEvent(new Event('change'));true"));
            unsafe {
                editor_probe_pump(0.15);
            }
            gestures.clear();
            params.flush(
                &mut instance.inactive_plugin_handle().unwrap(),
                &InputEvents::empty(),
                &mut OutputEvents::from_buffer(&mut gestures),
            );
            assert_eq!(gestures.len(), 2);
            assert!(
                gestures
                    .get(0)
                    .unwrap()
                    .as_event::<ParamValueEvent>()
                    .is_some()
            );
            assert!(
                gestures
                    .get(1)
                    .unwrap()
                    .as_event::<ParamGestureEndEvent>()
                    .is_some()
            );
            assert_eq!(
                params.get_value(&instance.plugin_handle(), ClapId::new(1)),
                Some(1.25)
            );
            // Host automation also reaches visible controls, without editor events.
            let changes = [
                ParamValueEvent::new(0, ClapId::new(1), Pckn::match_all(), 2.0),
                ParamValueEvent::new(0, ClapId::new(2), Pckn::match_all(), 0.0),
            ];
            params.flush(
                &mut instance.inactive_plugin_handle().unwrap(),
                &InputEvents::from_buffer(&changes),
                &mut OutputEvents::void(),
            );
            parent.wait_for("document.getElementById('gain-number').value==='2'&&!document.getElementById('bypass').checked");
            let stopped = instance.activate(
                |_, _| (),
                PluginAudioConfiguration {
                    sample_rate: 1000.0,
                    min_frames_count: 1,
                    max_frames_count: 8,
                },
            )?;
            let (ready, waiting) = std::sync::mpsc::sync_channel(0);
            let (resume, resumed) = std::sync::mpsc::sync_channel(0);
            let stopped = std::thread::scope(|scope| {
                let worker = scope.spawn(move || {
                    let mut started = stopped.start_processing().unwrap();
                    let mut inputs = [[0.25_f32; 8], [-0.5; 8]];
                    let mut outputs = [[0.0; 8]; 2];
                    let mut in_ports = AudioPorts::with_capacity(2, 1);
                    let mut out_ports = AudioPorts::with_capacity(2, 1);
                    let mut output_events = EventBuffer::with_capacity(8);
                    for callback in 0..2 {
                        output_events.clear();
                        let input = in_ports.with_input_buffers([AudioPortBuffer {
                            latency: 0,
                            channels: AudioPortBufferType::f32_input_only(
                                inputs.iter_mut().map(InputChannel::variable),
                            ),
                        }]);
                        let mut output = out_ports.with_output_buffers([AudioPortBuffer {
                            latency: 0,
                            channels: AudioPortBufferType::f32_output_only(
                                outputs.iter_mut().map(|c| c.as_mut_slice()),
                            ),
                        }]);
                        started
                            .process(
                                &input,
                                &mut output,
                                &InputEvents::empty(),
                                &mut OutputEvents::from_buffer(&mut output_events),
                                None,
                                None,
                            )
                            .unwrap();
                        assert_eq!(outputs, [[0.5; 8], [-1.0; 8]]);
                        if callback == 1 {
                            assert_eq!(output_events.len(), 2);
                            assert!(
                                output_events
                                    .get(0)
                                    .unwrap()
                                    .as_event::<ParamGestureBeginEvent>()
                                    .is_some()
                            );
                            assert!(
                                output_events
                                    .get(1)
                                    .unwrap()
                                    .as_event::<ParamGestureEndEvent>()
                                    .is_some()
                            );
                        }
                        if callback == 0 {
                            ready.send(()).unwrap();
                            // This test host waits off-callback while the main
                            // thread destroys the GUI; production callbacks do not.
                            resumed.recv().unwrap();
                        }
                    }
                    started.stop_processing()
                });
                waiting.recv().unwrap();
                parent.wait_for("document.getElementById('output-0').value===0.5&&document.getElementById('output-1').value===1&&document.getElementById('input-0').value===0.25");
                gui.hide(&instance.plugin_handle()).unwrap();
                assert!(unsafe { editor_probe_hidden(parent.0) });
                gui.show(&instance.plugin_handle()).unwrap();
                assert!(!unsafe { editor_probe_hidden(parent.0) });
                if cycle == 2
                    && let Some(ref path) = screenshot
                {
                    let path = CString::new(path.to_str().expect("screenshot path must be UTF-8"))
                        .unwrap();
                    assert!(
                        unsafe { editor_probe_snapshot(parent.0, path.as_ptr()) },
                        "screenshot failed"
                    );
                }
                // Destroying an editor during an open drag must enqueue end.
                assert!(parent.eval(
                    "document.getElementById('gain').dispatchEvent(new Event('pointerdown'));true"
                ));
                unsafe {
                    editor_probe_pump(0.15);
                }
                gui.destroy(&instance.plugin_handle());
                unsafe {
                    editor_probe_pump(0.15);
                }
                assert_eq!(unsafe { editor_probe_children(parent.0) }, 0);
                resume.send(()).unwrap();
                worker.join().unwrap()
            });
            instance.deactivate(stopped);
        }
        println!(
            "RAVET editor probe: three create/attach/show/edit/gesture/automation/meter/hide/destroy cycles passed."
        );
        Ok(())
    }
}
#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    macos::run()
}
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("The native editor probe currently requires macOS.");
    std::process::exit(1);
}
