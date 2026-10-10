#include "RavetPerformance.h"
#include "RavetLv2Processor.hpp"
#include <cstring>
#include <new>
namespace {
using Instance = ravet::native::NativeLv2Processor<1, 2, true, true>;
LV2_Handle instantiate(const LV2_Descriptor *descriptor, double rate, const char *, const LV2_Feature *const *features) {
  if (!descriptor || !descriptor->URI) return nullptr;
  uint32_t channels;
  if (!std::strcmp(descriptor->URI, "urn:ravet:performance-sine:mono")) channels = 1;
  else if (!std::strcmp(descriptor->URI, "urn:ravet:performance-sine:stereo")) channels = 2;
  else return nullptr;
  auto *instance = new (std::nothrow) Instance(ravet_performance_processor_v1(32), ravet_performance_cached_parameters_v1(32),
                                             ravet_performance_state_v1(32), channels, rate, "urn:ravet:performance-sine:state", features);
  if (instance && !instance->valid()) { delete instance; instance = nullptr; }
  return instance;
}
void connect(LV2_Handle handle, uint32_t port, void *data) { static_cast<Instance *>(handle)->connect(port, data); }
void activate(LV2_Handle handle) { static_cast<Instance *>(handle)->activate(); }
void run(LV2_Handle handle, uint32_t frames) { static_cast<Instance *>(handle)->run(frames); }
void deactivate(LV2_Handle handle) { static_cast<Instance *>(handle)->deactivate(); }
void cleanup(LV2_Handle handle) { delete static_cast<Instance *>(handle); }
LV2_State_Status save(LV2_Handle instance, LV2_State_Store_Function store, LV2_State_Handle handle,
                      uint32_t, const LV2_Feature *const *) { return static_cast<Instance *>(instance)->save(store, handle); }
LV2_State_Status restore(LV2_Handle instance, LV2_State_Retrieve_Function retrieve, LV2_State_Handle handle,
                         uint32_t, const LV2_Feature *const *) { return static_cast<Instance *>(instance)->restore(retrieve, handle); }
const LV2_State_Interface stateInterface{save, restore};
const void *extension(const char *uri) { return uri && !std::strcmp(uri, LV2_STATE__interface) ? &stateInterface : nullptr; }
const LV2_Descriptor descriptors[] = {
    {"urn:ravet:performance-sine:mono", instantiate, connect, activate, run, deactivate, cleanup, extension},
    {"urn:ravet:performance-sine:stereo", instantiate, connect, activate, run, deactivate, cleanup, extension}};
}
LV2_SYMBOL_EXPORT const LV2_Descriptor *lv2_descriptor(uint32_t index) { return index < 2 ? &descriptors[index] : nullptr; }
