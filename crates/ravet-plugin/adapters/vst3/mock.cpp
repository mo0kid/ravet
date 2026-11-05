// Independent C++ RavetPluginEntryV1 fixture; no Rust symbols required.
#include "RavetPlugin.h"
#include <algorithm>
#include <array>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <new>
#include <vector>
namespace ravet_mock {
struct Instance {
    std::array<double, 5> values{{1., 0., 1., .5, .25}};
    RavetNativeProcessorConfigV1 config{};
    bool active = false, editor = false, changed = false;
    uint32_t latency = 0;
    std::vector<RavetPluginEditV1> edits;
};
const RavetPluginLayoutV1 layouts[] = {{2, 2, 0, 0}, {1, 1, 0, 0}};
const RavetPluginProductV1 product = {
    "RAVET Generic Mock", "RAVET", "0.1.0", "org.ravet.generic.mock", "https://ravet.org",
    RAVET_PLUGIN_EFFECT, 1, 1, 2, layouts, 5, 1, "org.ravet.generic.mock",
    {0x4a18eac1, 0xac247439, 0xb642f713, 0x26fd0fd1},
    {0x4a18eac2, 0xac247439, 0xb642f713, 0x26fd0fd1},
    0, 0, 0, 0, 0, "urn:ravet:generic-mock", 400, 240, 1, 0
};
const RavetPluginParameterV1 parameters[] = {
    {101, 1, 0, 0, 0., 2., 1., "gain", "Gain", "x"},
    {102, 3 | RAVET_PLUGIN_PARAMETER_STEPPED, 0, 1, 0., 1., 0., "bypass", "Bypass", ""},
    {103, 1 | RAVET_PLUGIN_PARAMETER_STEPPED, 0, 3, 0., 3., 1., "mode", "Mode", ""},
    {0x60000000, RAVET_PLUGIN_PARAMETER_HIDDEN, 0, 0, 0., 1., .5, "hidden", "Hidden", ""},
    {105, RAVET_PLUGIN_PARAMETER_READ_ONLY, 0, 0, 0., 1., .25, "meter", "Meter", ""}
};
int32_t parameter(uint32_t i, RavetPluginParameterV1* p) { if (i >= 5 || !p) return 1; *p = parameters[i]; return 0; }
int32_t index(uint32_t id) {
    for (uint32_t i = 0; i < 5; ++i) if (parameters[i].id == id) return int32_t(i);
    return -1;
}
double snap(uint32_t i, double v) {
    auto p = parameters[i]; v = std::clamp(v, p.minimum, p.maximum);
    return p.steps ? p.minimum + std::round((v - p.minimum) * p.steps / (p.maximum - p.minimum)) * (p.maximum - p.minimum) / p.steps : v;
}
int32_t normalize(uint32_t i, double p, double* n) {
    if (i >= 5 || !n || !std::isfinite(p)) return 1;
    *n = (snap(i, p) - parameters[i].minimum) / (parameters[i].maximum - parameters[i].minimum); return 0;
}
int32_t denormalize(uint32_t i, double n, double* p) {
    if (i >= 5 || !p || !std::isfinite(n)) return 1;
    *p = snap(i, parameters[i].minimum + std::clamp(n, 0., 1.) * (parameters[i].maximum - parameters[i].minimum)); return 0;
}
int32_t text(uint32_t i, double p, char* out, uint32_t cap) {
    if (i >= 5 || !out || cap < 32) return 1;
    if (i == 1) std::snprintf(out, cap, "%s", p >= .5 ? "On" : "Off");
    else std::snprintf(out, cap, "%.6g", p); return 0;
}
int32_t parse(uint32_t i, const char* text, double* p) {
    if (i >= 5 || !text || !p) return 1;
    if (i == 1 && (!std::strcmp(text, "On") || !std::strcmp(text, "Off"))) { *p = !std::strcmp(text, "On"); return 0; }
    char* end = nullptr; auto value = std::strtod(text, &end);
    if (end == text || *end || !std::isfinite(value)) return 1;
    *p = snap(i, value); return 0;
}
void* create() { return new(std::nothrow) Instance; }
void destroy(void* p) { delete static_cast<Instance*>(p); }
double get(const void* p, uint32_t i) { return i < 5 ? static_cast<const Instance*>(p)->values[i] : 0.; }
int32_t set(void* p, uint32_t i, double v) {
    if (!p || i >= 5 || !std::isfinite(v)) return 1;
    static_cast<Instance*>(p)->values[i] = snap(i, v); return 0;
}
int32_t save(void* p, uint8_t* out, uint32_t cap, uint32_t* length) {
    if (!length) return 1;
    *length = 48; if (!out) return 0;
    if (cap < 48) return 1;
    std::memcpy(out, "MOCK0001", 8);
    std::memcpy(out + 8, static_cast<Instance*>(p)->values.data(), 40); return 0;
}
int32_t load(void* p, const uint8_t* data, uint32_t length) {
    if (!data || length != 48 || std::memcmp(data, "MOCK0001", 8)) return 1;
    std::array<double, 5> values{}; std::memcpy(values.data(), data + 8, 40);
    for (uint32_t i = 0; i < 5; ++i) if (!std::isfinite(values[i]) || snap(i, values[i]) != values[i]) return 1;
    static_cast<Instance*>(p)->values = values; return 0;
}
int32_t prepare(void* p, const RavetNativeProcessorConfigV1* c) {
    if (!c || c->version != 1 || c->bus_count != 1 || !c->maximum_frames || c->maximum_frames > 65536 ||
        !std::isfinite(c->sample_rate) || c->sample_rate <= 0. ||
        c->buses[0].inputs != c->buses[0].outputs || c->buses[0].inputs < 1 || c->buses[0].inputs > 2) return 1;
    auto& s = *static_cast<Instance*>(p); s.config = *c; s.active = true; return 0;
}
// These test observations are control-thread-only after callbacks return.
std::array<RavetNativeEventV1, 1024> received{};
uint32_t receivedCount = 0, processCalls = 0, closed = 0, idled = 0;
int32_t process(void* p, const RavetNativeProcessorBusV1* b, uint32_t buses, uint32_t frames,
    const RavetNativeEventV1* input, uint32_t count, const RavetNativeTransportV1*,
    RavetNativeEventV1* output, uint32_t cap, RavetNativeProcessorResultV1* result) {
    auto& s = *static_cast<Instance*>(p);
    if (!s.active || buses != 1 || frames > s.config.maximum_frames || count > 1024 || cap < 1024) return 1;
    ++processCalls; receivedCount = count; std::copy_n(input, count, received.begin());
    result->status = 0; result->event_count = 0; uint32_t cursor = 0;
    for (uint32_t frame = 0; frame <= frames; ++frame) {
        while (cursor < count && input[cursor].offset == frame) {
            auto e = input[cursor++];
            if (e.kind == RAVET_NATIVE_PARAMETER_VALUE) {
                auto i = index(e.parameter_id); if (i >= 0) s.values[uint32_t(i)] = e.value;
                if (i == 2 && e.value == 3.) { s.latency = 32; s.changed = true; }
            } else output[result->event_count++] = e;
        }
        if (frame == frames) break;
        for (uint32_t channel = 0; channel < b->output_channels; ++channel) {
            auto in = static_cast<float* const*>(b->inputs)[channel];
            auto out = static_cast<float* const*>(b->outputs)[channel];
            out[frame] = in[frame] * float(s.values[1] >= .5 ? 1. : s.values[0]);
        }
    }
    return 0;
}
int32_t reset(void*) { return 0; }
int32_t deactivate(void* p) { static_cast<Instance*>(p)->active = false; return 0; }
int32_t timing(void* p, RavetNativeProcessorTimingV1* t) { *t = {static_cast<Instance*>(p)->latency, 1, 24000}; static_cast<Instance*>(p)->changed = false; return 0; }
uint32_t changed(void* p) { return static_cast<Instance*>(p)->changed; }
uint32_t edits(void* p, RavetPluginEditV1* out, uint32_t cap) {
    auto& e = static_cast<Instance*>(p)->edits; auto n = uint32_t(std::min<size_t>(cap, e.size()));
    std::copy_n(e.begin(), n, out); e.erase(e.begin(), e.begin() + n); return n;
}
int32_t open(void* p, uint32_t, void*, uint32_t, uint32_t) { static_cast<Instance*>(p)->editor = true; return 0; }
int32_t resize(void*, uint32_t, uint32_t) { return 0; }
void close(void* p) { static_cast<Instance*>(p)->editor = false; ++closed; }
void idle(void* p) {
    ++idled;
    auto& s = *static_cast<Instance*>(p);
    // Exercise the complete gesture queue once when the fixture editor idles.
    if (s.editor && s.values[0] != 1.5) {
        s.values[0] = 1.5;
        s.edits = {{RAVET_PLUGIN_EDIT_BEGIN, 0, 0.}, {RAVET_PLUGIN_EDIT_VALUE, 0, 1.5}, {RAVET_PLUGIN_EDIT_END, 0, 0.}};
    }
}
const RavetPluginEntryV1 table = {1, sizeof(RavetPluginEntryV1), &product,
    parameter, index, text, parse, normalize, denormalize, create, destroy, get, set,
    save, load, prepare, process, reset, deactivate, timing, changed, edits, open, resize, close, idle};
} // namespace ravet_mock
extern "C" const RavetPluginEntryV1* ravet_plugin_v1() { return &ravet_mock::table; }
