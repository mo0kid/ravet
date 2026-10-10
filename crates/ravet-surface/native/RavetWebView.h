#pragma once
#include <stddef.h>

// Native web backend. Every call and callback belongs on the host UI thread.
// Parents: NSView on macOS, HWND on Windows, X11 Window ID cast to void* on Linux.
// Returned views are owned by this API until destroy. Context outlives the view.
// Message/snapshot strings are UTF-8; callbacks may allocate off the audio thread.
// Cocoa/WebKitGTK suppress browser context menus in every loaded frame while
// preserving event propagation for consumer right-click/reset/custom-menu actions.
typedef void (*RavetWebMessage)(void* context, const char* json);
typedef const char* (*RavetWebSnapshot)(void* context);
// Buffer snapshots copy into caller-owned storage; return UTF-8 byte count, or 0.
typedef size_t (*RavetWebSnapshotBuffer)(void* context, char* output, size_t capacity);

extern "C" {
bool ravet_web_is_main_thread();
void* ravet_web_create_html(const char* html, double width, double height, void* context,
                            RavetWebMessage message, RavetWebSnapshotBuffer snapshot);
void* ravet_web_create(const char* htmlPath, double width, double height, void* context,
                       RavetWebMessage message, RavetWebSnapshot snapshot);
// Opaque backing colour, initially white. Finite RGB components clamp to [0,1].
// Returns false for a null view or nonfinite component; leaves colour unchanged.
bool ravet_web_set_background(void* view, double red, double green, double blue);
void ravet_web_attach(void* view, void* parent);
#if defined(__APPLE__)
// macOS owned-view resize; finite positive dimensions up to 16384 logical pixels.
bool ravet_web_set_size(void* view, double width, double height);
#endif
void ravet_web_set_visible(void* view, bool visible);
void ravet_web_detach(void* view);
void ravet_web_destroy(void* view);
// Linux hosts must call this from their UI run loop (e.g. every 10 ms).
// Dispatches at most 32 ready GLib iterations; a no-op on macOS/Windows.
void ravet_web_pump();
}
