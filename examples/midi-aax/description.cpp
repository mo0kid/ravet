// Public reference descriptor using the externally supplied SDK API.
#include "public.sdk/source/vst/aaxwrapper/aaxwrapper_description.h"
#include <cstring>
namespace {
AAX_MIDI_Desc midi[]={{"MIDI Input",0xffff},{nullptr,0}};
AAX_Plugin_Desc variants[] = {{"org.ravet.midisine.mono", "RAVET MIDI Sine", 0x52764d31,
                               0, 0, 1, 0, midi, nullptr, nullptr, 0},
                              {"org.ravet.midisine.stereo", "RAVET MIDI Sine",
                               0x52764d32, 0, 0, 2, 0, midi, nullptr,
                               nullptr, 0},
                              {nullptr}};
AAX_Effect_Desc description = {"RAVET",    "RAVET MIDI Sine", 0x52617674,
                               0x52764d53, "Instrument",         {0},
                               1,          nullptr,      variants};
} // namespace
int *forceLinkAAXWrapper = &AAXWrapper_linkAnchor;
__attribute__((visibility("default"))) AAX_Effect_Desc *AAXWrapper_GetDescription() {
  const Steinberg::FUID id(0xA14A9E23, 0x20CC40C0, 0x821E74D1, 0x7EBBA308);
  std::memcpy(description.mVST3PluginID, static_cast<const char *>(id),
              sizeof(description.mVST3PluginID));
  return &description;
}
