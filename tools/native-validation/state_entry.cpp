#include "RavetNativeState.h"
#include "RavetGain.h"
#include <array>
#include <algorithm>
#include <cstdio>
#include <cstring>
static bool check(bool value, const char *message) { if (!value) std::fprintf(stderr, "native live state: %s\n", message); return value; }
bool precision(uint32_t bits) {
    const auto *table = ravet_native_state_v1(bits);
    if (!check(table && table->version == 1 && table->byte_size == sizeof(*table) && table->sample_bits == bits && table->parameter_count == 2, "table")) return false;
    const auto *parameters = table->parameters_entry(); const auto *base = parameters->processor_entry();
    if (!check(parameters->sample_bits == bits && base->sample_bits == bits && base->create == ravet_native_processor_v1(bits)->create, "pairing")) return false;
    void *handle = base->create(); void *publication = table->create(nullptr, 0);
    if (!check(handle && publication, "creation")) return false;
    const auto cleanup = [&] { table->destroy(publication); base->destroy(handle); };
    RavetNativeProcessorConfigV1 config{}; config.version = 1; config.maximum_frames = 8; config.maximum_events = 2; config.bus_count = 1; config.sample_rate = 48000.; config.buses[0] = {1, 1};
    RavetNativeStateApplyV1 applied{}; RavetNativeStateRevisionV1 revision{}, reflected{};
    if (!check(table->apply(handle, publication, &applied) == 1 && base->prepare(handle, &config) == 0 && table->apply(handle, publication, &applied) == 0 && applied.changed == 1 && applied.revision.generation == 0, "default apply/lifecycle")) {cleanup(); return false;}
    std::array<float, 10> input32{}, output32{}; std::array<double, 10> input64{}, output64{};
    input32.fill(.25f); input64.fill(1e200); output32.fill(91.f); output64.fill(91.);
    std::array<const float *, 1> in32{input32.data()}; std::array<float *, 1> out32{output32.data()};
    std::array<const double *, 1> in64{input64.data()}; std::array<double *, 1> out64{output64.data()};
    RavetNativeProcessorBusV1 bus{1, 1, bits == 32 ? static_cast<const void *>(in32.data()) : static_cast<const void *>(in64.data()), bits == 32 ? static_cast<const void *>(out32.data()) : static_cast<const void *>(out64.data())};
    RavetNativeProcessorResultV1 result{};
    std::array<uint8_t, 32> bytes{}; std::array<uint8_t, 33> saved{}; saved.fill(0xff);
    std::array<RavetNativeParameterValueV1, 3> values{{{77, 91.f}, {77, 91.f}, {77, 91.f}}};
    for (uint32_t cycle = 0; cycle < 1000; ++cycle) {
        const float gain = float(cycle % 5); const uint32_t bypass = cycle % 2;
        if (!check(ravet_gain_encode(gain, bypass, bytes.data(), bytes.size()) == 0 && table->publish(publication, bytes.data(), bytes.size(), &revision) == 0 && table->apply(handle, publication, &applied) == 0 && applied.changed == 1 && applied.revision.generation == revision.generation, "publish/apply")) {cleanup(); return false;}
        if (!check(table->apply(handle, publication, &applied) == 0 && applied.changed == 0 && base->process(handle, &bus, 1, 8, nullptr, 0, nullptr, nullptr, 0, &result) == 0, "same generation/process")) {cleanup(); return false;}
        const double multiplier = bypass ? 1. : gain;
        if (!check((bits == 32 ? output32[0] == .25f * float(multiplier) : output64[0] == 1e200 * multiplier) && (bits == 32 ? output32[8] == 91.f : output64[8] == 91.), "wide audio/tails")) {cleanup(); return false;}
        if (!check(table->reflect(handle, publication, &reflected) == 0 && reflected.generation == revision.generation && table->snapshot(publication, values.data(), values.size(), &reflected) == 0 && values[0].id == 1 && values[0].value == gain && values[1].value == float(bypass) && values[2].id == 77 && values[2].value == 91.f && table->save_state(publication, saved.data(), saved.size(), &reflected) == 0 && std::equal(bytes.begin(), bytes.end(), saved.begin()) && saved[32] == 0xff, "coherent reflection/observer/canonical state")) {cleanup(); return false;}
    }
    // Restore arriving after apply always wins over the older audio-side reflect.
    ravet_gain_encode(.5f, 0, bytes.data(), bytes.size());
    if (!check(table->publish(publication, bytes.data(), bytes.size(), &revision) == 0, "newer control restore")) {cleanup(); return false;}
    const RavetNativeStateRevisionV1 sentinel{77, 99}; reflected = sentinel;
    if (!check(table->reflect(handle, publication, &reflected) == 4 && reflected.instance == 77 && reflected.generation == 99 && table->save_state(publication, saved.data(), saved.size(), &reflected) == 0 && std::equal(bytes.begin(), bytes.end(), saved.begin()) && table->apply(handle, publication, &applied) == 0, "stale reflection preserves newer control state")) {cleanup(); return false;}
    // Audio automation changes the base target, and reflection must not cause
    // repeated restoration (which would snap the ongoing Gain smoothing ramp).
    RavetNativeEventV1 event{}; event.kind = RAVET_NATIVE_PARAMETER_VALUE; event.parameter_id = 1; event.value = 2.;
    if (!check(base->process(handle, &bus, 1, 8, &event, 1, nullptr, nullptr, 0, &result) == 0 && table->reflect(handle, publication, &reflected) == 0 && reflected.generation == revision.generation + 2 && table->apply(handle, publication, &applied) == 0 && applied.changed == 0 && base->process(handle, &bus, 1, 8, nullptr, 0, nullptr, nullptr, 0, &result) == 0 && (bits == 32 ? output32[0] > .125f && output32[0] < .5f : output64[0] > .5e200 && output64[0] < 2e200), "reflected automation retains smoothing")) {cleanup(); return false;}
    auto invalid = bytes; invalid[0] ^= 1; reflected = sentinel;
    if (!check(table->publish(publication, invalid.data(), invalid.size(), &reflected) == 1 && reflected.instance == 77 && reflected.generation == 99, "invalid restore preserves outputs")) {cleanup(); return false;}
    const std::array<uint8_t, 20> legacy{{'R','A','V','E','T','G','N',0, 1,0,0,0, 1,0,0,0, 0,0,0,0x3f}};
    if (!check(table->publish(publication, legacy.data(), legacy.size(), &revision) == 0 && table->apply(handle, publication, &applied) == 0 && table->snapshot(publication, values.data(), values.size(), &reflected) == 0 && values[0].value == .5f && values[1].value == 0.f, "explicit legacy migration")) {cleanup(); return false;}
    if (!check(base->deactivate(handle) == 0 && table->apply(handle, publication, &applied) == 1 && base->prepare(handle, &config) == 0 && table->apply(handle, publication, &applied) == 0 && applied.changed == 1, "stopped reprepare clears accepted revision")) {cleanup(); return false;}
    cleanup(); return true;
}
int main() {
    if (!check(ravet_native_state_v1(16) == nullptr, "invalid precision") || !precision(32) || !precision(64)) return 1;
    std::puts("native live state actual-library f32/f64 2000 apply/audio/reflect cycles, coherent observer/state, newer-restore priority, retained smoothing and legacy migration passed"); return 0;
}
