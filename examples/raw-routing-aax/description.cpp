// Public reference descriptor using the externally supplied SDK API.
#include "public.sdk/source/vst/aaxwrapper/aaxwrapper_description.h"
#include <cstring>
namespace {
AAX_MIDI_Desc midi[]={{"MIDI Input",0xffff},{nullptr,0}};
AAX_Aux_Desc auxiliary[]={{"Detector monitor",2},{nullptr,0}};
AAX_Plugin_Desc variants[] = {
 {"org.ravet.rawrouting.mono","RAVET Raw Routing",0x52765231,0,1,1,0,midi,nullptr,nullptr,0},
 {"org.ravet.rawrouting.stereo","RAVET Raw Routing",0x52765232,0,2,2,0,midi,nullptr,nullptr,0},
 {"org.ravet.rawrouting.aux-detector-first","RAVET Raw Routing",0x52765233,0,2,2,1,midi,auxiliary,nullptr,0},
 {"org.ravet.rawrouting.aux-output-first","RAVET Raw Routing",0x52765234,0,2,2,1,midi,auxiliary,nullptr,0},
 {"org.ravet.rawrouting.delaymono","RAVET Raw Routing",0x52765235,0,1,1,0,midi,nullptr,nullptr,8},
 {"org.ravet.rawrouting.delaystereo","RAVET Raw Routing",0x52765236,0,2,2,0,midi,nullptr,nullptr,8},
 {nullptr}};
AAX_Effect_Desc description = {"RAVET",    "RAVET Raw Routing", 0x52617674,
                               0x52765277, "Effect",         {0},
                               1,          nullptr,      variants};
} // namespace
int *forceLinkAAXWrapper = &AAXWrapper_linkAnchor;
__attribute__((visibility("default"))) AAX_Effect_Desc *AAXWrapper_GetDescription() {
  const Steinberg::FUID id(0xA24A9E23, 0x20CC40C0, 0x821E74D1, 0x7EBBA308);
  std::memcpy(description.mVST3PluginID, static_cast<const char *>(id),
              sizeof(description.mVST3PluginID));
  return &description;
}
