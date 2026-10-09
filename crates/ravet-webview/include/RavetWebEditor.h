#ifndef RAVET_WEB_EDITOR_H
#define RAVET_WEB_EDITOR_H
#include <stdint.h>
#include <stddef.h>
#ifdef __cplusplus
extern "C" {
#endif
// macOS Cocoa main-thread ABI. Opaque live handles, buffers and parent NSView
// pointers must be valid; null checks cannot validate forged native pointers.
// All foreign callbacks must be nonthrowing. Message bytes are borrowed for the
// call only. Snapshot writes <=capacity bytes and returns UTF-8 JSON byte count
// (0 means unchanged); no NUL terminator is required. Invalid snapshots reject.
// A nullable release callback consumes context exactly once on creation failure
// or after teardown and any entered callback completes. Without release, context
// stays borrowed and caller must retain it through teardown/callback completion.
// Callbacks can destroy their owner; retain context through their return. Nested
// callbacks are skipped. Never use a handle after successful destruction.
typedef struct RavetWebEditor RavetWebEditor;
typedef struct RavetEditorCallbacks {
    void* context;
    void (*message)(void*, const uint8_t*, size_t);
    size_t (*snapshot)(void*, uint8_t*, size_t);
    void (*release)(void*);
} RavetEditorCallbacks;
// Status: 0 success, 1 invalid input, 2 wrong thread, 3 native/panic failure.
// All operations are UI-only. Wrong-thread destroy retains ownership for retry.
// Accepted destroy consumes the handle even if a destructor panic returns 3.
// HTML is trusted, copied and <=1 MiB UTF-8 without NUL. No assets/window/runloop
// are created. Native callback polling is coalesced; no delivery deadline claim.
RavetWebEditor* ravet_editor_create_html(const uint8_t* html, size_t length,
    double width, double height, RavetEditorCallbacks callbacks, uint32_t* status);
uint32_t ravet_editor_attach(RavetWebEditor*, void* parent_nsview);
uint32_t ravet_editor_detach(RavetWebEditor*);
uint32_t ravet_editor_set_size(RavetWebEditor*, double width, double height);
uint32_t ravet_editor_set_visible(RavetWebEditor*, uint32_t visible);
uint32_t ravet_editor_set_background(RavetWebEditor*, double red, double green, double blue);
uint32_t ravet_editor_destroy(RavetWebEditor*); // null is a successful no-op
#ifdef __cplusplus
}
#endif
#endif
