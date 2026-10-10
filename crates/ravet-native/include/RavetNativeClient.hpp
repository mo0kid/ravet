#pragma once
#include "RavetNativeAudioState.h"
#include "RavetNativeTelemetry.h"

namespace ravet::native {
// Optional SDK-independent C++17 owner for exact native v1 processor companions.
// Construct/prepare/deactivate/retire on CONTROL after callbacks stop; retain
// table/library externally for the complete lifetime. No copy/move or hidden
// mailbox/state/codec. Every DSP callback requires exclusive serialized owner;
// cached info permits shared reads. Foreign ranges obey the paired native APIs.
// Normal forwarding uses no C++ heap/locks/I/O; native/consumer obligations apply.
// Invalid profiles stay invalid (no fallback); callbacks return 1 without output
// mutation. Native callback failures retain their own result/silence semantics.
class NativeProcessorClient final {
public:
    explicit NativeProcessorClient(const RavetNativeProcessorEntryV1 *core,
                                   const RavetNativeAudioStateEntryV1 *state = nullptr,
                                   const RavetNativeTelemetryEntryV1 *telemetry = nullptr)
        : core_(core), state_(state), telemetry_(telemetry) {
        if (!validCore(core)) return;
        if (state) {
            if (state->version != 1 || state->byte_size != sizeof(*state) ||
                state->sample_bits != core->sample_bits || state->parameter_count > 1024 ||
                !state->cached_parameters_entry || !state->apply || !state->snapshot) return;
            parameters_ = state->cached_parameters_entry();
            if (!parameters_ || parameters_->version != 1 || parameters_->byte_size != sizeof(*parameters_) ||
                parameters_->sample_bits != core->sample_bits || parameters_->parameter_count != state->parameter_count ||
                !parameters_->parameters_entry || !parameters_->create || !parameters_->destroy ||
                !parameters_->info || !parameters_->normalize || !parameters_->denormalize || !parameters_->convert_events) return;
            const auto *schema = parameters_->parameters_entry();
            if (!schema || schema->version != 1 || schema->byte_size != sizeof(*schema) ||
                schema->sample_bits != core->sample_bits || schema->parameter_count != state->parameter_count ||
                !schema->processor_entry || !matchingCore(schema->processor_entry(), core)) return;
        }
        if (telemetry && (telemetry->version != 1 || telemetry->byte_size != sizeof(*telemetry) ||
            telemetry->sample_bits != core->sample_bits || telemetry->telemetry_count > 1024 ||
            !telemetry->processor_entry || !telemetry->create_cache || !telemetry->destroy_cache ||
            !telemetry->info || !telemetry->snapshot || !matchingCore(telemetry->processor_entry(), core))) return;
        if (state) { parameterCache_ = parameters_->create(); if (!parameterCache_) return; }
        if (telemetry) { telemetryCache_ = telemetry->create_cache(); if (!telemetryCache_) return; }
        processor_ = core->create();
    }
    ~NativeProcessorClient() {
        if (processor_) core_->destroy(processor_);
        if (telemetryCache_) telemetry_->destroy_cache(telemetryCache_);
        if (parameterCache_) parameters_->destroy(parameterCache_);
    }
    NativeProcessorClient(const NativeProcessorClient &) = delete;
    NativeProcessorClient &operator=(const NativeProcessorClient &) = delete;
    NativeProcessorClient(NativeProcessorClient &&) = delete;
    NativeProcessorClient &operator=(NativeProcessorClient &&) = delete;
    bool valid() const { return processor_ != nullptr; }
    uint32_t sampleBits() const { return valid() ? core_->sample_bits : 0; }
    bool hasAudioState() const { return valid() && state_ != nullptr; }
    bool hasTelemetry() const { return valid() && telemetry_ != nullptr; }
    uint32_t parameterCount() const { return hasAudioState() ? state_->parameter_count : 0; }
    uint32_t telemetryCount() const { return hasTelemetry() ? telemetry_->telemetry_count : 0; }
    int32_t prepare(const RavetNativeProcessorConfigV1 &config) { return valid() ? core_->prepare(processor_, &config) : 1; }
    int32_t reset() { return valid() ? core_->reset(processor_) : 1; }
    int32_t deactivate() { return valid() ? core_->deactivate(processor_) : 1; }
    int32_t timing(RavetNativeProcessorTimingV1 &output) { return valid() ? core_->timing(processor_, &output) : 1; }
    int32_t process(const RavetNativeProcessorBusV1 *buses, uint32_t busCount, uint32_t frames,
                    const RavetNativeEventV1 *events, uint32_t eventCount,
                    const RavetNativeTransportV1 *transport, RavetNativeEventV1 *outputEvents,
                    uint32_t outputCapacity, RavetNativeProcessorResultV1 &result) {
        return valid() ? core_->process(processor_, buses, busCount, frames, events, eventCount,
                                        transport, outputEvents, outputCapacity, &result) : 1;
    }
    // Explicit recall only when consumer scheduling selects new complete state;
    // repeated apply can restart smoothing. No accepted generation is invented.
    int32_t applyState(const RavetNativeParameterValueV1 *values, uint32_t count) {
        return hasAudioState() ? state_->apply(processor_, parameterCache_, values, count) : 1;
    }
    int32_t snapshotState(RavetNativeParameterValueV1 *values, uint32_t capacity) {
        return hasAudioState() ? state_->snapshot(processor_, parameterCache_, values, capacity) : 1;
    }
    int32_t parameterInfo(uint32_t index, RavetNativeParameterInfoV1 &output) const {
        return hasAudioState() ? parameters_->info(parameterCache_, index, &output) : 1;
    }
    int32_t telemetryInfo(uint32_t index, RavetNativeTelemetryInfoV1 &output) const {
        return hasTelemetry() ? telemetry_->info(telemetryCache_, index, &output) : 1;
    }
    int32_t snapshotTelemetry(RavetNativeTelemetryValueV1 *values, uint32_t capacity) {
        return hasTelemetry() ? telemetry_->snapshot(processor_, telemetryCache_, values, capacity) : 1;
    }
private:
    const RavetNativeProcessorEntryV1 *core_ = nullptr;
    const RavetNativeAudioStateEntryV1 *state_ = nullptr;
    const RavetNativeTelemetryEntryV1 *telemetry_ = nullptr;
    const RavetNativeCachedParameterEntryV1 *parameters_ = nullptr;
    void *processor_ = nullptr, *parameterCache_ = nullptr, *telemetryCache_ = nullptr;
    // Rust constant promotion may yield equivalent tables at distinct addresses.
    // Compare callback profiles, not table addresses. These checks do not
    // authenticate arbitrary foreign code; exact type/library pairing is still
    // the caller's obligation. Callback identities are required by this profile.
    static bool matchingCore(const RavetNativeProcessorEntryV1 *left,
                             const RavetNativeProcessorEntryV1 *right) {
        return validCore(left) && left->sample_bits == right->sample_bits &&
            left->create == right->create && left->destroy == right->destroy &&
            left->prepare == right->prepare && left->reset == right->reset &&
            left->deactivate == right->deactivate && left->process == right->process &&
            left->timing == right->timing;
    }
    static bool validCore(const RavetNativeProcessorEntryV1 *core) {
        return core && core->version == 1 && core->byte_size == sizeof(*core) &&
            (core->sample_bits == 32 || core->sample_bits == 64) && core->reserved == 0 &&
            core->create && core->destroy && core->prepare && core->reset && core->deactivate &&
            core->process && core->timing;
    }
};
} // namespace ravet::native
