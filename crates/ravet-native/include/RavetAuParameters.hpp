#pragma once
#include "RavetCachedParameters.h"
#include <AudioToolbox/AudioToolbox.h>
#include <CoreFoundation/CoreFoundation.h>
#include <algorithm>
#include <array>
#include <charconv>
#include <cmath>
#include <cstring>
#include <limits>

namespace ravet::au {
// Optional Apple-SDK consumer, C++17. CONTROL constructs/destroys and owns
// metadata/text calls; those calls may allocate CF objects. Keep the originating
// native tables/library alive until this helper and all borrowers retire.
// Cached scalar preflight may run during serialized prepared callbacks without
// consumer-provider calls, allocation, DSP access or SDK queue/storage mutation.
// AU global values are explicitly normalized f32; text uses physical f32 values.
// expectedBits defaults32; explicit32/64 requires matching native cache/parameter
// provenance, independently of AU scalar width. This does not select SDK audio.
// Foreign outputs/CF objects must be live, aligned and disjoint from the helper.
// No AU registration, groups, schedules, notifications or bypass routing implied.
template <size_t MaximumParameters = 1024>
class NativeAuParameters final {
    static_assert(MaximumParameters > 0 && MaximumParameters <= 1024);
public:
    explicit NativeAuParameters(const RavetNativeCachedParameterEntryV1 *cache, uint32_t expectedBits = 32) : cache_(cache) {
        if ((expectedBits != 32 && expectedBits != 64) || !cache || cache->version != 1 || cache->byte_size != sizeof(*cache) || cache->sample_bits != expectedBits ||
            !cache->parameter_count || cache->parameter_count > MaximumParameters || !cache->parameters_entry ||
            !cache->create || !cache->destroy || !cache->info || !cache->normalize || !cache->denormalize) return;
        parameters_ = cache->parameters_entry();
        if (!parameters_ || parameters_->version != 1 || parameters_->byte_size != sizeof(*parameters_) ||
            parameters_->sample_bits != expectedBits || parameters_->parameter_count != cache->parameter_count) return;
        handle_ = cache->create();
        if (!handle_) return;
        for (uint32_t i = 0; i < cache->parameter_count; ++i) {
            auto &info = infos_[i];
            if (cache->info(handle_, i, &info) || info.reserved || (info.flags & ~7u) || info.scale > 1 || info.intervals > 65535 ||
                !std::isfinite(info.minimum) || !std::isfinite(info.maximum) || !std::isfinite(info.default_value) ||
                info.minimum >= info.maximum || info.default_value < info.minimum || info.default_value > info.maximum ||
                (info.scale == 1 && info.minimum <= 0.f) ||
                ((info.flags & RAVET_PARAMETER_BYPASS) && (info.minimum != 0.f || info.maximum != 1.f || info.intervals != 1))) return;
            for (uint32_t j = 0; j < i; ++j) if (infos_[j].id == info.id) return;
            if (!validText(info.name, info.name_length) || !info.name_length || !validText(info.unit, info.unit_length)) return;
            names_[i] = CFStringCreateWithBytes(nullptr, info.name, info.name_length, kCFStringEncodingUTF8, false);
            units_[i] = CFStringCreateWithBytes(nullptr, info.unit, info.unit_length, kCFStringEncodingUTF8, false);
            if (!names_[i] || !units_[i] || cache->normalize(handle_, info.id, info.default_value, &defaults_[i]) ||
                !normalized(defaults_[i])) return;
        }
        valid_ = true;
    }
    ~NativeAuParameters() {
        for (auto name : names_) if (name) CFRelease(name);
        for (auto unit : units_) if (unit) CFRelease(unit);
        if (handle_) cache_->destroy(handle_);
    }
    NativeAuParameters(const NativeAuParameters &) = delete;
    NativeAuParameters &operator=(const NativeAuParameters &) = delete;
    bool valid() const { return valid_; }
    uint32_t count() const { return valid_ ? cache_->parameter_count : 0; }
    const RavetNativeParameterEntryV1 *parametersEntry() const { return valid_ ? parameters_ : nullptr; }
    bool contains(AudioUnitParameterID id) const { return index(id) < count(); }
    // Null IDs with capacity zero query count. Failed capacity/helper preflight
    // preserves both count and the entire caller array; successful tails survive.
    bool list(AudioUnitParameterID *ids, uint32_t capacity, uint32_t &countOutput) const {
        if (!valid_ || (!ids && capacity) || (ids && capacity < count())) return false;
        if (ids) for (uint32_t i = 0; i < count(); ++i) ids[i] = infos_[i].id;
        countOutput = count(); return true;
    }
    // Returned CF name/unit references belong to the caller. CFNameRelease is
    // set; release unitName when present. Names are complete CF UTF-8 conversions,
    // not clipped to legacy name[] or VST3 String128 fields.
    bool info(AudioUnitParameterID id, AudioUnitParameterInfo &output) const {
        const auto i = index(id); if (i == count()) return false;
        AudioUnitParameterInfo next{};
        next.cfNameString = static_cast<CFStringRef>(CFRetain(names_[i]));
        next.minValue = 0.f; next.maxValue = 1.f; next.defaultValue = defaults_[i];
        next.flags = kAudioUnitParameterFlag_HasCFNameString | kAudioUnitParameterFlag_CFNameRelease | kAudioUnitParameterFlag_IsReadable;
        if (!(infos_[i].flags & RAVET_PARAMETER_READ_ONLY)) next.flags |= kAudioUnitParameterFlag_IsWritable;
        if (!(infos_[i].flags & RAVET_PARAMETER_AUTOMATABLE)) next.flags |= kAudioUnitParameterFlag_NonRealTime;
        next.unit = kAudioUnitParameterUnit_Generic;
        if (infos_[i].flags & RAVET_PARAMETER_BYPASS) next.unit = kAudioUnitParameterUnit_Boolean;
        else if (infos_[i].unit_length) {
            next.unit = kAudioUnitParameterUnit_CustomUnit;
            next.unitName = static_cast<CFStringRef>(CFRetain(units_[i]));
        }
        output = next; return true;
    }
    // Setter policy: declared writable ID, finite normalized 0..1 and offset
    // <65536. Active-frame timing/capacity and eventual SDK mutation remain the
    // consumer's responsibility. Native range/log/step/ties-up mapping supplies
    // canonical normalized intent; no caller output changes on rejection.
    bool control(AudioUnitParameterID id, AudioUnitParameterValue value, uint32_t offset, AudioUnitParameterValue &output) const {
        const auto i = index(id);
        if (i == count() || (infos_[i].flags & RAVET_PARAMETER_READ_ONLY) || offset >= 65536 || !normalized(value)) return false;
        float physical = 0.f, canonical = 0.f;
        if (!physicalValue(i, value, physical) || cache_->normalize(handle_, id, physical, &canonical) || !normalized(canonical)) return false;
        output = canonical; return true;
    }
    bool format(AudioUnitParameterID id, AudioUnitParameterValue value, CFStringRef &output) const {
        const auto i = index(id); float physical = 0.f;
        if (i == count() || !physicalValue(i, value, physical)) return false;
        CFStringRef next = nullptr;
        if (infos_[i].flags & RAVET_PARAMETER_BYPASS) next = static_cast<CFStringRef>(CFRetain(physical >= .5f ? CFSTR("On") : CFSTR("Off")));
        else {
            std::array<char, 64> text{};
            const auto result = std::to_chars(text.data(), text.data() + text.size() - 1, physical, std::chars_format::general);
            if (result.ec != std::errc{}) return false;
            next = CFStringCreateWithBytes(nullptr, reinterpret_cast<const UInt8 *>(text.data()), result.ptr - text.data(), kCFStringEncodingASCII, false);
        }
        if (!next) return false;
        output = next; return true;
    }
    // Complete <=128 ASCII code-unit numeric input; surrounding ASCII whitespace
    // and one leading plus are accepted. Reject embedded NUL, trailing junk,
    // nonfinite and physical out-of-range values; native mapping snaps steps.
    // No enum label catalogue, locale parsing or implicit range clamping.
    bool parse(AudioUnitParameterID id, CFStringRef text, AudioUnitParameterValue &output) const {
        const auto i = index(id);
        if (i == count() || !text || CFGetTypeID(text) != CFStringGetTypeID()) return false;
        const auto length = CFStringGetLength(text);
        std::array<char, 129> bytes{};
        if (length <= 0 || length > 128 || !CFStringGetCString(text, bytes.data(), bytes.size(), kCFStringEncodingASCII) ||
            std::strlen(bytes.data()) != size_t(length)) return false;
        size_t first = 0, last = size_t(length);
        const auto space = [](char c) { return c == ' ' || c == '\t' || c == '\r' || c == '\n'; };
        while (first < last && space(bytes[first])) ++first;
        while (last > first && space(bytes[last - 1])) --last;
        float physical = 0.f;
        if ((infos_[i].flags & RAVET_PARAMETER_BYPASS) && last - first == 2 && !std::memcmp(bytes.data() + first, "On", 2)) physical = 1.f;
        else if ((infos_[i].flags & RAVET_PARAMETER_BYPASS) && last - first == 3 && !std::memcmp(bytes.data() + first, "Off", 3)) physical = 0.f;
        else {
            if (first < last && bytes[first] == '+') {
                ++first;
                if (first < last && (bytes[first] == '+' || bytes[first] == '-')) return false;
            }
            if (first == last) return false;
            const auto result = std::from_chars(bytes.data() + first, bytes.data() + last, physical, std::chars_format::general);
            if (result.ec != std::errc{} || result.ptr != bytes.data() + last) return false;
        }
        if (!std::isfinite(physical) || physical < infos_[i].minimum || physical > infos_[i].maximum) return false;
        float next = 0.f;
        if (cache_->normalize(handle_, id, physical, &next) || !normalized(next)) return false;
        output = next; return true;
    }
private:
    const RavetNativeCachedParameterEntryV1 *cache_ = nullptr;
    const RavetNativeParameterEntryV1 *parameters_ = nullptr;
    void *handle_ = nullptr;
    bool valid_ = false;
    std::array<RavetNativeParameterInfoV1, MaximumParameters> infos_{};
    std::array<CFStringRef, MaximumParameters> names_{}, units_{};
    std::array<float, MaximumParameters> defaults_{};
    static bool normalized(float value) { return std::isfinite(value) && value >= 0.f && value <= 1.f; }
    template <size_t N> static bool validText(const uint8_t (&bytes)[N], uint32_t length) {
        return length < N && !std::memchr(bytes, 0, length) &&
            std::all_of(bytes + length, bytes + N, [](uint8_t value) { return value == 0; });
    }
    uint32_t index(AudioUnitParameterID id) const {
        for (uint32_t i = 0; i < count(); ++i) if (infos_[i].id == id) return i;
        return count();
    }
    bool physicalValue(uint32_t i, float value, float &physical) const {
        return normalized(value) && !cache_->denormalize(handle_, infos_[i].id, value, &physical) &&
            std::isfinite(physical) && physical >= infos_[i].minimum && physical <= infos_[i].maximum;
    }
};
} // namespace ravet::au
