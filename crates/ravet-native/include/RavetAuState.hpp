#pragma once
#include "RavetNativeState.h"
#include <AudioToolbox/AudioToolbox.h>
#include <CoreFoundation/CoreFoundation.h>
#include <initializer_list>

namespace ravet::au {
// CONTROL-only, serialized, isolated complete-state validator. Never borrow the
// live processor: native publication decoding changes only this owned validator.
// Exact originating parameter/state tables and loaded library must outlive it.
// expectedBits defaults32; explicit32/64 requires matching table provenance.
// This codec validator does not select or prove a wrapped SDK audio precision.
// Creation/destruction/publication may allocate according to native CONTROL
// contracts. This object is not a publication consumed by audio callbacks.
// SDK restore/notifications/host bypass policy remain explicit consumer concerns;
// preflight cannot roll back later backend failures. Foreign CF objects are live.
class NativeAuState final {
public:
    NativeAuState(const RavetNativeStateEntryV1 *state, const RavetNativeParameterEntryV1 *parameters, uint32_t expectedBits = 32) : state_(state) {
        if ((expectedBits != 32 && expectedBits != 64) || !state || !parameters || state->version != 1 || state->byte_size != sizeof(*state) || state->sample_bits != expectedBits ||
            !state->parameter_count || state->parameter_count > 1024 || !state->parameters_entry || !state->create ||
            !state->destroy || !state->publish || state->parameters_entry() != parameters ||
            parameters->version != 1 || parameters->byte_size != sizeof(*parameters) || parameters->sample_bits != expectedBits ||
            parameters->parameter_count != state->parameter_count || !parameters->state_bytes || parameters->state_bytes > 8208) return;
        publication_ = state->create(nullptr, 0);
    }
    ~NativeAuState() { if (publication_) state_->destroy(publication_); }
    NativeAuState(const NativeAuState &) = delete;
    NativeAuState &operator=(const NativeAuState &) = delete;
    bool valid() const { return publication_ != nullptr; }
    bool validateData(const void *value) {
        if (!valid() || !value || CFGetTypeID(value) != CFDataGetTypeID()) return false;
        const auto data = static_cast<CFDataRef>(value);
        const auto length = CFDataGetLength(data);
        if (length <= 0 || length > 8208) return false;
        RavetNativeStateRevisionV1 revision{};
        return state_->publish(publication_, CFDataGetBytePtr(data), uint32_t(length), &revision) == 0;
    }
    // Both native streams and exact AU version/subtype/manufacturer are checked
    // before the consumer may call SDK restore. Native explicit migrations are
    // accepted; voices/transient history follow the native complete-state codec.
    bool validate(CFPropertyListRef value, const AudioComponentDescription &description) {
        if (!valid() || !value || CFGetTypeID(value) != CFDictionaryGetTypeID()) return false;
        const auto dictionary = static_cast<CFDictionaryRef>(value);
        if (CFDictionaryContainsKey(dictionary, CFSTR("part")) ||
            !validateData(CFDictionaryGetValue(dictionary, CFSTR("Processor State"))) ||
            !validateData(CFDictionaryGetValue(dictionary, CFSTR("Controller State")))) return false;
        const UInt32 expected[] = {0, description.componentSubType, description.componentManufacturer};
        uint32_t i = 0;
        for (const auto key : {CFSTR("version"), CFSTR("subtype"), CFSTR("manufacturer")}) {
            const auto field = CFDictionaryGetValue(dictionary, key);
            if (!field || CFGetTypeID(field) != CFNumberGetTypeID()) return false;
            SInt32 decoded = 0;
            if (!CFNumberGetValue(static_cast<CFNumberRef>(field), kCFNumberSInt32Type, &decoded) || UInt32(decoded) != expected[i++]) return false;
        }
        return true;
    }
    // The mutable dictionary is consumer-owned. Complete processor state is
    // authoritative; malformed data leaves the dictionary untouched.
    bool cohere(CFMutableDictionaryRef dictionary) {
        if (!dictionary || CFGetTypeID(dictionary) != CFDictionaryGetTypeID()) return false;
        const auto data = CFDictionaryGetValue(dictionary, CFSTR("Processor State"));
        if (!validateData(data)) return false;
        CFDictionarySetValue(dictionary, CFSTR("Controller State"), data); return true;
    }
private:
    const RavetNativeStateEntryV1 *state_ = nullptr;
    void *publication_ = nullptr;
};
} // namespace ravet::au
