#pragma once
// RAVET-owned optional SDK consumer. Include an externally supplied VST3 SDK;
// no SDK implementation is copied or vendored here. All calls are CONTROL-only.
#include "RavetNativeParameters.h"
#include "public.sdk/source/vst/vstparameters.h"
#include "pluginterfaces/base/ustring.h"
#include <algorithm>
#include <array>
#include <charconv>
#include <cmath>
#include <limits>
#include <vector>

namespace ravet::vst3 {
namespace detail {
// Complete UTF-8 -> UTF-16, without clipping. ParameterInfo fields have 128
// code units including terminator; supplementary characters consume two units.
template <size_t N>
inline bool text(const uint8_t (&source)[N], uint32_t length, Steinberg::Vst::String128 output) {
    if (length >= N || !std::all_of(source + length, source + N, [](uint8_t b) { return b == 0; })) return false;
    size_t read = 0, used = 0;
    while (read < length) {
        uint32_t c = source[read++], minimum = 0; unsigned continuation = 0;
        if (c == 0) return false;
        if (c < 0x80) {}
        else if (c >= 0xc2 && c <= 0xdf) { c &= 0x1f; continuation = 1; minimum = 0x80; }
        else if (c >= 0xe0 && c <= 0xef) { c &= 0x0f; continuation = 2; minimum = 0x800; }
        else if (c >= 0xf0 && c <= 0xf4) { c &= 7; continuation = 3; minimum = 0x10000; }
        else return false;
        if (continuation > length - read) return false;
        for (unsigned i = 0; i < continuation; ++i) {
            const uint8_t next = source[read++]; if ((next & 0xc0) != 0x80) return false;
            c = (c << 6) | (next & 63);
        }
        if (c < minimum || c > 0x10ffff || (c >= 0xd800 && c <= 0xdfff)) return false;
        if (used + (c > 0xffff ? 2 : 1) > 127) return false;
        if (c > 0xffff) { c -= 0x10000; output[used++] = Steinberg::Vst::TChar(0xd800 + (c >> 10)); output[used++] = Steinberg::Vst::TChar(0xdc00 + (c & 1023)); }
        else output[used++] = Steinberg::Vst::TChar(c);
    }
    output[used] = 0; return true;
}
struct Candidate {
    Steinberg::Vst::ParameterInfo info{};
    float minimum = 0.f, maximum = 0.f;
};
inline bool candidate(const RavetNativeParameterEntryV1 *table, uint32_t index, Candidate &result) {
    RavetNativeParameterInfoV1 native{};
    if (table->info(index, &native) != 0 || native.id == Steinberg::Vst::kNoParamId || native.reserved ||
        (native.flags & ~7u) || native.scale > 1 || native.intervals > 65535 || !native.name_length ||
        !std::isfinite(native.minimum) || !std::isfinite(native.maximum) || !std::isfinite(native.default_value) ||
        native.minimum >= native.maximum || native.default_value < native.minimum || native.default_value > native.maximum ||
        (native.scale == 1 && native.minimum <= 0.f) ||
        ((native.flags & RAVET_PARAMETER_BYPASS) && (native.minimum != 0.f || native.maximum != 1.f || native.intervals != 1))) return false;
    Candidate next;
    if (!text(native.name, native.name_length, next.info.title) || !text(native.unit, native.unit_length, next.info.units)) return false;
    std::copy_n(next.info.title, 128, next.info.shortTitle);
    float normalized = 0.f;
    if (table->normalize(native.id, native.default_value, &normalized) != 0 || !std::isfinite(normalized) || normalized < 0.f || normalized > 1.f) return false;
    next.info.id = native.id; next.info.stepCount = int32_t(native.intervals);
    next.info.defaultNormalizedValue = normalized; next.info.unitId = Steinberg::Vst::kRootUnitId;
    if (native.flags & RAVET_PARAMETER_AUTOMATABLE) next.info.flags |= Steinberg::Vst::ParameterInfo::kCanAutomate;
    if (native.flags & RAVET_PARAMETER_BYPASS) next.info.flags |= Steinberg::Vst::ParameterInfo::kIsBypass;
    if (native.flags & RAVET_PARAMETER_READ_ONLY) next.info.flags |= Steinberg::Vst::ParameterInfo::kIsReadOnly;
    next.minimum = native.minimum; next.maximum = native.maximum; result = next; return true;
}
} // namespace detail

// Retain the immutable exact native table/library until every SDK parameter is
// destroyed. Physical values intentionally use the native global f32 contract.
// Nonfinite/unrepresentable conversion/provider errors return NaN, not a guessed
// mapping. The table's pure callbacks may acquire metadata; these are not audio
// automation conversion functions. SDK UString supplies numeric formatting.
class SchemaParameter final : public Steinberg::Vst::RangeParameter {
public:
    SchemaParameter(const RavetNativeParameterEntryV1 *table, const detail::Candidate &candidate)
        : RangeParameter(candidate.info, candidate.minimum, candidate.maximum), table_(table) {}
    Steinberg::Vst::ParamValue toPlain(Steinberg::Vst::ParamValue normalized) const override {
        return convert(normalized, table_->denormalize);
    }
    Steinberg::Vst::ParamValue toNormalized(Steinberg::Vst::ParamValue physical) const override {
        return convert(physical, table_->normalize);
    }
    void toString(Steinberg::Vst::ParamValue normalized, Steinberg::Vst::String128 output) const override {
        const double physical = toPlain(normalized);
        if (!std::isfinite(physical)) { output[0] = 0; return; }
        Steinberg::UString text(output, 128);
        if (getInfo().flags & Steinberg::Vst::ParameterInfo::kIsBypass) {
            text.assign(physical >= .5 ? u"On" : u"Off"); return;
        }
        // SDK stepped formatters assume integer choices or On/Off. Native
        // choices can be fractional or arbitrary two-point physical ranges.
        if (!text.printFloat(physical, getPrecision())) output[0] = 0;
    }
    bool fromString(const Steinberg::Vst::TChar *text, Steinberg::Vst::ParamValue &normalized) const override {
        if (!text) return false;
        size_t length = 0; while (length < 128 && text[length]) ++length;
        if (length == 128) return false;
        std::array<char, 128> ascii{};
        for (size_t i = 0; i < length; ++i) {
            if (text[i] > 127) return false;
            ascii[i] = char(text[i]);
        }
        const auto whitespace = [](char c) { return c == ' ' || c == '\t' || c == '\r' || c == '\n'; };
        size_t first = 0, last = length;
        while (first < last && whitespace(ascii[first])) ++first;
        while (first < last && whitespace(ascii[last - 1])) --last;
        if (getInfo().flags & Steinberg::Vst::ParameterInfo::kIsBypass) {
            if (last - first == 2 && ascii[first] == 'O' && ascii[first + 1] == 'n') { normalized = 1.; return true; }
            if (last - first == 3 && ascii[first] == 'O' && ascii[first + 1] == 'f' && ascii[first + 2] == 'f') { normalized = 0.; return true; }
        }
        if (first < last && ascii[first] == '+') {
            ++first;
            if (first < last && (ascii[first] == '+' || ascii[first] == '-')) return false;
        }
        if (first == last) return false;
        double physical = 0.;
        const auto parsed = std::from_chars(ascii.data() + first, ascii.data() + last, physical, std::chars_format::general);
        if (parsed.ec != std::errc{} || parsed.ptr != ascii.data() + last || !std::isfinite(physical)) return false;
        const double result = toNormalized(physical);
        if (!std::isfinite(result)) return false;
        normalized = result; return true;
    }
private:
    const RavetNativeParameterEntryV1 *table_;
    double convert(double value, int32_t (*mapping)(uint32_t, float, float *)) const {
        if (!std::isfinite(value) || value > std::numeric_limits<float>::max() || value < -std::numeric_limits<float>::max()) return std::numeric_limits<double>::quiet_NaN();
        float output = 0.f;
        if (mapping(getInfo().id, float(value), &output) != 0 || !std::isfinite(output)) return std::numeric_limits<double>::quiet_NaN();
        return double(output);
    }
};

// Complete schema/UTF-16/ID/conflict/default preflight before changing container.
// <=1024 descriptors. Reject VST3's reserved UINT32_MAX ID, duplicate IDs, invalid
// boolean bypass ranges and names exceeding 127 UTF-16 units rather than clipping.
// Existing parameters survive preflight rejection. SDK allocation/insertion
// exceptions and control-thread ownership remain consumer/SDK obligations; this
// helper supplies no allocation-failure transaction, audio mapping, groups,
// programs, state exchange, factory/editor, notification or host scheduling.
inline bool registerParameters(Steinberg::Vst::ParameterContainer &container,
                               const RavetNativeParameterEntryV1 *table) {
    if (!table || table->version != 1 || table->byte_size != sizeof(*table) ||
        (table->sample_bits != 32 && table->sample_bits != 64) || table->parameter_count > 1024 ||
        !table->info || !table->normalize || !table->denormalize) return false;
    std::vector<detail::Candidate> candidates(table->parameter_count);
    for (uint32_t index = 0; index < table->parameter_count; ++index) {
        if (!detail::candidate(table, index, candidates[index]) || container.getParameter(candidates[index].info.id)) return false;
        for (uint32_t previous = 0; previous < index; ++previous)
            if (candidates[previous].info.id == candidates[index].info.id) return false;
    }
    for (const auto &candidate : candidates) container.addParameter(new SchemaParameter(table, candidate));
    return true;
}
} // namespace ravet::vst3
