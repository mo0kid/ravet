#pragma once
#include "RavetCachedParameters.h"
#include "RavetNativeScalars.hpp"
#include "AAX_CParameter.h"
#include "AAX_CParameterManager.h"
#include "AAX_CLinearTaperDelegate.h"
#include "AAX_IDisplayDelegate.h"
#include <algorithm>
#include <array>
#include <charconv>
#include <cstring>
#include <memory>

namespace ravet::aax {
// Optional external-AAX C++17 consumer. Serialized CONTROL owns construction,
// metadata/text, real SDK object staging/registration and retirement. Scalar
// preflight uses the immutable native cache without DSP or SDK queue mutation.
// Native values remain f32, independently of AAX normalized double storage.
// Retain exact originating tables/library through every cloned display borrower.
// No host scheduling, native audio/state, groups, signing or deadline implied.
template <size_t MaximumParameters = 1024>
class NativeAaxParameters final {
    static_assert(MaximumParameters > 0 && MaximumParameters <= 1024);
    struct State {
        const RavetNativeCachedParameterEntryV1 *entry = nullptr;
        const RavetNativeParameterEntryV1 *parameters = nullptr;
        void *handle = nullptr;
        bool valid = false;
        uint32_t count = 0, bypass = UINT32_MAX;
        std::array<RavetNativeParameterInfoV1, MaximumParameters> infos{};
        std::array<std::array<char, 10>, MaximumParameters> ids{};
        std::array<float, MaximumParameters> defaults{};
        ~State() { if (handle) entry->destroy(handle); }
    };
    std::shared_ptr<State> state_;
    static bool normalized(const double &v) { return native::scalar::finite(v) && v >= 0. && v <= 1.; }
    static bool normalized(const float &v) { return native::scalar::finite(v) && v >= 0.f && v <= 1.f; }
    // Complete UTF8, no embedded NUL, and canonical zero-padded native strings.
    template <size_t N> static bool text(const uint8_t (&bytes)[N], uint32_t length) {
        if (length >= N || std::memchr(bytes, 0, length) ||
            !std::all_of(bytes + length, bytes + N, [](uint8_t b) { return b == 0; })) return false;
        for (uint32_t i = 0; i < length;) {
            uint32_t value = bytes[i++], extra = 0, minimum = 0;
            if (value < 0x80) continue;
            if (value >= 0xc2 && value <= 0xdf) { value &= 31; extra = 1; minimum = 0x80; }
            else if (value >= 0xe0 && value <= 0xef) { value &= 15; extra = 2; minimum = 0x800; }
            else if (value >= 0xf0 && value <= 0xf4) { value &= 7; extra = 3; minimum = 0x10000; }
            else return false;
            if (extra > length - i) return false;
            while (extra--) { const auto b = bytes[i++]; if ((b & 0xc0) != 0x80) return false; value = (value << 6) | (b & 63); }
            if (value < minimum || value > 0x10ffff || (value >= 0xd800 && value <= 0xdfff)) return false;
        }
        return true;
    }
    static bool physical(const State &s, uint32_t i, double value, float &output) {
        if (!normalized(value)) return false;
        const float narrowed = static_cast<float>(value);
        float next = 0.f;
        if (!normalized(narrowed) || s.entry->denormalize(s.handle, s.infos[i].id, narrowed, &next) ||
            !native::scalar::finite(next) || next < s.infos[i].minimum || next > s.infos[i].maximum) return false;
        output = next; return true;
    }
    static bool format(const State &s, uint32_t i, double value, int32_t maximum, AAX_IString &output) {
        float amount = 0.f;
        if (maximum <= 0 || !physical(s, i, value, amount)) return false;
        std::array<char, 64> bytes{};
        size_t length = 0;
        if (s.infos[i].flags & RAVET_PARAMETER_BYPASS) {
            const char *word = amount >= .5f ? "On" : "Off";
            length = std::strlen(word); std::memcpy(bytes.data(), word, length);
        } else {
            const auto result = std::to_chars(bytes.data(), bytes.data() + bytes.size() - 1, amount, std::chars_format::general);
            if (result.ec != std::errc{}) return false;
            length = size_t(result.ptr - bytes.data());
        }
        if (length > uint32_t(maximum) || length > output.MaxLength()) return false;
        output.Set(bytes.data()); return true;
    }
    static bool parse(const State &s, uint32_t i, const AAX_IString &input, double &output) {
        const auto length = input.Length(); const char *bytes = input.Get();
        if (!length || length > 128 || !bytes || std::memchr(bytes, 0, length) || bytes[length] != 0 ||
            std::any_of(bytes, bytes + length, [](unsigned char b) { return b > 127; })) return false;
        size_t first = 0, last = length;
        const auto space = [](char b) { return b == ' ' || b == '\t' || b == '\n' || b == '\r'; };
        while (first < last && space(bytes[first])) ++first;
        while (first < last && space(bytes[last - 1])) --last;
        float amount = 0.f;
        if ((s.infos[i].flags & RAVET_PARAMETER_BYPASS) && last - first == 2 && !std::memcmp(bytes + first, "On", 2)) amount = 1.f;
        else if ((s.infos[i].flags & RAVET_PARAMETER_BYPASS) && last - first == 3 && !std::memcmp(bytes + first, "Off", 3)) amount = 0.f;
        else {
            if (first < last && bytes[first] == '+') { ++first; if (first < last && (bytes[first] == '+' || bytes[first] == '-')) return false; }
            if (first == last) return false;
            const auto parsed = std::from_chars(bytes + first, bytes + last, amount, std::chars_format::general);
            if (parsed.ec != std::errc{} || parsed.ptr != bytes + last) return false;
        }
        float next = 0.f;
        if (!native::scalar::finite(amount) || amount < s.infos[i].minimum || amount > s.infos[i].maximum ||
            s.entry->normalize(s.handle, s.infos[i].id, amount, &next) || !normalized(next)) return false;
        output = double(next); return true;
    }
    class Display final : public AAX_IDisplayDelegate<double> {
        std::shared_ptr<const State> state_; uint32_t index_;
    public:
        Display(std::shared_ptr<const State> state, uint32_t i) : state_(std::move(state)), index_(i) {}
        Display *Clone() const override { return new Display(*this); }
        bool ValueToString(double value, AAX_CString *output) const override { return output && format(*state_, index_, value, 63, *output); }
        bool ValueToString(double value, int32_t maximum, AAX_CString *output) const override { return output && format(*state_, index_, value, maximum, *output); }
        bool StringToValue(const AAX_CString &input, double *output) const override { return output && parse(*state_, index_, input, *output); }
    };
public:
    using Prepared = std::array<std::unique_ptr<AAX_IParameter>, MaximumParameters>;
    explicit NativeAaxParameters(const RavetNativeCachedParameterEntryV1 *entry, uint32_t expectedBits = 32) : state_(std::make_shared<State>()) {
        auto &s = *state_; s.entry = entry;
        if ((expectedBits != 32 && expectedBits != 64) || !entry || entry->version != 1 || entry->byte_size != sizeof(*entry) ||
            entry->sample_bits != expectedBits || !entry->parameter_count || entry->parameter_count > MaximumParameters ||
            !entry->parameters_entry || !entry->create || !entry->destroy || !entry->info || !entry->normalize || !entry->denormalize) return;
        s.parameters = entry->parameters_entry();
        if (!s.parameters || s.parameters->version != 1 || s.parameters->byte_size != sizeof(*s.parameters) ||
            s.parameters->sample_bits != expectedBits || s.parameters->parameter_count != entry->parameter_count) return;
        s.handle = entry->create(); if (!s.handle) return;
        for (uint32_t i = 0; i < entry->parameter_count; ++i) {
            auto &info = s.infos[i];
            if (entry->info(s.handle, i, &info) || info.reserved || (info.flags & ~7u) || info.scale > 1 || info.intervals > 65535 ||
                !native::scalar::finite(info.minimum) || !native::scalar::finite(info.maximum) || !native::scalar::finite(info.default_value) ||
                info.minimum >= info.maximum || info.default_value < info.minimum || info.default_value > info.maximum ||
                (info.scale == 1 && info.minimum <= 0.f) || !info.name_length || !text(info.name, info.name_length) || !text(info.unit, info.unit_length)) return;
            for (uint32_t j = 0; j < i; ++j) if (s.infos[j].id == info.id) return;
            if (info.flags & RAVET_PARAMETER_BYPASS) {
                if (s.bypass != UINT32_MAX || info.minimum != 0.f || info.maximum != 1.f || info.intervals != 1 || (info.flags & RAVET_PARAMETER_READ_ONLY)) return;
                s.bypass = i;
            }
            auto &id = s.ids[i]; id[0] = 'p';
            char reverse[8]{}; uint32_t value = info.id, n = 0;
            do { reverse[n++] = "0123456789ABCDEF"[value & 15]; value >>= 4; } while (value);
            for (uint32_t j = 0; j < n; ++j) id[j + 1] = reverse[n - j - 1];
            if (entry->normalize(s.handle, info.id, info.default_value, &s.defaults[i]) || !normalized(s.defaults[i])) return;
        }
        s.count = entry->parameter_count; s.valid = true;
    }
    NativeAaxParameters(const NativeAaxParameters &) = delete;
    NativeAaxParameters &operator=(const NativeAaxParameters &) = delete;
    bool valid() const { return state_->valid; }
    uint32_t count() const { return valid() ? state_->count : 0; }
    const RavetNativeParameterEntryV1 *parametersEntry() const { return valid() ? state_->parameters : nullptr; }
    const char *id(uint32_t i) const { return i < count() ? state_->ids[i].data() : nullptr; }
    const char *masterBypass() const { return valid() && state_->bypass < count() ? id(state_->bypass) : nullptr; }
    uint32_t index(const char *value) const {
        if (!value) return count();
        size_t n = 0; while (n < 10 && value[n]) ++n;
        if (n < 2 || n > 9) return count();
        for (uint32_t i = 0; i < count(); ++i) if (!std::strcmp(value, id(i))) return i;
        return count();
    }
    const RavetNativeParameterInfoV1 *info(const char *value) const { const auto i = index(value); return i < count() ? &state_->infos[i] : nullptr; }
    bool value(const char *id, double input, double &output) const {
        const auto i = index(id); float amount = 0.f, next = 0.f;
        if (i == count() || !physical(*state_, i, input, amount) || state_->entry->normalize(state_->handle, state_->infos[i].id, amount, &next) || !normalized(next)) return false;
        output = double(next); return true;
    }
    bool control(const char *id, double input, double &output) const {
        const auto *metadata = info(id);
        return metadata && !(metadata->flags & RAVET_PARAMETER_READ_ONLY) && value(id, input, output);
    }
    // Pure exact writable block-start event mapping; whole output commits last.
    bool event(const char *id, double input, RavetNativeEventV1 &output) const {
        const auto i = index(id); float amount = 0.f;
        if (i == count() || (state_->infos[i].flags & RAVET_PARAMETER_READ_ONLY) || !physical(*state_, i, input, amount)) return false;
        RavetNativeEventV1 next{}; next.kind = RAVET_NATIVE_PARAMETER_VALUE;
        next.parameter_id = state_->infos[i].id; next.value = double(amount);
        output = next; return true;
    }
    bool normalizedPhysical(uint32_t index, float input, double &output) const {
        if (index >= count() || !native::scalar::finite(input)) return false;
        float next = 0.f;
        if (state_->entry->normalize(state_->handle, state_->infos[index].id, input, &next) || !normalized(next)) return false;
        output = double(next); return true;
    }
    bool defaultValue(const char *id, double &output) const { const auto i = index(id); if (i == count()) return false; output = double(state_->defaults[i]); return true; }
    bool format(const char *id, double value, int32_t maximum, AAX_IString &output) const { const auto i = index(id); return i < count() && format(*state_, i, value, maximum, output); }
    bool parse(const char *id, const AAX_IString &input, double &output) const { const auto i = index(id); return i < count() && parse(*state_, i, input, output); }
    // Staging complete real SDK objects is CONTROL-only and can allocate/throw.
    // Failure before commit preserves the caller's previous prepared object set.
    bool prepare(Prepared &output) const {
        if (!valid()) return false;
        Prepared next{};
        for (uint32_t i = 0; i < count(); ++i) {
            const auto &info = state_->infos[i];
            const bool automate = (info.flags & RAVET_PARAMETER_AUTOMATABLE) && !(info.flags & RAVET_PARAMETER_READ_ONLY);
            auto object = std::make_unique<AAX_CParameter<double>>(id(i), AAX_CString(reinterpret_cast<const char *>(info.name)),
                double(state_->defaults[i]), AAX_CLinearTaperDelegate<double>(0., 1.), Display(state_, i), automate);
            object->SetType(info.intervals ? AAX_eParameterType_Discrete : AAX_eParameterType_Continuous);
            if (info.intervals) object->SetNumberOfSteps(info.intervals + 1);
            next[i] = std::move(object);
        }
        output = std::move(next); return true;
    }
    // The caller owns an exclusively serialized empty SDK manager. Known whole
    // shape/ID faults leave manager and staged set untouched. Preserve the exact
    // owned objects returned by prepare; replacing/mutating their SDK definitions
    // is outside this registration contract. SDK insertion can
    // allocate and has no notification rollback; any failure retires the owned
    // accepted prefix through public SDK removal and preserves untransferred set.
    bool registerEmpty(AAX_CParameterManager &manager, Prepared &prepared) const {
        if (!valid() || manager.NumParameters()) return false;
        for (uint32_t i = 0; i < MaximumParameters; ++i) {
            if (i < count() ? (!prepared[i] || std::strcmp(prepared[i]->Identifier(), id(i))) : bool(prepared[i])) return false;
        }
        for (uint32_t i = 0; i < count(); ++i) {
            auto *raw = prepared[i].release();
            try { manager.AddParameter(raw); }
            catch (...) {
                bool adopted = false;
                for (int32_t j = 0; j < manager.NumParameters(); ++j) if (manager.GetParameter(j) == raw) adopted = true;
                if (adopted) manager.RemoveParameter(raw); else delete raw;
                manager.RemoveAllParameters(); return false;
            }
            if (manager.GetParameterByID(id(i)) != raw) {
                bool adopted = false;
                for (int32_t j = 0; j < manager.NumParameters(); ++j) if (manager.GetParameter(j) == raw) adopted = true;
                if (adopted) manager.RemoveParameter(raw); else delete raw;
                manager.RemoveAllParameters(); return false;
            }
        }
        return true;
    }
};
} // namespace ravet::aax
