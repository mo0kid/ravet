#pragma once
/*
 * RAVET generic plug-in table, version 1 (decision 0281, 0.1.0 export layer).
 *
 * A product crate calls ravet_plugin::export_plugin!(MyPlugin). Besides the CLAP
 * entry, the macro exports `ravet_plugin_v1`, which returns this table. The
 * VST3, AUv2, AAX and LV2 adapters are written once against it, so any RAVET
 * product exports to every format without product-specific C++.
 *
 * Everything a format needs comes from here: product identity and per-format IDs,
 * layouts, parameters (metadata, text, normalization), the versioned state
 * envelope (with the product's migrate/import hooks), the shared parameter
 * cache, editor edits destined for the host, the processor (already wrapped in
 * the product's automation/bypass policies and denormal flushing), and the web
 * editor (opened by RAVET inside a host-provided native view).
 *
 * Threads and ownership
 *  - "MAIN": the host's main/UI thread. "AUDIO": the thread that owns
 *    processing. Functions not marked may be called from any thread.
 *  - One instance per plug-in instance. create/destroy/prepare/deactivate/
 *    save_state/load_state/editor_* are MAIN; process/reset are AUDIO.
 *    prepare/deactivate require that processing has stopped.
 *  - process and reset never allocate, lock or block.
 *  - All strings are NUL-terminated UTF-8 owned by the library and valid for the
 *    library's lifetime, unless stated otherwise.
 *
 * Return codes: 0 success, 1 rejected (invalid argument or state), 2 the
 * product panicked; the instance is then faulted until the next prepare:
 * process, reset and timing return 2 (process silences its outputs) until a
 * successful prepare, which recreates the processor. Parameter, state and
 * editor functions keep working.
 */
#include <stddef.h>
#include <stdint.h>
#include "RavetNativeEvent.h"
#include "RavetNativeTransport.h"
#include "RavetNativeProcessor.h"

