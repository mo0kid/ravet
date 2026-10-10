#pragma once
#include "RavetNativeProcessor.h"
#include "RavetNativeScalars.hpp"
#include <AudioToolbox/AudioToolbox.h>
#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <limits>
#include <type_traits>

namespace ravet::au {
template <typename Sample> class NativeAuAudioBusMapFor;
// Caller-owned fixed storage for native planar pointer arrays. Copies retarget
// native views to their own storage. Keep the block alive and exclusively owned
// through backend use; do not remap it while any backend borrows its views.
template <typename Sample> class NativeAuAudioBlockFor final {
    static_assert(std::is_same<Sample,float>::value || std::is_same<Sample,double>::value, "Native AU samples must be float or double");
public:
    NativeAuAudioBlockFor() = default;
    NativeAuAudioBlockFor(const NativeAuAudioBlockFor &other) { *this = other; }
    NativeAuAudioBlockFor &operator=(const NativeAuAudioBlockFor &other) {
        if (this != &other) {
            inputs_ = other.inputs_; outputs_ = other.outputs_; layouts_ = other.layouts_;
            count_ = other.count_; frames_ = other.frames_; retarget();
        }
        return *this;
    }
    const RavetNativeProcessorBusV1 *buses() const { return views_.data(); }
    uint32_t busCount() const { return count_; }
    uint32_t frames() const { return frames_; }
private:
    friend class NativeAuAudioBusMapFor<Sample>;
    uint32_t count_ = 0, frames_ = 0;
    std::array<RavetNativeBusLayoutV1, 9> layouts_{};
    std::array<std::array<const Sample *, 64>, 9> inputs_{};
    std::array<std::array<Sample *, 64>, 9> outputs_{};
    std::array<RavetNativeProcessorBusV1, 9> views_{};
    void retarget() {
        views_ = {};
        for (uint32_t i = 0; i < count_; ++i) {
            views_[i] = {layouts_[i].inputs, layouts_[i].outputs,
                frames_ && layouts_[i].inputs ? inputs_[i].data() : nullptr,
                frames_ && layouts_[i].outputs ? outputs_[i].data() : nullptr};
        }
    }
};

// Optional Apple-SDK C++17 explicit f32/f64 noninterleaved profile.
// No sample conversion: the originating native table must match Sample. Configure on stopped
// CONTROL; failed configuration retains the prior profile. Main/aux native
// order maps separately to compact AU input/output element indices: zero-channel
// directions consume no AU element. Consumers explicitly configure/register
// their component to those counts; no routing or AU discovery is inferred.
// Foreign metadata/pointer arrays/sample ranges are live/aligned and cover their
// declared sizes, disjoint from writable mapped storage. Metadata/pointer arrays
// are also disjoint from writable samples; sample aliases follow the rules below.
// This validates shapes/addresses/capacities, not foreign memory ownership.
// Prepared map uses fixed scratch/sort, no heap, locks, CF allocation or I/O.
template <typename Sample> class NativeAuAudioBusMapFor final {
    static_assert(std::is_same<Sample,float>::value || std::is_same<Sample,double>::value, "Native AU samples must be float or double");
public:
    bool configure(const RavetNativeProcessorConfigV1 &config) {
        if (config.version != 1 || config.reserved || !config.bus_count || config.bus_count > 9 ||
            !config.maximum_frames || config.maximum_frames > 65536 || config.maximum_events > 1024 ||
            config.maximum_output_events > 1024 || !native::scalar::finite(config.sample_rate) || config.sample_rate <= 0.) return false;
        NativeAuAudioBusMapFor<Sample> next; next.count_ = config.bus_count; next.frames_ = config.maximum_frames;
        for (uint32_t i = 0; i < 9; ++i) {
            const auto layout = config.buses[i];
            if (i >= config.bus_count) { if (layout.inputs || layout.outputs) return false; continue; }
            if (layout.inputs > 64 || layout.outputs > 64 || (!layout.inputs && !layout.outputs)) return false;
            next.layouts_[i] = layout;
            next.inputs_[i] = layout.inputs ? int32_t(next.inputCount_++) : -1;
            next.outputs_[i] = layout.outputs ? int32_t(next.outputCount_++) : -1;
        }
        *this = next; return true;
    }
    bool valid() const { return count_ != 0; }
    uint32_t busCount() const { return count_; }
    uint32_t inputBusCount() const { return inputCount_; }
    uint32_t outputBusCount() const { return outputCount_; }
    uint32_t maximumFrames() const { return frames_; }
    // Exact directional element counts/list/channel shapes, including frame0.
    // Explicit frame0 omission requires both supplied direction counts zero;
    // then native declared channels remain, with null native sample arrays.
    // Positive frames require every planar channel and sufficient byte capacity.
    // A single interleaved buffer for multiple channels is rejected. Inputs may
    // alias outputs across all buses; all output active prefixes must be disjoint.
    // Whole preflight precedes pointer publication; failed block/sample outputs
    // remain unchanged. This does not run DSP or silence caller buffers.
    bool map(const AudioBufferList *const *inputs, uint32_t inputCount,
             AudioBufferList *const *outputs, uint32_t outputCount,
             uint32_t frames, NativeAuAudioBlockFor<Sample> &block) const {
        if (!valid() || frames > frames_) return false;
        const bool omitted = frames == 0 && inputCount == 0 && outputCount == 0;
        if (!omitted && (inputCount != inputCount_ || outputCount != outputCount_ ||
            (inputCount && !range(inputs, alignof(AudioBufferList *), uintptr_t(inputCount) * sizeof(AudioBufferList *))) ||
            (outputCount && !range(outputs, alignof(AudioBufferList *), uintptr_t(outputCount) * sizeof(AudioBufferList *))))) return false;
        NativeAuAudioBlockFor<Sample> next; next.count_ = count_; next.frames_ = frames; next.layouts_ = layouts_;
        std::array<Range, 576> ranges{}; size_t used = 0;
        for (uint32_t i = 0; i < count_; ++i) {
            if (omitted) continue;
            if (inputs_[i] >= 0) {
                const auto list = inputs[inputs_[i]];
                if (!listShape(list, layouts_[i].inputs)) return false;
                for (uint32_t c = 0; c < layouts_[i].inputs; ++c) {
                    const auto &buffer = list->mBuffers[c];
                    if (!samples(buffer, frames)) return false;
                    if (frames) next.inputs_[i][c] = static_cast<const Sample *>(buffer.mData);
                }
            }
            if (outputs_[i] >= 0) {
                const auto list = outputs[outputs_[i]];
                if (!listShape(list, layouts_[i].outputs)) return false;
                for (uint32_t c = 0; c < layouts_[i].outputs; ++c) {
                    const auto &buffer = list->mBuffers[c];
                    if (!samples(buffer, frames)) return false;
                    if (frames) {
                        next.outputs_[i][c] = static_cast<Sample *>(buffer.mData);
                        const auto begin = uintptr_t(buffer.mData);
                        ranges[used++] = {begin, begin + uintptr_t(frames) * sizeof(Sample)};
                    }
                }
            }
        }
        std::sort(ranges.begin(), ranges.begin() + used, [](const Range &a, const Range &b) { return a.begin < b.begin; });
        for (size_t i = 1; i < used; ++i) if (ranges[i].begin < ranges[i - 1].end) return false;
        block = next; return true;
    }
private:
    struct Range { uintptr_t begin, end; };
    static bool range(const void *pointer, size_t alignment, uintptr_t bytes) {
        const auto begin = uintptr_t(pointer);
        return begin && begin % alignment == 0 && begin <= std::numeric_limits<uintptr_t>::max() - bytes;
    }
    static bool listShape(const AudioBufferList *list, uint32_t channels) {
        const auto bytes = offsetof(AudioBufferList, mBuffers) + uintptr_t(channels) * sizeof(AudioBuffer);
        return range(list, alignof(AudioBufferList), bytes) && list->mNumberBuffers == channels;
    }
    static bool samples(const AudioBuffer &buffer, uint32_t frames) {
        if (buffer.mNumberChannels != 1) return false;
        if (!frames) return true;
        const auto bytes = uintptr_t(frames) * sizeof(Sample);
        return buffer.mDataByteSize >= bytes && range(buffer.mData, alignof(Sample), bytes);
    }
    uint32_t count_ = 0, frames_ = 0, inputCount_ = 0, outputCount_ = 0;
    std::array<RavetNativeBusLayoutV1, 9> layouts_{};
    std::array<int32_t, 9> inputs_{}, outputs_{};
};
using NativeAuAudioBlock = NativeAuAudioBlockFor<float>;
using NativeAuAudioBusMap = NativeAuAudioBusMapFor<float>;
using NativeAuAudioBlock64 = NativeAuAudioBlockFor<double>;
using NativeAuAudioBusMap64 = NativeAuAudioBusMapFor<double>;
} // namespace ravet::au
