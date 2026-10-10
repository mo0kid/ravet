use crate::{Format, Project, Result, create, framework_root, load, regenerate};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

/// Coalesced UI snapshot. Logs retain at most 24 KiB of their newest UTF-8 text.
#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    /// Latest accepted native directory choice, consumed once by the UI.
    pub directory_selection: Option<crate::DirectorySelection>,
    /// True while an explicit operation is running on the worker.
    pub busy: bool,
    /// Human-readable progress or completion message.
    pub status: String,
    /// Last operation's error, if any.
    pub error: Option<String>,
    /// Bounded build/diagnostic output; stdout/stderr ordering is approximate.
    pub log: String,
    /// Current canonical consumer directory.
    pub root: Option<PathBuf>,
    /// Last loaded/generated settings.
    pub project: Option<Project>,
    /// Last built executable or bundled plug-in location.
    pub artifact: Option<PathBuf>,
    /// Default framework location for the wizard.
    pub framework_path: PathBuf,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case", deny_unknown_fields)]
enum Request {
    Create {
        destination: PathBuf,
        project: Project,
    },
    Open {
        root: PathBuf,
    },
    Regenerate {
        root: PathBuf,
        project: Project,
    },
    Build {
        root: PathBuf,
        #[serde(default)]
        release: bool,
        #[serde(default)]
        offline: bool,
        #[serde(default)]
        bundle: bool,
        #[serde(default)]
        format: Option<Format>,
    },
    Doctor {},
    Reveal {
        root: PathBuf,
    },
    Edit {
        root: PathBuf,
        editor: String,
    },
}
pub(crate) type State = Arc<Mutex<Snapshot>>;

/// One-worker controller shared by native frontends and the CLI.
/// Actions are explicit JSON requests, capped at 16 KiB. A busy controller rejects
/// a second action. Closing requests cancellation and waits for worker teardown;
/// a direct child build process is killed, but arbitrary descendant processes
/// launched by consumer build scripts are outside this ownership contract.
pub struct Controller {
    state: State,
    worker: Option<JoinHandle<()>>,
    cancel: Arc<AtomicBool>,
}
impl Default for Controller {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(Snapshot {
                directory_selection: None,
                busy: false,
                status: "Ready to create a project".into(),
                error: None,
                log: String::new(),
                root: None,
                project: None,
                artifact: None,
                framework_path: framework_root(),
            })),
            worker: None,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}
