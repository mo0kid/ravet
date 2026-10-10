#pragma once
#include "RavetNativeState.h"
#include "RavetCachedParameters.h"
#include "RavetLv2NoteInput.hpp"
#include "RavetLv2Transport.hpp"
#include "lv2/time/time.h"
#include "lv2/midi/midi.h"
#include "lv2/atom/atom.h"
#include "lv2/state/state.h"
#include "lv2/urid/urid.h"
#include <algorithm>
#include <array>
#include <cmath>
#include <cstring>
#include <limits>

namespace ravet::native {
// External-LV2-SDK, C++17, f32 main-bus/control-port profile. Every native table
// must originate from the same consumer and remain loaded. Instantiate/retire
// and restore require stopped exclusive ownership. Save may overlap run through
// the native publication companion, never by reading the DSP. No hidden thread,
// audio allocation, retry loop, UI, or threadSafeRestore declaration.
// Effect ports: inputs, outputs, then writable schema controls in declared order.
// Explicit Instrument=true: zero inputs, outputs, controls, then one MIDI Atom
// Sequence port, with mandatory cached urid:map. Shared control/note budget1024.
// Explicit Transport=true additionally collects exact block-start time:Position
// before state transfer/DSP; later validated positions are omitted, no timeline
// inference or history carry. Default MIDI-only and Gain profiles are unchanged.
// Host ranges are live/aligned and cover frame counts; controls and instance
// metadata stay disjoint from writable samples. Inputs may alias all outputs;
// outputs are mutually disjoint. Compile with ordinary IEEE finite semantics.
// Native consumer hooks retain their own atomicity and deadline obligations.
template <size_t Parameters, size_t MaximumChannels = 2, bool Instrument = false, bool Transport = false>
class NativeLv2Processor final {
    static_assert(Parameters > 0 && Parameters <= 1024);
    static_assert(MaximumChannels > 0 && MaximumChannels <= 64);
    static_assert(!Transport || Instrument, "Transport profile requires the existing instrument Atom input");
public:
    NativeLv2Processor(const RavetNativeProcessorEntryV1 *core,
                       const RavetNativeCachedParameterEntryV1 *cache,
                       const RavetNativeStateEntryV1 *state,
                       uint32_t channels, double rate, const char *stateKey,
                       const LV2_Feature *const *features)
        : core_(core), cache_(cache), state_(state), channels_(channels) {
        if (!core || !cache || !state || channels == 0 || channels > MaximumChannels ||
            !std::isfinite(rate) || rate <= 0. || !stateKey || !*stateKey ||
            core->version != 1 || core->byte_size != sizeof(*core) || core->sample_bits != 32 || core->reserved ||
            !core->create || !core->destroy || !core->prepare || !core->reset || !core->deactivate || !core->process || !core->timing ||
            cache->version != 1 || cache->byte_size != sizeof(*cache) || cache->sample_bits != 32 || cache->parameter_count != Parameters ||
            !cache->parameters_entry || !cache->create || !cache->destroy || !cache->info || !cache->normalize || !cache->denormalize ||
            state->version != 1 || state->byte_size != sizeof(*state) || state->sample_bits != 32 || state->parameter_count != Parameters ||
            !state->parameters_entry || !state->create || !state->destroy || !state->publish || !state->save_state || !state->apply || !state->reflect)
            return;
        schema_ = cache->parameters_entry();
        const auto *stateSchema = state->parameters_entry();
        if (!schema_ || !stateSchema || schema_->version != 1 || schema_->byte_size != sizeof(*schema_) ||
            schema_->sample_bits != 32 || schema_->parameter_count != Parameters || !schema_->processor_entry ||
            schema_->state_bytes == 0 || schema_->state_bytes > 8208 ||
            !matchingCore(schema_->processor_entry(), core) ||
            stateSchema->version != 1 || stateSchema->byte_size != sizeof(*stateSchema) || stateSchema->sample_bits != 32 ||
            stateSchema->parameter_count != Parameters || stateSchema->processor_entry == nullptr ||
            !matchingCore(stateSchema->processor_entry(), core) || stateSchema->state_bytes != schema_->state_bytes ||
            stateSchema->state_version != schema_->state_version || std::memcmp(stateSchema->state_product, schema_->state_product, 8))
            return;
        cacheHandle_ = cache->create();
        if (!cacheHandle_) return;
        for (uint32_t i = 0; i < Parameters; ++i) {
            if (cache->info(cacheHandle_, i, &info_[i]) || (info_[i].flags & RAVET_PARAMETER_READ_ONLY)) return;
        }
        publication_ = state->create(nullptr, 0);
        processor_ = core->create();
        if (!publication_ || !processor_) return;
        RavetNativeProcessorConfigV1 config{};
        config.version = 1; config.maximum_frames = maximumFrames;
        config.maximum_events = eventCapacity; config.bus_count = 1;
        config.sample_rate = rate; config.buses[0] = {inputChannels(), channels};
        if (core->prepare(processor_, &config)) return;
        if (features) {
            for (uint32_t i = 0; i < 256 && features[i]; ++i) {
                if (features[i]->URI && std::strcmp(features[i]->URI, LV2_URID__map) == 0) {
                    const auto *map = static_cast<const LV2_URID_Map *>(features[i]->data);
                    if (map && map->map) {
                        key_ = map->map(map->handle, stateKey); type_ = map->map(map->handle, LV2_ATOM__Chunk);
                        if constexpr (Instrument) {
                            sequenceType_ = map->map(map->handle, LV2_ATOM__Sequence);
                            midiType_ = map->map(map->handle, LV2_MIDI__MidiEvent);
                            frameTimeType_ = map->map(map->handle, LV2_ATOM__frameTime);
                            if constexpr (Transport) {
                                timeTypes_ = {sequenceType_, frameTimeType_, map->map(map->handle, LV2_ATOM__Object),
                                    map->map(map->handle, LV2_TIME__Position), map->map(map->handle, LV2_ATOM__Long),
                                    map->map(map->handle, LV2_ATOM__Double), map->map(map->handle, LV2_ATOM__Float),
                                    map->map(map->handle, LV2_ATOM__Int), map->map(map->handle, LV2_TIME__frame),
                                    map->map(map->handle, LV2_TIME__beat), map->map(map->handle, LV2_TIME__beatsPerMinute),
                                    map->map(map->handle, LV2_TIME__speed), map->map(map->handle, LV2_TIME__beatsPerBar),
                                    map->map(map->handle, LV2_TIME__beatUnit)};
                            }
                        }
                    }
                    break;
                }
            }
        }
        if constexpr (Instrument) { if (!sequenceType_ || !midiType_ || !frameTimeType_) return; }
        if constexpr (Transport) { if (!timeTypes_.valid()) return; }
        valid_ = true;
    }
    ~NativeLv2Processor() {
        if (processor_) { core_->deactivate(processor_); core_->destroy(processor_); }
        if (publication_) state_->destroy(publication_);
        if (cacheHandle_) cache_->destroy(cacheHandle_);
    }
    NativeLv2Processor(const NativeLv2Processor &) = delete;
    NativeLv2Processor &operator=(const NativeLv2Processor &) = delete;
    bool valid() const { return valid_; }
    void connect(uint32_t index, void *data) {
        if (!valid_) return;
        if constexpr (Instrument) { if (index == audioPorts() + Parameters) { notes_ = static_cast<const LV2_Atom_Sequence *>(data); return; } }
        if (index < audioPorts() + Parameters) ports_[index] = static_cast<float *>(data);
    }
    void activate() { active_ = valid_ && !failed_ && core_->reset(processor_) == 0; }
    void deactivate() { active_ = false; }
    void run(uint32_t frames) {
        if (!valid_) return;
        std::array<float, Parameters> controls{};
        if (!active_ || frames > maximumFrames || !validateAudio(frames) || !readControls(controls)) { silence(frames); return; }
        std::array<RavetNativeEventV1, eventCapacity> events{};
        uint32_t count = 0;
        for (uint32_t i = 0; i < Parameters; ++i) {
            if (!seen_[i] || controls[i] != lastControls_[i]) {
                events[count].kind = RAVET_NATIVE_PARAMETER_VALUE;
                events[count].parameter_id = info_[i].id;
                events[count++].value = controls[i];
            }
        }
        if constexpr (Instrument) {
            uint32_t noteCount = 0;
            if (!Lv2NoteInput::convert(notes_, sequenceType_, midiType_, frameTimeType_, frames,
                                      events.data() + count, eventCapacity - count, noteCount)) { silence(frames); return; }
            count += noteCount;
        }
        RavetNativeTransportV1 transport{};
        const RavetNativeTransportV1 *context = nullptr;
        if constexpr (Transport) {
            if (!Lv2TimePosition::collect(notes_, timeTypes_, frames, transport)) { silence(frames); return; }
            if (transport.present) context = &transport;
        }
        // Only complete preflight can consume a pending state restore. Consumer
        // backend failure after apply cannot generically roll back DSP history.
        RavetNativeStateApplyV1 applied{};
        const auto accepted = state_->apply(processor_, publication_, &applied);
        if (accepted && accepted != 3) { fail(accepted, frames); return; }
        std::array<const float *, MaximumChannels> inputs{};
        std::array<float *, MaximumChannels> outputs{};
        for (uint32_t c = 0; c < channels_; ++c) {
            if constexpr (!Instrument) inputs[c] = ports_[c];
            outputs[c] = ports_[inputChannels() + c];
        }
        const RavetNativeProcessorBusV1 bus{inputChannels(), channels_, Instrument ? nullptr : inputs.data(), outputs.data()};
        RavetNativeProcessorResultV1 result{};
        const auto processed = core_->process(processor_, &bus, 1, frames, events.data(), count, context, nullptr, 0, &result);
        if (processed || result.status > 1 || result.event_count) { fail(processed ? processed : 1, frames); return; }
        lastControls_ = controls; seen_.fill(true);
        RavetNativeStateRevisionV1 revision{};
        const auto reflected = state_->reflect(processor_, publication_, &revision);
        if (reflected && reflected != 3 && reflected != 4) fail(reflected, frames);
    }
    LV2_State_Status save(LV2_State_Store_Function store, LV2_State_Handle handle) const {
        if (!valid_ || !store) return LV2_STATE_ERR_UNKNOWN;
        if (!key_ || !type_) return LV2_STATE_ERR_NO_FEATURE;
        std::array<uint8_t, 8208> bytes{};
        RavetNativeStateRevisionV1 revision{};
        if (state_->save_state(publication_, bytes.data(), schema_->state_bytes, &revision)) return LV2_STATE_ERR_UNKNOWN;
        return store(handle, key_, bytes.data(), schema_->state_bytes, type_, LV2_STATE_IS_POD | LV2_STATE_IS_PORTABLE);
    }
    LV2_State_Status restore(LV2_State_Retrieve_Function retrieve, LV2_State_Handle handle) {
        if (!valid_ || !retrieve) return LV2_STATE_ERR_UNKNOWN;
        if (!key_ || !type_) return LV2_STATE_ERR_NO_FEATURE;
        size_t size = 0; uint32_t type = 0, flags = 0;
        const auto *bytes = static_cast<const uint8_t *>(retrieve(handle, key_, &size, &type, &flags));
        std::array<uint8_t, 8208> defaults{};
        if (!bytes) {
            // LV2 empty-map restore means schema defaults, never old state.
            void *temporary = state_->create(nullptr, 0);
            if (!temporary) return LV2_STATE_ERR_UNKNOWN;
            RavetNativeStateRevisionV1 revision{};
            const auto code = state_->save_state(temporary, defaults.data(), schema_->state_bytes, &revision);
            state_->destroy(temporary);
            if (code) return LV2_STATE_ERR_UNKNOWN;
            bytes = defaults.data(); size = schema_->state_bytes;
        } else {
            if (type != type_) return LV2_STATE_ERR_BAD_TYPE;
            if ((flags & (LV2_STATE_IS_POD | LV2_STATE_IS_PORTABLE)) != (LV2_STATE_IS_POD | LV2_STATE_IS_PORTABLE)) return LV2_STATE_ERR_BAD_FLAGS;
            if (size > 8208) return LV2_STATE_ERR_UNKNOWN;
        }
        RavetNativeStateRevisionV1 revision{};
        if (state_->publish(publication_, bytes, static_cast<uint32_t>(size), &revision)) return LV2_STATE_ERR_UNKNOWN;
        // Host ports are independently persisted by LV2. Hold the explicit
        // restored state until a connected canonical control changes; controls
        // the host changes during its own recall take precedence at next run.
        std::array<float, Parameters> baseline{};
        if (readControls(baseline)) { lastControls_ = baseline; seen_.fill(true); }
        else seen_.fill(false);
        return LV2_STATE_SUCCESS;
    }
    static constexpr uint32_t maximumFrames = 65536;
private:
    const RavetNativeProcessorEntryV1 *core_;
    const RavetNativeCachedParameterEntryV1 *cache_;
    const RavetNativeStateEntryV1 *state_;
    const RavetNativeParameterEntryV1 *schema_ = nullptr;
    void *processor_ = nullptr, *cacheHandle_ = nullptr, *publication_ = nullptr;
    uint32_t channels_, key_ = 0, type_ = 0, sequenceType_ = 0, midiType_ = 0, frameTimeType_ = 0;
    const LV2_Atom_Sequence *notes_ = nullptr;
    Lv2TimePositionTypes timeTypes_{};
    static constexpr uint32_t eventCapacity = Instrument ? 1024 : Parameters;
    uint32_t inputChannels() const { return Instrument ? 0 : channels_; }
    uint32_t audioPorts() const { return inputChannels() + channels_; }
    bool valid_ = false, active_ = false, failed_ = false;
    std::array<float *, 2 * MaximumChannels + Parameters> ports_{};
    std::array<RavetNativeParameterInfoV1, Parameters> info_{};
    std::array<float, Parameters> lastControls_{};
    std::array<bool, Parameters> seen_{};
    static bool matchingCore(const RavetNativeProcessorEntryV1 *a, const RavetNativeProcessorEntryV1 *b) {
        return a && a->version == 1 && a->byte_size == sizeof(*a) && a->sample_bits == b->sample_bits && a->reserved == 0 &&
            a->create == b->create && a->destroy == b->destroy && a->prepare == b->prepare && a->reset == b->reset &&
            a->deactivate == b->deactivate && a->process == b->process && a->timing == b->timing;
    }
    static bool range(const float *pointer, uint32_t frames, uintptr_t &begin, uintptr_t &end) {
        begin = reinterpret_cast<uintptr_t>(pointer);
        const auto bytes = static_cast<uint64_t>(frames) * sizeof(float);
        if (!pointer || begin % alignof(float) || bytes > std::numeric_limits<uintptr_t>::max() - begin) return false;
        end = begin + static_cast<uintptr_t>(bytes); return true;
    }
    bool validateAudio(uint32_t frames) const {
        if (!frames) return true;
        for (uint32_t c = 0; c < channels_; ++c) {
            uintptr_t begin, end;
            if constexpr (!Instrument) { if (!range(ports_[c], frames, begin, end)) return false; }
            if (!range(ports_[inputChannels() + c], frames, begin, end)) return false;
            for (uint32_t earlier = 0; earlier < c; ++earlier) {
                uintptr_t otherBegin, otherEnd;
                if (!range(ports_[inputChannels() + earlier], frames, otherBegin, otherEnd) || (begin < otherEnd && otherBegin < end)) return false;
            }
        }
        return true;
    }
    bool readControls(std::array<float, Parameters> &values) const {
        for (uint32_t i = 0; i < Parameters; ++i) {
            const auto *port = ports_[audioPorts() + i];
            uintptr_t begin, end;
            float normalized;
            if (!range(port, 1, begin, end) || cache_->normalize(cacheHandle_, info_[i].id, *port, &normalized) ||
                cache_->denormalize(cacheHandle_, info_[i].id, normalized, &values[i])) return false;
        }
        return true;
    }
    void silence(uint32_t frames) {
        const auto count = std::min(frames, maximumFrames);
        for (uint32_t c = 0; c < channels_; ++c) {
            uintptr_t begin, end;
            if (count && range(ports_[inputChannels() + c], count, begin, end)) std::fill_n(ports_[inputChannels() + c], count, 0.f);
        }
    }
    void fail(int32_t code, uint32_t frames) { if (code == 2) { failed_ = true; active_ = false; } silence(frames); }
};
} // namespace ravet::native
