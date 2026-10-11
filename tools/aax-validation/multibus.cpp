#include "RavetAaxFactoryRegistration.hpp"
#include "AAX_IComponentDescriptor.h"
#include "AAX_IPropertyMap.h"
#include "TransportControllerHost.hpp"
#include "RavetAaxEffectParameters.hpp"
#include "RavetBusRouting.h"

#include "AAX_CString.h"
#include <array>
#include <cmath>
#include <dlfcn.h>
#include <iostream>
#include <map>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>
namespace {
void require(bool yes, const char *message) { if (!yes) throw std::runtime_error(message); }
struct Properties final : AAX_IPropertyMap {
 std::map<AAX_EProperty,AAX_CPropertyValue> values; std::map<AAX_EProperty,const void*> pointers;
 AAX_CBoolean GetProperty(AAX_EProperty p,AAX_CPropertyValue *v) const override { const auto i=values.find(p); if(i==values.end()||!v)return false; *v=i->second;return true; }
 AAX_CBoolean GetPointerProperty(AAX_EProperty p,const void **v) const override { const auto i=pointers.find(p);if(i==pointers.end()||!v)return false;*v=i->second;return true; }
 AAX_Result AddProperty(AAX_EProperty p,AAX_CPropertyValue v) override {values[p]=v;return AAX_SUCCESS;}
 AAX_Result AddPointerProperty(AAX_EProperty p,const void *v) override {pointers[p]=v;return AAX_SUCCESS;}
 AAX_Result AddPointerProperty(AAX_EProperty p,const char *v) override {return AddPointerProperty(p,static_cast<const void*>(v));}
 AAX_Result RemoveProperty(AAX_EProperty p) override {values.erase(p);pointers.erase(p);return AAX_SUCCESS;}
 AAX_Result AddPropertyWithIDArray(AAX_EProperty,const AAX_SPlugInIdentifierTriad*,uint32_t) override {return AAX_ERROR_UNIMPLEMENTED;}
 AAX_CBoolean GetPropertyWithIDArray(AAX_EProperty,const AAX_SPlugInIdentifierTriad**,uint32_t*)const override{return false;}
 IACFUnknown *GetIUnknown()override{return nullptr;}
};
struct Component final : AAX_IComponentDescriptor {
 mutable Properties properties; AAX_CProcessProc process=nullptr; AAX_CInstanceInitProc init=nullptr;
 int audioIn=-1,audioOut=-1,frames=-1,privateField=-1,privateSize=0,sidechain=-1;
 AAX_Result auxStatus=AAX_SUCCESS;std::vector<std::pair<int32_t,std::string>> aux;std::vector<char> busDeclarations;
 AAX_Result Clear()override{return AAX_SUCCESS;}
 AAX_Result AddAudioIn(AAX_CFieldIndex p)override{audioIn=p;return AAX_SUCCESS;}
 AAX_Result AddAudioOut(AAX_CFieldIndex p)override{audioOut=p;return AAX_SUCCESS;}
 AAX_Result AddAudioBufferLength(AAX_CFieldIndex p)override{frames=p;return AAX_SUCCESS;}
 AAX_Result AddSampleRate(AAX_CFieldIndex)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddClock(AAX_CFieldIndex)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddSideChainIn(AAX_CFieldIndex p)override{sidechain=p;busDeclarations.push_back('S');return AAX_SUCCESS;}
 AAX_Result AddDataInPort(AAX_CFieldIndex,uint32_t,AAX_EDataInPortType)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddAuxOutputStem(AAX_CFieldIndex p,int32_t stem,const char name[])override{if(p!=3)return AAX_ERROR_INVALID_ARGUMENT;aux.emplace_back(stem,name);busDeclarations.push_back('A');return auxStatus;}
 AAX_Result AddPrivateData(AAX_CFieldIndex p,int32_t n,uint32_t)override{privateField=p;privateSize=n;return AAX_SUCCESS;}
 AAX_Result AddTemporaryData(AAX_CFieldIndex,uint32_t)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddDmaInstance(AAX_CFieldIndex,AAX_IDma::EMode)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddMeters(AAX_CFieldIndex,const AAX_CTypeID*,uint32_t)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddMIDINode(AAX_CFieldIndex,AAX_EMIDINodeType,const char[],uint32_t)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddReservedField(AAX_CFieldIndex,uint32_t)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_IPropertyMap *NewPropertyMap()const override{return &properties;}
 AAX_IPropertyMap *DuplicatePropertyMap(AAX_IPropertyMap*)const override{return nullptr;}
 AAX_Result AddProcessProc_Native(AAX_CProcessProc p,AAX_IPropertyMap*,AAX_CInstanceInitProc i,AAX_CBackgroundProc,AAX_CSelector*)override{process=p;init=i;return AAX_SUCCESS;}
 AAX_Result AddProcessProc_TI(const char[],const char[],AAX_IPropertyMap*,const char[],const char[],AAX_CSelector*)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddProcessProc(AAX_IPropertyMap*,AAX_CSelector*,int32_t)override{return AAX_ERROR_UNIMPLEMENTED;}
};
struct Descriptor final : AAX_IEffectDescriptor {
 Component component; Properties properties; void *create=nullptr; int createCalls=0; AAX_Result createStatus=AAX_SUCCESS;
 AAX_IComponentDescriptor *NewComponentDescriptor()override{return &component;}
 AAX_Result AddComponent(AAX_IComponentDescriptor *p)override{return p==&component?AAX_SUCCESS:AAX_ERROR_INVALID_ARGUMENT;}
 AAX_Result AddName(const char*)override{return AAX_SUCCESS;}
 AAX_Result AddCategory(uint32_t)override{return AAX_SUCCESS;}
 AAX_Result AddCategoryBypassParameter(uint32_t,AAX_CParamID)override{return AAX_SUCCESS;}
 AAX_Result AddProcPtr(void *p,AAX_CProcPtrID kind)override{if(kind==kAAX_ProcPtrID_Create_EffectParameters){++createCalls;if(createStatus)return createStatus;create=p;}return AAX_SUCCESS;}
 AAX_IPropertyMap *NewPropertyMap()override{return &properties;}
 AAX_Result SetProperties(AAX_IPropertyMap*)override{return AAX_SUCCESS;}
 AAX_Result AddResourceInfo(AAX_EResourceType,const char*)override{return AAX_SUCCESS;}
 AAX_Result AddMeterDescription(AAX_CTypeID,const char*,AAX_IPropertyMap*)override{return AAX_SUCCESS;}
 AAX_Result AddControlMIDINode(AAX_CTypeID,AAX_EMIDINodeType,const char[],uint32_t)override{return AAX_SUCCESS;}
};
struct Collection final : AAX_ICollection {
 std::array<Descriptor,8> descriptors;size_t used=0;std::vector<std::pair<std::string,AAX_IEffectDescriptor*>> effects;Properties properties; AAX_Result effectStatus=AAX_SUCCESS;
 AAX_IEffectDescriptor *NewDescriptor()override{return used<descriptors.size()?&descriptors[used++]:nullptr;}
 AAX_Result AddEffect(const char *id,AAX_IEffectDescriptor *p)override{effects.emplace_back(id,p);return effectStatus;}
 AAX_Result SetManufacturerName(const char*)override{return AAX_SUCCESS;}
 AAX_Result AddPackageName(const char*)override{return AAX_SUCCESS;}
 AAX_Result SetPackageVersion(uint32_t)override{return AAX_SUCCESS;}
 AAX_IPropertyMap *NewPropertyMap()override{return &properties;}
 AAX_Result SetProperties(AAX_IPropertyMap*)override{return AAX_SUCCESS;}
 AAX_Result GetHostVersion(uint32_t *p)const override{if(p)*p=0;return AAX_SUCCESS;}
 AAX_IDescriptionHost *DescriptionHost()override{return nullptr;}
 const AAX_IDescriptionHost *DescriptionHost()const override{return nullptr;}
 IACFDefinition *HostDefinition()const override{return nullptr;}
};
using Model=ravet::aax::NativeAaxEffectParameters<2,32,false,1024,true,true>;
RavetNativeProcessorEntryV1 core{};RavetNativeParameterEntryV1 parametersTable{};RavetNativeCachedParameterEntryV1 cache{};RavetNativeAudioStateEntryV1 audio{};RavetNativeStateEntryV1 state{};RavetNativeTelemetryEntryV1 telemetry{};
void *lastHandle=nullptr;uint32_t nativeCalls=0;
const RavetNativeProcessorEntryV1 *coreOrigin(){return &core;}
const RavetNativeParameterEntryV1 *parameterOrigin(){return &parametersTable;}
const RavetNativeCachedParameterEntryV1 *cacheOrigin(){return &cache;}
int32_t recordProcess(void *p,const RavetNativeProcessorBusV1 *b,uint32_t count,uint32_t frames,const RavetNativeEventV1 *e,uint32_t n,const RavetNativeTransportV1 *t,RavetNativeEventV1 *out,uint32_t capacity,RavetNativeProcessorResultV1 *r){lastHandle=p;++nativeCalls;return ravet_bus_routing_processor_v1(32)->process(p,b,count,frames,e,n,t,out,capacity,r);}
void initializeTables(){
 require(ravet_bus_routing_processor_v1(32)&&ravet_bus_routing_audio_state_v1(32)&&ravet_bus_routing_state_v1(32)&&ravet_bus_routing_telemetry_v1(32),"actual complete native export acquisition, including valid telemetry schema");
 core=*ravet_bus_routing_processor_v1(32);core.process=recordProcess;
 audio=*ravet_bus_routing_audio_state_v1(32);cache=*audio.cached_parameters_entry();parametersTable=*cache.parameters_entry();
 parametersTable.processor_entry=coreOrigin;cache.parameters_entry=parameterOrigin;audio.cached_parameters_entry=cacheOrigin;
 state=*ravet_bus_routing_state_v1(32);state.parameters_entry=parameterOrigin;
 telemetry=*ravet_bus_routing_telemetry_v1(32);telemetry.processor_entry=coreOrigin;
}
constexpr const char *identities[]={"org.ravet.validation.busrouting.0","org.ravet.validation.busrouting.1","org.ravet.validation.busrouting.2","org.ravet.validation.busrouting.3","org.ravet.validation.busrouting.4","org.ravet.validation.busrouting.5","org.ravet.validation.busrouting.6","org.ravet.validation.busrouting.7"};
Model::PreparedProfile profile(unsigned variant){
 const uint32_t main=1+(variant&1),aux=1+((variant>>1)&1),detector=(variant&4)?2:1,monitor=3-detector;
 ravet::aax::AaxNativeAudioLayout layout{};layout.busCount=3;layout.maximumFrames=1024;layout.inputCapacity=main+1;layout.outputCapacity=main+aux;
 layout.buses[0]={main,main};layout.buses[detector]={1,0};layout.buses[monitor]={0,aux};layout.inputFirst[detector]=ravet::aax::AaxNativeAudioLayout::Sidechain;
 const auto stem=main==1?AAX_eStemFormat_Mono:AAX_eStemFormat_Stereo;return {layout,stem,stem};
}
template<unsigned Variant>AAX_IEffectParameters *AAX_CALLBACK create(){try{return new Model({identities[Variant],1+(Variant&1),0x52617674,0x52764273,0x52764231+Variant,0x52765331},&core,&audio,&cache,&state,profile(Variant));}catch(...){return nullptr;}}
using Create=AAX_IEffectParameters*(AAX_CALLBACK*)();
constexpr Create factories[]={create<0>,create<1>,create<2>,create<3>,create<4>,create<5>,create<6>,create<7>};
AAX_Result describe(AAX_ICollection &host,unsigned variant){
 auto *descriptor=host.NewDescriptor();if(!descriptor)return AAX_ERROR_NULL_OBJECT;
 auto status=descriptor->AddName("RAVET Bus Routing validation");if(status)return status;
 status=descriptor->AddProcPtr(reinterpret_cast<void*>(factories[variant]),kAAX_ProcPtrID_Create_EffectParameters);if(status)return status;
 auto *component=descriptor->NewComponentDescriptor();if(!component)return AAX_ERROR_NULL_OBJECT;
 using Context=Model::Context;
 status=component->AddPrivateData(AAX_FIELD_INDEX(Context,owner),sizeof(Model::Owner*));if(status)return status;
 status=component->AddAudioBufferLength(AAX_FIELD_INDEX(Context,frames));if(status)return status;
 status=component->AddAudioIn(AAX_FIELD_INDEX(Context,inputs));if(status)return status;
 status=component->AddAudioOut(AAX_FIELD_INDEX(Context,outputs));if(status)return status;
 const auto selected=profile(variant);
 // Consume source order explicitly; input-only buses do not create output stems.
 for(uint32_t bus=1;bus<selected.layout.busCount;++bus){
  if(selected.layout.buses[bus].inputs){status=component->AddSideChainIn(AAX_FIELD_INDEX(Context,sidechainFirst));if(status)return status;}
  if(selected.layout.buses[bus].outputs){status=component->AddAuxOutputStem(AAX_FIELD_INDEX(Context,outputs),selected.layout.buses[bus].outputs==1?AAX_eStemFormat_Mono:AAX_eStemFormat_Stereo,"Detector monitor");if(status)return status;}
 }
 auto *properties=component->NewPropertyMap();if(!properties)return AAX_ERROR_NULL_OBJECT;
 const std::pair<AAX_EProperty,AAX_CPropertyValue> records[]={{AAX_eProperty_ManufacturerID,0x52617674},{AAX_eProperty_ProductID,0x52764273},{AAX_eProperty_InputStemFormat,selected.inputStem},{AAX_eProperty_OutputStemFormat,selected.outputStem},{AAX_eProperty_PlugInID_Native,0x52764231+variant},{AAX_eProperty_CanBypass,1}};
 for(const auto &record:records){status=properties->AddProperty(record.first,record.second);if(status)return status;}
 status=component->AddProcessProc_Native<Context>(Model::process,properties,nullptr,nullptr);if(status)return status;
 status=descriptor->AddComponent(component);if(status)return status;return host.AddEffect(identities[variant],descriptor);
}
void contracts(){
 initializeTables();
 Collection failed;failed.descriptors[0].component.auxStatus=AAX_ERROR_UNIMPLEMENTED;
 require(describe(failed,4)==AAX_ERROR_UNIMPLEMENTED&&failed.effects.empty()&&failed.descriptors[0].component.aux.size()==1&&!failed.descriptors[0].component.process,"late aux host refusal retained before callback/effect publication; prior calls not rolled back");
 Collection collection;for(unsigned variant=0;variant<8;++variant)require(describe(collection,variant)==AAX_SUCCESS,"register all actual SDK main/aux variants");
 void *telemetryCache=telemetry.create_cache();require(telemetryCache,"authoritative transient telemetry cache");
 for(unsigned variant=0;variant<8;++variant){
  auto &descriptor=collection.descriptors[variant];auto &component=descriptor.component;const auto selected=profile(variant);const uint32_t main=selected.layout.buses[0].outputs,aux=selected.layout.outputCapacity-main;
  require(collection.effects[variant].first==identities[variant]&&component.privateField==0&&component.privateSize==sizeof(Model::Owner*)&&component.frames==1&&component.audioIn==2&&component.audioOut==3&&component.sidechain==4&&component.aux.size()==1&&component.aux[0].first==(aux==1?AAX_eStemFormat_Mono:AAX_eStemFormat_Stereo)&&component.aux[0].second=="Detector monitor"&&component.process,"registered exact SDK context/aux declaration");
  require(component.busDeclarations==((variant&4)?std::vector<char>{'A','S'}:std::vector<char>{'S','A'}),"SDK input-only/output-only declarations preserve source order");
  TransportControllerHost host;host.effect=identities[variant];host.input=selected.inputStem;host.output=selected.outputStem;
  std::unique_ptr<AAX_IEffectParameters> parameters(reinterpret_cast<Create>(descriptor.create)());require(parameters&&parameters->Initialize(host.unknown())==AAX_SUCCESS&&host.references==4&&host.registered.size()==2,"registered factory actual SDK Initialize prepares full native layout");
  Model::Owner *owner=nullptr;require(parameters->ResetFieldData(0,&owner,sizeof(owner))==AAX_SUCCESS&&owner,"actual published ready owner");
  std::array<std::array<float,1026>,4> input{};std::array<std::array<float,1026>,4> output{};
  float *inputs[]={input[0].data(),input[1].data(),input[2].data(),input[3].data()},*outputs[]={output[0].data(),output[1].data(),output[2].data(),output[3].data()};
  for(uint32_t ch=0;ch<main;++ch)input[ch].fill(float(ch+1));input[main].fill(.25f);for(auto &buffer:output)buffer.fill(77.f);
  int32_t frames=1024,sidechain=int32_t(main);Model::Context context{&owner,&frames,inputs,outputs,&sidechain};void *contexts[]={&context};
  auto invoke=[&]{component.process(contexts,contexts+1);};
  auto samples=[&]{RavetNativeTelemetryValueV1 value{};require(lastHandle&&telemetry.snapshot(lastHandle,telemetryCache,&value,1)==0&&value.id==1&&value.flags==0&&value.value==0.,"actual Rust transient telemetry snapshot");return value.samples;};
  auto oracle=[&](float gain){for(uint32_t ch=0;ch<main+aux;++ch){const float expected=(ch<main?float(ch+1):.25f)*gain;require(output[ch][0]==expected&&output[ch][1023]==expected&&output[ch][1024]==77.f,"independent actual main/detector/aux routing and tail oracle");}};
  invoke();require(owner->lastProcessStatus()==0&&samples()==1024,"actual initial multibus native callback");oracle(1.f);
  // Real native adapter must copy ALL inputs before ANY main/aux output write.
  context.outputs=outputs;outputs[0]=input[main].data();invoke();require(input[main][0]==1.f&&output[main][0]==.25f&&output[main][1023]==.25f,"cross-bus alias preserves original detector before main output writes");outputs[0]=output[0].data();input[main].fill(.25f);
  require(parameters->UpdateParameterNormalizedValue("p1",.125,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS,"queued physical gain .5");
  uint32_t bytes=0;require(parameters->GetChunkSize(0x52765331,&bytes)==AAX_SUCCESS&&bytes==32,"complete routing state format");
  alignas(AAX_SPlugInChunk) std::array<uint8_t,128> storage{};auto *chunk=new(storage.data()) AAX_SPlugInChunk{};chunk->fSize=int32_t(bytes);chunk->fChunkID=0x52765331;chunk->fManufacturerID=0x52617674;chunk->fProductID=0x52764273;chunk->fPlugInID=0x52764231+variant;
  require(parameters->GetChunk(chunk->fChunkID,chunk)==AAX_SUCCESS,"save acknowledged routing intent before queued AUDIO");invoke();oracle(.5f);
  require(parameters->UpdateParameterNormalizedValue("p1",.75,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS&&parameters->SetChunk(chunk->fChunkID,chunk)==AAX_SUCCESS&&parameters->UpdateParameterNormalizedValue("p1",.5,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS,"complete restore supersedes old queued gain, trailing gain2 survives");
  const auto before=samples();const auto calls=nativeCalls;outputs[main]=output[0].data()+1;invoke();require(owner->lastProcessStatus()==1&&samples()==before&&nativeCalls==calls,"late aux partial overlap rejects BEFORE native DSP/complete restore/queue");for(uint32_t ch=0;ch<main;++ch)require(output[ch][0]==0.f&&output[ch][1025]==77.f,"late map supported main silence preserves outside overlapping prefix union");outputs[main]=output[main].data();output[0][1024]=77.f;
  sidechain=-1;invoke();require(owner->lastProcessStatus()==1&&samples()==before&&nativeCalls==calls,"negative sidechain retains actual transient history and global queue");sidechain=0;invoke();require(owner->lastProcessStatus()==1&&samples()==before&&nativeCalls==calls,"sidechain cannot overlap accepted main input slice");sidechain=int32_t(main+1);invoke();require(owner->lastProcessStatus()==1&&samples()==before&&nativeCalls==calls,"sidechain index is bounded by explicit live input capacity");sidechain=int32_t(main);
  // Output metadata and callback fields are protected before any write.
  const auto originalFrames=frames;outputs[main]=reinterpret_cast<float*>(&frames);invoke();require(owner->lastProcessStatus()==1&&frames==originalFrames&&samples()==before&&nativeCalls==calls,"late aux cannot overwrite live SDK frame metadata");outputs[main]=output[main].data();
  invoke();oracle(2.f);require(samples()==before+1024,"valid follow-up alone advances native history with retained complete intent");
  auto *model=static_cast<Model*>(parameters.get());auto rejected=selected;rejected.layout.inputCapacity=main;require(model->configureProfileStopped(rejected)==AAX_ERROR_INVALID_ARGUMENT&&owner->ready()&&owner->layoutStopped().inputCapacity==main+1&&samples()==before+1024,"known late stopped layout rejection retains original ready configuration and history");
  host.input=AAX_eStemFormat_None;require(model->configureProfileStopped(selected)==AAX_ERROR_INVALID_ARGUMENT&&owner->ready()&&samples()==before+1024,"SDK stem mismatch before native prepare retains previous layout");host.input=selected.inputStem;
  require(model->configureProfileStopped(selected)==AAX_SUCCESS,"checked explicit stopped profile replacement");invoke();oracle(2.f);require(samples()==1024,"successful stopped prepare resets only transient frame history and restores complete intent");
  frames=0;context.inputs=nullptr;context.outputs=nullptr;context.sidechainFirst=nullptr;invoke();require(owner->lastProcessStatus()==0&&samples()==1024,"explicit zero-frame omitted audio does not infer/read absent dynamic sidechain selector");context.inputs=inputs;context.outputs=outputs;context.sidechainFirst=&sidechain;
  frames=1025;invoke();require(owner->lastProcessStatus()==1&&samples()==1024,"excessive block rejects before actual DSP");for(uint32_t ch=0;ch<main+aux;++ch)require(output[ch][0]==0.f&&output[ch][1023]==0.f&&output[ch][1024]==77.f,"supported whole main/aux prefix silence and tail preservation");frames=1024;
  auto backendUnsupported=selected;const auto detector=(variant&4)?2u:1u;backendUnsupported.layout.buses[detector].inputs=2;backendUnsupported.layout.inputCapacity=main+2;
  require(model->configureProfileStopped(backendUnsupported)==AAX_ERROR_NULL_OBJECT&&model->nativeStatus()==1&&!owner->ready()&&model->profileStopped().layout.inputCapacity==main+1,"authoritative Rust rejects unsupported detector width, retains original code and model intent profile while owner fails closed");
  invoke();require(owner->lastProcessStatus()==1&&output[main][0]==0.f,"failed backend setup never renders stale ready DSP");
  require(parameters->Uninitialize()==AAX_SUCCESS&&host.references==1&&host.registered.empty(),"actual SDK/native owner retirement");invoke();require(owner->lastProcessStatus()==1&&output[main][0]==0.f,"retained callback context fails closed after retirement");
  require(parameters->Initialize(host.unknown())==AAX_SUCCESS&&parameters->ResetFieldData(0,&owner,sizeof(owner))==AAX_SUCCESS,"fresh SDK initialization restores last accepted explicit profile after backend failure");invoke();oracle(2.f);require(samples()==1024,"fresh prepared profile restores complete intent and transient history");require(parameters->Uninitialize()==AAX_SUCCESS&&host.references==1,"fresh recovered SDK retirement");
 }
 telemetry.destroy_cache(telemetryCache);
}
}
int main(){try{contracts();std::cout<<"PASS registered AAX initialized native main/detector/aux routing eight layouts, alias/state/history/fault/retirement contracts\n";return 0;}catch(const std::exception &error){std::cerr<<error.what()<<'\n';return 1;}}
