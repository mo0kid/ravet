// Public reference descriptor using the externally supplied SDK API.
#include "public.sdk/source/vst/aaxwrapper/aaxwrapper_description.h"
#include <cstring>
namespace {
AAX_Plugin_Desc variants[] = {{"org.ravet.gain.mono", "RAVET Gain", 0x52764731,
                               0, 1, 1, 0, nullptr, nullptr, nullptr, 0},
                              {"org.ravet.gain.stereo", "RAVET Gain",
                               0x52764732, 0, 2, 2, 0, nullptr, nullptr,
                               nullptr, 0},
                              {nullptr}};
AAX_Effect_Desc description = {"RAVET",    "RAVET Gain", 0x52617674,
                               0x5276476e, "Fx",         {0},
                               1,          nullptr,      variants};
} // namespace
int *forceLinkAAXWrapper = &AAXWrapper_linkAnchor;
AAX_Effect_Desc *AAXWrapper_GetDescription() {
  const Steinberg::FUID id(0xAFF1AE72, 0xCC374810, 0x89136FEE, 0xF4348390);
  std::memcpy(description.mVST3PluginID, static_cast<const char *>(id),
              sizeof(description.mVST3PluginID));
  return &description;
}
