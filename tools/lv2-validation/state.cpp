#include "lv2/core/lv2.h"
#include "lv2/atom/atom.h"
#include "lv2/state/state.h"
#include "lv2/urid/urid.h"
#include "RavetLv2Processor.hpp"
#include <array>
#include <atomic>
#include <cmath>
#include <cstdlib>
#include <cstring>
#include <dlfcn.h>
#include <iostream>
#include <limits>
#include <new>
#include <stdexcept>
#include <thread>
static std::atomic<uint64_t> allocations{0}, deletions{0};
void *operator new(size_t size) { allocations.fetch_add(1); if (auto *p = std::malloc(size ? size : 1)) return p; throw std::bad_alloc(); }
void *operator new[](size_t size) { return ::operator new(size); }
void operator delete(void *p) noexcept { deletions.fetch_add(1); std::free(p); }
void operator delete[](void *p) noexcept { ::operator delete(p); }
void operator delete(void *p, size_t) noexcept { ::operator delete(p); }
void operator delete[](void *p, size_t) noexcept { ::operator delete(p); }
void require(bool okay, const char *message) { if (!okay) throw std::runtime_error(message); }
struct Property {
  std::array<uint8_t, 64> bytes{};
  size_t size = 0;
  uint32_t key = 1, type = 2, flags = LV2_STATE_IS_POD | LV2_STATE_IS_PORTABLE;
  bool present = false;
  LV2_State_Status response = LV2_STATE_SUCCESS;
};
uint32_t map(void *handle, const char *uri) {
  ++*static_cast<uint32_t *>(handle);
  if (!std::strcmp(uri, "urn:ravet:gain:state")) return 1;
  if (!std::strcmp(uri, LV2_ATOM__Chunk)) return 2;
  return 0;
}
LV2_State_Status store(void *handle, uint32_t key, const void *bytes, size_t size, uint32_t type, uint32_t flags) {
  auto &p = *static_cast<Property *>(handle);
  if (p.response) return p.response;
  if (size > p.bytes.size()) return LV2_STATE_ERR_NO_SPACE;
  p.key = key; p.size = size; p.type = type; p.flags = flags; p.present = true;
  std::memcpy(p.bytes.data(), bytes, size); return LV2_STATE_SUCCESS;
}
const void *retrieve(void *handle, uint32_t key, size_t *size, uint32_t *type, uint32_t *flags) {
  const auto &p = *static_cast<Property *>(handle);
  if (!p.present || key != p.key) return nullptr;
  *size = p.size; *type = p.type; *flags = p.flags; return p.bytes.data();
}
uint32_t word(const Property &p, size_t offset) {
  return uint32_t(p.bytes[offset]) | (uint32_t(p.bytes[offset + 1]) << 8) |
         (uint32_t(p.bytes[offset + 2]) << 16) | (uint32_t(p.bytes[offset + 3]) << 24);
}
float scalar(const Property &p, size_t offset) { const auto bits = word(p, offset); float value; std::memcpy(&value, &bits, 4); return value; }
void scalar(Property &p, size_t offset, float value) {
  uint32_t bits; std::memcpy(&bits, &value, 4);
  for (uint32_t i = 0; i < 4; ++i) p.bytes[offset + i] = uint8_t(bits >> (8 * i));
}
void expectState(const Property &p, float gain, float bypass) {
  require(p.present && p.key == 1 && p.type == 2 && p.flags == 3 && p.size == 32, "canonical portable state property");
  require(!std::memcmp(p.bytes.data(), "RAVETGN\0", 8) && word(p, 8) == 2 && word(p, 12) == 2 && word(p, 16) == 1 && word(p, 24) == 2, "consumer schema codec identity");
  require(scalar(p, 20) == gain && scalar(p, 28) == bypass, "complete state values");
}
int main(int argc, char **argv) {
  try {
    require(argc == 2, "binary argument");
    auto *library = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL); require(library, "actual module load");
    auto descriptor = reinterpret_cast<const LV2_Descriptor *(*)(uint32_t)>(dlsym(library, "lv2_descriptor"));
    require(descriptor && !descriptor(2), "bounded descriptors");
    const auto coreFactory = reinterpret_cast<const RavetNativeProcessorEntryV1 *(*)(uint32_t)>(dlsym(library, "ravet_native_processor_v1"));
    const auto cacheFactory = reinterpret_cast<const RavetNativeCachedParameterEntryV1 *(*)(uint32_t)>(dlsym(library, "ravet_native_cached_parameters_v1"));
    const auto stateFactory = reinterpret_cast<const RavetNativeStateEntryV1 *(*)(uint32_t)>(dlsym(library, "ravet_native_state_v1"));
    require(coreFactory && cacheFactory && stateFactory, "actual native companions");
    {
      float untouched = 99.f;
      const LV2_Feature *none[] = {nullptr};
      auto missingDestroy = *stateFactory(32); missingDestroy.destroy = nullptr;
      ravet::native::NativeLv2Processor<2> invalidTable(coreFactory(32), cacheFactory(32), &missingDestroy, 2, 48000., "urn:ravet:gain:state", none);
      require(!invalidTable.valid(), "missing publication destroy rejects");
      invalidTable.connect(2, &untouched); invalidTable.activate(); invalidTable.run(1); invalidTable.deactivate();
      for (uint32_t channels : {0u, 3u, UINT32_MAX}) {
        ravet::native::NativeLv2Processor<2> invalidChannels(coreFactory(32), cacheFactory(32), stateFactory(32), channels, 48000., "urn:ravet:gain:state", none);
        require(!invalidChannels.valid(), "invalid helper channels reject");
        invalidChannels.connect(0, &untouched); invalidChannels.connect(channels, &untouched); invalidChannels.activate(); invalidChannels.run(1); invalidChannels.deactivate();
      }
      require(untouched == 99.f, "invalid helper callbacks leave storage unchanged");
    }
    for (uint32_t index = 0; index < 2; ++index) {
      const auto *d = descriptor(index); const uint32_t channels = index + 1;
      require(!d->extension_data(nullptr) && !d->extension_data("unknown"), "unknown extension rejected");
      const auto *state = static_cast<const LV2_State_Interface *>(d->extension_data(LV2_STATE__interface));
      require(state && state->save && state->restore, "state interface advertised");
      const LV2_Feature *empty[] = {nullptr}; Property saved;
      auto missing = d->instantiate(d, 48000., "", empty); require(missing, "URID map optional for processing");
      require(state->save(missing, store, &saved, 0, empty) == LV2_STATE_ERR_NO_FEATURE, "state requires cached URID map");
      d->cleanup(missing);
      uint32_t mapCalls = 0; LV2_URID_Map mapper{&mapCalls, map}; LV2_Feature feature{LV2_URID__map, &mapper};
      const LV2_Feature *features[] = {&feature, nullptr};
      auto instance = d->instantiate(d, 48000., "", features); require(instance && mapCalls == 2, "instantiate maps once on control");
      std::array<float, 65> left{}, right{}, outLeft{}, outRight{};
      left.fill(.25f); right.fill(-.5f); outLeft.fill(99.f); outRight.fill(99.f);
      d->connect_port(instance, 0, left.data()); if (channels == 2) d->connect_port(instance, 1, right.data());
      d->connect_port(instance, channels, outLeft.data()); if (channels == 2) d->connect_port(instance, channels + 1, outRight.data());
      float gain = .5f, bypass = 0.f;
      d->connect_port(instance, 2 * channels, &gain); d->connect_port(instance, 2 * channels + 1, &bypass);
      d->connect_port(instance, 999, nullptr); d->activate(instance);
      for (uint32_t i = 0; i < 5; ++i) d->run(instance, 64);
      require(!state->save(instance, store, &saved, 0, features), "save processed base state"); expectState(saved, .5f, 0.f);
      Property restored = saved; scalar(restored, 20, 2.f); scalar(restored, 28, 1.f);
      require(!state->restore(instance, retrieve, &restored, 0, features), "complete stopped restore");
      gain = std::numeric_limits<float>::quiet_NaN(); d->run(instance, 64);
      require(outLeft[0] == 0.f && outLeft[64] == 99.f, "malformed controls fault silence and tails");
      require(!state->save(instance, store, &saved, 0, features), "rejected controls preserve publication"); expectState(saved, 2.f, 1.f);
      gain = .5f; d->run(instance, 64);
      require(outLeft[0] == .25f && (channels == 1 || outRight[0] == -.5f), "unchanged host controls do not overwrite recall");
      require(!state->save(instance, store, &saved, 0, features), "restored state reflected"); expectState(saved, 2.f, 1.f);
      // Each subsequent host control change wins only for its own parameter.
      gain = .25f; d->run(instance, 0);
      require(!state->save(instance, store, &saved, 0, features), "zero-frame control publication"); expectState(saved, .25f, 1.f);
      bypass = 1.f; d->run(instance, 0); bypass = 0.f; d->run(instance, 0);
      require(!state->save(instance, store, &saved, 0, features), "changed bypass control wins"); expectState(saved, .25f, 0.f);
      // Reject every codec/type/flag failure before changing saved state.
      for (uint32_t bad = 0; bad < 6; ++bad) {
        auto malformed = saved;
        if (bad == 0) malformed.type = 999;
        if (bad == 1) malformed.flags = LV2_STATE_IS_POD;
        if (bad == 2) malformed.size = 31;
        if (bad == 3) malformed.bytes[0] ^= 1;
        if (bad == 4) scalar(malformed, 20, std::numeric_limits<float>::quiet_NaN());
        if (bad == 5) malformed.bytes[24] = 1;
        require(state->restore(instance, retrieve, &malformed, 0, features) != LV2_STATE_SUCCESS, "malformed restore rejected");
        Property after; require(!state->save(instance, store, &after, 0, features), "save after rejected restore");
        require(after.bytes == saved.bytes, "malformed restore atomic");
      }
      auto refusal = saved; refusal.response = LV2_STATE_ERR_NO_SPACE;
      require(state->save(instance, store, &refusal, 0, features) == LV2_STATE_ERR_NO_SPACE, "host store refusal propagated");
      Property absent; require(!state->restore(instance, retrieve, &absent, 0, features), "empty-map restore defaults");
      d->run(instance, 0); require(!state->save(instance, store, &saved, 0, features), "default save"); expectState(saved, 1.f, 0.f);
      for (uint32_t i = 0; i < 5; ++i) d->run(instance, 64);
      require(outLeft[63] == .25f, "default recall rendered");
      // Disconnected input / overlapping outputs must not consume state or controls.
      auto pending = saved; scalar(pending, 20, 3.f);
      require(!state->restore(instance, retrieve, &pending, 0, features), "pending recall");
      gain = 2.f; d->connect_port(instance, 0, nullptr); d->run(instance, 64);
      require(outLeft[0] == 0.f, "disconnected input silence");
      Property after; require(!state->save(instance, store, &after, 0, features), "save disconnected fault"); expectState(after, 3.f, 0.f);
      d->connect_port(instance, 0, left.data()); gain = .25f; d->run(instance, 0);
      require(!state->save(instance, store, &after, 0, features), "pending restore not consumed by rejected block"); expectState(after, 3.f, 0.f);
      if (channels == 2) {
        d->connect_port(instance, channels + 1, outLeft.data() + 1); gain = 2.f; d->run(instance, 64);
        require(!state->save(instance, store, &after, 0, features), "save overlap rejection"); expectState(after, 3.f, 0.f);
        d->connect_port(instance, channels + 1, outRight.data()); gain = .25f;
      }
      // Zero frames permit disconnected samples; controls remain mandatory.
      d->connect_port(instance, 0, nullptr); d->connect_port(instance, channels, nullptr); d->run(instance, 0);
      d->connect_port(instance, 0, left.data()); d->connect_port(instance, channels, outLeft.data());
      d->connect_port(instance, 2 * channels, reinterpret_cast<void *>(uintptr_t(&gain) + 1));
      d->run(instance, 64); require(outLeft[0] == 0.f, "misaligned control rejection without dereference");
      d->connect_port(instance, 2 * channels, &gain);
      d->connect_port(instance, channels, reinterpret_cast<void *>(uintptr_t(outLeft.data()) + 1));
      d->run(instance, 64);
      require(!state->save(instance, store, &after, 0, features), "save misaligned output rejection"); expectState(after, 3.f, 0.f);
      d->connect_port(instance, channels, outLeft.data());
      // Oversized blocks clear only the supported prefix, preserving its tail.
      std::array<float, 65537> largeInput{}, largeOutput{};
      largeInput.fill(.25f); largeOutput.fill(99.f);
      d->connect_port(instance, 0, largeInput.data()); d->connect_port(instance, channels, largeOutput.data());
      if (channels == 2) { d->connect_port(instance, 1, largeInput.data()); d->connect_port(instance, channels + 1, nullptr); }
      d->run(instance, 65537);
      require(largeOutput[0] == 0.f && largeOutput[65535] == 0.f && largeOutput[65536] == 99.f, "oversized rejection bounded silence and tail");
      d->connect_port(instance, 0, left.data()); d->connect_port(instance, channels, outLeft.data());
      if (channels == 2) { d->connect_port(instance, 1, right.data()); d->connect_port(instance, channels + 1, outRight.data()); }
      const auto beforeNew = allocations.load(), beforeDelete = deletions.load();
      for (uint32_t cycle = 0; cycle < 2000; ++cycle) {
        gain = cycle % 2 ? 2.f : .5f; bypass = cycle % 2 ? 1.f : 0.f;
        d->run(instance, 64); Property current;
        require(!state->save(instance, store, &current, 0, features), "bounded repeated save");
        expectState(current, gain, bypass);
      }
      require(allocations.load() == beforeNew && deletions.load() == beforeDelete && mapCalls == 2, "2000 run/save cycles no C++ heap or URI mapper calls");
      // LV2 allows save concurrently with audio; only publication is observed.
      std::atomic<bool> start{false}, done{false}, coherent{true}; std::atomic<uint32_t> successes{0};
      std::thread saver([&] {
        while (!start.load()) std::this_thread::yield();
        do {
          Property current;
          if (!state->save(instance, store, &current, 0, features)) {
            const auto g = scalar(current, 20), b = scalar(current, 28);
            if (!((g == 2.f && b == 1.f) || (g == .5f && b == 0.f))) coherent.store(false);
            ++successes;
          }
        } while (!done.load());
      });
      start.store(true);
      for (uint32_t cycle = 0; cycle < 10000; ++cycle) {
        gain = cycle % 2 ? 2.f : .5f; bypass = cycle % 2 ? 1.f : 0.f; d->run(instance, 1);
      }
      done.store(true); saver.join();
      require(coherent.load() && successes.load(), "concurrent saves observe whole schema state");
      d->deactivate(instance); d->cleanup(instance);
    }
    dlclose(library);
    std::cout << "LV2 actual-binary native schemas/publication/state, atomic controls, 4000 allocation-free C++ run/save cycles and concurrent save coherence passed\n";
  } catch (const std::exception &error) { std::cerr << error.what() << '\n'; return 1; }
}
