// A small C++ implementation of RavetPluginEntryV1 for testing format adapters
// before (and alongside) the Rust table. It is a stand-in for a product library
// and is swapped for the real one by pointing the build at another library.
//
// Product: "RAVET Mock", stereo (preferred) and mono effect with notes in/out.
//   0 gain    id 0x101  -60..12 dB, default 0, automatable
//   1 bypass  id 0x102  bypass toggle
//   2 mode    id 0x103  stepped 0..3 (Clean, Warm, Hot, Fuzz); latency = 32 × mode
//   3 level   id 0x104  read-only 0..1 (last block's peak output)
// Processing: out = in × gain (copy when bypassed); MIDI input is echoed.
// A probe records what the adapter delivered, for tests.
#include "RavetPlugin.h"

#include <algorithm>
#include <atomic>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <mutex>
#include <vector>

#if defined(_WIN32)
#define MOCK_EXPORT extern "C" __declspec(dllexport)
#else
#define MOCK_EXPORT extern "C" __attribute__((visibility("default")))
#endif

struct RavetMockLogEntry {
    uint32_t call, frames;
    RavetNativeEventV1 event;
};

struct RavetMockProbe {
    uint32_t prepare_calls, deactivate_calls, process_calls, last_frames, max_frames_seen;
    RavetNativeProcessorConfigV1 config;
    uint32_t log_count;
    RavetMockLogEntry log[4096];
    uint32_t transport_count;
    RavetNativeTransportV1 transport[64];  // one per process call, absent → present=0xffffffff
    uint32_t editor_open_calls, editor_kind, editor_width, editor_height, editor_close_calls, editor_idle_calls,
        editor_resize_calls;
    void *editor_parent;
    uint32_t load_calls;
};

