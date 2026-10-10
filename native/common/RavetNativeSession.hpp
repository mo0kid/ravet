#pragma once
#include "RavetNativeState.h"
#include <array>
#include <cmath>
#include <cstring>
#include <mutex>
#include <stdexcept>
#include <vector>

namespace ravet {
// One stopped/control owner, one serialized audio owner. The entry tables and
// their loaded library must outlive this object. Control methods may allocate or
// lock; render/apply/reflect do neither. Stop callbacks before prepare/destruction.
class NativeSession {
public:
    static constexpr uint32_t maximumParameters = 128;
    const RavetNativeStateEntryV1 *state;
    const RavetNativeParameterEntryV1 *parameters;
    const RavetNativeProcessorEntryV1 *processor;
    void *dsp = nullptr, *publication = nullptr;
    std::array<RavetNativeParameterInfoV1, maximumParameters> schema{};
    std::array<RavetNativeParameterValueV1, maximumParameters> audioValues{};
    bool prepared = false;

    explicit NativeSession(const RavetNativeStateEntryV1 *entry) : state(entry) {
        if (!state || state->version != 1 || state->byte_size != sizeof(*state) ||
            state->sample_bits != 32 || state->parameter_count > maximumParameters)
            throw std::invalid_argument("Unsupported native state entry");
        parameters = state->parameters_entry();
        processor = parameters ? parameters->processor_entry() : nullptr;
        if (!parameters || !processor || parameters->version != 1 || processor->version != 1 ||
            parameters->byte_size != sizeof(*parameters) || processor->byte_size != sizeof(*processor) ||
            parameters->sample_bits != 32 || processor->sample_bits != 32 ||
            parameters->parameter_count != state->parameter_count ||
            parameters->state_bytes != 16 + 8 * state->parameter_count)
            throw std::invalid_argument("Mismatched native entry tables");
        for (uint32_t i = 0; i < count(); ++i) {
            if (parameters->info(i, &schema[i])) throw std::invalid_argument("Invalid native schema");
            audioValues[i] = {schema[i].id, schema[i].default_value};
        }
        dsp = processor->create();
        publication = state->create(nullptr, 0);
        if (!dsp || !publication) {
            if (dsp) processor->destroy(dsp);
            if (publication) state->destroy(publication);
            throw std::runtime_error("Native instance creation failed");
        }
    }
    ~NativeSession() {
        if (prepared) processor->deactivate(dsp);
        processor->destroy(dsp);
        state->destroy(publication);
    }
    NativeSession(const NativeSession &) = delete;
    NativeSession &operator=(const NativeSession &) = delete;
    uint32_t count() const { return state->parameter_count; }
    int index(uint32_t id) const {
        for (uint32_t i = 0; i < count(); ++i) if (schema[i].id == id) return int(i);
        return -1;
    }
    bool prepare(double rate, uint32_t inputs, uint32_t outputs, uint32_t frames, uint32_t outputEvents = 0) {
        stop();
        RavetNativeProcessorConfigV1 config{};
        config.version = 1; config.sample_rate = rate; config.maximum_frames = frames;
        config.maximum_events = 1024; config.maximum_output_events = outputEvents;
        config.bus_count = 1; config.buses[0] = {inputs, outputs};
        prepared = processor->prepare(dsp, &config) == 0;
        return prepared;
    }
    void stop() {
        if (prepared) processor->deactivate(dsp);
        prepared = false;
    }
    // Control publication is complete and canonical; malformed recall preserves
    // the previous state. Concurrent control edits serialize here, never in DSP.
    bool restore(const uint8_t *bytes, uint32_t length) {
        std::lock_guard<std::mutex> guard(controlMutex);
        RavetNativeStateRevisionV1 revision{};
        return state->publish(publication, bytes, length, &revision) == 0;
    }
    std::vector<uint8_t> save() {
        std::lock_guard<std::mutex> guard(controlMutex);
        std::vector<uint8_t> bytes(parameters->state_bytes);
        RavetNativeStateRevisionV1 revision{};
        if (state->save_state(publication, bytes.data(), uint32_t(bytes.size()), &revision)) return {};
        return bytes;
    }
    bool set(uint32_t id, float value) {
        const int slot = index(id);
        if (slot < 0 || !std::isfinite(value) || (schema[slot].flags & RAVET_PARAMETER_READ_ONLY)) return false;
        float normalized = 0, snapped = 0;
        if (parameters->normalize(id, value, &normalized) || parameters->denormalize(id, normalized, &snapped)) return false;
        std::lock_guard<std::mutex> guard(controlMutex);
        std::array<uint8_t, 16 + 8 * maximumParameters> bytes{};
        RavetNativeStateRevisionV1 revision{};
        // A concurrent audio reflection can win between read and publish. The
        // UI edit deliberately publishes its complete observed snapshot.
        if (state->save_state(publication, bytes.data(), parameters->state_bytes, &revision)) return false;
        uint32_t bits = 0; std::memcpy(&bits, &snapped, sizeof(bits));
        for (unsigned b = 0; b < 4; ++b) bytes[16 + 8 * slot + 4 + b] = uint8_t(bits >> (8 * b));
        return state->publish(publication, bytes.data(), parameters->state_bytes, &revision) == 0;
    }
    float get(uint32_t id) const {
        const int slot = index(id);
        if (slot < 0) return 0;
        std::array<RavetNativeParameterValueV1, maximumParameters> values{};
        RavetNativeStateRevisionV1 revision{};
        if (state->snapshot(publication, values.data(), count(), &revision)) return schema[slot].default_value;
        return values[slot].value;
    }
    int apply(bool *changed = nullptr) {
        std::array<RavetNativeParameterValueV1, maximumParameters> values{};
        RavetNativeStateRevisionV1 revision{};
        const int snapshotCode = state->snapshot(publication, values.data(), count(), &revision);
        if (snapshotCode) return snapshotCode == 3 ? 0 : snapshotCode;
        RavetNativeStateApplyV1 result{};
        const int code = state->apply(dsp, publication, &result);
        if (changed) *changed = code == 0 && result.changed != 0;
        if (code == 0 && result.changed) {
            if (revision.instance != result.revision.instance || revision.generation != result.revision.generation) {
                if (state->snapshot(publication, values.data(), count(), &revision) ||
                    revision.instance != result.revision.instance || revision.generation != result.revision.generation) {
                    // A newer control publication raced the accepted restore.
                    // DSP remains coherent; retain the last known ramp base.
                    // Do not turn ordinary concurrent edits into audio failure.
                    return 0;
                }
            }
            audioValues = values;
        }
        return code == 3 ? 0 : code; // Busy retains the last accepted DSP state.
    }
    int render(const float *const *inputs, float *const *outputs, uint32_t inputChannels,
               uint32_t outputChannels, uint32_t frames, const RavetNativeEventV1 *events,
               uint32_t eventCount, const RavetNativeTransportV1 *transport = nullptr) {
        RavetNativeProcessorBusV1 bus{inputChannels, outputChannels, inputs, outputs};
        RavetNativeProcessorResultV1 result{};
        const int code = processor->process(dsp, &bus, 1, frames, events, eventCount, transport,
                                            generated.data(), uint32_t(generated.size()), &result);
        generatedCount = code == 0 ? result.event_count : 0;
        return code;
    }
    int reflect() {
        RavetNativeStateRevisionV1 revision{};
        return state->reflect(dsp, publication, &revision); // Busy/Stale never replace newer recall.
    }
    std::array<RavetNativeEventV1, 1024> generated{};
    uint32_t generatedCount = 0;
private:
    std::mutex controlMutex;
};
}
