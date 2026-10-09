//! Actual-binary guarded editor hosting; trusted compliant bundles only.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod native {
    use clack_extensions::gui::{GuiApiType, GuiConfiguration, GuiSize, Window};
    use clack_host::prelude::*;
    use ravet_foundation::{
        AudioBuffer, AudioLayout, ProcessBlock, ProcessConfig, Tail, Transport,
    };
    use ravet_graph::{Connection, Destination, Graph, GraphNode, GraphStatus, NodeId, Source};
    use ravet_host::{
        ClapLibrary, EditorError, HostRequests, HostedGraphNodeOptions, RequestHost,
        prepare_hosted_graph_node,
    };
    use std::{ffi::c_void, path::PathBuf};
    unsafe extern "C" {
        fn editor_probe_create() -> *mut c_void;
        fn editor_probe_view(parent: *mut c_void) -> *mut c_void;
        fn editor_probe_resize(parent: *mut c_void, width: f64, height: f64);
        fn editor_probe_children(parent: *mut c_void) -> usize;
        fn editor_probe_pump(seconds: f64);
        fn editor_probe_destroy(parent: *mut c_void);
    }
    struct Parent(*mut c_void);
    impl Drop for Parent {
        fn drop(&mut self) {
            unsafe { editor_probe_destroy(self.0) }
        }
    }
    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let path = PathBuf::from(
            std::env::args_os()
                .nth(1)
                .ok_or("usage: graph-hosted <trusted zero-latency stereo gain CLAP bundle>")?,
        );
        // SAFETY: this fixture is for explicit caller-selected trusted compliant
        // bundles, with all native parent operations on the process main thread.
        let library = unsafe { ClapLibrary::load(&path) }?;
        let id = library.plugins()[0].id.clone();
        if ![c"org.ravet.gain", c"org.rust-audio.clack.gain-egui"].contains(&id.as_c_str()) {
            return Err(
                "graph-hosted supports the explicit public gain reference profiles only".into(),
            );
        }
        let info = HostInfo::new("RAVET guarded editor fixture", "RAVET", "", "0.1")?;
        let requests = HostRequests::new();
        let instance = PluginInstance::<RequestHost>::new(
            |_| requests.clone(),
            |_| (),
            library.entry(),
            &id,
            &info,
        )?;
        let options = HostedGraphNodeOptions {
            configuration: ProcessConfig::new(48000., 16, 0, AudioLayout::STEREO)?,
            declared_latency: 0,
            declared_tail: Tail::None,
        };
        let (mut control, node) = prepare_hosted_graph_node(instance, options, |_, _| ())?;
        let routes: Vec<_> = (0..2)
            .flat_map(|channel| {
                [
                    Connection {
                        source: Source::Input(channel),
                        destination: Destination::Node {
                            id: NodeId(1),
                            channel,
                        },
                        gain: 1.,
                    },
                    Connection {
                        source: Source::Node {
                            id: NodeId(1),
                            channel,
                        },
                        destination: Destination::Output(channel),
                        gain: 1.,
                    },
                ]
            })
            .collect();
        let mut graph = Box::new(Graph::new(
            AudioLayout::STEREO,
            vec![GraphNode {
                id: NodeId(1),
                layout: AudioLayout::STEREO,
                processor: Box::new(node),
            }],
            &routes,
        )?);
        graph.prepare(options.configuration)?;
        let (command, receive) = std::sync::mpsc::sync_channel(0);
        let (result, wait) = std::sync::mpsc::sync_channel(0);
        let worker = std::thread::spawn(move || {
            {
                let mut session = graph.start_audio().unwrap();
                let input = [0.25f32; 16];
                let inputs = [input.as_slice(); 2];
                let mut output = [[0.; 16]; 2];
                for _ in 0..3 {
                    receive.recv().unwrap();
                    let counts = allocation_counter::measure(|| {
                        for _ in 0..1000 {
                            let mut outputs =
                                output.each_mut().map(|samples| samples.as_mut_slice());
                            let audio = AudioBuffer::separate(&inputs, &mut outputs, 16).unwrap();
                            assert_eq!(
                                session.process(&mut ProcessBlock::new(
                                    audio,
                                    &[],
                                    Transport::default()
                                )),
                                GraphStatus::Continue
                            );
                            assert_eq!(output, [[0.25; 16]; 2]);
                        }
                    });
                    assert_eq!(counts.count_total, 0);
                    assert_eq!(counts.count_current, 0);
                    result.send(()).unwrap();
                }
                receive.recv().unwrap();
                session.finish();
            }
            graph
        });
        drop(library);
        let parent = Parent(unsafe { editor_probe_create() });
        for cycle in 0..3 {
            unsafe {
                editor_probe_pump(0.15);
            }
            assert_eq!(
                unsafe { editor_probe_children(parent.0) },
                0,
                "previous GUI leaked"
            );
            {
                let mut editor = control.editor(GuiConfiguration {
                    api_type: GuiApiType::COCOA,
                    is_floating: false,
                })?;
                assert!(matches!(editor.show(), Err(EditorError::ParentRequired)));
                let initial = editor.size().ok_or("missing size")?;
                unsafe {
                    editor_probe_resize(
                        parent.0,
                        f64::from(initial.width),
                        f64::from(initial.height),
                    );
                    editor.set_parent(Window::from_cocoa_nsview(editor_probe_view(parent.0)))?;
                }
                editor.show()?;
                editor.show()?;
                assert!(editor.shown());
                unsafe {
                    editor_probe_pump(0.2);
                }
                assert!(
                    unsafe { editor_probe_children(parent.0) } > 0,
                    "embedded child absent"
                );
                requests.defer(ravet_host::PendingHostRequests {
                    callback: true,
                    restart: true,
                    process: true,
                });
                assert!(editor.service_callback(&requests));
                let pending = requests.take();
                assert!(pending.restart && pending.process);
                requests.defer(pending);
                command.send(())?;
                loop {
                    editor.service_callback(&requests);
                    match wait.try_recv() {
                        Ok(()) => break,
                        Err(std::sync::mpsc::TryRecvError::Empty) => unsafe {
                            editor_probe_pump(0.01);
                        },
                        Err(error) => return Err(error.into()),
                    }
                }
                assert!(matches!(
                    editor.resize(GuiSize {
                        width: 0,
                        height: 100
                    }),
                    Err(EditorError::Size)
                ));
                match editor.resize(GuiSize {
                    width: 640,
                    height: 480,
                }) {
                    Ok(size) => {
                        assert!(size.width > 0 && size.height > 0);
                        unsafe {
                            editor_probe_resize(
                                parent.0,
                                f64::from(size.width),
                                f64::from(size.height),
                            );
                        }
                    }
                    Err(EditorError::Unsupported) => (),
                    other => panic!("resize result {other:?}"),
                }
                editor.hide()?;
                editor.hide()?;
                assert!(!editor.shown());
                editor.show()?;
                unsafe {
                    editor_probe_pump(0.15);
                }
                if cycle == 1 {
                    continue;
                } // Drop also cleans an early return path.
            }
            unsafe {
                editor_probe_pump(0.15);
            }
            assert_eq!(
                unsafe { editor_probe_children(parent.0) },
                0,
                "child leaked after guarded destroy"
            );
        }
        unsafe {
            editor_probe_pump(0.15);
        }
        assert_eq!(unsafe { editor_probe_children(parent.0) }, 0);
        command.send(())?;
        let graph = worker
            .join()
            .map_err(|_| "graph editor audio owner panicked")?;
        drop(graph);
        assert!(control.reclaim());
        assert_eq!(control.statistics().processed_blocks, 3000);
        assert_eq!(control.statistics().backend_failures, 0);
        let instance = control.into_instance().ok().ok_or("inactive return")?;
        assert!(!instance.is_active());
        println!(
            "Guarded graph editor hosting, 3000 native blocks with zero measured allocations, owner stop/main retirement: three create/parent/show/resize/hide/drop cycles passed for {}",
            id.to_string_lossy()
        );
        Ok(())
    }
}
#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    native::run()
}
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("Native guarded editor fixture currently requires macOS");
    std::process::exit(1);
}
