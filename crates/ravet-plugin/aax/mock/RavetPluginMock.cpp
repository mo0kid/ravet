// A small stand-in for a product library's ravet_plugin_v1(), used to build and
// test the generic AAX adapter before (or without) a Rust product. Link a real
// product's static library instead to build that product; nothing else changes.
//
// Parameters: Gain (dB, continuous), Mode (stepped 0..2; sets latency to
// mode*64 samples), Bypass (host bypass role), Output Peak (read-only) and a
// hidden Calibration parameter. Layouts: mono 1x1 and stereo 2x2. MIDI input is
// echoed to MIDI output. A 480x320 editor is reported; editor_open only records
// the parent view. Test hooks are exported as ravet_mock_* for the adapter tests.
#include "RavetPlugin.h"
#include <algorithm>
#include <atomic>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <deque>
#include <mutex>

namespace {

enum : uint32_t { Gain, Mode, Bypass, Peak, Calibration, Count };

const RavetPluginLayoutV1 layouts[] = {{1, 1, 0x526d4d31 /* RmM1 */, 0}, {2, 2, 0x526d5332 /* RmS2 */, 0}};

const RavetPluginParameterV1 parameters[Count] = {
    {0x47414e00, RAVET_PLUGIN_PARAMETER_AUTOMATABLE, 0, 0, -60., 12., 0., "gain_db", "Gain", "dB"},
    {0x4d4f4445, RAVET_PLUGIN_PARAMETER_AUTOMATABLE | RAVET_PLUGIN_PARAMETER_STEPPED, 0, 2, 0., 2., 0., "mode", "Mode", ""},
    {0x42595053, RAVET_PLUGIN_PARAMETER_AUTOMATABLE | RAVET_PLUGIN_PARAMETER_BYPASS | RAVET_PLUGIN_PARAMETER_STEPPED, 0, 1, 0., 1., 0., "bypass", "Bypass", ""},
    {0x5045414b, RAVET_PLUGIN_PARAMETER_READ_ONLY, 0, 0, 0., 1., 0., "peak", "Output Peak", ""},
    {0x43414c49, RAVET_PLUGIN_PARAMETER_HIDDEN, 0, 0, -1., 1., 0., "calibration", "Calibration", ""},
};

RavetPluginProductV1 product() {
    RavetPluginProductV1 p{};
    p.name = "RAVET Mock";
    p.vendor = "RAVET";
    p.version = "0.1.0";
    p.plugin_id = "org.ravet.mock";
    p.url = "";
    p.category = RAVET_PLUGIN_EFFECT;
    p.note_input = 1;
    p.note_output = 1;
    p.layout_count = 2;
    p.layouts = layouts;
    p.parameter_count = Count;
    p.bypass_index = Bypass;
    p.clap_id = "org.ravet.mock";
    p.aax_manufacturer = 0x52617674; // Ravt
    p.aax_product = 0x526d6f63;      // Rmoc
    p.lv2_uri = "urn:org.ravet.mock";
    p.editor_width = 480;
    p.editor_height = 320;
    return p;
}
const RavetPluginProductV1 mockProduct = product();

struct Instance {
    std::atomic<double> values[Count];
    double processorValues[Count];
    uint32_t channels = 0;
    bool prepared = false;
    uint32_t latency = 0, latencyRead = 0;
    std::atomic<uint32_t> latencyChanged{0};
    std::mutex editLock;
    std::deque<RavetPluginEditV1> edits;
    // observations for tests
    std::atomic<uint32_t> blocks{0}, lastEvents{0}, lastTransport{0}, editorIdle{0};
    double lastTempo = 0.;
    void *editorParent = nullptr;
    uint32_t editorKind = 0;
    std::atomic<uint32_t> parameterEvents{0};
    Instance() {
        for (uint32_t i = 0; i < Count; ++i) values[i] = processorValues[i] = parameters[i].default_value;
    }
};

bool known(uint32_t index) { return index < Count; }

double snap(uint32_t index, double v) {
    const auto &p = parameters[index];
    v = std::min(std::max(v, p.minimum), p.maximum);
    if (p.steps) v = p.minimum + std::round((v - p.minimum) / (p.maximum - p.minimum) * p.steps) * (p.maximum - p.minimum) / p.steps;
    return v;
}

int32_t parameter(uint32_t index, RavetPluginParameterV1 *out) {
    if (!known(index) || !out) return 1;
    *out = parameters[index];
    return 0;
}
int32_t parameterIndex(uint32_t id) {
    for (uint32_t i = 0; i < Count; ++i)
        if (parameters[i].id == id) return int32_t(i);
    return -1;
}
int32_t valueToText(uint32_t index, double v, char *text, uint32_t capacity) {
    if (!known(index) || !text || !capacity) return 1;
    if (index == Mode) {
        static const char *names[] = {"Clean", "Warm", "Hot"};
        std::snprintf(text, capacity, "%s", names[int(snap(index, v))]);
    } else if (index == Bypass) {
        std::snprintf(text, capacity, "%s", v >= .5 ? "On" : "Off");
    } else {
        std::snprintf(text, capacity, "%.2f%s%s", v, parameters[index].unit[0] ? " " : "", parameters[index].unit);
    }
    return 0;
}
int32_t textToValue(uint32_t index, const char *text, double *out) {
    if (!known(index) || !text || !out) return 1;
    if (index == Mode) {
        static const char *names[] = {"Clean", "Warm", "Hot"};
        for (int i = 0; i < 3; ++i)
            if (!std::strcmp(text, names[i])) return *out = i, 0;
    }
    if (index == Bypass) {
        if (!std::strcmp(text, "On")) return *out = 1, 0;
        if (!std::strcmp(text, "Off")) return *out = 0, 0;
    }
    char *end = nullptr;
    const double v = std::strtod(text, &end);
    if (end == text || !std::isfinite(v)) return 1;
    *out = snap(index, v);
    return 0;
}
int32_t normalize(uint32_t index, double physical, double *out) {
    if (!known(index) || !out || !std::isfinite(physical)) return 1;
    const auto &p = parameters[index];
    *out = (snap(index, physical) - p.minimum) / (p.maximum - p.minimum);
    return 0;
}
int32_t denormalize(uint32_t index, double normalized, double *out) {
    if (!known(index) || !out || !std::isfinite(normalized)) return 1;
    const auto &p = parameters[index];
    *out = snap(index, p.minimum + std::min(std::max(normalized, 0.), 1.) * (p.maximum - p.minimum));
    return 0;
}

void *create() { return new Instance; }
void destroy(void *i) { delete static_cast<Instance *>(i); }
double getValue(const void *i, uint32_t index) {
    return i && known(index) ? static_cast<const Instance *>(i)->values[index].load() : 0.;
}
int32_t setValue(void *i, uint32_t index, double v) {
    if (!i || !known(index) || !std::isfinite(v) || (parameters[index].flags & RAVET_PLUGIN_PARAMETER_READ_ONLY)) return 1;
    static_cast<Instance *>(i)->values[index] = snap(index, v);
    return 0;
}

const char magic[8] = {'M', 'O', 'C', 'K', 'S', 'T', 'A', 'T'};
int32_t saveState(void *i, uint8_t *data, uint32_t capacity, uint32_t *length) {
    if (!i || !length) return 1;
    auto *self = static_cast<Instance *>(i);
    const uint32_t size = 8 + 4 + (Count - 1) * 12;
    *length = size;
    if (!data) return 0;
    if (capacity < size) return 1;
    std::memcpy(data, magic, 8);
    uint32_t n = Count - 1, at = 12;
    std::memcpy(data + 8, &n, 4);
    for (uint32_t k = 0; k < Count; ++k) {
        if (k == Peak) continue;
        const double v = self->values[k];
        std::memcpy(data + at, &parameters[k].id, 4);
        std::memcpy(data + at + 4, &v, 8);
        at += 12;
    }
    return 0;
}
int32_t loadState(void *i, const uint8_t *data, uint32_t length) {
    if (!i || !data || length < 12 || std::memcmp(data, magic, 8)) return 1;
    uint32_t n = 0;
    std::memcpy(&n, data + 8, 4);
    if (length != 12 + n * 12) return 1;
    double staged[Count];
    for (uint32_t k = 0; k < Count; ++k) staged[k] = parameters[k].default_value;
    for (uint32_t r = 0; r < n; ++r) {
        uint32_t id = 0;
        double v = 0;
        std::memcpy(&id, data + 12 + r * 12, 4);
        std::memcpy(&v, data + 16 + r * 12, 8);
        const int32_t index = parameterIndex(id);
        if (index < 0) continue;
        if (!std::isfinite(v) || v < parameters[index].minimum || v > parameters[index].maximum) return 1;
        staged[index] = v;
    }
    auto *self = static_cast<Instance *>(i);
    for (uint32_t k = 0; k < Count; ++k)
        if (k != Peak) self->values[k] = staged[k];
    return 0;
}

int32_t prepare(void *i, const RavetNativeProcessorConfigV1 *config) {
    if (!i || !config || config->version != 1 || config->bus_count != 1) return 1;
    const auto &bus = config->buses[0];
    if (!((bus.inputs == 1 && bus.outputs == 1) || (bus.inputs == 2 && bus.outputs == 2))) return 1;
    auto *self = static_cast<Instance *>(i);
    self->channels = bus.outputs;
    self->prepared = true;
    for (uint32_t k = 0; k < Count; ++k) self->processorValues[k] = self->values[k];
    self->latency = self->latencyRead = uint32_t(self->processorValues[Mode]) * 64;
    return 0;
}

int32_t process(void *i, const RavetNativeProcessorBusV1 *buses, uint32_t busCount, uint32_t frames,
                const RavetNativeEventV1 *input, uint32_t inputCount, const RavetNativeTransportV1 *transport,
                RavetNativeEventV1 *output, uint32_t capacity, RavetNativeProcessorResultV1 *result) {
    auto *self = static_cast<Instance *>(i);
    if (!self || !self->prepared || !buses || busCount != 1 || !result || frames > 1024) return 1;
    *result = {};
    // Cached host-side changes and editor edits arrive at the start of the block.
    for (uint32_t k = 0; k < Count; ++k)
        if (k != Peak) self->processorValues[k] = self->values[k];
    uint32_t events = 0;
    for (uint32_t e = 0; e < inputCount; ++e) {
        if (input[e].kind == RAVET_NATIVE_PARAMETER_VALUE) {
            const int32_t index = parameterIndex(input[e].parameter_id);
            if (index >= 0) self->processorValues[index] = snap(uint32_t(index), input[e].value);
            self->parameterEvents++;
        } else if (input[e].kind == RAVET_NATIVE_MIDI && events < capacity) {
            output[events++] = input[e];
        }
    }
    const uint32_t latency = uint32_t(self->processorValues[Mode]) * 64;
    if (latency != self->latency) {
        self->latency = latency;
        if (latency != self->latencyRead) self->latencyChanged = 1;
    }
    const auto &bus = buses[0];
    auto *const *in = static_cast<float *const *>(bus.inputs);
    auto *const *out = static_cast<float *const *>(bus.outputs);
    const bool bypass = self->processorValues[Bypass] >= .5;
    const float gain = bypass ? 1.f : float(std::pow(10., self->processorValues[Gain] / 20.));
    float peak = 0.f;
    for (uint32_t c = 0; c < bus.output_channels; ++c)
        for (uint32_t f = 0; f < frames; ++f) {
            const float v = (c < bus.input_channels ? in[c][f] : 0.f) * gain;
            out[c][f] = v;
            peak = std::max(peak, std::fabs(v));
        }
    self->values[Peak] = std::min(1., double(peak));
    self->blocks++;
    self->lastEvents = inputCount;
    self->lastTransport = transport ? transport->present : 0;
    self->lastTempo = transport && (transport->present & RAVET_TRANSPORT_TEMPO) ? transport->tempo : 0.;
    result->event_count = events;
    return 0;
}
int32_t reset(void *i) { return i ? 0 : 1; }
int32_t deactivate(void *i) {
    if (!i) return 1;
    static_cast<Instance *>(i)->prepared = false;
    return 0;
}
int32_t timing(void *i, RavetNativeProcessorTimingV1 *t) {
    auto *self = static_cast<Instance *>(i);
    if (!self || !t || !self->prepared) return 1;
    *t = {};
    t->latency_samples = self->latency;
    t->tail_kind = 1;
    t->tail_samples = 4800;
    self->latencyRead = self->latency;
    self->latencyChanged = 0;
    return 0;
}
uint32_t latencyChanged(void *i) { return i ? static_cast<Instance *>(i)->latencyChanged.load() : 0; }
uint32_t takeEdits(void *i, RavetPluginEditV1 *edits, uint32_t capacity) {
    auto *self = static_cast<Instance *>(i);
    if (!self || !edits) return 0;
    std::lock_guard<std::mutex> guard(self->editLock);
    uint32_t n = 0;
    while (n < capacity && !self->edits.empty()) {
        edits[n++] = self->edits.front();
        self->edits.pop_front();
    }
    return n;
}
int32_t editorOpen(void *i, uint32_t kind, void *parent, uint32_t width, uint32_t height) {
    auto *self = static_cast<Instance *>(i);
    if (!self || !parent || width != 480 || height != 320) return 1;
    self->editorParent = parent;
    self->editorKind = kind;
    return 0;
}
int32_t editorResize(void *i, uint32_t, uint32_t) { return i ? 0 : 1; }
void editorClose(void *i) {
    if (auto *self = static_cast<Instance *>(i)) self->editorParent = nullptr;
}
void editorIdle(void *i) {
    if (auto *self = static_cast<Instance *>(i)) self->editorIdle++;
}

const RavetPluginEntryV1 table = {
    RAVET_PLUGIN_VERSION_1, sizeof(RavetPluginEntryV1), &mockProduct,
    parameter, parameterIndex, valueToText, textToValue, normalize, denormalize,
    create, destroy, getValue, setValue, saveState, loadState,
    prepare, process, reset, deactivate, timing, latencyChanged, takeEdits,
    editorOpen, editorResize, editorClose, editorIdle,
};

} // namespace

extern "C" const RavetPluginEntryV1 *ravet_plugin_v1(void) { return &table; }

// ---- Test hooks (not part of RavetPlugin.h) ----
extern "C" void ravet_mock_push_edit(void *instance, uint32_t kind, uint32_t index, double value) {
    auto *self = static_cast<Instance *>(instance);
    std::lock_guard<std::mutex> guard(self->editLock);
    self->edits.push_back({kind, index, value});
}
extern "C" uint32_t ravet_mock_blocks(void *instance) { return static_cast<Instance *>(instance)->blocks; }
extern "C" uint32_t ravet_mock_last_transport(void *instance) { return static_cast<Instance *>(instance)->lastTransport; }
extern "C" double ravet_mock_last_tempo(void *instance) { return static_cast<Instance *>(instance)->lastTempo; }
extern "C" void *ravet_mock_editor_parent(void *instance) { return static_cast<Instance *>(instance)->editorParent; }
extern "C" uint32_t ravet_mock_editor_kind(void *instance) { return static_cast<Instance *>(instance)->editorKind; }
extern "C" uint32_t ravet_mock_editor_idle(void *instance) { return static_cast<Instance *>(instance)->editorIdle; }
