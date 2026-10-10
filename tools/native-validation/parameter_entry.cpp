#include "RavetNativeParameters.h"
#include "RavetGain.h"
#include <array>
#include <algorithm>
#include <cstdio>
#include <cstring>
static bool check(bool value, const char *message) { if (!value) std::fprintf(stderr, "native parameter entry: %s\n", message); return value; }
bool precision(uint32_t bits) {
    const auto *table = ravet_native_parameters_v1(bits);
    const auto *base = ravet_native_processor_v1(bits);
    if (!check(table && base && table->version == 1 && table->byte_size == sizeof(*table) && table->sample_bits == bits && table->parameter_count == 2 && table->state_bytes == 32 && table->state_version == 2, "table metadata")) return false;
    const auto *paired = table->processor_entry();
    if (!check(paired && paired->create == base->create && paired->process == base->process && std::memcmp(table->state_product, "RAVETGN\0", 8) == 0, "processor/protocol pairing")) return false;
    void *handle = base->create(); if (!check(handle != nullptr, "create")) return false;
    const auto cleanup = [&] { base->destroy(handle); };
    RavetNativeProcessorConfigV1 config{}; config.version = 1; config.maximum_frames = 8; config.maximum_events = 2; config.bus_count = 1; config.sample_rate = 48000.; config.buses[0] = {1, 1};
    if (!check(base->prepare(handle, &config) == 0, "prepare")) {cleanup(); return false;}
    RavetNativeParameterInfoV1 gain{}, bypass{};
    if (!check(table->info(0, &gain) == 0 && table->info(1, &bypass) == 0 && gain.id == 1 && gain.flags == RAVET_PARAMETER_AUTOMATABLE && gain.minimum == 0.f && gain.maximum == 4.f && gain.default_value == 1.f && gain.scale == 0 && gain.intervals == 0 && bypass.id == 2 && bypass.flags == (RAVET_PARAMETER_AUTOMATABLE | RAVET_PARAMETER_BYPASS) && bypass.intervals == 1 && bypass.default_value == 0.f, "Gain metadata/roles")) {cleanup(); return false;}
    const auto before = gain; if (!check(table->info(2, &gain) == 1 && std::memcmp(&gain, &before, sizeof(gain)) == 0, "invalid descriptor preserves output")) {cleanup(); return false;}
    float normalized = 99.f;
    if (!check(table->normalize(1, 2.f, &normalized) == 0 && normalized == .5f && table->denormalize(1, normalized, &normalized) == 0 && normalized == 2.f, "range mapping")) {cleanup(); return false;}
    std::array<uint8_t, 33> saved{}; saved.fill(0xff);
    std::array<uint8_t, 32> expected{};
    std::array<RavetNativeParameterValueV1, 3> snapshot{{{99, 91.f}, {99, 91.f}, {99, 91.f}}};
    std::array<float, 10> input32{}, output32{}; std::array<double, 10> input64{}, output64{};
    input32.fill(.25f); input64.fill(1e200); output32.fill(91.f); output64.fill(91.);
    std::array<const float *, 1> in32{input32.data()}; std::array<float *, 1> out32{output32.data()};
    std::array<const double *, 1> in64{input64.data()}; std::array<double *, 1> out64{output64.data()};
    RavetNativeProcessorBusV1 bus{1, 1, bits == 32 ? static_cast<const void *>(in32.data()) : static_cast<const void *>(in64.data()), bits == 32 ? static_cast<const void *>(out32.data()) : static_cast<const void *>(out64.data())};
    RavetNativeProcessorResultV1 result{99, 99};
    for (uint32_t cycle = 0; cycle < 1000; ++cycle) {
        const float requested = float(cycle % 5); const uint32_t requestedBypass = cycle % 2;
        if (!check(ravet_gain_encode(requested, requestedBypass, expected.data(), expected.size()) == 0 && table->load_state(handle, expected.data(), expected.size()) == 0 && table->snapshot(handle, snapshot.data(), snapshot.size()) == 0 && snapshot[0].id == 1 && snapshot[0].value == requested && snapshot[1].id == 2 && snapshot[1].value == float(requestedBypass) && snapshot[2].id == 99 && snapshot[2].value == 91.f, "legacy codec restore/snapshot")) {cleanup(); return false;}
        if (!check(table->save_state(handle, saved.data(), saved.size()) == 0 && std::equal(expected.begin(), expected.end(), saved.begin()) && saved[32] == 0xff, "canonical save/tail")) {cleanup(); return false;}
        float decoded = 99.f; uint32_t decodedBypass = 99;
        if (!check(ravet_gain_decode(saved.data(), 32, &decoded, &decodedBypass) == 0 && decoded == requested && decodedBypass == requestedBypass, "existing complete codec parity")) {cleanup(); return false;}
        if (!check(base->process(handle, &bus, 1, 8, nullptr, 0, nullptr, nullptr, 0, &result) == 0 && result.status == 0, "restored audio")) {cleanup(); return false;}
        const double multiplier = requestedBypass ? 1. : requested;
        if (!check((bits == 32 ? double(output32[0]) == double(.25f * float(multiplier)) : output64[0] == 1e200 * multiplier) && (bits == 32 ? output32[8] == 91.f : output64[8] == 91.), "state/audio precision and tails")) {cleanup(); return false;}
    }
    auto invalid = expected; invalid[0] ^= 1;
    if (!check(table->load_state(handle, invalid.data(), invalid.size()) == 1 && table->save_state(handle, saved.data(), saved.size()) == 0 && std::equal(expected.begin(), expected.end(), saved.begin()), "wrong-product atomic rejection")) {cleanup(); return false;}
    auto duplicate = expected; std::copy_n(expected.data() + 16, 4, duplicate.data() + 24);
    if (!check(table->load_state(handle, duplicate.data(), duplicate.size()) == 1, "duplicate-ID rejection")) {cleanup(); return false;}
    auto reordered = expected; std::copy_n(expected.data() + 24, 8, reordered.data() + 16); std::copy_n(expected.data() + 16, 8, reordered.data() + 24);
    if (!check(table->load_state(handle, reordered.data(), reordered.size()) == 0, "stable-ID field order")) {cleanup(); return false;}
    // Existing public legacy scalar fixture, version 1 ID1/value .5: migration
    // uses the explicit Gain decoder and resets missing bypass to off.
    const std::array<uint8_t, 20> legacy{{'R','A','V','E','T','G','N',0, 1,0,0,0, 1,0,0,0, 0,0,0,0x3f}};
    if (!check(table->load_state(handle, legacy.data(), legacy.size()) == 0 && table->snapshot(handle, snapshot.data(), snapshot.size()) == 0 && snapshot[0].value == .5f && snapshot[1].value == 0.f, "explicit legacy scalar migration")) {cleanup(); return false;}
    if (!check(base->deactivate(handle) == 0 && table->save_state(handle, saved.data(), saved.size()) == 0, "inactive snapshot/save")) {cleanup(); return false;}
    cleanup(); return true;
}
int main() {
    if (!check(ravet_native_parameters_v1(16) == nullptr, "invalid precision") || !precision(32) || !precision(64)) return 1;
    std::puts("native parameter/state actual-library f32/f64 metadata, mappings, snapshots, 2000 complete recall/audio cycles, canonical/legacy codec parity and atomic rejection passed"); return 0;
}