impl Controller {
    /// Publish an explicit native directory choice without saving settings or
    /// running a worker. Cancellation (`None`) leaves the existing form untouched.
    /// Native frontends call this on their UI thread after the chooser completes.
    pub fn directory_selected(
        &mut self,
        request: &crate::DirectoryRequest,
        path: Option<PathBuf>,
    ) -> Result<()> {
        if !crate::directories::valid_field(&request.field) {
            return Err("Unknown directory field".into());
        }
        let Some(path) = path else {
            return Ok(());
        };
        let path = path.canonicalize()?;
        if !path.is_dir() || path.as_os_str().len() > 4096 {
            return Err("Choose an existing directory with a path under 4096 bytes".into());
        }
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.busy {
            return Err("An operation is already running".into());
        }
        let sequence = state
            .directory_selection
            .as_ref()
            .map_or(1, |s| s.sequence + 1);
        state.directory_selection = Some(crate::DirectorySelection {
            field: request.field.clone(),
            path,
            sequence,
        });
        Ok(())
    }
    /// Snapshot current state without blocking for a build to finish.
    pub fn snapshot(&mut self) -> Snapshot {
        if self.worker.as_ref().is_some_and(|w| w.is_finished())
            && let Some(worker) = self.worker.take()
        {
            let _ = worker.join();
        }
        self.state.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    /// Submit a local UI/CLI action. Errors are also reflected in snapshots.
    pub fn message(&mut self, text: &str) -> Result<()> {
        let request = (|| -> Result<Request> {
            if text.len() > 16_384 {
                return Err("Foundry action exceeds 16 KiB".into());
            }
            Ok(serde_json::from_str(text)?)
        })();
        let request = match request {
            Ok(r) => r,
            Err(e) => {
                self.state.lock().unwrap_or_else(|e| e.into_inner()).error = Some(e.to_string());
                return Err(e);
            }
        };
        if self.snapshot().busy {
            return Err("An operation is already running".into());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.cancel.store(false, Ordering::Release);
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.busy = true;
            state.error = None;
            state.log.clear();
            state.status = "Working…".into();
            if matches!(request, Request::Build { .. }) {
                state.artifact = None;
            }
        }
        let state = self.state.clone();
        let cancel = self.cancel.clone();
        self.worker = Some(thread::spawn(move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                execute(request, &state, &cancel)
            }));
            let mut snapshot = state.lock().unwrap_or_else(|e| e.into_inner());
            match outcome {
                Ok(Ok(status)) => snapshot.status = status,
                Ok(Err(e)) => {
                    snapshot.status = "Operation failed".into();
                    snapshot.error = Some(e.to_string());
                }
                Err(_) => {
                    snapshot.status = "Operation failed".into();
                    snapshot.error = Some("Worker panicked".into());
                }
            }
            snapshot.busy = false;
        }));
        Ok(())
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn append(state: &State, text: &str) {
    let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
    state.log.push_str(text);
    if state.log.len() > 24_576 {
        let mut start = state.log.len() - 24_576;
        while !state.log.is_char_boundary(start) {
            start += 1;
        }
        state.log.drain(..start);
    }
}
pub(crate) fn process(command: &mut Command, state: &State, cancel: &AtomicBool) -> Result<()> {
    append(state, &format!("$ {command:?}\n"));
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()?;
    let mut readers = Vec::new();
    for mut input in [
        Box::new(child.stdout.take().ok_or("Missing stdout")?) as Box<dyn Read + Send>,
        Box::new(child.stderr.take().ok_or("Missing stderr")?) as Box<dyn Read + Send>,
    ] {
        let state = state.clone();
        readers.push(thread::spawn(move || {
            let mut buffer = [0; 4096];
            // Retain split UTF-8 sequences between reads instead of corrupting Unicode paths.
            let mut pending = Vec::new();
            loop {
                match input.read(&mut buffer) {
                    Ok(0) => {
                        if !pending.is_empty() {
                            append(&state, &String::from_utf8_lossy(&pending));
                        }
                        break;
                    }
                    Ok(n) => {
                        pending.extend_from_slice(&buffer[..n]);
                        let valid = match std::str::from_utf8(&pending) {
                            Ok(_) => pending.len(),
                            Err(e) => e.valid_up_to(),
                        };
                        if valid > 0 {
                            append(&state, &String::from_utf8_lossy(&pending[..valid]));
                            pending.drain(..valid);
                        }
                        if pending.len() > 4 {
                            append(&state, &String::from_utf8_lossy(&pending));
                            pending.clear();
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(e) => {
                        append(&state, &format!("\nOutput read failed: {e}\n"));
                        break;
                    }
                }
            }
        }));
    }
    let result = (|| -> Result<()> {
        loop {
            if cancel.load(Ordering::Acquire) {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Build cancelled while closing Foundry".into());
            }
            if let Some(status) = child.try_wait()? {
                if !status.success() {
                    return Err(format!("Command failed with {status}").into());
                }
                return Ok(());
            }
            thread::sleep(Duration::from_millis(30));
        }
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    for reader in readers {
        let _ = reader.join();
    }
    result
}

/// Check Rust/Cargo versions and the compile-time checkout. GUI runtime dependency
/// failures are reported by the native launcher; generated SDK targets are separate.
pub fn doctor() -> Result<String> {
    let mut result = format!(
        "Platform: {} / {}\nRAVET: {}\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
        framework_root().display()
    );
    for name in ["rustc", "cargo"] {
        let output = Command::new(name)
            .arg("--version")
            .stdin(Stdio::null())
            .output()?;
        if !output.status.success() {
            return Err(format!("{name} --version failed").into());
        }
        result += &String::from_utf8_lossy(&output.stdout);
    }
    result += "Foundry GUI: Cocoa on macOS; CMake native launcher on Windows/Linux.\nStandalone template: macOS Rust window; CLAP custom editor: macOS only.\n";
    Ok(result)
}

fn remember(state: &State, root: PathBuf, project: Project) {
    let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
    state.root = Some(root);
    state.project = Some(project);
    state.artifact = None;
}
fn execute(request: Request, state: &State, cancel: &AtomicBool) -> Result<String> {
    match request {
        Request::Create {
            destination,
            project,
        } => {
            let root = create(&destination, project)?;
            let project = load(&root)?;
            remember(state, root, project);
            Ok("Project created · source is ready to edit".into())
        }
        Request::Open { root } => {
            let project = load(&root)?;
            let root = if root.is_dir() {
                root
            } else {
                root.parent().ok_or("Project needs a directory")?.to_owned()
            };
            remember(state, root.canonicalize()?, project);
            Ok("Project opened".into())
        }
        Request::Regenerate { root, project } => {
            regenerate(&root, project)?;
            remember(state, root.canonicalize()?, load(&root)?);
            Ok("Build settings regenerated · source preserved".into())
        }
        Request::Doctor {} => {
            append(state, &doctor()?);
            Ok("Toolchain check complete".into())
        }
        Request::Reveal { root } => {
            load(&root)?;
            let root = root.canonicalize()?;
            #[cfg(target_os = "macos")]
            let mut command = Command::new("open");
            #[cfg(target_os = "windows")]
            let mut command = Command::new("explorer.exe");
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let mut command = Command::new("xdg-open");
            process(command.arg(root), state, cancel)?;
            Ok("Project folder opened".into())
        }
        Request::Edit { root, editor } => {
            load(&root)?;
            if editor.trim().is_empty()
                || editor.len() > 4096
                || editor.chars().any(char::is_control)
            {
                return Err(
                    "Provide an editor executable name or absolute path without arguments".into(),
                );
            }
            let root = root.canonicalize()?;
            append(
                state,
                &format!("Opening {} with {}\n", root.display(), editor),
            );
            // Explicit editor launch transfers the external application's lifecycle
            // to the desktop. It must outlive Foundry rather than be killed on close.
            Command::new(editor)
                .arg(root)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            Ok("Project opened in the selected editor".into())
        }
        Request::Build {
            root,
            release,
            offline,
            bundle,
            format,
        } => {
            let root = root.canonicalize()?;
            let project = load(&root)?;
            if project.kind == crate::Kind::Standalone && !cfg!(target_os = "macos") {
                return Err("The standalone template currently requires macOS; Foundry itself has Windows/Linux native launchers".into());
            }
            if bundle && project.kind != crate::Kind::Plugin {
                return Err("Bundle is only available for plug-in projects".into());
            }
            let selected = if project.kind == crate::Kind::Plugin {
                let selected = format.unwrap_or(project.formats[0]);
                crate::exports::preflight(&project, selected)?;
                Some(selected)
            } else {
                if format.is_some() {
                    return Err("Export format requires a plug-in project".into());
                }
                None
            };
            // Settings edited by hand must be applied before building. This also
            // catches managed-file conflicts instead of silently building stale IDs.
            regenerate(&root, project.clone())?;
            if !root.join("Cargo.lock").is_file() {
                let mut command = Command::new("cargo");
                command.arg("generate-lockfile").current_dir(&root);
                if offline {
                    command.arg("--offline");
                }
                process(&mut command, state, cancel)?;
            }
            let mut command = Command::new("cargo");
            command
                .args(["build", "--locked", "--jobs", "2", "--target-dir"])
                .arg(root.join("target"))
                .current_dir(&root);
            if release {
                command.arg("--release");
            }
            if offline {
                command.arg("--offline");
            }
            process(&mut command, state, cancel)?;
            let profile = if release { "release" } else { "debug" };
            let artifact = if let Some(format) = selected.filter(|f| *f != Format::Clap) {
                crate::exports::build(&root, &project, format, release, state, cancel)?
            } else if project.kind == crate::Kind::Plugin {
                let stem = project.name.replace('-', "_");
                let library = if cfg!(target_os = "macos") {
                    format!("lib{stem}.dylib")
                } else if cfg!(target_os = "windows") {
                    format!("{stem}.dll")
                } else {
                    format!("lib{stem}.so")
                };
                let library = root.join("target").join(profile).join(library);
                if bundle {
                    package(&root, &project, profile, &library, state, cancel)?
                } else {
                    library
                }
            } else {
                root.join("target").join(profile).join(format!(
                    "{}{}",
                    project.name,
                    std::env::consts::EXE_SUFFIX
                ))
            };
            let mut snapshot = state.lock().unwrap_or_else(|e| e.into_inner());
            snapshot.root = Some(root);
            snapshot.project = Some(project);
            snapshot.artifact = Some(artifact);
            Ok("Build complete".into())
        }
    }
}
#[cfg(target_os = "macos")]
fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn package(
    root: &Path,
    p: &Project,
    profile: &str,
    library: &Path,
    state: &State,
    cancel: &AtomicBool,
) -> Result<PathBuf> {
    // Package filenames use validated Cargo names; display titles cannot escape paths.
    let artifact = root
        .join("target/clap")
        .join(profile)
        .join(format!("{}.clap", p.name));
    fs::create_dir_all(artifact.parent().ok_or("Bundle needs parent")?)?;
    #[cfg(target_os = "macos")]
    {
        let executable = p.name.replace('-', "_");
        let contents = artifact.join("Contents");
        fs::create_dir_all(contents.join("MacOS"))?;
        fs::copy(library, contents.join("MacOS").join(&executable))?;
        let entries = [
            ("CFBundleExecutable", executable.as_str()),
            ("CFBundleIdentifier", p.plugin_id.as_str()),
            ("CFBundleName", p.display_name.as_str()),
            ("CFBundlePackageType", "BNDL"),
            ("CFBundleVersion", p.version.as_str()),
            ("CFBundleShortVersionString", p.version.as_str()),
        ];
        let body = entries
            .into_iter()
            .map(|(k, v)| format!("<key>{k}</key><string>{}</string>", xml(v)))
            .collect::<String>();
        fs::write(
            contents.join("Info.plist"),
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?><!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\"><plist version=\"1.0\"><dict>{body}</dict></plist>"
            ),
        )?;
        process(
            Command::new("codesign")
                .args(["--force", "--sign", "-"])
                .arg(&artifact),
            state,
            cancel,
        )?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (state, cancel);
        fs::copy(library, &artifact)?;
    }
    Ok(artifact)
}
