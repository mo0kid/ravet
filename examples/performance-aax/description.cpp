// Public reference descriptor using the externally supplied SDK API.
#include "public.sdk/source/vst/aaxwrapper/aaxwrapper_description.h"
#include <cstring>
namespace {
AAX_MIDI_Desc midi[]={{"MIDI Input",0xffff},{nullptr,0}};
AAX_Plugin_Desc variants[] = {{"org.ravet.performance.mono", "RAVET Performance", 0x52765031,
                               0, 0, 1, 0, midi, nullptr, nullptr, 0},
                              {"org.ravet.performance.stereo", "RAVET Performance",
                               0x52765032, 0, 0, 2, 0, midi, nullptr,
                               nullptr, 0},
                              {nullptr}};
AAX_Effect_Desc description = {"RAVET",    "RAVET Performance", 0x52617674,
                               0x52765073, "Instrument",         {0},
                               1,          nullptr,      variants};
} // namespace
int *forceLinkAAXWrapper = &AAXWrapper_linkAnchor;
AAX_Effect_Desc *AAXWrapper_GetDescription() {
  const Steinberg::FUID id(0xCF831A21, 0x64C94718, 0xA66CC589, 0x3461309B);
  std::memcpy(description.mVST3PluginID, static_cast<const char *>(id),
              sizeof(description.mVST3PluginID));
  return &description;
}
