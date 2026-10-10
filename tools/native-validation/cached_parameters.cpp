#include "RavetCachedParameters.h"
#include <array>
#include <cstdio>
#include <cstring>
#include <limits>
bool check(bool value, const char *message) { if (!value) std::fprintf(stderr, "cached parameters: %s\n", message); return value; }
bool precision(uint32_t bits) {
    const auto *table = ravet_native_cached_parameters_v1(bits);
    if (!check(table && table->version == 1 && table->byte_size == sizeof(*table) && table->sample_bits == bits && table->parameter_count == 2, "entry")) return false;
    const auto *paired = table->parameters_entry();
    if (!check(paired && paired->sample_bits == bits && paired->parameter_count == 2 && paired->processor_entry()->create == ravet_native_processor_v1(bits)->create, "pairing")) return false;
    void *cache = table->create(); if (!check(cache != nullptr, "create")) return false;
    const auto cleanup = [&] { table->destroy(cache); };
    RavetNativeParameterInfoV1 info{}; float value = 91.f;
    std::array<RavetNativeNormalizedParameterEventV1, 3> input{{{0, 1, .5}, {0, 2, .5}, {7, 1, .25}}};
    std::array<RavetNativeEventV1, 4> output{}; output[3].kind = 99; output[3].value = 91.;
    for (unsigned cycle = 0; cycle < 1000; ++cycle) {
        if (!check(table->info(cache, 0, &info) == 0 && info.id == 1 && info.minimum == 0.f && info.maximum == 4.f && table->normalize(cache, 1, 2.f, &value) == 0 && value == .5f && table->denormalize(cache, 1, .25f, &value) == 0 && value == 1.f && table->convert_events(cache, input.data(), input.size(), 8, output.data(), output.size()) == 0, "cached metadata/scalar/bulk")) {cleanup(); return false;}
        if (!check(output[0].offset == 0 && output[0].kind == RAVET_NATIVE_PARAMETER_VALUE && output[0].parameter_id == 1 && output[0].value == 2. && output[1].offset == 0 && output[1].parameter_id == 2 && output[1].value == 1. && output[2].offset == 7 && output[2].value == 1. && output[3].kind == 99 && output[3].value == 91., "physical event/tie/tail")) {cleanup(); return false;}
    }
    const auto original = output;
    for (unsigned test = 0; test < 7; ++test) {
        auto invalid = input;
        if (test == 0) invalid[2].id = 99;
        if (test == 1) invalid[2].normalized = std::numeric_limits<double>::quiet_NaN();
        if (test == 2) invalid[2].normalized = -.001;
        if (test == 3) invalid[2].normalized = 1.001;
        if (test == 4) invalid[2].offset = 8;
        if (test == 5) invalid[1].offset = 6, invalid[2].offset = 5;
        if (!check(table->convert_events(cache, invalid.data(), invalid.size(), 8, output.data(), test == 6 ? 2 : output.size()) == 1 && std::memcmp(output.data(), original.data(), sizeof(output)) == 0, "complete rejection preserves destination")) {cleanup(); return false;}
    }
    value = 91.f;
    if (!check(table->denormalize(cache, 99, .5f, &value) == 1 && value == 91.f && table->convert_events(cache, nullptr, 0, 0, nullptr, 0) == 0, "failed scalar/empty flush")) {cleanup(); return false;}
    const auto before = info;
    if (!check(table->info(cache, 2, &info) == 1 && std::memcmp(&info, &before, sizeof(info)) == 0, "invalid metadata index")) {cleanup(); return false;}
    cleanup(); return true;
}
int main() {
    if (!check(ravet_native_cached_parameters_v1(16) == nullptr, "invalid precision") || !precision(32) || !precision(64)) return 1;
    std::puts("cached native actual Gain f32/f64 metadata/mapping, 2000 normalized event conversions, ties/tails and complete rejection passed"); return 0;
}
