#include "RavetNativeProcessor.h"
#include "RavetGain.h"
#include <array>
#include <cstdio>
#include <type_traits>
#include <limits>

static bool check(bool condition, const char *message) {
    if (!condition) std::fprintf(stderr, "native processor entry: %s\n", message);
    return condition;
}
template<class S> bool precision(uint32_t bits) {
    const auto *entry = ravet_native_processor_v1(bits);
    if (!check(entry && entry->version == 1 && entry->byte_size == sizeof(*entry) && entry->sample_bits == bits && entry->reserved == 0, "entry identity/layout")) return false;
    void *handle = entry->create();
    using Legacy = std::conditional_t<std::is_same_v<S, float>, RavetGain, RavetGain64>;
    Legacy *legacy = nullptr;
    if constexpr (std::is_same_v<S, float>) legacy = ravet_gain_create();
    else legacy = ravet_gain64_create();
    if (!check(handle && legacy, "create")) return false;
    const auto destroy = [&] {
        entry->destroy(handle);
        if constexpr (std::is_same_v<S, float>) ravet_gain_destroy(legacy);
        else ravet_gain64_destroy(legacy);
    };
    RavetNativeProcessorConfigV1 config{};
    config.version = 1; config.maximum_frames = 8; config.maximum_events = 4;
    config.bus_count = 1; config.sample_rate = 48000.; config.buses[0] = {2, 2};
    int legacyPreparation;
    if constexpr (std::is_same_v<S, float>) legacyPreparation = ravet_gain_prepare(legacy, 48000., 2, 8);
    else legacyPreparation = ravet_gain64_prepare(legacy, 48000., 2, 8);
    if (!check(entry->prepare(handle, &config) == 0 && legacyPreparation == 0, "prepare")) {destroy(); return false;}
    RavetNativeProcessorTimingV1 timing{99, 99, 99};
    if (!check(entry->timing(handle, &timing) == 0 && timing.latency_samples == 0 && timing.tail_kind == 0 && timing.tail_samples == 0, "Gain timing")) {destroy(); return false;}
    std::array<S, 10> left{}, right{}, legacyLeft{}, legacyRight{};
    std::array<const S *, 2> inputs{left.data(), right.data()}, oldInputs{legacyLeft.data(), legacyRight.data()};
    std::array<S *, 2> outputs{right.data(), left.data()}, oldOutputs{legacyRight.data(), legacyLeft.data()};
    RavetNativeProcessorBusV1 bus{2, 2, inputs.data(), outputs.data()};
    RavetNativeTransportV1 transport{};
    transport.present = 1023; transport.flags = RAVET_TRANSPORT_FLAG_PLAYING | RAVET_TRANSPORT_FLAG_LOOP_ACTIVE;
    transport.sample_position = 42; transport.beat_position = -2.; transport.tempo = 137.25; transport.tempo_increment = -0.0000123456789;
    transport.beat_loop_start = -4.; transport.beat_loop_end = 12.; transport.sample_loop_start = -128; transport.sample_loop_end = 128;
    transport.bar_start = -4.; transport.bar_number = -1; transport.signature_numerator = 7; transport.signature_denominator = 8;
    float gain = 0.; uint32_t bypass = 0;
    const auto legacyProcess = [&](uint32_t frames, const RavetGainEvent *events, uint32_t count) {
        if constexpr (std::is_same_v<S, float>) return ravet_gain_process(legacy, oldInputs.data(), oldOutputs.data(), 2, frames, events, count, &gain, &bypass);
        else return ravet_gain64_process(legacy, oldInputs.data(), oldOutputs.data(), 2, frames, events, count, &gain, &bypass);
    };
    const auto refill = [&] {left.fill(S(.25)); right.fill(S(.5)); legacyLeft.fill(S(.25)); legacyRight.fill(S(.5));};
    RavetNativeProcessorResultV1 result{99, 99};
    for (uint32_t block = 0; block < 1000; ++block) {
        refill();
        std::array<RavetGainEvent, 2> oldEvents{{{0, 1, float(1 + block % 3)}, {3, 2, float(block % 2)}}};
        std::array<RavetNativeEventV1, 2> events{};
        for (size_t i = 0; i < 2; ++i) {events[i].offset = oldEvents[i].offset; events[i].kind = RAVET_NATIVE_PARAMETER_VALUE; events[i].parameter_id = oldEvents[i].id; events[i].value = double(oldEvents[i].value);}
        if (!check(entry->process(handle, &bus, 1, 8, events.data(), 2, &transport, nullptr, 0, &result) == 0 && result.status == 0 && result.event_count == 0, "timed process") ||
            !check(legacyProcess(8, oldEvents.data(), 2) == 0 && left == legacyLeft && right == legacyRight, "legacy/native precision and cross-alias parity")) {destroy(); return false;}
    }
    // Establish unity state through a zero-frame edit, then reset smoothing.
    std::array<RavetNativeEventV1, 2> unity{};
    unity[0].kind = unity[1].kind = RAVET_NATIVE_PARAMETER_VALUE;
    unity[0].parameter_id = 1; unity[0].value = 1.; unity[1].parameter_id = 2;
    std::array<RavetGainEvent, 2> oldUnity{{{0, 1, 1.f}, {0, 2, 0.f}}};
    const RavetNativeProcessorBusV1 zeroBus{2, 2, nullptr, nullptr};
    if (!check(entry->process(handle, &zeroBus, 1, 0, unity.data(), 2, nullptr, nullptr, 0, &result) == 0 && legacyProcess(0, oldUnity.data(), 2) == 0 && entry->reset(handle) == 0, "zero-frame/reset")) {destroy(); return false;}
    if constexpr (std::is_same_v<S, float>) ravet_gain_reset(legacy);
    else ravet_gain64_reset(legacy);
    refill();
    auto invalid = unity; invalid[0].value = 4.; invalid[1].kind = 999;
    if (!check(entry->process(handle, &bus, 1, 8, invalid.data(), 2, nullptr, nullptr, 0, &result) == 1 && result.status == 2 && result.event_count == 0, "atomic native event rejection")) {destroy(); return false;}
    for (size_t frame = 0; frame < 8; ++frame) if (!check(left[frame] == 0 && right[frame] == 0, "fault silence")) {destroy(); return false;}
    if (!check(left[8] == S(.25) && right[8] == S(.5), "fault tails")) {destroy(); return false;}
    refill();
    if (!check(entry->process(handle, &bus, 1, 8, nullptr, 0, nullptr, nullptr, 0, &result) == 0 && legacyProcess(8, nullptr, 0) == 0 && left == legacyLeft && right == legacyRight, "rejected prefix preserves state")) {destroy(); return false;}
    auto badTransport = transport; badTransport.tempo = -1.;
    if (!check(entry->process(handle, &bus, 1, 8, unity.data(), 1, &badTransport, nullptr, 0, &result) == 1, "transport rejection")) {destroy(); return false;}
    if constexpr (std::is_same_v<S, double>) {
        left.fill(1e200); right.fill(-1e200);
        if (!check(entry->process(handle, &bus, 1, 8, nullptr, 0, nullptr, nullptr, 0, &result) == 0 && right[0] == 1e200 && left[0] == -1e200, "wide f64 native samples")) {destroy(); return false;}
    }
    if (!check(entry->deactivate(handle) == 0 && entry->timing(handle, &timing) == 1, "deactivate/query")) {destroy(); return false;}
    destroy();
    return true;
}
int main() {
    if (!check(ravet_native_processor_v1(16) == nullptr, "unknown precision") || !precision<float>(32) || !precision<double>(64)) return 1;
    std::puts("native processor v1 actual-library f32/f64 lifecycle, timing, 2000 timed/cross-alias blocks, atomic rejection, zero-frame and wide-f64 contracts passed");
    return 0;
}
