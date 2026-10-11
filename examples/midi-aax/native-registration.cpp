#include "MidiParameters.hpp"
#include "AAX_ICollection.h"
#include "AAX_IEffectDescriptor.h"
#include "AAX_IComponentDescriptor.h"
#include "AAX_IPropertyMap.h"
#include "AAX_Properties.h"
namespace {
AAX_IEffectParameters *AAX_CALLBACK mono(){try{return createMidiAax(1);}catch(...){return nullptr;}}
AAX_IEffectParameters *AAX_CALLBACK stereo(){try{return createMidiAax(2);}catch(...){return nullptr;}}
AAX_Result variant(AAX_ICollection &host,uint32_t channels){
 auto *descriptor=host.NewDescriptor();if(!descriptor)return AAX_ERROR_NULL_OBJECT;
 auto status=descriptor->AddName("RAVET MIDI Sine");if(status)return status;
 status=descriptor->AddCategory(AAX_ePlugInCategory_SWGenerators);if(status)return status;
 status=descriptor->AddProcPtr(reinterpret_cast<void*>(channels==1?mono:stereo),kAAX_ProcPtrID_Create_EffectParameters);if(status)return status;
 auto *component=descriptor->NewComponentDescriptor();if(!component)return AAX_ERROR_NULL_OBJECT;
 using Model=RavetMidiAaxParameters;using Context=Model::Context;
 status=component->AddPrivateData(AAX_FIELD_INDEX(Context,owner),sizeof(Model::Owner*));if(status)return status;
 status=component->AddAudioBufferLength(AAX_FIELD_INDEX(Context,frames));if(status)return status;
 status=component->AddAudioOut(AAX_FIELD_INDEX(Context,outputs));if(status)return status;
 status=component->AddMIDINode(AAX_FIELD_INDEX(Context,notes),AAX_eMIDINodeType_LocalInput,"MIDI Input",0xffff);if(status)return status;
 auto *properties=component->NewPropertyMap();if(!properties)return AAX_ERROR_NULL_OBJECT;
 const std::pair<AAX_EProperty,AAX_CPropertyValue> records[]{
  {AAX_eProperty_ManufacturerID,0x52617674},{AAX_eProperty_ProductID,0x52764d53},
  {AAX_eProperty_CanBypass,0},{AAX_eProperty_LatencyContribution,0},{AAX_eProperty_Constraint_MultiMonoSupport,0},
  {AAX_eProperty_InputStemFormat,AAX_eStemFormat_None},
  {AAX_eProperty_OutputStemFormat,channels==1?AAX_eStemFormat_Mono:AAX_eStemFormat_Stereo},
  {AAX_eProperty_PlugInID_Native,channels==1?0x52764d31:0x52764d32}};
 for(const auto &record:records){status=properties->AddProperty(record.first,record.second);if(status)return status;}
 status=component->AddProcessProc_Native<Context>(Model::process,properties,nullptr,nullptr);if(status)return status;
 status=descriptor->AddComponent(component);if(status)return status;
 return host.AddEffect(channels==1?"org.ravet.midisine.mono":"org.ravet.midisine.stereo",descriptor);
}
}
#if defined(_WIN32)
__declspec(dllexport)
#else
__attribute__((visibility("default")))
#endif
AAX_Result GetEffectDescriptions(AAX_ICollection *host){
 if(!host)return AAX_ERROR_NULL_ARGUMENT;
 // Exact first refusal is retained. Prior registration calls have no rollback.
 auto status=host->SetManufacturerName("RAVET");if(status)return status;
 status=host->AddPackageName("RAVET MIDI Sine");if(status)return status;
 status=host->SetPackageVersion(1);if(status)return status;
 status=variant(*host,1);if(status)return status;return variant(*host,2);
}