namespace {
constexpr uint32_t parameterCount = 4;
const RavetPluginLayoutV1 layouts[] = {{2, 2, 0, 0}, {1, 1, 0, 0}};
const char *modes[] = {"Clean", "Warm", "Hot", "Fuzz"};
RavetMockProbe probe{};
std::mutex probeLock;

const RavetPluginParameterV1 parameters[parameterCount] = {
    {0x101, RAVET_PLUGIN_PARAMETER_AUTOMATABLE, 0, 0, -60.0, 12.0, 0.0, "gain", "Gain", "dB"},
    {0x102, RAVET_PLUGIN_PARAMETER_AUTOMATABLE | RAVET_PLUGIN_PARAMETER_BYPASS | RAVET_PLUGIN_PARAMETER_STEPPED, 0, 1,
     0.0, 1.0, 0.0, "bypass", "Bypass", ""},
    {0x103, RAVET_PLUGIN_PARAMETER_AUTOMATABLE | RAVET_PLUGIN_PARAMETER_STEPPED, 0, 3, 0.0, 3.0, 1.0, "mode", "Mode", ""},
    {0x104, RAVET_PLUGIN_PARAMETER_READ_ONLY, 0, 0, 0.0, 1.0, 0.0, "level", "Level", ""},
};

const RavetPluginProductV1 product = {
    "RAVET Mock", "RAVET", "1.2.3", "org.ravet.mock", "https://example.org/ravet-mock",
    RAVET_PLUGIN_EFFECT, 1, 1, 2, layouts, parameterCount, 1, "org.ravet.mock",
    {0, 0, 0, 0}, {0, 0, 0, 0}, 0, 0, 0, 0, 0, "urn:org.ravet.mock",
    640, 360, 1, 0,
};

struct Instance {
    std::atomic<double> values[parameterCount];
    std::atomic<bool> pendingReset{false};
    double rate = 0;
    uint32_t maxFrames = 0, outputs = 0, latency = 0;
    std::atomic<uint32_t> latencyChanged{0};
    bool prepared = false, editor = false;
    std::mutex editsLock;
    std::vector<RavetPluginEditV1> edits;
    Instance() { for (uint32_t i = 0; i < parameterCount; ++i) values[i] = parameters[i].default_value; }
};
std::atomic<Instance *> lastInstance{nullptr};

double snap(uint32_t index, double value) {
    const auto &p = parameters[index];
    value = std::min(std::max(value, p.minimum), p.maximum);
    if (p.steps) {
        const double step = (p.maximum - p.minimum) / p.steps;
        value = p.minimum + std::round((value - p.minimum) / step) * step;
    }
    return value;
}

int32_t parameter(uint32_t index, RavetPluginParameterV1 *out) {
    if (index >= parameterCount || !out) return 1;
    *out = parameters[index];
    return 0;
}
int32_t parameterIndex(uint32_t id) {
    for (uint32_t i = 0; i < parameterCount; ++i) if (parameters[i].id == id) return int32_t(i);
    return -1;
}
int32_t valueToText(uint32_t index, double value, char *text, uint32_t capacity) {
    if (index >= parameterCount || !text || !capacity) return 1;
    value = snap(index, value);
    if (index == 1) std::snprintf(text, capacity, "%s", value >= 0.5 ? "On" : "Off");
    else if (index == 2) std::snprintf(text, capacity, "%s", modes[int(value)]);
    else std::snprintf(text, capacity, "%.1f%s%s", value, *parameters[index].unit ? " " : "", parameters[index].unit);
    return 0;
}
int32_t textToValue(uint32_t index, const char *text, double *value) {
    if (index >= parameterCount || !text || !value) return 1;
    *value = snap(index, std::atof(text));
    return 0;
}
int32_t normalize(uint32_t index, double physical, double *normalized) {
    if (index >= parameterCount || !normalized) return 1;
    const auto &p = parameters[index];
    *normalized = (snap(index, physical) - p.minimum) / (p.maximum - p.minimum);
    return 0;
}
int32_t denormalize(uint32_t index, double normalized, double *physical) {
    if (index >= parameterCount || !physical) return 1;
    const auto &p = parameters[index];
    *physical = snap(index, p.minimum + std::min(std::max(normalized, 0.0), 1.0) * (p.maximum - p.minimum));
    return 0;
}
void *create() {
    auto *instance = new Instance();
    lastInstance = instance;
    return instance;
}
void destroy(void *handle) {
    auto *instance = static_cast<Instance *>(handle);
    Instance *expected = instance;
    lastInstance.compare_exchange_strong(expected, nullptr);
    delete instance;
}
double getValue(const void *handle, uint32_t index) {
    return index < parameterCount ? static_cast<const Instance *>(handle)->values[index].load() : 0.0;
}
int32_t setValue(void *handle, uint32_t index, double value) {
    if (index >= parameterCount || !std::isfinite(value)) return 1;
    static_cast<Instance *>(handle)->values[index] = snap(index, value);
    return 0;
}
// State: "MOCKSTA1" then four little-endian doubles.
int32_t saveState(void *handle, uint8_t *data, uint32_t capacity, uint32_t *length) {
    if (!length) return 1;
    constexpr uint32_t size = 8 + 8 * parameterCount;
    *length = size;
    if (!data) return 0;
    if (capacity < size) return 1;
    std::memcpy(data, "MOCKSTA1", 8);
    for (uint32_t i = 0; i < parameterCount; ++i) {
        const double value = static_cast<Instance *>(handle)->values[i];
        std::memcpy(data + 8 + 8 * i, &value, 8);
    }
    return 0;
}
int32_t loadState(void *handle, const uint8_t *data, uint32_t length) {
    auto *instance = static_cast<Instance *>(handle);
    if (!data || length != 8 + 8 * parameterCount || std::memcmp(data, "MOCKSTA1", 8)) return 1;
    double values[parameterCount];
    for (uint32_t i = 0; i < parameterCount; ++i) {
        std::memcpy(&values[i], data + 8 + 8 * i, 8);
        if (!std::isfinite(values[i])) return 1;
    }
    for (uint32_t i = 0; i < parameterCount; ++i) instance->values[i] = snap(i, values[i]);
    instance->pendingReset = true;
    std::lock_guard<std::mutex> lock(probeLock);
    ++probe.load_calls;
    return 0;
}
uint32_t latencyFor(const Instance &instance) { return 32u * uint32_t(instance.values[2].load()); }
int32_t prepare(void *handle, const RavetNativeProcessorConfigV1 *config) {
    auto *instance = static_cast<Instance *>(handle);
    if (!config || config->version != 1 || config->bus_count != 1 || !(config->sample_rate > 0)) return 1;
    bool known = false;
    for (const auto &layout : layouts)
        known |= layout.inputs == config->buses[0].inputs && layout.outputs == config->buses[0].outputs;
    if (!known) return 1;
    instance->rate = config->sample_rate;
    instance->maxFrames = config->maximum_frames;
    instance->outputs = config->buses[0].outputs;
    instance->latency = latencyFor(*instance);
    instance->prepared = true;
    std::lock_guard<std::mutex> lock(probeLock);
    ++probe.prepare_calls;
    probe.config = *config;
    return 0;
}
int32_t process(void *handle, const RavetNativeProcessorBusV1 *buses, uint32_t busCount, uint32_t frames,
                const RavetNativeEventV1 *input, uint32_t inputCount, const RavetNativeTransportV1 *transport,
                RavetNativeEventV1 *output, uint32_t outputCapacity, RavetNativeProcessorResultV1 *result) {
    auto *instance = static_cast<Instance *>(handle);
    if (!instance->prepared || !result || busCount != 1 || !buses || frames > instance->maxFrames) return 1;
    *result = {0, 0};
    {
        std::lock_guard<std::mutex> lock(probeLock);
        const uint32_t call = probe.process_calls++;
        probe.last_frames = frames;
        probe.max_frames_seen = std::max(probe.max_frames_seen, frames);
        for (uint32_t i = 0; i < inputCount && probe.log_count < 4096; ++i) probe.log[probe.log_count++] = {call, frames, input[i]};
        if (probe.transport_count < 64) {
            RavetNativeTransportV1 absent{};
            absent.present = 0xffffffffu;
            probe.transport[probe.transport_count++] = transport ? *transport : absent;
        }
    }
    for (uint32_t i = 0; i < inputCount; ++i) {
        const auto &event = input[i];
        if (event.offset >= std::max(frames, 1u)) return 1;
        if (event.kind == RAVET_NATIVE_PARAMETER_VALUE) {
            const int32_t index = parameterIndex(event.parameter_id);
            if (index >= 0) instance->values[index] = snap(uint32_t(index), event.value);
        } else if (event.kind == RAVET_NATIVE_MIDI && output && result->event_count < outputCapacity) {
            output[result->event_count++] = event;
        }
    }
    const uint32_t latency = latencyFor(*instance);
    if (latency != instance->latency) { instance->latency = latency; instance->latencyChanged = 1; }
    const double gain = std::pow(10.0, instance->values[0].load() / 20.0);
    const bool bypassed = instance->values[1].load() >= 0.5;
    const auto &bus = buses[0];
    const auto *const *in = static_cast<const float *const *>(bus.inputs);
    auto *const *out = static_cast<float *const *>(bus.outputs);
    float peak = 0;
    for (uint32_t c = 0; c < bus.output_channels && frames; ++c) {
        const float *source = in && c < bus.input_channels ? in[c] : nullptr;
        for (uint32_t f = 0; f < frames; ++f) {
            const float x = source ? source[f] : 0.f;
            out[c][f] = bypassed ? x : float(x * gain);
            peak = std::max(peak, std::fabs(out[c][f]));
        }
    }
    instance->values[3] = std::min(1.0, double(peak));
    return 0;
}
int32_t reset(void *) { return 0; }
int32_t deactivate(void *handle) {
    static_cast<Instance *>(handle)->prepared = false;
    std::lock_guard<std::mutex> lock(probeLock);
    ++probe.deactivate_calls;
    return 0;
}
int32_t timing(void *handle, RavetNativeProcessorTimingV1 *timing) {
    auto *instance = static_cast<Instance *>(handle);
    if (!timing || !instance->prepared) return 1;
    *timing = {instance->latency, 0, 0};
    return 0;
}
uint32_t latencyChanged(void *handle) { return static_cast<Instance *>(handle)->latencyChanged.exchange(0); }
uint32_t takeEdits(void *handle, RavetPluginEditV1 *edits, uint32_t capacity) {
    auto *instance = static_cast<Instance *>(handle);
    std::lock_guard<std::mutex> lock(instance->editsLock);
    const uint32_t count = std::min<uint32_t>(capacity, uint32_t(instance->edits.size()));
    std::copy(instance->edits.begin(), instance->edits.begin() + count, edits);
    instance->edits.erase(instance->edits.begin(), instance->edits.begin() + count);
    return count;
}
int32_t editorOpen(void *handle, uint32_t kind, void *parent, uint32_t width, uint32_t height) {
    auto *instance = static_cast<Instance *>(handle);
    if (instance->editor || !parent) return 1;
    instance->editor = true;
    probe.editor_open_calls++; probe.editor_kind = kind; probe.editor_parent = parent;
    probe.editor_width = width; probe.editor_height = height;
    return 0;
}
int32_t editorResize(void *, uint32_t width, uint32_t height) {
    probe.editor_resize_calls++; probe.editor_width = width; probe.editor_height = height;
    return 0;
}
void editorClose(void *handle) { static_cast<Instance *>(handle)->editor = false; probe.editor_close_calls++; }
void editorIdle(void *) { probe.editor_idle_calls++; }

const RavetPluginEntryV1 entry = {
    RAVET_PLUGIN_VERSION_1, sizeof(RavetPluginEntryV1), &product,
    parameter, parameterIndex, valueToText, textToValue, normalize, denormalize,
    create, destroy, getValue, setValue, saveState, loadState,
    prepare, process, reset, deactivate, timing, latencyChanged, takeEdits,
    editorOpen, editorResize, editorClose, editorIdle,
};
} // namespace

MOCK_EXPORT const RavetPluginEntryV1 *ravet_plugin_v1(void) { return &entry; }

// ---- Test probe (not part of RavetPlugin.h) ----
MOCK_EXPORT RavetMockProbe *ravet_mock_probe(void) { return &probe; }
MOCK_EXPORT void ravet_mock_probe_reset(void) {
    std::lock_guard<std::mutex> lock(probeLock);
    const uint32_t opens = probe.editor_open_calls;
    std::memset(&probe, 0, sizeof probe);
    probe.editor_open_calls = opens;
}
MOCK_EXPORT void *ravet_mock_last_instance(void) { return lastInstance.load(); }
MOCK_EXPORT void ravet_mock_push_edit(void *handle, uint32_t kind, uint32_t index, double value) {
    auto *instance = static_cast<Instance *>(handle);
    std::lock_guard<std::mutex> lock(instance->editsLock);
    instance->edits.push_back({kind, index, value});
    if (kind == RAVET_PLUGIN_EDIT_VALUE && index < parameterCount) instance->values[index] = snap(index, value);
}