#ifdef __cplusplus
extern "C" {
#endif

enum { RAVET_PLUGIN_VERSION_1 = 1 };

enum RavetPluginCategoryV1 {
    RAVET_PLUGIN_EFFECT = 0,
    RAVET_PLUGIN_INSTRUMENT = 1,
    RAVET_PLUGIN_MIDI_EFFECT = 2
};

enum RavetPluginParameterFlagsV1 {
    RAVET_PLUGIN_PARAMETER_AUTOMATABLE = 1,
    RAVET_PLUGIN_PARAMETER_BYPASS = 2,
    RAVET_PLUGIN_PARAMETER_READ_ONLY = 4,
    RAVET_PLUGIN_PARAMETER_HIDDEN = 8,
    RAVET_PLUGIN_PARAMETER_STEPPED = 16
};

/* One supported main-bus layout: channel counts of the main input and output. */
typedef struct RavetPluginLayoutV1 {
    uint32_t inputs, outputs;
    /* AAX plug-in type ID for this layout. */
    uint32_t aax_type_id;
    uint32_t reserved;
} RavetPluginLayoutV1;

/* Product description and resolved per-format identities. */
typedef struct RavetPluginProductV1 {
    const char *name, *vendor, *version, *plugin_id, *url; /* url may be "" */
    uint32_t category;            /* RavetPluginCategoryV1 */
    uint32_t note_input;          /* 1 if the product takes notes/MIDI */
    uint32_t note_output;         /* 1 if the product emits notes/MIDI */
    uint32_t layout_count;        /* >= 1; preferred first */
    const RavetPluginLayoutV1 *layouts;
    uint32_t parameter_count;     /* <= 1024 */
    int32_t bypass_index;         /* index of the host bypass parameter, or -1 */
    const char *clap_id;
    /* VST3 class IDs as the four 32-bit words of Steinberg's FUID. */
    uint32_t vst3_processor_cid[4], vst3_controller_cid[4];
    /* Four-character codes, big-endian packed (e.g. 'aufx'). */
    uint32_t au_type, au_subtype, au_manufacturer;
    uint32_t aax_manufacturer, aax_product;
    const char *lv2_uri;
    /* Web editor; editor_width/height are 0 when the product has no editor. */
    uint32_t editor_width, editor_height, editor_resizable, reserved;
} RavetPluginProductV1;

typedef struct RavetPluginParameterV1 {
    uint32_t id;                  /* permanent host ID */
    uint32_t flags;               /* RavetPluginParameterFlagsV1 */
    uint32_t scale;               /* 0 linear, 1 logarithmic */
    uint32_t steps;               /* 0 continuous, else steps+1 values */
    double minimum, maximum, default_value; /* physical */
    const char *key, *name, *unit;
} RavetPluginParameterV1;

/* An editor edit to report to the host, in order. */
enum RavetPluginEditKindV1 {
    RAVET_PLUGIN_EDIT_BEGIN = 0,
    RAVET_PLUGIN_EDIT_VALUE = 1,
    RAVET_PLUGIN_EDIT_END = 2
};
typedef struct RavetPluginEditV1 {
    uint32_t kind, index;
    double value;                 /* physical; VALUE only */
} RavetPluginEditV1;

/* Native view kinds for editor_open. */
enum RavetPluginViewKindV1 {
    RAVET_PLUGIN_VIEW_COCOA = 1,  /* NSView* */
    RAVET_PLUGIN_VIEW_WIN32 = 2,  /* HWND */
    RAVET_PLUGIN_VIEW_X11 = 3     /* X11 Window, passed as an integer */
};

typedef struct RavetPluginEntryV1 {
    uint32_t version, byte_size;
    const RavetPluginProductV1 *product;

    /* ---- Parameter metadata (pure, any thread) ---- */
    int32_t (*parameter)(uint32_t index, RavetPluginParameterV1 *out);
    /* Index for a host ID, or -1. */
    int32_t (*parameter_index)(uint32_t id);
    /* Text uses the product's formatting and unit; capacity includes the NUL.
       Text that does not fit is truncated at a character boundary, still
       NUL-terminated, and value_to_text returns 1. */
    int32_t (*value_to_text)(uint32_t index, double physical, char *text, uint32_t capacity);
    int32_t (*text_to_value)(uint32_t index, const char *text, double *physical);
    /* 0..1 mapping honouring scale and steps; results are clamped and snapped. */
    int32_t (*normalize)(uint32_t index, double physical, double *normalized);
    int32_t (*denormalize)(uint32_t index, double normalized, double *physical);

    /* ---- Instance lifetime (MAIN) ---- */
    void *(*create)(void);        /* NULL on failure */
    void (*destroy)(void *instance);

    /* ---- Parameter cache ----
       get_value: any thread. set_value: MAIN, a host-side change (not automation
       inside process); it updates the cache at once, so state saved afterwards
       includes it, and reaches the processor at the start of the next block.
       Values are clamped and snapped; set_value returns 1 for an unknown index
       or a nonfinite value, and get_value returns NaN for an unknown index. */
    double (*get_value)(const void *instance, uint32_t index);
    int32_t (*set_value)(void *instance, uint32_t index, double physical);

    /* ---- State (MAIN) ----
       save_state writes the versioned envelope. With data NULL it only reports
       the required size in *length; otherwise capacity must be at least that.
       load_state accepts current and earlier envelopes and the product's import
       format; it applies all values or none, and the processor receives every
       value and a reset before its next block. */
    int32_t (*save_state)(void *instance, uint8_t *data, uint32_t capacity, uint32_t *length);
    int32_t (*load_state)(void *instance, const uint8_t *data, uint32_t length);

    /* ---- Processing ----
       prepare (MAIN): config as RavetNativeProcessorConfigV1, main bus only for
       now (bus_count 1). The layout must be one of product->layouts.
       process (AUDIO): bus 0 is the main bus; samples are 32-bit float. Input
       events carry host automation (RAVET_NATIVE_PARAMETER_VALUE with the
       parameter's host ID and a physical value; the adapter converts normalized
       host values first; the value need not be exactly representable as float)
       and notes/MIDI. Parameter values are clamped, snapped and written to the
       cache; unknown IDs and nonfinite values are dropped. The table adds cached
       host-side changes and editor edits at offset 0 (at most 1024 events in
       total reach the processor; changes that do not fit wait for the next
       block), applies the product's automation and bypass policies, and
       flushes denormals. Output events: notes/MIDI the product generates.
       process returns the RavetNativeProcessorEntryV1 codes and fills result. */
    int32_t (*prepare)(void *instance, const RavetNativeProcessorConfigV1 *config);
    int32_t (*process)(void *instance, const RavetNativeProcessorBusV1 *buses, uint32_t bus_count,
                       uint32_t frames, const RavetNativeEventV1 *input, uint32_t input_count,
                       const RavetNativeTransportV1 *transport, RavetNativeEventV1 *output,
                       uint32_t output_capacity, RavetNativeProcessorResultV1 *result);
    int32_t (*reset)(void *instance);
    int32_t (*deactivate)(void *instance);
    /* Latency and tail of the prepared processor (MAIN or AUDIO, not during process). */
    int32_t (*timing)(void *instance, RavetNativeProcessorTimingV1 *timing);
    /* 1 when the prepared processor's latency changed since it was last read
       with timing(); the adapter then asks the host to restart (MAIN). */
    uint32_t (*latency_changed)(void *instance);

    /* ---- Editor → host edits ----
       The editor's begin/value/end edits, in order, for the adapter to report as
       host gestures and automation. Call from the thread that may report them
       (VST3: MAIN via performEdit; AU/AAX/LV2 per their rules). Returns the
       number written, at most capacity; unread edits stay queued (256 maximum). */
    uint32_t (*take_edits)(void *instance, RavetPluginEditV1 *edits, uint32_t capacity);

    /* ---- Editor view (MAIN) ----
       editor_open attaches the product's web editor to a host native view of
       the given kind and size in logical pixels. RAVET injects the editor
       client and runs the editor protocol. One editor per instance. A view
       kind the platform cannot host returns 1; this release hosts
       RAVET_PLUGIN_VIEW_COCOA on macOS only. Resizable editors accept one to
       four times the product's editor size (editor_open clamps to that range;
       editor_resize returns 1 outside it); other editors keep their size. */
    int32_t (*editor_open)(void *instance, uint32_t view_kind, void *parent, uint32_t width, uint32_t height);
    int32_t (*editor_resize)(void *instance, uint32_t width, uint32_t height);
    void (*editor_close)(void *instance);
    /* Bring the editor up to date with the cache: call periodically on MAIN
       (e.g. from a 30 Hz timer) while the editor is open. (The macOS web view
       currently polls the cache itself, so this does nothing there.) */
    void (*editor_idle)(void *instance);
} RavetPluginEntryV1;

/* Exported by every product built with ravet_plugin::export_plugin!. */
const RavetPluginEntryV1 *ravet_plugin_v1(void);

#ifdef __cplusplus
}
#endif
