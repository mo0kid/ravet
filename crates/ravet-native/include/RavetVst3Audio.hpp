#pragma once
// Optional external-SDK C++17 fixed native planar bus profile.
#include "RavetNativeProcessor.h"
#include "RavetNativeScalars.hpp"
#include "pluginterfaces/vst/ivstaudioprocessor.h"
#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <limits>
namespace ravet::vst3 {
// Logical native main/auxiliary order maps independently to compact SDK input
// and output bus indices. Zero-channel directions occupy no SDK bus. Configure
// on stopped control; failed configuration retains the previous valid profile.
// This validates metadata/pointer shape and complete sample ranges, not foreign
// memory ownership. Callers supply live aligned SDK metadata, pointer arrays and
// samples without writable overlap with metadata/events/private owner storage.
// Normal mapping uses bounded stack scratch and std::sort, no heap/locks/I/O.
// It leaves SDK samples/silence flags and failed native destinations unchanged.
class NativeAudioBusMap final {
public:
    bool configure(const RavetNativeProcessorConfigV1 &config, uint32_t sampleBits) {
        if (config.version != 1 || config.reserved || config.bus_count < 1 || config.bus_count > 9 ||
            !config.maximum_frames || config.maximum_frames > 65536 || config.maximum_events > 1024 ||
            config.maximum_output_events > 1024 || !native::scalar::finite(config.sample_rate) || config.sample_rate <= 0. ||
            (sampleBits != 32 && sampleBits != 64)) return false;
        NativeAudioBusMap candidate; candidate.count_ = config.bus_count; candidate.frames_ = config.maximum_frames; candidate.bits_ = sampleBits;
        for (uint32_t index = 0; index < 9; ++index) {
            const auto layout = config.buses[index];
            if (index >= config.bus_count) { if (layout.inputs || layout.outputs) return false; continue; }
            if (layout.inputs > 64 || layout.outputs > 64 || (!layout.inputs && !layout.outputs)) return false;
            candidate.layouts_[index] = layout;
            candidate.inputs_[index] = layout.inputs ? candidate.inputCount_++ : -1;
            candidate.outputs_[index] = layout.outputs ? candidate.outputCount_++ : -1;
        }
        *this = candidate; return true;
    }
    bool valid() const { return count_ != 0; }
    uint32_t busCount() const { return count_; }
    uint32_t sampleBits() const { return bits_; }
    Steinberg::int32 inputBusCount() const { return inputCount_; }
    Steinberg::int32 outputBusCount() const { return outputCount_; }
    // Fault cleanup touches only exact prepared output shapes and supported
    // frame ranges and either known SDK precision (even a process mismatch).
    // Clear each valid aligned sample prefix; skip
    // null/misaligned/overflowing pointers without dereferencing samples.
    // Output overlaps are harmless zero writes. Set only actually cleared
    // channel bits; untouched/unsupported metadata and backing tails survive.
    // Live foreign metadata/pointer-array/sample ownership remains caller duty.
    void silenceSupportedOutputs(Steinberg::Vst::ProcessData &data) const {
        using namespace Steinberg::Vst;
        if (!valid() || data.numSamples <= 0 || uint32_t(data.numSamples) > frames_ ||
            (data.symbolicSampleSize != kSample32 && data.symbolicSampleSize != kSample64) ||
            data.numOutputs != outputCount_ || !aligned(data.outputs, alignof(AudioBusBuffers))) return;
        for (uint32_t index = 0; index < count_; ++index) {
            if (outputs_[index] < 0) continue;
            auto &bus = data.outputs[outputs_[index]];
            if (bus.numChannels != Steinberg::int32(layouts_[index].outputs)) continue;
            bus.silenceFlags = data.symbolicSampleSize == kSample32 ? silence<float>(bus.channelBuffers32, layouts_[index].outputs, uint32_t(data.numSamples)) : silence<double>(bus.channelBuffers64, layouts_[index].outputs, uint32_t(data.numSamples));
        }
    }
    // Omitted zero-frame audio is accepted only when both SDK direction counts
    // are zero. Otherwise exact prepared directional counts/channel shapes are
    // required even at zero frames; sample pointer arrays are not read then.
    // Positive frames require every channel pointer, including flagged silence;
    // null-silent inputs are not synthesized and flags never override samples.
    // Inputs may alias outputs across any bus. All output sample prefixes must
    // be disjoint; pointer alignment/address overflow preflight precedes success.
    bool map(const Steinberg::Vst::ProcessData &data,
             std::array<RavetNativeProcessorBusV1, 9> &output) const {
        using namespace Steinberg::Vst;
        if (!valid() || data.numSamples < 0 || uint32_t(data.numSamples) > frames_ ||
            data.symbolicSampleSize != (bits_ == 32 ? kSample32 : kSample64) || data.numInputs < 0 || data.numOutputs < 0) return false;
        const bool omitted = data.numSamples == 0 && data.numInputs == 0 && data.numOutputs == 0;
        if (!omitted && (data.numInputs != inputCount_ || data.numOutputs != outputCount_ ||
            (inputCount_ && !aligned(data.inputs, alignof(AudioBusBuffers))) ||
            (outputCount_ && !aligned(data.outputs, alignof(AudioBusBuffers))))) return false;
        std::array<RavetNativeProcessorBusV1, 9> candidate{};
        std::array<Range, 576> ranges; size_t used = 0;
        for (uint32_t index = 0; index < count_; ++index) {
            auto &bus = candidate[index]; const auto layout = layouts_[index];
            bus.input_channels = layout.inputs; bus.output_channels = layout.outputs;
            if (omitted) continue;
            if (inputs_[index] >= 0) {
                const auto &input = data.inputs[inputs_[index]];
                if (input.numChannels != Steinberg::int32(layout.inputs)) return false;
                if (data.numSamples > 0) {
                    bus.inputs = bits_ == 32 ? static_cast<const void *>(input.channelBuffers32) : static_cast<const void *>(input.channelBuffers64);
                    if (bits_ == 32 ? !channels<float>(input.channelBuffers32, layout.inputs, uint32_t(data.numSamples), nullptr, used) : !channels<double>(input.channelBuffers64, layout.inputs, uint32_t(data.numSamples), nullptr, used)) return false;
                }
            }
            if (outputs_[index] >= 0) {
                const auto &out = data.outputs[outputs_[index]];
                if (out.numChannels != Steinberg::int32(layout.outputs)) return false;
                if (data.numSamples > 0) {
                    bus.outputs = bits_ == 32 ? static_cast<const void *>(out.channelBuffers32) : static_cast<const void *>(out.channelBuffers64);
                    if (bits_ == 32 ? !channels<float>(out.channelBuffers32, layout.outputs, uint32_t(data.numSamples), ranges.data(), used) : !channels<double>(out.channelBuffers64, layout.outputs, uint32_t(data.numSamples), ranges.data(), used)) return false;
                }
            }
        }
        std::sort(ranges.begin(), ranges.begin() + used, [](const Range &a, const Range &b) { return a.begin < b.begin; });
        for (size_t index = 1; index < used; ++index) if (ranges[index].begin < ranges[index - 1].end) return false;
        output = candidate; return true;
    }
private:
    struct Range { uintptr_t begin, end; };
    static bool aligned(const void *pointer, size_t alignment) { return pointer && uintptr_t(pointer) % alignment == 0; }
    template <class S> static uint64_t silence(S *const *pointers, uint32_t count, uint32_t frames) {
        if (!aligned(pointers, alignof(S *))) return 0;
        uint64_t flags = 0; const auto bytes = uintptr_t(frames) * sizeof(S);
        for (uint32_t channel = 0; channel < count; ++channel) {
            auto *pointer = pointers[channel];
            if (!aligned(pointer, alignof(S)) || uintptr_t(pointer) > std::numeric_limits<uintptr_t>::max() - bytes) continue;
            std::fill_n(pointer, frames, S(0)); flags |= UINT64_C(1) << channel;
        }
        return flags;
    }
    template <class S> static bool channels(S *const *pointers, uint32_t count, uint32_t frames, Range *ranges, size_t &used) {
        if (!aligned(pointers, alignof(S *))) return false;
        const auto bytes = uintptr_t(frames) * sizeof(S);
        for (uint32_t channel = 0; channel < count; ++channel) {
            const auto *pointer = pointers[channel]; if (!aligned(pointer, alignof(S))) return false;
            const auto begin = uintptr_t(pointer); if (begin > std::numeric_limits<uintptr_t>::max() - bytes) return false;
            if (ranges) ranges[used++] = {begin, begin + bytes};
        }
        return true;
    }
    uint32_t count_ = 0, frames_ = 0, bits_ = 0;
    Steinberg::int32 inputCount_ = 0, outputCount_ = 0;
    std::array<RavetNativeBusLayoutV1, 9> layouts_{};
    std::array<Steinberg::int32, 9> inputs_{}, outputs_{};
};
} // namespace ravet::vst3
