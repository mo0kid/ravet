// Table-only test product: deliberately no Rust or format SDK dependency.
#include "RavetPlugin.h"
#include <algorithm>
#include <array>
#include <atomic>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <pthread.h>

namespace {
constexpr uint32_t gainID = 101, bypassID = 707, choiceID = 999;
struct Instance {
    std::array<std::atomic<double>, 3> values{{1., 0., 0.}};
    uint32_t maximum = 0;
    bool prepared = false, editor = false, edited = false, changed = false;
    std::array<RavetPluginEditV1, 3> edits{};
    uint32_t editCount = 0;
};
std::atomic<uint32_t> live{0}, opened{0}, closed{0}, idle{0}, threadFaults{0}, transportCalls{0};
void mainThread() { if (!pthread_main_np()) ++threadFaults; }
#if RAVET_MOCK_INSTRUMENT
constexpr uint32_t type = 0x61756d75, category = RAVET_PLUGIN_INSTRUMENT;
const RavetPluginLayoutV1 layouts[]{{0, 2, 0, 0}, {0, 1, 0, 0}};
constexpr const char *name = "RAVET AU TEST Instrument", *id = "org.ravet.test.auv2.instrument";
constexpr uint32_t subtype = 0x52764931;
#elif RAVET_MOCK_MIDI
constexpr uint32_t type = 0x61756d66, category = RAVET_PLUGIN_MIDI_EFFECT;
const RavetPluginLayoutV1 layouts[]{{2, 2, 0, 0}, {1, 1, 0, 0}};
constexpr const char *name = "RAVET AU TEST MIDI", *id = "org.ravet.test.auv2.midi";
constexpr uint32_t subtype = 0x52764d31;
#else
constexpr uint32_t type = 0x61756678, category = RAVET_PLUGIN_EFFECT;
const RavetPluginLayoutV1 layouts[]{{2, 2, 0, 0}, {1, 1, 0, 0}};
constexpr const char *name = "RAVET AU TEST Gain", *id = "org.ravet.test.auv2.gain";
constexpr uint32_t subtype = 0x52765431;
#endif
const RavetPluginProductV1 product{
    name, "RAVET Tests", "0.1.0", id, "", category,
    category == RAVET_PLUGIN_EFFECT ? 0u : 1u, category == RAVET_PLUGIN_EFFECT ? 0u : 1u,
    2, layouts, 3, 1, id, {}, {}, type, subtype, 0x52765473, 0, 0, "urn:ravet:test", 320, 180, 1, 0
};
const RavetPluginParameterV1 parameters[]{
    {gainID, RAVET_PLUGIN_PARAMETER_AUTOMATABLE, 0, 0, 0, 2, 1, "gain", "Gain", ""},
    {bypassID, RAVET_PLUGIN_PARAMETER_AUTOMATABLE | RAVET_PLUGIN_PARAMETER_BYPASS | RAVET_PLUGIN_PARAMETER_STEPPED,
        0, 1, 0, 1, 0, "bypass", "Bypass", ""},
    {choiceID, RAVET_PLUGIN_PARAMETER_AUTOMATABLE | RAVET_PLUGIN_PARAMETER_STEPPED,
        0, 2, 0, 2, 0, "choice", "Choice", ""}
};
int32_t parameter(uint32_t i, RavetPluginParameterV1 *p) { if (i >= 3 || !p) return 1; *p = parameters[i]; return 0; }
int32_t index(uint32_t id) { for (int i = 0; i < 3; ++i) if (parameters[i].id == id) return i; return -1; }
int32_t normalize(uint32_t i, double value, double *out) {
    if (i >= 3 || !out || !std::isfinite(value)) return 1;
    value = std::clamp(value, parameters[i].minimum, parameters[i].maximum);
    if (parameters[i].steps) value = std::floor(value + .5);
    *out = value / parameters[i].maximum; return 0;
}
int32_t denormalize(uint32_t i, double value, double *out) {
    if (i >= 3 || !out || !std::isfinite(value)) return 1;
    value = std::clamp(value, 0., 1.) * parameters[i].maximum;
    *out = parameters[i].steps ? std::floor(value + .5) : value; return 0;
}
int32_t format(uint32_t i, double value, char *text, uint32_t capacity) {
    if (i >= 3 || !text || !capacity || !std::isfinite(value)) return 1;
    const char *choices[]{"First", "Second", "Third"};
    const auto n = i == 2 ? std::snprintf(text, capacity, "%s", choices[std::clamp(int(std::round(value)), 0, 2)]) :
        std::snprintf(text, capacity, "%.3f", value);
    return n < 0 || uint32_t(n) >= capacity;
}
int32_t parse(uint32_t i, const char *text, double *value) {
    if (i >= 3 || !text || !value) return 1;
    if (i == 2) {
        const char *choices[]{"First", "Second", "Third"};
        for (int j = 0; j < 3; ++j) if (!std::strcmp(text, choices[j])) { *value = j; return 0; }
    }
    char *end = nullptr; const auto decoded = std::strtod(text, &end);
    if (end == text || *end || !std::isfinite(decoded)) return 1;
    *value = decoded; return 0;
}
void *create() { mainThread(); ++live; return new Instance; }
void destroy(void *p) { mainThread(); --live; delete static_cast<Instance *>(p); }
double get(const void *p, uint32_t i) { return i < 3 ? static_cast<const Instance *>(p)->values[i].load() : 0; }
int32_t set(void *p, uint32_t i, double value) {
    mainThread(); double n = 0, canonical = 0;
    if (normalize(i, value, &n) || denormalize(i, n, &canonical)) return 1;
    static_cast<Instance *>(p)->values[i] = canonical; return 0;
}
struct State { char magic[8]; double values[3]; };
int32_t save(void *p, uint8_t *data, uint32_t capacity, uint32_t *length) {
    mainThread(); if (!length) return 1; *length = sizeof(State); if (!data) return 0;
    if (capacity < sizeof(State)) return 1;
    State state{{'R','A','V','E','T','T','S','T'}, {get(p, 0), get(p, 1), get(p, 2)}};
    std::memcpy(data, &state, sizeof(state)); return 0;
}
int32_t load(void *p, const uint8_t *data, uint32_t length) {
    mainThread(); if (!data || length != sizeof(State)) return 1;
    State state{}; std::memcpy(&state, data, sizeof(state));
    if (std::memcmp(state.magic, "RAVETTST", 8)) return 1;
    for (uint32_t i = 0; i < 3; ++i) if (!std::isfinite(state.values[i]) || state.values[i] < 0 || state.values[i] > parameters[i].maximum) return 1;
    for (uint32_t i = 0; i < 3; ++i) static_cast<Instance *>(p)->values[i] = state.values[i];
    return 0;
}
int32_t prepare(void *p, const RavetNativeProcessorConfigV1 *config) {
    mainThread(); auto &s = *static_cast<Instance *>(p);
    if (!config || config->version != 1 || config->bus_count != 1 || !config->maximum_frames || config->maximum_events != 1024) return 1;
    s.maximum = config->maximum_frames; s.prepared = true; return 0;
}
int32_t process(void *p, const RavetNativeProcessorBusV1 *bus, uint32_t count, uint32_t frames,
    const RavetNativeEventV1 *events, uint32_t eventCount, const RavetNativeTransportV1 *transport,
    RavetNativeEventV1 *out, uint32_t capacity, RavetNativeProcessorResultV1 *result) {
    auto &s = *static_cast<Instance *>(p); *result = {};
    if (!s.prepared || count != 1 || frames > s.maximum) return 1;
    if (transport && transport->present == (RAVET_TRANSPORT_BEATS | RAVET_TRANSPORT_TEMPO | RAVET_TRANSPORT_PLAYING |
            RAVET_TRANSPORT_SAMPLES | RAVET_TRANSPORT_DETAILS) && transport->tempo == 123 && transport->beat_position == 5 &&
            transport->sample_position == 456 && transport->flags == RAVET_TRANSPORT_FLAG_PLAYING) ++transportCalls;
    uint32_t next = 0;
    for (uint32_t frame = 0; frame < frames; ++frame) {
        while (next < eventCount && events[next].offset == frame) {
            const auto &e = events[next++];
            if (e.kind == RAVET_NATIVE_PARAMETER_VALUE) {
                const auto i = index(e.parameter_id); if (i < 0) return 1;
                s.values[i] = e.value;
                if (i == 2) s.changed = true;
            } else if (e.kind == RAVET_NATIVE_MIDI && result->event_count < capacity) out[result->event_count++] = e;
        }
        const float gain = s.values[1] >= .5 ? 1.f : float(s.values[0].load());
        auto outputs = static_cast<float *const *>(bus->outputs);
        auto inputs = static_cast<const float *const *>(bus->inputs);
        for (uint32_t c = 0; c < bus->output_channels; ++c)
            outputs[c][frame] = (bus->input_channels ? inputs[c][frame] : .25f) * gain;
    }
    return 0;
}
int32_t reset(void *) { return 0; }
int32_t deactivate(void *p) { mainThread(); static_cast<Instance *>(p)->prepared = false; return 0; }
int32_t timing(void *p, RavetNativeProcessorTimingV1 *t) {
    auto &s = *static_cast<Instance *>(p); if (!s.prepared) return 1;
    *t = {uint32_t(s.values[2].load()) * 48, 1, 480}; s.changed = false; return 0;
}
uint32_t changed(void *p) { return static_cast<Instance *>(p)->changed; }
uint32_t edits(void *p, RavetPluginEditV1 *out, uint32_t capacity) {
    mainThread(); auto &s = *static_cast<Instance *>(p); const auto n = std::min(capacity, s.editCount);
    std::copy_n(s.edits.begin(), n, out); s.editCount = 0; return n;
}
int32_t editorOpen(void *p, uint32_t kind, void *parent, uint32_t, uint32_t) {
    mainThread(); if (kind != RAVET_PLUGIN_VIEW_COCOA || !parent) return 1;
    auto &s = *static_cast<Instance *>(p); if (s.editor) return 1; s.editor = true; ++opened; return 0;
}
int32_t editorResize(void *, uint32_t, uint32_t) { mainThread(); return 0; }
void editorClose(void *p) { mainThread(); auto &s = *static_cast<Instance *>(p); if (s.editor) { s.editor = false; ++closed; } }
void editorIdle(void *p) {
    mainThread(); ++idle; auto &s = *static_cast<Instance *>(p);
    if (!s.edited) {
        s.edited = true; s.values[0] = 1.25;
        s.edits = {{{RAVET_PLUGIN_EDIT_BEGIN, 0, 0}, {RAVET_PLUGIN_EDIT_VALUE, 0, 1.25}, {RAVET_PLUGIN_EDIT_END, 0, 0}}};
        s.editCount = 3;
    }
}
const RavetPluginEntryV1 entry{1, sizeof(RavetPluginEntryV1), &product,
    parameter, index, format, parse, normalize, denormalize, create, destroy, get, set,
    save, load, prepare, process, reset, deactivate, timing, changed, edits,
    editorOpen, editorResize, editorClose, editorIdle};
}
extern "C" const RavetPluginEntryV1 *ravet_plugin_v1() { return &entry; }
extern "C" void ravet_au_mock_stats(uint32_t *stats) {
    stats[0] = live; stats[1] = opened; stats[2] = closed; stats[3] = idle; stats[4] = threadFaults; stats[5] = transportCalls;
}
