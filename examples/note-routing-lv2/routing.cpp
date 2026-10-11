#include "RavetNoteRouting.h"
#include "RavetLv2Processor.hpp"
#include <cstring>
#include <new>
namespace {
using Instance=ravet::native::NativeLv2Processor<1,2,true,true,1024,1024>;
LV2_Handle instantiate(const LV2_Descriptor* descriptor,double rate,const char*,const LV2_Feature* const* features){
 if(!descriptor||!descriptor->URI)return nullptr;uint32_t channels=0;
 if(!std::strcmp(descriptor->URI,"urn:ravet:note-routing:mono"))channels=1;
 else if(!std::strcmp(descriptor->URI,"urn:ravet:note-routing:stereo"))channels=2;
 else return nullptr;
 auto* instance=new(std::nothrow)Instance(ravet_note_routing_processor_v1(32),ravet_note_routing_cached_parameters_v1(32),ravet_note_routing_state_v1(32),channels,rate,"urn:ravet:note-routing:state",features);
 if(instance&&!instance->valid()){delete instance;instance=nullptr;}return instance;
}
void connect(LV2_Handle instance,uint32_t port,void* data){static_cast<Instance*>(instance)->connect(port,data);}
void activate(LV2_Handle instance){static_cast<Instance*>(instance)->activate();}
void run(LV2_Handle instance,uint32_t frames){static_cast<Instance*>(instance)->run(frames);}
void deactivate(LV2_Handle instance){static_cast<Instance*>(instance)->deactivate();}
void cleanup(LV2_Handle instance){delete static_cast<Instance*>(instance);}
LV2_State_Status save(LV2_Handle instance,LV2_State_Store_Function store,LV2_State_Handle handle,uint32_t,const LV2_Feature* const*){return static_cast<Instance*>(instance)->save(store,handle);}
LV2_State_Status restore(LV2_Handle instance,LV2_State_Retrieve_Function retrieve,LV2_State_Handle handle,uint32_t,const LV2_Feature* const*){return static_cast<Instance*>(instance)->restore(retrieve,handle);}
const LV2_State_Interface state{save,restore};
const void* extension(const char* uri){return uri&&!std::strcmp(uri,LV2_STATE__interface)?&state:nullptr;}
const LV2_Descriptor descriptors[]{
 {"urn:ravet:note-routing:mono",instantiate,connect,activate,run,deactivate,cleanup,extension},
 {"urn:ravet:note-routing:stereo",instantiate,connect,activate,run,deactivate,cleanup,extension}};
}
LV2_SYMBOL_EXPORT const LV2_Descriptor* lv2_descriptor(uint32_t index){return index<2?&descriptors[index]:nullptr;}
