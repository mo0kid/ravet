#pragma once
#include "RavetAuAudio.hpp"

namespace ravet::au {
// Complete stopped CONTROL profile. No ASBD inference or precision conversion.
// Native table/handle identity and loaded-library lifetime remain consumer duties.
// Foreign input tables/config/formats are live, aligned, complete and disjoint
// from this owner; shape/address checks cannot authenticate foreign memory.
// Failed preflight retains every accepted format/config/map field. SDK element
// mutations, allocation/property notifications and their rollback are external:
// assemble a candidate, apply consumer changes, then publish only on success.
// AUBase may normalize scope and discard original ASBD reserved/padding bytes;
// this direct full-structure validator does not promise original facade checks.
template <typename Sample> class NativeAuAudioFormatProfile final {
public:
    using Map = NativeAuAudioBusMapFor<Sample>;
    using Block = NativeAuAudioBlockFor<Sample>;
    static constexpr uint32_t sampleBits = uint32_t(sizeof(Sample) * 8);
    static bool format(const AudioStreamBasicDescription &value, uint32_t channels, double rate) {
        const auto flags = UInt32(kAudioFormatFlagsNativeFloatPacked) | UInt32(kAudioFormatFlagIsNonInterleaved);
        return channels >= 1 && channels <= 64 && native::scalar::finite(rate) && rate > 0. &&
            native::scalar::finite(value.mSampleRate) && value.mSampleRate == rate &&
            value.mFormatID == kAudioFormatLinearPCM &&
            (value.mFormatFlags == flags || (channels == 1 && value.mFormatFlags == kAudioFormatFlagsNativeFloatPacked)) &&
            value.mBytesPerPacket == sizeof(Sample) && value.mFramesPerPacket == 1 &&
            value.mBytesPerFrame == sizeof(Sample) && value.mChannelsPerFrame == channels &&
            value.mBitsPerChannel == sampleBits && value.mReserved == 0;
    }
    bool configure(const RavetNativeProcessorEntryV1 *entry, const RavetNativeProcessorConfigV1 &config,
                   const AudioStreamBasicDescription *inputs, uint32_t inputCount,
                   const AudioStreamBasicDescription *outputs, uint32_t outputCount) {
        if (!range(entry,alignof(RavetNativeProcessorEntryV1),sizeof(*entry)) || entry->version != 1 ||
            entry->byte_size != sizeof(*entry) || entry->sample_bits != sampleBits || entry->reserved ||
            !entry->create || !entry->destroy || !entry->prepare || !entry->reset || !entry->deactivate ||
            !entry->process || !entry->timing) return false;
        NativeAuAudioFormatProfile next;
        if (!next.map_.configure(config) || inputCount != next.map_.inputBusCount() ||
            outputCount != next.map_.outputBusCount() ||
            (inputCount && !range(inputs,alignof(AudioStreamBasicDescription),sizeof(*inputs) * uintptr_t(inputCount))) ||
            (outputCount && !range(outputs,alignof(AudioStreamBasicDescription),sizeof(*outputs) * uintptr_t(outputCount)))) return false;
        uint32_t in = 0, out = 0;
        for (uint32_t i = 0; i < config.bus_count; ++i) {
            if (config.buses[i].inputs) {
                if (!format(inputs[in],config.buses[i].inputs,config.sample_rate)) return false;
                next.inputs_[in] = inputs[in]; ++in;
            }
            if (config.buses[i].outputs) {
                if (!format(outputs[out],config.buses[i].outputs,config.sample_rate)) return false;
                next.outputs_[out] = outputs[out]; ++out;
            }
        }
        next.entry_ = entry; next.config_ = config; *this = next; return true;
    }
    bool valid() const { return map_.valid(); }
    const RavetNativeProcessorEntryV1 *entry() const { return entry_; }
    const RavetNativeProcessorConfigV1 &config() const { return config_; }
    const AudioStreamBasicDescription *inputFormats() const { return inputs_.data(); }
    const AudioStreamBasicDescription *outputFormats() const { return outputs_.data(); }
    const Map &mapper() const { return map_; }
    bool map(const AudioBufferList *const *inputs, uint32_t inputCount,
             AudioBufferList *const *outputs, uint32_t outputCount, uint32_t frames, Block &block) const {
        return map_.map(inputs,inputCount,outputs,outputCount,frames,block);
    }
private:
    static bool range(const void *pointer, size_t alignment, uintptr_t bytes) {
        const auto begin = uintptr_t(pointer);
        return begin && begin % alignment == 0 && begin <= std::numeric_limits<uintptr_t>::max() - bytes;
    }
    const RavetNativeProcessorEntryV1 *entry_ = nullptr;
    RavetNativeProcessorConfigV1 config_{};
    Map map_;
    std::array<AudioStreamBasicDescription,9> inputs_{},outputs_{};
};
using NativeAuAudioFormat = NativeAuAudioFormatProfile<float>;
using NativeAuAudioFormat64 = NativeAuAudioFormatProfile<double>;
} // namespace ravet::au
