# 0293: Plug-in editors on Windows and Linux, and the editor asset origin

**Date:** 5 November 2026  
**Status:** Prototype. REAPER 7.82 editor runs on Linux x86_64 (CLAP, VST3,
LV2) and Windows 11 x64 (CLAP, VST3, LV2); Windows AAX through a synthetic
AAX host only; native asset probes on all three platforms.

## Context

Decision [0281](0281-generic-export-layer.md) §5 promised one editor ABI over
the WebKit, WebView2 and WebKitGTK backends, and a local URL scheme for editor
assets so editors are not limited to 1 MiB of inline HTML or to 64 KiB
snapshot chunks. Until now the native table and the CLAP GUI hosted Cocoa views
only: Windows and Linux bundles had no editor (AAX on Windows registered a GUI
that stayed empty), and every asset had to be inlined into the page.

Linux hosts pass a plain X11 window as the parent. Few speak XEmbed, so a
GtkPlug inside such a parent is never mapped by its embedder. REAPER also
unloads a CLAP library once its last instance is gone, while WebKitGTK still
holds callbacks into that library.

## Decision

- **One view kind per platform.** `editor_open` accepts
  `RAVET_PLUGIN_VIEW_COCOA` on macOS, `RAVET_PLUGIN_VIEW_WIN32` (an `HWND`) on
  Windows and `RAVET_PLUGIN_VIEW_X11` (an X11 Window ID) on Linux, and returns 1
  for every other kind. The CLAP GUI extension offers `cocoa`, `win32` or `x11`
  accordingly, embedded only. The header is unchanged; only its comments are
  extended, so existing adapters keep compiling.
- **Linux pumping.** `editor_idle` dispatches pending GTK work (at most 32
  GLib iterations) on Linux. The VST3 adapter already calls it from the host
  `IRunLoop` timer, LV2 from the UI idle interface and AAX from its GUI timer;
  the CLAP GUI registers a 10 ms host timer (timer-support) while an editor
  exists. Hosts that run GLib themselves (REAPER's SWELL) need none of these.
- **Non-XEmbed parents.** The GtkPlug is created for the host window and its
  GDK window is mapped explicitly after `gtk_widget_show_all`, so it is
  viewable inside a parent that never sends XEmbed messages. Evidence is
  recorded inside REAPER's SWELL child windows.
- **Asset origin.** A product may declare `EditorAsset`s
  (`Product::with_editor_assets`, path + optional content type + `&'static`
  bytes). The page is then served as `index.html` from a local origin that also
  serves the assets: a `WKURLSchemeHandler` for `ravet://editor/` on macOS, a
  WebKitGTK URI scheme `ravet-<module address>://editor/` on Linux (WebKitGTK
  schemes are process-wide, so each loaded binary registers its own), and
  `WebResourceRequested` for the reserved host `https://editor.ravet.invalid/`
  on Windows. Requests are answered from memory: GET only, 404 for unknown
  paths, no file system or network access. Products without assets keep the
  inline page, so their behaviour is unchanged. Limits: 4,096 assets, 64 MiB
  each, 256 MiB in total, paths of at most 512 bytes from `[A-Za-z0-9._-/]`
  without `.`/`..`/empty segments; `index.html` is reserved. Editor messages
  and snapshots stay limited to 64 KiB; the asset origin is for static assets.
- **Module lifetime.** Once a web view has been created, the backend pins its
  own module (`RTLD_NODELETE` on Linux, `GET_MODULE_HANDLE_EX_FLAG_PIN` on
  Windows), so callbacks that GLib or WebView2 deliver after the last view is
  destroyed never jump into an unloaded library.
- **No loader DLL in plug-in bundles.** Cargo MSVC builds of `ravet-webview`
  link Microsoft's `WebView2LoaderStatic.lib`; GNU builds and the CMake
  library keep loading `WebView2Loader.dll` from beside the module. The
  bundle tool passes `RAVET_WEBVIEW2_SDK` (also `--webview2-sdk` or
  `[sdks].webview2`) to Windows product builds.
- **LV2 UIs everywhere.** Product LV2 bundles declare their UI on every
  platform (`X11UI`, `WindowsUI`, `CocoaUI`); products without an editor still
  get none.

## Consequences

- Windows and Linux bundles of an editor product now open its web editor in
  CLAP, VST3, LV2 and (Windows) AAX. Windows builds need the WebView2 SDK at
  build time and the WebView2 Runtime on the user's machine; Linux builds need
  GTK 3, WebKitGTK 4.1 and JSON-GLib.
- Windows editor sizes are the host's physical pixels. WebView2 scales page
  content by the monitor's DPI, so above 100 % scaling a fixed-size page is
  laid out in fewer CSS pixels than its declared size; content scaling is not
  negotiated yet.
- Keyboard input in Windows and Linux editors is not validated, and XEmbed
  focus messages are not emulated for non-XEmbed parents; the macOS keyboard
  focus handling of [0286](0286-editor-keyboard-focus.md) has no counterpart
  there yet. Native Wayland parents are not supported; hosts already running
  GTK on Wayland are refused, as before.
- A pinned module is never unloaded, so a host that reloads a plug-in library
  in the same process reuses the loaded copy.
- Evidence and its limits are recorded in [WORK_STATUS](../WORK_STATUS.md);
  no DAW other than REAPER, no Pro Tools on Windows, no Wayland host and no
  real-time deadline is claimed.
