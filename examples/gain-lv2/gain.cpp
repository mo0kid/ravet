// Native reference wrapper; LV2 SDK headers remain external.
#include "RavetGain.h"
#include "lv2/core/lv2.h"
#include <algorithm>
#include <array>
#include <cmath>
#include <cstring>
#include <new>
namespace {
constexpr uint32_t maximumFrames = 65536;
struct Instance {
  RavetGain *dsp = nullptr;
  uint32_t channels = 0;
  std::array<float *, 6> ports{};
  bool active = false;
  bool failed = false;
  float requestedGain = 1.f;
  uint32_t requestedBypass = 0;
};
LV2_Handle instantiate(const LV2_Descriptor *descriptor, double rate,
                       const char *, const LV2_Feature *const *) {
  if (!descriptor || !std::isfinite(rate) || rate <= 0.)
    return nullptr;
  auto *instance = new (std::nothrow) Instance;
  if (!instance)
    return nullptr;
  instance->channels =
      std::strcmp(descriptor->URI, "urn:ravet:gain:mono") == 0 ? 1 : 2;
  instance->dsp = ravet_gain_create();
  if (!instance->dsp ||
      ravet_gain_prepare(instance->dsp, rate, instance->channels,
                         maximumFrames) != 0) {
    ravet_gain_destroy(instance->dsp);
    delete instance;
    return nullptr;
  }
  return instance;
}
void connect(LV2_Handle handle, uint32_t port, void *data) {
  auto &instance = *static_cast<Instance *>(handle);
  if (port < 2 * instance.channels + 2)
    instance.ports[port] = static_cast<float *>(data);
}
void activate(LV2_Handle handle) {
  auto &instance = *static_cast<Instance *>(handle);
  ravet_gain_reset(instance.dsp);
  instance.active = !instance.failed;
}
void silence(Instance &instance, uint32_t frames) {
  for (uint32_t c = 0; c < instance.channels; ++c)
    if (auto *out = instance.ports[instance.channels + c])
      std::fill_n(out, frames, 0.f);
}
void run(LV2_Handle handle, uint32_t frames) {
  auto &instance = *static_cast<Instance *>(handle);
  const auto control = 2 * instance.channels;
  auto *gainPort = instance.ports[control];
  auto *bypassPort = instance.ports[control + 1];
  if (!instance.active || frames > maximumFrames || !gainPort || !bypassPort ||
      !std::isfinite(*gainPort) || !std::isfinite(*bypassPort)) {
    silence(instance, frames);
    return;
  }
  const std::array<RavetGainEvent, 2> events{
      {{0, 1, std::clamp(*gainPort, 0.f, 4.f)},
       {0, 2, *bypassPort >= 0.5f ? 1.f : 0.f}}};
  std::array<const float *, 2> inputs{};
  std::array<float *, 2> outputs{};
  for (uint32_t c = 0; c < instance.channels; ++c) {
    inputs[c] = instance.ports[c];
    outputs[c] = instance.ports[instance.channels + c];
  }
  const auto result = ravet_gain_process(
      instance.dsp, inputs.data(), outputs.data(), instance.channels, frames,
      events.data(), events.size(), &instance.requestedGain,
      &instance.requestedBypass);
  if (result == 2) {
    instance.failed = true;
    instance.active = false;
  }
}
void deactivate(LV2_Handle handle) {
  static_cast<Instance *>(handle)->active = false;
}
void cleanup(LV2_Handle handle) {
  auto *instance = static_cast<Instance *>(handle);
  ravet_gain_destroy(instance->dsp);
  delete instance;
}
const void *extension(const char *) { return nullptr; }
const LV2_Descriptor descriptors[] = {
    {"urn:ravet:gain:mono", instantiate, connect, activate, run, deactivate,
     cleanup, extension},
    {"urn:ravet:gain:stereo", instantiate, connect, activate, run, deactivate,
     cleanup, extension}};
} // namespace
LV2_SYMBOL_EXPORT const LV2_Descriptor *lv2_descriptor(uint32_t index) {
  return index < 2 ? &descriptors[index] : nullptr;
}
