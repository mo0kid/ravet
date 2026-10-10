use ravet_foundry::{Controller, Format, Kind, PluginType, Project, Result};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

fn operation(value: Value) -> Result<()> {
    let mut controller = Controller::default();
    controller.message(&value.to_string())?;
    let snapshot = loop {
        let s = controller.snapshot();
        if !s.busy {
            break s;
        }
        thread::sleep(Duration::from_millis(40));
    };
    print!("{}", snapshot.log);
    if let Some(error) = snapshot.error {
        return Err(error.into());
    }
    println!("{}", snapshot.status);
    if let Some(root) = snapshot.root {
        println!("Project: {}", root.display());
    }
    if let Some(artifact) = snapshot.artifact {
        println!("Artifact: {}", artifact.display());
    }
    Ok(())
}
fn cli(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("--help" | "help") => {
            println!(
                "RAVET Foundry\n\nNo arguments: graphical application (macOS; native CMake launcher on Windows/Linux).\nnew <directory> <package-name> <offline|standalone|plugin> <licence> [--framework <checkout>] [--type effect|instrument] [--formats clap,vst3,au,auv3,aax,lv2]\nopen <directory-or-ravet-project.toml>\nregenerate <directory>\nbuild <directory> [--release] [--offline] [--bundle] [--format <format>]\nedit <directory> <editor-executable>\ndoctor\n--ui-test <screenshot-directory> (macOS hidden native fixture)\n\nGenerated source remains consumer-owned. Builds execute trusted consumer Cargo scripts."
            );
            Ok(())
        }
        Some("new") if args.len() >= 5 => {
            let kind = match args[3].as_str() {
                "offline" => Kind::Offline,
                "standalone" => Kind::Standalone,
                "plugin" | "clap" => Kind::Plugin,
                _ => return Err("Unknown template".into()),
            };
            let mut project = Project::new(&args[2], kind);
            project.license = args[4].clone();
            for option in args[5..].chunks(2) {
                if option.len() != 2 {
                    return Err("New-project options require values".into());
                }
                match option[0].as_str() {
                    "--framework" => project.framework_path = PathBuf::from(&option[1]),
                    "--type" => {
                        project.plugin_type = match option[1].as_str() {
                            "effect" => PluginType::Effect,
                            "instrument" => PluginType::Instrument,
                            _ => return Err("Unknown plug-in type".into()),
                        }
                    }
                    "--formats" => {
                        project.formats = option[1]
                            .split(',')
                            .map(Format::parse)
                            .collect::<Result<Vec<_>>>()?
                    }
                    "--vst3-sdk" => project.sdks.vst3 = Some(PathBuf::from(&option[1])),
                    "--audio-unit-sdk" => project.sdks.audio_unit = Some(PathBuf::from(&option[1])),
                    "--aax-sdk" => project.sdks.aax = Some(PathBuf::from(&option[1])),
                    "--lv2-sdk" => project.sdks.lv2 = Some(PathBuf::from(&option[1])),
                    _ => return Err("Unexpected new-project option".into()),
                }
            }
            operation(json!({"action":"create","destination":args[1],"project":project}))
        }
        Some("open") if args.len() == 2 => operation(json!({"action":"open","root":args[1]})),
        Some("regenerate") if args.len() == 2 => operation(
            json!({"action":"regenerate","root":args[1],"project":ravet_foundry::load(Path::new(&args[1]))?}),
        ),
        Some("build") if args.len() >= 2 => {
            let mut format = None;
            let mut release = false;
            let mut offline = false;
            let mut bundle = false;
            let mut options = args[2..].iter();
            while let Some(option) = options.next() {
                match option.as_str() {
                    "--release" => release = true,
                    "--offline" => offline = true,
                    "--bundle" => bundle = true,
                    "--format" => {
                        format = Some(Format::parse(options.next().ok_or("Missing format")?)?)
                    }
                    _ => return Err("Unknown build option".into()),
                }
            }
            operation(
                json!({"action":"build","root":args[1],"release":release,"offline":offline,"bundle":bundle,"format":format}),
            )
        }
        Some("doctor") if args.len() == 1 => operation(json!({"action":"doctor"})),
        Some("edit") if args.len() == 3 => {
            operation(json!({"action":"edit","root":args[1],"editor":args[2]}))
        }
        _ => Err("Invalid command; run ravet-foundry --help".into()),
    }
}
#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use ravet_webview::{Callbacks, StandaloneOptions, StandaloneWindow};
    use std::{cell::RefCell, rc::Rc};
    type SharedController = Rc<RefCell<Controller>>;
    type Pending = Rc<RefCell<Option<ravet_foundry::DirectoryRequest>>>;
    struct Bridge {
        controller: SharedController,
        pending: Pending,
    }
    fn browse(controller: &SharedController, pending: &Pending, probe: bool) -> Result<()> {
        let request = pending.borrow_mut().take();
        if let Some(request) = request {
            let path = ravet_foundry::choose_directory(&request, probe)?;
            controller.borrow_mut().directory_selected(&request, path)?;
        }
        Ok(())
    }
    impl Callbacks for Bridge {
        fn message(&mut self, text: &str) {
            if let Ok(Some(request)) = ravet_foundry::DirectoryRequest::from_message(text) {
                if !self.controller.borrow_mut().snapshot().busy && self.pending.borrow().is_none()
                {
                    *self.pending.borrow_mut() = Some(request);
                }
            } else {
                let _ = self.controller.borrow_mut().message(text);
            }
        }
        fn snapshot(&mut self) -> Option<String> {
            serde_json::to_string(&self.controller.borrow_mut().snapshot()).ok()
        }
    }
    pub fn run(test: Option<&Path>) -> Result<()> {
        let options = StandaloneOptions {
            title: "RAVET Foundry".into(),
            width: 1120.,
            height: 800.,
            // Explicit opt-out: Foundry manages projects and opens no audio devices.
            io_settings: false,
            hidden: test.is_some(),
            ..StandaloneOptions::default()
        };
        let window = StandaloneWindow::new(options).map_err(|e| e.to_string())?;
        let controller = Rc::new(RefCell::new(Controller::default()));
        let pending = Rc::new(RefCell::new(None));
        let editor = window
            .embed(
                &ravet_foundry::document(),
                Bridge {
                    controller: controller.clone(),
                    pending: pending.clone(),
                },
            )
            .map_err(|e| e.to_string())?;
        if let Some(directory) = test {
            std::fs::create_dir_all(directory)?;
            let mut ready = false;
            for _ in 0..400 {
                window.pump(0.01);
                browse(&controller, &pending, test.is_some())?;
                if window
                    .eval("window.foundryReady===true&&window.foundryConnected===true")
                    .map_err(|e| e.to_string())?
                {
                    ready = true;
                    break;
                }
            }
            if !ready {
                return Err("Foundry native page failed to initialize".into());
            }
            if !window.eval("document.querySelector('#name').value==='my-audio-project'&&document.querySelectorAll('.template').length===3&&!document.querySelector('#io-settings')").map_err(|e|e.to_string())?{return Err("Foundry wizard fixture failed".into());}
            if !window
                .snapshot(&directory.join("foundry.png"))
                .map_err(|e| e.to_string())?
            {
                return Err("Foundry screenshot failed".into());
            }
            if !window.eval("document.querySelector('input[value=plugin]').click();document.querySelector('#plugin-settings details').open=true;document.getElementById('sdk-vst3').value='preserve-on-cancel';document.querySelector('[data-browse=\"sdk-vst3\"]').click();true").map_err(|e|e.to_string())? {return Err("SDK browse action failed".into());}
            window.pump(0.05);
            if pending.borrow().is_none() {
                return Err("Native SDK browse request missing".into());
            }
            browse(&controller, &pending, true)?;
            window.pump(0.05);
            if !window
                .eval("document.getElementById('sdk-vst3').value==='preserve-on-cancel'")
                .map_err(|e| e.to_string())?
            {
                return Err("Cancelled chooser changed the form".into());
            }
            let request = ravet_foundry::DirectoryRequest {
                field: "sdk-vst3".into(),
                initial: PathBuf::new(),
            };
            controller
                .borrow_mut()
                .directory_selected(&request, Some(ravet_foundry::framework_root()))?;
            window.pump(0.1);
            let selected = serde_json::to_string(&ravet_foundry::framework_root())?;
            if !window
                .eval(&format!(
                    "document.getElementById('sdk-vst3').value==={selected}"
                ))
                .map_err(|e| e.to_string())?
            {
                return Err("Chosen SDK directory did not reach the form".into());
            }
            if !window.eval("document.querySelector('#plugin-settings details').open=false;document.getElementById('sdk-vst3').value='';true").map_err(|e|e.to_string())? {return Err("Chooser fixture cleanup failed".into());}
            let parent =
                std::env::temp_dir().join(format!("ravet-foundry-ui-{}", std::process::id()));
            std::fs::create_dir(&parent)?;
            let result = (|| -> Result<()> {
                let destination = parent.join("instrument");
                let script = format!(
                    r#"(()=>{{
                    const get=id=>document.getElementById(id);
                    document.querySelector('input[value=plugin]').click();
                    get('plugin-type').value='instrument';get('plugin-type').dispatchEvent(new Event('change'));
                    get('destination').value={};get('name').value='foundry-ui-instrument';
                    get('display-name').value='UI Instrument';get('plugin-id').value='org.ravet.foundry.fixture';
                    if(!get('editor-field').hidden||!document.querySelector('.brand svg'))return false;
                    get('create').click();return true;
                }})()"#,
                    serde_json::to_string(&destination)?
                );
                if !window.eval(&script).map_err(|e| e.to_string())? {
                    return Err("Instrument wizard failed".into());
                }
                let mut created = false;
                for _ in 0..400 {
                    window.pump(0.01);
                    browse(&controller, &pending, test.is_some())?;
                    if window.eval("window.foundryState?.project?.plugin_type==='instrument'&&!window.foundryState.busy").map_err(|e|e.to_string())? {created=true;break;}
                }
                if !created {
                    return Err("Native instrument creation failed".into());
                }
                let source = destination.join("src/lib.rs");
                std::fs::write(&source, "// Consumer-owned source\n")?;
                if !window.eval("document.getElementById('version').value='0.2.0';document.getElementById('regenerate').click();true").map_err(|e|e.to_string())? {return Err("Native regeneration action failed".into());}
                let mut saved = false;
                for _ in 0..400 {
                    window.pump(0.01);
                    browse(&controller, &pending, test.is_some())?;
                    if window.eval("window.foundryState?.project?.version==='0.2.0'&&!window.foundryState.busy").map_err(|e|e.to_string())? {saved=true;break;}
                }
                if !saved || std::fs::read_to_string(source)? != "// Consumer-owned source\n" {
                    return Err("Native regeneration failed to preserve source".into());
                }
                window
                    .snapshot(&directory.join("foundry-instrument.png"))
                    .map_err(|e| e.to_string())?;
                Ok(())
            })();
            let _ = std::fs::remove_dir_all(&parent);
            result?;
            println!(
                "Foundry native directory chooser/cancellation/selection, logo/controls/create/regenerate/source-ownership fixture PASS"
            );
        } else {
            editor.set_visible(true);
            while window.pump(0.01) {
                browse(&controller, &pending, false)?;
            }
        }
        drop(editor);
        drop(window);
        Ok(())
    }
}
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let result = if args.is_empty() {
        #[cfg(target_os = "macos")]
        {
            native::run(None)
        }
        #[cfg(not(target_os = "macos"))]
        {
            Err("Use the native CMake Foundry GUI launcher on this platform, or --help for CLI commands".into())
        }
    } else if args.first().is_some_and(|a| a == "--ui-test") && args.len() == 2 {
        #[cfg(target_os = "macos")]
        {
            native::run(Some(Path::new(&args[1])))
        }
        #[cfg(not(target_os = "macos"))]
        {
            Err("The native screenshot fixture requires macOS".into())
        }
    } else {
        cli(&args)
    };
    if let Err(error) = result {
        eprintln!("Foundry: {error}");
        std::process::exit(1);
    }
}
