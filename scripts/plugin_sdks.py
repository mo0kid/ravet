"""Explicit SDK discovery for the unified bundle tool; no sibling-project probes."""
import os
from pathlib import Path
import sys
import tomllib

ENVIRON = {"vst3": "RAVET_VST3_SDK", "audio_unit": "RAVET_AUDIO_UNIT_SDK",
           "aax": "RAVET_AAX_SDK", "lv2": "RAVET_LV2_SDK",
           # Windows web editors (ravet-webview): the extracted WebView2 NuGet package.
           "webview2": "RAVET_WEBVIEW2_SDK"}


def user_settings(environment=None, platform=None):
    env = os.environ if environment is None else environment
    home = Path(env.get("HOME", str(Path.home())))
    platform = platform or sys.platform
    if env.get("XDG_CONFIG_HOME"):
        config = Path(env["XDG_CONFIG_HOME"])
    elif platform == "darwin":
        config = home / "Library/Application Support"
    elif platform == "win32":
        config = Path(env.get("APPDATA", str(home / "AppData/Roaming")))
    else:
        config = home / ".config"
    return config / "ravet/ravet-sdks.toml"


def read_settings(path):
    """Resolve SDK paths relative to their owning settings file."""
    if not path.is_file():
        return {}, None
    with path.open("rb") as source:
        data = tomllib.load(source)
    unknown = set(data) - {"sdks", "signing"}
    sdks = data.get("sdks", {})
    if unknown or set(sdks) - set(ENVIRON):
        raise ValueError(f"unknown SDK settings in {path}")
    resolved = {}
    for name, value in sdks.items():
        if not isinstance(value, str) or not value.strip():
            raise ValueError(f"{path}: sdks.{name} must be a nonempty path string")
        expanded = Path(value).expanduser()
        resolved[name] = (path.parent / expanded).resolve()
    signing = data.get("signing", {})
    if set(signing) - {"command"}:
        raise ValueError(f"unknown signing settings in {path}")
    command = signing.get("command")
    if command is not None and (not isinstance(command, list) or not command or
                               any(not isinstance(arg, str) or not arg for arg in command)):
        raise ValueError(f"{path}: signing.command must be a nonempty argument array")
    return resolved, command


def discover(project, explicit=None, *, settings=None, environment=None, user_file=None):
    """Flags > environment > selected/nearest project file > per-user settings."""
    env = os.environ if environment is None else environment
    paths, signing = read_settings(user_file or user_settings(env))
    if settings is None:
        settings = next((directory / "ravet-sdks.toml" for directory in
                         (project, *project.parents) if (directory / "ravet-sdks.toml").is_file()), None)
    elif not settings.is_file():
        raise ValueError(f"SDK settings file does not exist: {settings}")
    if settings:
        project_paths, project_signing = read_settings(settings.resolve())
        paths.update(project_paths)
        if project_signing is not None:
            signing = project_signing
    for name, variable in ENVIRON.items():
        if env.get(variable):
            paths[name] = Path(env[variable]).expanduser().resolve()
    for name, path in (explicit or {}).items():
        if path is not None:
            paths[name] = Path(path).expanduser().resolve()
    return paths, signing


def required(paths, name):
    if name not in paths:
        raise ValueError(f"Missing {name} SDK path: set {ENVIRON[name]}, [sdks].{name} in "
                         "ravet-sdks.toml, or the explicit SDK flag")
    return paths[name]
