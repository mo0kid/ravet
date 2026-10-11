#include "RavetAaxFactoryRegistration.hpp"
#include "AAX_IComponentDescriptor.h"
#include "AAX_IPropertyMap.h"
#include "TransportControllerHost.hpp"
#include "GainNativeParameters.hpp"
#include "FaultTables.hpp"
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
 int audioIn=-1,audioOut=-1,frames=-1,privateField=-1,privateSize=0;
 AAX_Result Clear()override{return AAX_SUCCESS;}
 AAX_Result AddAudioIn(AAX_CFieldIndex p)override{audioIn=p;return AAX_SUCCESS;}
 AAX_Result AddAudioOut(AAX_CFieldIndex p)override{audioOut=p;return AAX_SUCCESS;}
 AAX_Result AddAudioBufferLength(AAX_CFieldIndex p)override{frames=p;return AAX_SUCCESS;}
 AAX_Result AddSampleRate(AAX_CFieldIndex)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddClock(AAX_CFieldIndex)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddSideChainIn(AAX_CFieldIndex)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddDataInPort(AAX_CFieldIndex,uint32_t,AAX_EDataInPortType)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddAuxOutputStem(AAX_CFieldIndex,int32_t,const char[])override{return AAX_ERROR_UNIMPLEMENTED;}
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
 std::array<Descriptor,4> descriptors;size_t used=0;std::vector<std::pair<std::string,AAX_IEffectDescriptor*>> effects;Properties properties; AAX_Result effectStatus=AAX_SUCCESS;
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
void defaultMetadataContracts(Collection &registered){
 using Model=RavetGainNativeAaxParameters;
 for(uint32_t channels:{1u,2u}){
  auto &descriptor=registered.descriptors[channels-1];auto &component=descriptor.component;TransportControllerHost host,referenceHost;
  host.effect=referenceHost.effect=channels==1?"org.ravet.gain.mono":"org.ravet.gain.stereo";host.input=host.output=referenceHost.input=referenceHost.output=channels==1?AAX_eStemFormat_Mono:AAX_eStemFormat_Stereo;
  using Create=AAX_IEffectParameters*(AAX_CALLBACK*)();auto factory=reinterpret_cast<Create>(descriptor.create);std::unique_ptr<AAX_IEffectParameters> parameters(factory()),reference(factory());
  require(parameters&&reference&&parameters->Initialize(host.unknown())==AAX_SUCCESS&&reference->Initialize(referenceHost.unknown())==AAX_SUCCESS,"default main factories actual initialized metadata guard oracle");
  Model::Owner *owner=nullptr,*referenceOwner=nullptr;require(parameters->ResetFieldData(0,&owner,sizeof(owner))==AAX_SUCCESS&&reference->ResetFieldData(0,&referenceOwner,sizeof(referenceOwner))==AAX_SUCCESS,"default exact typed context owners");
  std::array<std::array<float,130>,2> input{},a{},b{};for(auto &buffer:input)buffer.fill(1.f);for(auto &buffer:a)buffer.fill(77.f);for(auto &buffer:b)buffer.fill(77.f);
  float *inputs[]={input[0].data(),input[1].data()},*outputs[]={a[0].data(),a[1].data()},*expected[]={b[0].data(),b[1].data()};
  alignas(Model::Context) std::array<std::byte,1024> contextStorage{};alignas(int32_t) std::array<std::byte,1024> frameStorage{};
  auto *frames=new(frameStorage.data()) int32_t(128);auto *context=new(contextStorage.data()) Model::Context{&owner,frames,inputs,outputs};int32_t referenceFrames=128;Model::Context referenceContext{&referenceOwner,&referenceFrames,inputs,expected};void *contexts[]={context},*referenceContexts[]={&referenceContext};
  component.process(contexts,contexts+1);component.process(referenceContexts,referenceContexts+1);
  for(auto *p:{parameters.get(),reference.get()})require(p->UpdateParameterNormalizedValue("p1",.125,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS,"acknowledged pending default Gain intent");
  uint32_t bytes=0;require(parameters->GetChunkSize(0x52765331,&bytes)==AAX_SUCCESS&&bytes==32,"default Gain complete state oracle");alignas(AAX_SPlugInChunk) std::array<uint8_t,128> stateStorage{};auto *chunk=new(stateStorage.data()) AAX_SPlugInChunk{};chunk->fSize=32;chunk->fChunkID=0x52765331;chunk->fManufacturerID=0x52617674;chunk->fProductID=0x5276476e;chunk->fPlugInID=channels==1?0x52764731:0x52764732;
  require(parameters->GetChunk(chunk->fChunkID,chunk)==AAX_SUCCESS,"default Gain queued before render save");for(auto *p:{parameters.get(),reference.get()})require(p->SetChunk(chunk->fChunkID,chunk)==AAX_SUCCESS&&p->UpdateParameterNormalizedValue("p1",.5,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS,"default complete restore/trailing Gain before callback metadata fault");
  context->frames=reinterpret_cast<int32_t*>(owner);component.process(contexts,contexts+1);require(owner->lastProcessStatus()==1&&owner->ready()&&*frames==128,"default Gain self-aliased frame metadata rejects before dereference");context->frames=frames;
  outputs[0]=reinterpret_cast<float*>(&owner);component.process(contexts,contexts+1);require(owner&&context->owner==&owner&&owner->lastProcessStatus()==1,"default Gain sample/owner-pointer metadata alias never clears ownership word");
  outputs[0]=reinterpret_cast<float*>(frames);component.process(contexts,contexts+1);require(*frames==128&&owner->lastProcessStatus()==1&&owner->ready(),"default Gain late sample/frame metadata overlap rejects without frame overwrite");outputs[0]=reinterpret_cast<float*>(context);const auto original=*context;component.process(contexts,contexts+1);require(!std::memcmp(context,&original,sizeof(original))&&owner->lastProcessStatus()==1&&owner->ready(),"default Gain late sample/context overlap preserves every callback pointer");outputs[0]=a[0].data();
  component.process(contexts,contexts+1);component.process(referenceContexts,referenceContexts+1);for(uint32_t ch=0;ch<channels;++ch)require(!std::memcmp(a[ch].data(),b[ch].data(),128*sizeof(float))&&a[ch][128]==77.f,"independent default Gain smoothing phase proves metadata faults did not render/apply restore/dequeue");require(a[0][127]>1.f,"default Gain retained trailing target is actually rendered");
  require(parameters->Uninitialize()==AAX_SUCCESS&&reference->Uninitialize()==AAX_SUCCESS&&host.references==1&&referenceHost.references==1,"default Gain guarded callback retirement");
 }
}
void componentContracts(const char *path){
 void *module=dlopen(path,RTLD_NOW|RTLD_LOCAL);if(!module)throw std::runtime_error(dlerror());
 using Describe=AAX_Result(*)(AAX_ICollection*);auto describe=reinterpret_cast<Describe>(dlsym(module,"_Z21GetEffectDescriptionsP15AAX_ICollection"));require(describe,"actual native descriptor export");
 Collection refused;refused.descriptors[0].createStatus=AAX_ERROR_UNIMPLEMENTED;require(describe(&refused)==AAX_ERROR_UNIMPLEMENTED&&refused.effects.empty()&&refused.used==1,"owned aggregate preserves first refusal without later registration");
 Collection registered;require(describe(&registered)==AAX_SUCCESS&&registered.effects.size()==2,"actual native module public mono/stereo registration");
 defaultMetadataContracts(registered);
 for(uint32_t channels:{1u,2u}){
  auto &descriptor=registered.descriptors[channels-1];auto &component=descriptor.component;
  TransportControllerHost host;host.effect=channels==1?"org.ravet.gain.mono":"org.ravet.gain.stereo";host.input=host.output=channels==1?AAX_eStemFormat_Mono:AAX_eStemFormat_Stereo;
  require(registered.effects[channels-1].first==host.effect&&component.privateField==0&&component.privateSize==sizeof(void*)&&component.frames==1&&component.audioIn==2&&component.audioOut==3&&component.process&&!component.init,"owned exact public descriptor profile/context");
  require(component.properties.values.at(AAX_eProperty_PlugInID_Native)==(channels==1?0x52764731:0x52764732)&&!component.properties.values.count(AAX_eProperty_PlugInID_AudioSuite),"preserved native identity with no AudioSuite advertisement");
  using Create=AAX_IEffectParameters*(AAX_CALLBACK*)();std::unique_ptr<AAX_IEffectParameters> parameters(reinterpret_cast<Create>(descriptor.create)());require(bool(parameters),"actual registered owned native factory");
  require(parameters->Initialize(host.unknown())==AAX_SUCCESS&&host.references==4&&host.registered.size()==2,"actual registered real SDK Initialize controller/automation references");
  require(parameters->Initialize(host.unknown())==AAX_ERROR_INVALID_ARGUMENT&&host.references==4,"repeated Initialize rejects before proxy replacement");
  void *owner=nullptr;require(parameters->ResetFieldData(0,&owner,sizeof(owner))==AAX_SUCCESS&&owner,"ready checked native owner public context initialization");
  using Model=RavetGainNativeAaxParameters;auto *publishedOwner=static_cast<Model::Owner*>(owner);
  void *sentinel=reinterpret_cast<void*>(uintptr_t(77));require(parameters->ResetFieldData(9,&sentinel,sizeof(sentinel))==AAX_ERROR_INVALID_ARGUMENT&&sentinel==reinterpret_cast<void*>(uintptr_t(77)),"unknown field retains prior storage");require(parameters->ResetFieldData(0,&sentinel,sizeof(sentinel)-1)==AAX_ERROR_INVALID_ARGUMENT&&sentinel==reinterpret_cast<void*>(uintptr_t(77)),"private storage exact size preflight");
  std::array<std::array<float,1026>,2> inputs{},outputs{};float *in[]={inputs[0].data(),inputs[1].data()},*out[]={outputs[0].data(),outputs[1].data()};for(uint32_t ch=0;ch<channels;++ch){inputs[ch].fill(float(ch+1));outputs[ch].fill(77.f);}
  int32_t frames=1024;void *context[4]={&owner,&frames,in,out};void *contexts[]={context};
  component.process(contexts,contexts+1);for(uint32_t ch=0;ch<channels;++ch)require(outputs[ch][0]==float(ch+1)&&outputs[ch][1023]==float(ch+1)&&outputs[ch][1024]==77.f,"actual ordinary realtime first native mono/stereo1024 block without offline repair");
  if constexpr(RAVET_AAX_NATIVE_SAMPLE_BITS==64){const auto precision=publishedOwner->precisionDiagnosticsStopped();require(precision.error==Model::Owner::PrecisionError::None&&precision.nativeAttempted&&precision.nativeReturned&&precision.nativeStatus==0&&precision.samplesTransferred&&publishedOwner->lastNativeProcessStatus()==0,"actual distributed Gain factory traverses authoritative native64 DSP and complete host-f32 conversion");}
  require(publishedOwner->transportSnapshotStopped().available()==127&&publishedOwner->transportSnapshotStopped().transport().sample_position==123,"actual registered Gain initialized versioned transport forwarding");for(auto count:host.transportCalls)require(count==1,"actual Gain each transport getter once");
  require(parameters->SetParameterNormalizedValue("p1",.125)==AAX_SUCCESS&&host.requests==1,"host request distinct from acknowledged intent");component.process(contexts,contexts+1);require(outputs[0][1023]==1.f,"host request alone does not change AUDIO intent");
  require(parameters->UpdateParameterNormalizedValue("p1",.125,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS,"actual acknowledged native queue control");
  int32_t count=0;AAX_CTypeID chunkID=0;uint32_t bytes=0;require(parameters->GetNumberOfChunks(&count)==AAX_SUCCESS&&count==1&&parameters->GetChunkIDFromIndex(0,&chunkID)==AAX_SUCCESS&&parameters->GetChunkSize(chunkID,&bytes)==AAX_SUCCESS&&bytes==32,"complete native chunk metadata");
  alignas(AAX_SPlugInChunk) std::array<uint8_t,128> saved{};auto *chunk=new(saved.data()) AAX_SPlugInChunk{};chunk->fSize=int32_t(bytes);chunk->fChunkID=chunkID;chunk->fManufacturerID=0x52617674;chunk->fProductID=0x5276476e;chunk->fPlugInID=channels==1?0x52764731:0x52764732;
  require(parameters->GetChunk(chunkID,chunk)==AAX_SUCCESS&&chunk->fVersion==1,"native save includes acknowledged queued state before render");AAX_CBoolean equal=false;require(parameters->CompareActiveChunk(chunk,&equal)==AAX_SUCCESS&&equal,"actual canonical intent equality");
  for(uint32_t i=0;i<4;++i)component.process(contexts,contexts+1);for(uint32_t ch=0;ch<channels;++ch)require(std::abs(outputs[ch][1023]-.5f*float(ch+1))<1e-6f,"actual native smoothed target after queue");
  require(parameters->UpdateParameterNormalizedValue("p1",.75,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS&&parameters->SetChunk(chunkID,chunk)==AAX_SUCCESS&&parameters->UpdateParameterNormalizedValue("p2",1.,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS,"complete restore supersedes old parameter and retains trailing bypass order");
  host.looping=true;host.loopStart=INT64_MAX-1;host.loopEnd=INT64_MAX;component.process(contexts,contexts+1);require(publishedOwner->lastProcessStatus()==1&&publishedOwner->ready()&&outputs[0][0]==0.f&&outputs[0][1024]==77.f,"actual Gain collapsed transport rejects before complete restore/trailing bypass and silences supported prefix");const auto failed=publishedOwner->transportResultStopped();require(failed.fault==ravet::aax::AaxTransportFault::InvalidProjection&&failed.statuses[5]==AAX_SUCCESS&&failed.returned==127,"actual Gain distinct original SDK status beside local precision rejection");host.looping=false;
  if(channels==2){float *overlap[]={outputs[0].data(),outputs[0].data()+1};context[3]=overlap;component.process(contexts,contexts+1);require(outputs[0][0]==0.f&&outputs[0][1025]==77.f,"actual late overlap fault supported silence and tail");context[3]=out;}
  component.process(contexts,contexts+1);for(uint32_t ch=0;ch<channels;++ch)require(outputs[ch][1023]==float(ch+1),"restore/trailing accepted intent retained across rejected block");
  chunk->fData[0]^=1;require(parameters->SetChunk(chunkID,chunk)==AAX_ERROR_INVALID_ARGUMENT,"malformed complete native state rejects before SDK/native queue mutation");chunk->fData[0]^=1;
  double value=77.;require(parameters->GetParameterNormalizedValue("p2",&value)==AAX_SUCCESS&&value==1.,"malformed restore retains acknowledged SDK values");
  for(const char *id:{"p01","p1tail","Byp"})require(parameters->UpdateParameterNormalizedValue(id,.5,AAX_eUpdateSource_Unspecified)==AAX_ERROR_INVALID_PARAMETER_ID,"exact identity before queues");
  require(parameters->NotificationReceived(AAX_eNotificationEvent_EnteringOfflineMode,nullptr,0)==AAX_SUCCESS&&parameters->NotificationReceived(AAX_eNotificationEvent_ExitingOfflineMode,nullptr,0)==AAX_SUCCESS,"explicit metadata-only mode notifications");
  for(uint32_t ch=0;ch<channels;++ch)outputs[ch][1024]=77.f;frames=1025;component.process(contexts,contexts+1);for(uint32_t ch=0;ch<channels;++ch)require(outputs[ch][0]==0.f&&outputs[ch][1023]==0.f&&outputs[ch][1024]==77.f,"actual excessive frame rejection silences declared prefix preserves tails");require(publishedOwner->lastProcessStatus()==1,"actual SDK void algorithm retains original rejected AUDIO status for typed control observation");frames=1024;component.process(contexts,contexts+1);require(publishedOwner->lastProcessStatus()==0,"actual successful algorithm replaces status with latest native result");
  require(parameters->Uninitialize()==AAX_SUCCESS&&host.references==1&&host.registered.empty(),"actual checked native/SDK retirement");component.process(contexts,contexts+1);require(outputs[0][1023]==0.f,"retained owner context fail-closed after retirement");
  require(parameters->Initialize(host.unknown())==AAX_SUCCESS&&parameters->ResetFieldData(0,&owner,sizeof(owner))==AAX_SUCCESS,"fresh initialization with accepted complete intent");component.process(contexts,contexts+1);require(outputs[0][1023]==1.f,"reinitialization restores accepted global bypass intent");
  require(parameters->Uninitialize()==AAX_SUCCESS&&host.references==1,"fresh SDK borrower cleanup");
 }
 {
  // Separate real-SDK synthetic owner uses the descriptor's declared public
  // Context type. Actual default Gain provider is not fault-injectable.
  using namespace ravet_aax_test;initializeFaultTables();
  using Model=RavetGainNativeAaxParameters;TransportControllerHost host;
  Model synthetic({"org.ravet.gain.mono",1,0x52617674,0x5276476e,0x52764731,0x52765331},&faultCore,&faultAudio,&faultCache,&faultState);
  require(synthetic.Initialize(host.unknown())==AAX_SUCCESS,"synthetic exact-origin owner actual SDK initialization");
  Model::Owner *owner=nullptr;require(synthetic.ResetFieldData(0,&owner,sizeof(owner))==AAX_SUCCESS,"synthetic ready owner published through public SDK field");
  std::array<float,1026> input{},output{};input.fill(1.f);output.fill(77.f);float *in[]={input.data()},*out[]={output.data()};int32_t frames=1024;
  Model::Context context{&owner,&frames,in,out};void *contexts[]={&context};
  processFault=2;registered.descriptors[0].component.process(contexts,contexts+1);require(synthetic.lastProcessStatus()==2&&!owner->ready()&&output[0]==0.f&&output[1023]==0.f&&output[1024]==77.f,"actual registered void algorithm retains original synthetic AUDIO backend poison for typed CONTROL observation");
  processFault=0;require(synthetic.Uninitialize()==AAX_SUCCESS&&host.references==1,"synthetic actual SDK poison retirement");
 }
 require(dlclose(module)==0,"actual module unload after all callbacks/models");
}
}
int main(int argc,char **argv){try{require(argc==2,"actual module required");componentContracts(argv[1]);std::cout<<"PASS transport capture and actual registered native AAX factories real SDK Initialize initial mono/stereo1024 realtime, parameter/global state queue, rejected buffer silence/tails and retirement contracts\n";return 0;}catch(const std::exception &error){std::cerr<<error.what()<<'\n';return 1;}}
