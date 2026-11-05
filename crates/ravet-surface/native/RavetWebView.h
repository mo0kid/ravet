#pragma once
#include <stddef.h>

// Native web backend. Every call and callback belongs on the host UI thread.
// Parents: NSView on macOS, HWND on Windows, X11 Window ID cast to void* on Linux.
// Returned views are owned by this API until destroy. Context outlives the view.
// Message/snapshot strings are UTF-8; callbacks may allocate off the audio thread.
// No exception crosses this ABI: the allocating entry points are noexcept, so an
// escaping C++/Objective-C exception (allocation failure) terminates the process,
// as a Rust allocation failure does. Calls from the wrong thread are refused.
// Cocoa/WebKitGTK suppress browser context menus in every loaded frame while
// preserving event propagation for consumer right-click/reset/custom-menu actions.
typedef void (*RavetWebMessage)(void* context, const char* json);
typedef const char* (*RavetWebSnapshot)(void* context);
// Buffer snapshots copy into caller-owned storage; return UTF-8 byte count, or 0.
typedef size_t (*RavetWebSnapshotBuffer)(void* context, char* output, size_t capacity);

extern "C" {
bool ravet_web_is_main_thread();
void* ravet_web_create_html(const char* html, double width, double height, void* context,
                            RavetWebMessage message, RavetWebSnapshotBuffer snapshot) noexcept;
void* ravet_web_create(const char* htmlPath, double width, double height, void* context,
                       RavetWebMessage message, RavetWebSnapshot snapshot) noexcept;
// Embedded editor assets: `path` is relative ("img/knob.png": ASCII letters,
// digits and "._-/", no "..", empty or leading/trailing/double "/"; at most
// 512 bytes), `mime` its Content-Type. The view copies path and mime;
// `data` must stay valid and unchanged until ravet_web_destroy (it is
// normally static data in the binary). At most 4096 assets of up to 64 MiB
// each, 256 MiB in total; "index.html" is reserved for the page.
typedef struct RavetWebAsset { const char* path; const char* mime; const void* data; size_t size; } RavetWebAsset;
// Like ravet_web_create_html, but loads `html` as "index.html" from a local
// asset origin that also serves `assets` (GET only, 404 for unknown paths, no
// file system or network access): "ravet://editor/" on macOS,
// "ravet-<id>://editor/" on Linux (one scheme per loaded binary, since
// WebKitGTK schemes are process-wide) and "https://editor.ravet.invalid/" on
// Windows, intercepted before any network request. Pages refer to assets by
// relative URL. Returns null for invalid assets; html up to 1 MiB.
void* ravet_web_create_assets(const char* html, const RavetWebAsset* assets, size_t count,
                              double width, double height, void* context,
                              RavetWebMessage message, RavetWebSnapshotBuffer snapshot) noexcept;
// Opaque backing colour, initially white. Finite RGB components clamp to [0,1].
// Returns false for a null view or nonfinite component; leaves colour unchanged.
bool ravet_web_set_background(void* view, double red, double green, double blue);
void ravet_web_attach(void* view, void* parent);
// Owned-view resize; finite positive dimensions up to 16384. Logical points on
// macOS and Linux, physical pixels on Windows (the parent HWND's client units).
bool ravet_web_set_size(void* view, double width, double height);
// Evaluate trusted script once the page can run it. The callback, if any, runs
// exactly once on the UI thread with whether the result was truthy; it is not
// called when the view is destroyed first. Returns false if not admitted.
typedef void (*RavetWebEvalDone)(void* context, bool truthy);
bool ravet_web_eval(void* view, const char* script, void* context, RavetWebEvalDone done) noexcept;
void ravet_web_set_visible(void* view, bool visible);
void ravet_web_detach(void* view);
void ravet_web_destroy(void* view);
// Linux hosts must call this from their UI run loop (e.g. every 10 ms).
// Dispatches at most 32 ready GLib iterations; a no-op on macOS/Windows.
void ravet_web_pump();
}

#if defined(__APPLE__)
#include <stdint.h>
// Explicit native companion for Rust custom-content consumers; UI/main thread only.
struct RavetAccessibilityNode {
    uint64_t id; const char* label; const char* text;
    double x,y,width,height,minimum,value,maximum,increment;
    uint32_t kind; bool enabled,hidden;
};
typedef bool (*RavetAccessibilityAction)(void*,uint64_t,uint64_t,uint32_t);
extern "C" uint32_t ravet_web_accessibility_set(void* view,const char* label,
    const RavetAccessibilityNode* nodes,size_t count,uint64_t focused,void* context,
    RavetAccessibilityAction action,uint64_t* revision);
#endif
