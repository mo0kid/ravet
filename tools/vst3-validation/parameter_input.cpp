#include "RavetVst3ParameterInput.hpp"
#include "public.sdk/source/vst/hosting/parameterchanges.h"
#include <cstdio>
#include <cstdlib>
#include <new>
namespace { bool measuring = false; size_t news = 0, deletes = 0; }
void *operator new(size_t bytes) { if (measuring) ++news; if (auto *value = std::malloc(bytes ? bytes : 1)) return value; throw std::bad_alloc(); }
void operator delete(void *value) noexcept { if (value && measuring) ++deletes; std::free(value); }
void operator delete(void *value, size_t) noexcept { ::operator delete(value); }
using namespace Steinberg;
using namespace Steinberg::Vst;
using namespace ravet::vst3;
namespace {
bool check(bool condition, const char *message) { if (!condition) std::fprintf(stderr, "VST3 parameter input: %s\n", message); return condition; }
class Queue final : public ParameterValueQueue {
public:
    Queue(uint32_t id) : ParameterValueQueue(id) {}
    std::array<GlobalParameterPoint, 1024> points{};
    int32 count = 0; int calls = 0; int failure = -1;
    int32 PLUGIN_API getPointCount() override { return count; }
    tresult PLUGIN_API getPoint(int32 index, int32 &offset, ParamValue &value) override {
        ++calls; if (index == failure || index < 0 || index >= count || index >= 1024) return kResultFalse;
        offset = int32(points[size_t(index)].offset); value = points[size_t(index)].normalized; return kResultOk;
    }
};
class Changes final : public ParameterChanges {
public:
    std::array<Queue *, 4> values{}; int32 count = 0; bool nullQueue = false;
    int32 PLUGIN_API getParameterCount() override { return count; }
    IParamValueQueue *PLUGIN_API getParameterData(int32 index) override {
        return nullQueue || index < 0 || index >= count || index >= 4 ? nullptr : values[size_t(index)];
    }
};
const RavetNativeCachedParameterEntryV1 *actual = nullptr;
unsigned infoCalls = 0, mapCalls = 0;
// Inject malformed foreign memory by bytes. Constructing/passing a floating NaN
// constant by value under fast-math violates that compilation mode's assumptions.
void malformed(double &output, uint64_t bits) {
    volatile uint64_t source = bits; const uint64_t copy = source;
    std::memcpy(&output, &copy, sizeof(copy));
}
int32_t info(const void *cache, uint32_t index, RavetNativeParameterInfoV1 *output) {
    ++infoCalls; const auto code = actual->info(cache, index, output);
    if (code == 0 && index == 1) output->flags |= RAVET_PARAMETER_READ_ONLY;
    return code;
}
int32_t denormalize(const void *cache, uint32_t id, float input, float *output) { ++mapCalls; return actual->denormalize(cache, id, input, output); }
bool profiles() {
    for (uint32_t bits : {32u, 64u}) {
        CachedParameterInput cache(ravet_native_cached_parameters_v1(bits)); float physical = 91.f;
        if (!check(cache.valid() && cache.sampleBits() == bits && cache.map(1, .5, physical, false) == ParameterInputResult::Value && physical == 2.f && cache.map(2, .5, physical, false) == ParameterInputResult::Value && physical == 1.f, "actual native f32/f64 profile")) return false;
        physical = 91.f;
        double invalid = 0.; malformed(invalid, UINT64_C(0x7ff8000000000001));
        if (!check(cache.map(99, .5, physical, false) == ParameterInputResult::Unknown && physical == 91.f && cache.map(1, invalid, physical, false) == ParameterInputResult::Rejected && physical == 91.f, "unknown/rejected output preservation")) return false;
    }
    actual = ravet_native_cached_parameters_v1(32); auto adapted = *actual; adapted.info = info; adapted.denormalize = denormalize;
    CachedParameterInput cache(&adapted); const auto providers = infoCalls; float physical = 91.f;
    if (!check(cache.valid() && cache.map(2, .5, physical, false) == ParameterInputResult::Rejected && physical == 91.f && cache.map(2, .5, physical, true) == ParameterInputResult::Value && physical == 1.f, "explicit read-only policy")) return false;
    Queue gain(1), unknown(99); gain.count = 2; gain.points[0] = {5, 1, .25}; gain.points[1] = {5, 1, .5}; unknown.count = 1; unknown.points[0] = {0, 99, .75};
    Changes changes; changes.count = 2; changes.values = {&gain, &unknown, nullptr, nullptr}; std::array<GlobalParameterPoint, 3> delivered{}; size_t used = 0;
    const auto visitor = [&](const GlobalParameterPoint &point) { delivered[used++] = point; return cache.map(point.id, point.normalized, physical, false) != ParameterInputResult::Rejected; };
    measuring = true;
    bool passed = true;
    for (unsigned cycle = 0; cycle < 1000; ++cycle) {
        used = 0; gain.calls = 0; unknown.calls = 0;
        if (!visitValidatedGlobalParameters(&changes, 8, visitor) || used != 3 || delivered[0].offset != 5 || delivered[1].offset != 5 || delivered[2].offset != 0 || gain.calls != 2 || unknown.calls != 1) { passed = false; break; }
    }
    measuring = false;
    if (!check(passed && news == 0 && deletes == 0 && infoCalls == providers, "1000 cached queue cycles: zero C++ heap/provider reads, single point reads and retained traversal/tie order")) return false;
    auto invalid = *actual; invalid.byte_size--;
    CachedParameterInput invalidProfile(&invalid);
    return check(!invalidProfile.valid(), "invalid cache table");
}
bool rejection() {
    Queue first(1), last(99); first.count = 1; first.points[0] = {0, 1, .5}; last.count = 1; last.points[0] = {0, 99, .5};
    Changes changes; changes.count = 2; changes.values = {&first, &last, nullptr, nullptr}; unsigned visited = 0;
    const auto visitor = [&](const GlobalParameterPoint &) { ++visited; return true; };
    for (unsigned test = 0; test < 11; ++test) {
        changes.count = 2; changes.nullQueue = false; last.count = 1; last.failure = -1; last.points[0] = {0, 99, .5}; last.points[1] = {0, 99, .5};
        if (test == 0) changes.count = -1;
        if (test == 1) changes.count = 1025;
        if (test == 2) changes.nullQueue = true;
        if (test == 3) last.count = -1;
        if (test == 4) last.count = 1024; // first + last exceeds total cap
        if (test == 5) last.failure = 0;
        if (test == 6) malformed(last.points[0].normalized, UINT64_C(0x7ff8000000000001));
        if (test == 7) malformed(last.points[0].normalized, UINT64_C(0x7ff0000000000000));
        if (test == 8) last.points[0].normalized = 1.001;
        if (test == 9) last.points[0].offset = 8;
        if (test == 10) { last.count = 2; last.points[0].offset = 3; last.points[1].offset = 2; }
        visited = 0;
        if (!check(!visitValidatedGlobalParameters(&changes, 8, visitor) && visited == 0, "whole raw/unknown/malformed prefix rejects before any visitor")) return false;
    }
    changes.count = 1; changes.nullQueue = false; visited = 0;
    if (!check(visitValidatedGlobalParameters(&changes, 0, visitor) && visited == 1, "zero-frame zero-offset flush")) return false;
    first.points[0].offset = 1; visited = 0;
    if (!check(!visitValidatedGlobalParameters(&changes, 0, visitor) && visited == 0 && !visitValidatedGlobalParameters(&changes, -1, visitor) && !visitValidatedGlobalParameters(&changes, 65537, visitor), "frame limits")) return false;
    first.points[0].offset = 0; first.count = 1024;
    for (auto &point : first.points) point = {0, 1, .5}; visited = 0;
    if (!check(visitValidatedGlobalParameters(&changes, 1, visitor) && visited == 1024, "full input capacity")) return false;
    visited = 0;
    if (!check(!visitValidatedGlobalParameters(&changes, 1, [&](const GlobalParameterPoint &) { return ++visited < 2; }) && visited == 2, "visitor failure retains caller scratch effects explicitly")) return false;
    return check(visitValidatedGlobalParameters(nullptr, 0, visitor), "absent queues");
}
}
int main() {
    if (!profiles() || !rejection()) return 1;
    std::puts("VST3 prepared parameter input: actual native32/64, explicit roles, complete raw validation/unknown/timing/capacity, single point reads and 1000 zero-C++heap cached cycles passed"); return 0;
}
