// Public reference descriptor using the externally supplied SDK API.
#include "public.sdk/source/vst/aaxwrapper/aaxwrapper_description.h"
#include <cstring>
namespace {
AAX_Plugin_Desc variants[] = {{"org.ravet.gainmeters.mono", "RAVET Gain Meters", 0x524d4731,
                               0, 1, 1, 0, nullptr, nullptr, nullptr, 0},
                              {"org.ravet.gainmeters.stereo", "RAVET Gain Meters",
                               0x524d4732, 0, 2, 2, 0, nullptr, nullptr,
                               nullptr, 0},
                              {nullptr}};
AAX_Effect_Desc description = {"RAVET",    "RAVET Gain Meters", 0x52617674,
                               0x52764d47, "Fx",         {0},
                               1,          nullptr,      variants};
} // namespace
int *forceLinkAAXWrapper = &AAXWrapper_linkAnchor;
AAX_Effect_Desc *AAXWrapper_GetDescription() {
  const Steinberg::FUID id(0x1A3FE771, 0xAA374BB1, 0x8BA41C17, 0x1140AB68);
  std::memcpy(description.mVST3PluginID, static_cast<const char *>(id),
              sizeof(description.mVST3PluginID));
  return &description;
}
