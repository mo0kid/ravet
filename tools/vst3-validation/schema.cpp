#include "RavetVst3Parameters.hpp"
#include <array>
#include <cstring>
#include <cstdio>
using namespace Steinberg;
using namespace Steinberg::Vst;
namespace {
enum class Mode { Normal, ReservedId, Duplicate, ProviderFailure, UnknownFlag, ReservedWord, InvalidDefault, InvalidRange, InvalidBypass, InvalidUtf8, LongName, Name127, Emoji127, Emoji128, InvalidMapping };
Mode mode = Mode::Normal;
bool check(bool condition, const char *message) { if (!condition) std::fprintf(stderr, "VST3 schema: %s\n", message); return condition; }
int32_t metadata(uint32_t index, RavetNativeParameterInfoV1 *output) {
    if (!output || index >= 4 || (mode == Mode::ProviderFailure && index == 3)) return 1;
    RavetNativeParameterInfoV1 value{}; value.id = 71 + index;
    const char *names[]{"Cutoff \xc2\xb5Hz", "Fractional \xf0\x9f\x8e\xb9", "Readout", "Bypass"};
    std::memcpy(value.name, names[index], std::strlen(names[index])); value.name_length = uint32_t(std::strlen(names[index]));
    value.flags = index == 2 ? RAVET_PARAMETER_READ_ONLY : RAVET_PARAMETER_AUTOMATABLE;
    value.minimum = -1.f; value.maximum = 1.f; value.default_value = 0.f;
    if (index == 0) { value.minimum = 20.f; value.maximum = 20000.f; value.default_value = 200.f; value.scale = 1; value.unit[0] = 'H'; value.unit[1] = 'z'; value.unit_length = 2; }
    if (index == 1) value.intervals = 4;
    if (index == 3) {
        value.minimum = 0.f; value.intervals = 1; value.flags |= RAVET_PARAMETER_BYPASS;
        switch (mode) {
            case Mode::ReservedId: value.id = UINT32_MAX; break;
            case Mode::Duplicate: value.id = 71; break;
            case Mode::UnknownFlag: value.flags |= 8; break;
            case Mode::ReservedWord: value.reserved = 1; break;
            case Mode::InvalidDefault: value.default_value = 2.f; break;
            case Mode::InvalidRange: value.minimum = 1.f; break;
            case Mode::InvalidBypass: value.intervals = 2; break;
            case Mode::InvalidUtf8: value.name[0] = 0xc0; value.name[1] = 0xaf; break;
            case Mode::LongName: case Mode::Name127:
                std::memset(value.name, 0, sizeof(value.name)); value.name_length = mode == Mode::LongName ? 128 : 127; std::memset(value.name, 'X', value.name_length); break;
            case Mode::Emoji127: case Mode::Emoji128:
                std::memset(value.name, 0, sizeof(value.name));
                for (size_t i = 0; i < 63; ++i) std::memcpy(value.name + i * 4, "\xf0\x9f\x8e\xb9", 4);
                value.name[252] = 'X'; value.name_length = 253;
                if (mode == Mode::Emoji128) { value.name[253] = 'Y'; value.name_length = 254; } break;
            default: break;
        }
    }
    *output = value; return 0;
}
int32_t normalize(uint32_t id, float input, float *output) {
    if (!output || id < 71 || id > 74 || !std::isfinite(input) || mode == Mode::InvalidMapping) return 1;
    if (id == 71) *output = std::log(std::clamp(input, 20.f, 20000.f) / 20.f) / std::log(1000.f);
    else if (id == 74) *output = input >= .5f ? 1.f : 0.f;
    else { const float value = (std::clamp(input, -1.f, 1.f) + 1.f) * .5f; *output = id == 72 ? std::round(value * 4.f) * .25f : value; }
    return 0;
}
int32_t denormalize(uint32_t id, float input, float *output) {
    if (!output || id < 71 || id > 74 || !std::isfinite(input)) return 1;
    const float value = std::clamp(input, 0.f, 1.f);
    if (id == 71) *output = 20.f * std::pow(1000.f, value);
    else if (id == 74) *output = value >= .5f ? 1.f : 0.f;
    else *output = (id == 72 ? std::round(value * 4.f) * .25f : value) * 2.f - 1.f;
    return 0;
}
RavetNativeParameterEntryV1 table() {
    RavetNativeParameterEntryV1 result{}; result.version = 1; result.byte_size = sizeof(result); result.sample_bits = 64; result.parameter_count = 4;
    result.info = metadata; result.normalize = normalize; result.denormalize = denormalize; return result;
}
bool synthetic() {
    auto native = table(); ParameterContainer parameters;
    if (!check(ravet::vst3::registerParameters(parameters, &native) && parameters.getParameterCount() == 4, "unrelated complete schema")) return false;
    auto *log = parameters.getParameter(71), *steps = parameters.getParameter(72), *readout = parameters.getParameter(73), *bypass = parameters.getParameter(74);
    if (!check(log && steps && readout && bypass && std::abs(log->getNormalized() - 1./3.) < 1e-6 && std::abs(log->toPlain(.5) - std::sqrt(400000.)) < .01 && std::abs(log->toNormalized(200.) - 1./3.) < 1e-6 && steps->toPlain(.3) == -.5 && steps->toNormalized(.4) == .75, "log/discrete/native defaults")) return false;
    if (!check(steps->getInfo().stepCount == 4 && readout->getInfo().flags == ParameterInfo::kIsReadOnly && bypass->getInfo().flags == (ParameterInfo::kCanAutomate | ParameterInfo::kIsBypass) && log->getInfo().title[7] == 0xb5 && steps->getInfo().title[11] == 0xd83c && steps->getInfo().title[12] == 0xdfb9 && log->getInfo().units[0] == u'H', "UTF16/roles/unit/steps")) return false;
    String128 display{}; steps->setPrecision(1); steps->toString(.75, display); double normalized = 91.;
    if (!check(display[0] == u'0' && display[1] == u'.' && display[2] == u'5' && steps->fromString(u"0.5", normalized) && normalized == .75, "fractional choice numeric text")) return false;
    normalized = 91.;
    if (!check(!steps->fromString(u"invalid", normalized) && normalized == 91. && std::isnan(log->toPlain(std::numeric_limits<double>::infinity())) && std::isnan(log->toNormalized(1e300)) && log->toPlain(-1.) == 20., "failed conversions preserve text output and finite mapping policy")) return false;
    for (const auto *invalid : {u"0.5junk", u"1e9999", u"nan", u"inf", u"+", u"+-0.5", u"0,5", u"\u00b5"}) {
        normalized = 91.; if (!check(!steps->fromString(invalid, normalized) && normalized == 91., "strict complete numeric parsing")) return false;
    }
    if (!check(steps->fromString(u" \t+.5\r\n", normalized) && normalized == .75, "bounded signed/whitespace decimal parsing")) return false;
    bypass->toString(0., display); normalized = 91.;
    if (!check(display[0] == u'O' && display[1] == u'f' && display[2] == u'f' && bypass->fromString(display, normalized) && normalized == 0., "bypass Off roundtrip")) return false;
    bypass->toString(.5, display);
    if (!check(display[0] == u'O' && display[1] == u'n' && bypass->fromString(display, normalized) && normalized == 1., "bypass On/ties-up roundtrip")) return false;
    for (Mode failure : {Mode::ReservedId, Mode::Duplicate, Mode::ProviderFailure, Mode::UnknownFlag, Mode::ReservedWord, Mode::InvalidDefault, Mode::InvalidRange, Mode::InvalidBypass, Mode::InvalidUtf8, Mode::LongName, Mode::Emoji128, Mode::InvalidMapping}) {
        mode = failure; ParameterContainer rejected; auto *existing = rejected.addParameter(u"Existing", nullptr, 0, 0., 0, 999);
        if (!check(!ravet::vst3::registerParameters(rejected, &native) && rejected.getParameterCount() == 1 && rejected.getParameter(999) == existing, "whole preflight failure preserves existing container")) return false;
    }
    for (Mode valid : {Mode::Name127, Mode::Emoji127}) { mode = valid; ParameterContainer accepted; if (!check(ravet::vst3::registerParameters(accepted, &native) && accepted.getParameter(74)->getInfo().title[127] == 0, "complete 127-unit boundary")) return false; }
    mode = Mode::Normal; ParameterContainer conflict; auto *existing = conflict.addParameter(u"Existing", nullptr, 0, 0., 0, 72);
    if (!check(!ravet::vst3::registerParameters(conflict, &native) && conflict.getParameterCount() == 1 && conflict.getParameter(72) == existing, "conflict preflight")) return false;
    ParameterContainer invalid;
    for (unsigned test = 0; test < 4; ++test) { auto bad = native; if (test == 0) bad.version = 2; if (test == 1) bad.byte_size--; if (test == 2) bad.parameter_count = 1025; if (test == 3) bad.info = nullptr; if (!check(!ravet::vst3::registerParameters(invalid, &bad) && invalid.getParameterCount() == 0, "invalid table preflight")) return false; }
    return true;
}
bool gain() {
    for (uint32_t bits : {32u, 64u}) {
        ParameterContainer parameters; const auto *native = ravet_native_parameters_v1(bits);
        if (!check(ravet::vst3::registerParameters(parameters, native) && parameters.getParameterCount() == 2, "actual Gain schema registration")) return false;
        auto *gain = parameters.getParameter(1), *bypass = parameters.getParameter(2);
        if (!check(gain && bypass && gain->toPlain(.5) == 2. && gain->toNormalized(1.) == .25 && gain->getNormalized() == .25 && bypass->toPlain(.5) == 1. && bypass->getInfo().stepCount == 1 && (bypass->getInfo().flags & ParameterInfo::kIsBypass), "actual Gain native mapping/roles/default")) return false;
    }
    return true;
}
}
int main() {
    if (!synthetic() || !gain()) return 1;
    std::puts("VST3 generic schema registration: unrelated log/fractional/UTF16/roles, full preflight rejection/boundaries and actual Gain native f32/f64 passed"); return 0;
}
