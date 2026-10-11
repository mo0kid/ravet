#include "RavetAaxFactoryRegistration.hpp"
#include "AAX_IComponentDescriptor.h"
#include "AAX_IPropertyMap.h"
#include "MeterControllerHost.hpp"
#include "GainMeterParameters.hpp"
#include "RavetAaxMeterRegistration.hpp"
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
#include <thread>
#include <atomic>
#include <chrono>
#include <filesystem>
#include <CoreFoundation/CoreFoundation.h>
#include "public.sdk/source/vst/aaxwrapper/aaxwrapper_description.h"
#include "pluginterfaces/base/ipluginbase.h"
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
 int audioIn=-1,audioOut=-1,frames=-1,privateField=-1,privateSize=0,meterField=-1; std::vector<AAX_CTypeID> meterIds;AAX_Result meterStatus=0;
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
 AAX_Result AddMeters(AAX_CFieldIndex f,const AAX_CTypeID* ids,uint32_t count)override{meterField=f;meterIds.assign(ids,ids+count);return meterStatus;}
 AAX_Result AddMIDINode(AAX_CFieldIndex,AAX_EMIDINodeType,const char[],uint32_t)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddReservedField(AAX_CFieldIndex,uint32_t)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_IPropertyMap *NewPropertyMap()const override{return &properties;}
 AAX_IPropertyMap *DuplicatePropertyMap(AAX_IPropertyMap*)const override{return nullptr;}
 AAX_Result AddProcessProc_Native(AAX_CProcessProc p,AAX_IPropertyMap*,AAX_CInstanceInitProc i,AAX_CBackgroundProc,AAX_CSelector*)override{process=p;init=i;return AAX_SUCCESS;}
 AAX_Result AddProcessProc_TI(const char[],const char[],AAX_IPropertyMap*,const char[],const char[],AAX_CSelector*)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddProcessProc(AAX_IPropertyMap*,AAX_CSelector*,int32_t)override{return AAX_ERROR_UNIMPLEMENTED;}
};
struct Descriptor final : AAX_IEffectDescriptor {
 Component component; Properties properties; std::vector<std::pair<AAX_CTypeID,std::string>> meters;AAX_Result descriptionStatus=0;uint32_t failDescription=UINT32_MAX;void *create=nullptr; int createCalls=0; AAX_Result createStatus=AAX_SUCCESS;
 AAX_IComponentDescriptor *NewComponentDescriptor()override{return &component;}
 AAX_Result AddComponent(AAX_IComponentDescriptor *p)override{return p==&component?AAX_SUCCESS:AAX_ERROR_INVALID_ARGUMENT;}
 AAX_Result AddName(const char*)override{return AAX_SUCCESS;}
 AAX_Result AddCategory(uint32_t)override{return AAX_SUCCESS;}
 AAX_Result AddCategoryBypassParameter(uint32_t,AAX_CParamID)override{return AAX_SUCCESS;}
 AAX_Result AddProcPtr(void *p,AAX_CProcPtrID kind)override{if(kind==kAAX_ProcPtrID_Create_EffectParameters){++createCalls;if(createStatus)return createStatus;create=p;}return AAX_SUCCESS;}
 AAX_IPropertyMap *NewPropertyMap()override{return &properties;}
 AAX_Result SetProperties(AAX_IPropertyMap*)override{return AAX_SUCCESS;}
 AAX_Result AddResourceInfo(AAX_EResourceType,const char*)override{return AAX_SUCCESS;}
 AAX_Result AddMeterDescription(AAX_CTypeID id,const char* name,AAX_IPropertyMap*)override{meters.emplace_back(id,name);return meters.size()-1==failDescription?descriptionStatus:AAX_SUCCESS;}
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

namespace meterFault {
 inline RavetNativeTelemetryEntryV1 telemetry{};inline int32_t snapshotStatus=0;inline bool snapshotThrow=false,corrupt=false,processThrow=false;
 inline int32_t(*originalProcess)(void*,const RavetNativeProcessorBusV1*,uint32_t,uint32_t,const RavetNativeEventV1*,uint32_t,const RavetNativeTransportV1*,RavetNativeEventV1*,uint32_t,RavetNativeProcessorResultV1*)=nullptr;
 const RavetNativeProcessorEntryV1* origin(){return &ravet_aax_test::faultCore;}
 int32_t snapshot(void* processor,const void* cache,RavetNativeTelemetryValueV1* values,uint32_t count){if(snapshotThrow)throw 91;if(snapshotStatus)return snapshotStatus;const auto status=ravet_native_telemetry_v1(RAVET_AAX_NATIVE_SAMPLE_BITS)->snapshot(processor,cache,values,count);if(!status&&corrupt){const uint64_t nan=UINT64_C(0x7ff8000000000001);std::memcpy(&values[count-1].value,&nan,8);}return status;}
 int32_t process(void* processor,const RavetNativeProcessorBusV1* buses,uint32_t busCount,uint32_t frames,const RavetNativeEventV1* events,uint32_t count,const RavetNativeTransportV1* transport,RavetNativeEventV1* output,uint32_t capacity,RavetNativeProcessorResultV1* report){if(processThrow)throw 97;return originalProcess(processor,buses,busCount,frames,events,count,transport,output,capacity,report);}
 void initialize(){ravet_aax_test::initializeFaultTables();originalProcess=ravet_aax_test::faultCore.process;ravet_aax_test::faultCore.process=process;telemetry=*ravet_native_telemetry_v1(RAVET_AAX_NATIVE_SAMPLE_BITS);telemetry.processor_entry=origin;telemetry.snapshot=snapshot;}
}
void registeredFaults(Component& component){
 using M=RavetGainMetersAaxParameters;
 for(uint32_t fault=0;fault<6;++fault){
  meterFault::initialize();MeterControllerHost host;host.effect="org.ravet.gainmeters.mono";auto model=std::make_unique<M>(M::Identity{host.effect.c_str(),1,0x52617674,0x52764d47,0x524d4731,0x52765331},&ravet_aax_test::faultCore,&ravet_aax_test::faultAudio,&ravet_aax_test::faultCache,&ravet_aax_test::faultState,&meterFault::telemetry,gainMeterBindings().data());require(model->Initialize(host.unknown())==0,"synthetic exact selected32/64 tables initialized through public SDK");M::Owner* owner=nullptr;require(model->ResetFieldData(0,&owner,sizeof(owner))==0,"synthetic metered ready callback owner");std::array<float,66> source{},output{};source.fill(.5);output.fill(77);float* in[]={source.data()},*out[]={output.data()};std::array<float,8> taps{};taps.fill(91);float* tap=taps.data();int32_t frames=64;M::Context context{&owner,&frames,in,out,&tap};void* contexts[]={&context};component.process(contexts,contexts+1);const auto prior=taps;require(owner->lastNativeProcessStatus()==0&&owner->meterDiagnosticsStopped().meter.published,"prior successful real registered callback establishes native0 and six taps");
  if(fault==0)meterFault::snapshotStatus=-771;else if(fault==1)meterFault::corrupt=true;else if(fault==2)meterFault::snapshotThrow=true;else if(fault==3)meterFault::snapshotStatus=2;else if(fault==4)ravet_aax_test::processFault=-861;else meterFault::processThrow=true;
  component.process(contexts,contexts+1);const auto d=owner->meterDiagnosticsStopped();require(taps==prior&&taps[6]==91&&output[64]==77,"whole failed publication/process preserves every prior tap and audio tail");
  if(fault<4){require(model->lastProcessStatus()==0&&d.nativeReturned&&d.nativeStatus==0&&owner->lastNativeProcessStatus()==0&&output[63]==.5f&&!d.meter.published,"post-DSP snapshot/projection refusal never relabels successful native audio");if(fault==0)require(d.meter.snapshotReturned&&d.meter.snapshotStatus==-771&&owner->ready(),"original telemetry refusal separate from native0");if(fault==1)require(d.meter.snapshotReturned&&d.meter.snapshotStatus==0&&d.meter.error==ravet::aax::AaxMeterDiagnostics::Error::Projection&&owner->ready(),"late malformed returned snapshot is local whole projection refusal");if(fault==2)require(!d.meter.snapshotReturned&&d.meter.error==ravet::aax::AaxMeterDiagnostics::Error::Exception&&!owner->ready(),"unreturned telemetry throw and future readiness poison separate");if(fault==3)require(d.meter.snapshotReturned&&d.meter.snapshotStatus==2&&!owner->ready(),"original telemetry poison2 invalidates future readiness without failed audio claim");}
  else{require(output[0]==0&&output[63]==0&&!owner->ready()&&d.nativeAttempted&&!d.meter.snapshotAttempted,"native refusal/throw known safe silence before meter publication");if(fault==4)require(d.nativeReturned&&d.nativeStatus==-861&&owner->lastNativeProcessStatus()==-861&&model->lastProcessStatus()==-861&&!d.exception,"actual SDK void algorithm retains original negative native AUDIO refusal");else require(!d.nativeReturned&&d.exception&&owner->lastNativeProcessStatus()==0&&model->lastProcessStatus()==1,"actual native violating throw records unreturned attempt and retains prior returned0");}
  meterFault::snapshotStatus=0;meterFault::snapshotThrow=false;meterFault::corrupt=false;meterFault::processThrow=false;ravet_aax_test::processFault=0;require(model->Uninitialize()==0&&host.references==1,"failed snapshot/process selected native and SDK retirement balanced");
 }
}
void componentContracts(const char* path){
 void* module=dlopen(path,RTLD_NOW|RTLD_LOCAL);if(!module)throw std::runtime_error(dlerror());
 using Enter=bool(*)(CFBundleRef);using Exit=bool(*)();auto entry=reinterpret_cast<Enter>(dlsym(module,"bundleEntry"));auto exit=reinterpret_cast<Exit>(dlsym(module,"bundleExit"));const auto directory=std::filesystem::path(path).parent_path().parent_path().parent_path().string();auto url=CFURLCreateFromFileSystemRepresentation(nullptr,reinterpret_cast<const UInt8*>(directory.data()),directory.size(),true);auto bundle=url?CFBundleCreate(nullptr,url):nullptr;if(url)CFRelease(url);require(entry&&exit&&bundle&&entry(bundle),"actual external SDK module initialization");
 MeterControllerHost startupHost;using Startup=AAX_Result(*)(IACFUnknown*);auto startup=reinterpret_cast<Startup>(dlsym(module,"ACFStartup"));auto shutdown=reinterpret_cast<Startup>(dlsym(module,"ACFShutdown"));require(startup&&shutdown&&startup(startupHost.unknown())==0,"actual external AAX SDK startup");
 using Compatibility=AAX_Effect_Desc*(*)();auto compatibility=reinterpret_cast<Compatibility>(dlsym(module,"_Z25AAXWrapper_GetDescriptionv"));auto* compatible=compatibility?compatibility():nullptr;require(compatible&&compatible->mPluginDesc&&!std::strcmp(compatible->mPluginDesc[0].mEffectID,"org.ravet.gainmeters.mono")&&!std::strcmp(compatible->mPluginDesc[1].mEffectID,"org.ravet.gainmeters.stereo")&&!compatible->mPluginDesc[2].mEffectID,"external bootstrap exact distinct metered identity hooks");
 using Describe=AAX_Result(*)(AAX_ICollection*);auto describe=reinterpret_cast<Describe>(dlsym(module,"_Z21GetEffectDescriptionsP15AAX_ICollection"));require(describe,"actual metered descriptor export");
 using Factory=Steinberg::IPluginFactory*(*)();auto bootstrap=reinterpret_cast<Factory>(dlsym(module,"GetPluginFactory"));require(bootstrap,"actual external SDK bootstrap");auto* factory=bootstrap();require(factory&&factory->countClasses()==0,"AAX-only reference bootstraps zero unrelated DSP classes");factory->release();
 Collection refused;refused.descriptors[0].createStatus=-711;require(describe(&refused)==-711&&refused.effects.empty()&&refused.used==1&&refused.descriptors[0].component.meterIds.empty(),"actual owned descriptor preserves original first factory-registration refusal before meter fields");
 ravet::aax::AaxPreparedMeters<6> metadata(ravet_native_processor_v1(RAVET_AAX_NATIVE_SAMPLE_BITS),ravet_native_telemetry_v1(RAVET_AAX_NATIVE_SAMPLE_BITS),gainMeterBindings().data());Descriptor meterRefused;meterRefused.component.meterStatus=-812;const auto fieldFailure=ravet::aax::registerAaxMeters(meterRefused,meterRefused.component,4,metadata);require(fieldFailure.status==-812&&fieldFailure.step==ravet::aax::AaxMeterRegistrationResult::Step::Field&&fieldFailure.descriptions==0&&meterRefused.meters.empty()&&meterRefused.component.meterIds.size()==6,"original SDK AddMeters refusal and no invented field rollback/later calls");Descriptor lateRefused;lateRefused.failDescription=4;lateRefused.descriptionStatus=-823;const auto lateFailure=ravet::aax::registerAaxMeters(lateRefused,lateRefused.component,4,metadata);require(lateFailure.status==-823&&lateFailure.step==ravet::aax::AaxMeterRegistrationResult::Step::Description&&lateFailure.descriptions==4&&lateRefused.meters.size()==5&&lateRefused.component.meterIds.size()==6,"original late description refusal retains exact completed/attempted prefix without SDK rollback");
 Collection registered;require(describe(&registered)==AAX_SUCCESS&&registered.effects.size()==2,"actual mono/stereo meter registration");
 using Model=RavetGainMetersAaxParameters;
 for(uint32_t channels:{1u,2u}){
  auto& descriptor=registered.descriptors[channels-1];auto& component=descriptor.component;
  require(component.privateField==0&&component.frames==1&&component.audioIn==2&&component.audioOut==3&&component.meterField==4&&component.meterIds==std::vector<AAX_CTypeID>{101,102,103,104,105,106},"exact typed flat context and six ordered host tap IDs");
  require(descriptor.meters.size()==6&&descriptor.meters[5].first==106&&descriptor.meters[5].second=="Output R RMS","immutable meter names/IDs survive descriptor registration");
  MeterControllerHost host;host.effect=channels==1?"org.ravet.gainmeters.mono":"org.ravet.gainmeters.stereo";host.input=host.output=channels==1?AAX_eStemFormat_Mono:AAX_eStemFormat_Stereo;
  require(registered.effects[channels-1].first==host.effect,"opt-in reference has distinct exact public effect identity");
  using Create=AAX_IEffectParameters*(AAX_CALLBACK*)();std::unique_ptr<AAX_IEffectParameters> parameters(reinterpret_cast<Create>(descriptor.create)());require(parameters&&parameters->Initialize(host.unknown())==0,"actual factory SDK Initialize");auto* model=static_cast<Model*>(parameters.get());
  Model::Owner* owner=nullptr;require(parameters->ResetFieldData(0,&owner,sizeof(owner))==0&&owner&&owner->ready(),"actual checked ready owner");
  std::array<float,1026> left{},right{},outL{},outR{};left.fill(.5f);right.fill(-.25f);outL.fill(77.f);outR.fill(77.f);float* in[]={left.data(),right.data()},*out[]={outL.data(),outR.data()};std::array<float,8> taps{};taps.fill(91.f);float* liveTaps=taps.data();int32_t frames=1024;Model::Context context{&owner,&frames,in,out,&liveTaps};void* contexts[]={&context};
  component.process(contexts,contexts+1);const std::array<float,6> unity{.5f,.5f,.5f,.5f,channels==2?.25f:0.f,channels==2?.25f:0.f};
  for(uint32_t i=0;i<6;++i)require(taps[i]==unity[i],"actual authoritative native32/64 unity independently predicted input/output peak/RMS");require(taps[6]==91.f&&taps[7]==91.f&&outL[1024]==77.f,"actual host meter/audio backing tails preserved");
  const auto first=owner->meterDiagnosticsStopped();require(first.nativeAttempted&&first.nativeReturned&&first.nativeStatus==0&&first.meter.published&&first.meter.snapshotReturned&&first.meter.snapshotStatus==0,"successful native processing and meter publication facts separate");
  uint32_t bytes=0;require(parameters->UpdateParameterNormalizedValue("p1",.125,AAX_eUpdateSource_Unspecified)==0&&parameters->GetChunkSize(0x52765331,&bytes)==0&&bytes==32,"canonical Gain scalar/schema/state unchanged");alignas(AAX_SPlugInChunk) std::array<uint8_t,128> saved{};auto* chunk=new(saved.data()) AAX_SPlugInChunk{};chunk->fSize=32;chunk->fChunkID=0x52765331;chunk->fManufacturerID=0x52617674;chunk->fProductID=0x52764d47;chunk->fPlugInID=channels==1?0x524d4731:0x524d4732;require(parameters->GetChunk(chunk->fChunkID,chunk)==0,"complete acknowledged Gain.5 intent");
  // A real registered callback runs on AUDIO while CONTROL publishes a distinct
  // restore plus trailing scalar after this block sampled its tail; following block consumes distinct 1.6.
  std::atomic<bool> entered{false},resume{false};host.onTransport=[&]{entered.store(true,std::memory_order_release);while(!resume.load(std::memory_order_acquire))std::this_thread::yield();};
  std::thread audio([&]{component.process(contexts,contexts+1);});const auto deadline=std::chrono::steady_clock::now()+std::chrono::seconds(5);while(!entered.load(std::memory_order_acquire)&&std::chrono::steady_clock::now()<deadline)std::this_thread::yield();if(!entered.load(std::memory_order_acquire)){resume.store(true,std::memory_order_release);audio.join();require(false,"fixture AUDIO getter barrier reached before timeout");}
  require(parameters->SetChunk(chunk->fChunkID,chunk)==0&&parameters->UpdateParameterNormalizedValue("p1",.4,AAX_eUpdateSource_Unspecified)==0&&parameters->SetParameterNormalizedValue("p1",.4)==0,"meter-only AUDIO preserves default SPSC complete restore/acknowledged scalar/host request");
  std::array<RavetNativeTelemetryValueV1,12> telemetry{};telemetry[0].samples=777;const auto before=telemetry;require(model->snapshotTelemetryStopped(telemetry.data(),12)==1&&!std::memcmp(telemetry.data(),before.data(),sizeof(before))&&model->resetStopped()==AAX_ERROR_INVALID_ARGUMENT&&model->Uninitialize()==AAX_ERROR_INVALID_ARGUMENT,"live stopped observation/lifecycle refuse without mutation");
  resume.store(true,std::memory_order_release);audio.join();host.onTransport={};require(std::abs(outL[1023]-.25f)<1e-6f,"current callback retains its previously sampled queue tail before blocked transport getter");component.process(contexts,contexts+1);require(!model->snapshotTelemetryStopped(telemetry.data(),12)&&std::abs(telemetry[6].value-double(.8f))<1e-6&&telemetry[6].samples==1024,"post-join native telemetry proves distinct Gain1.6 restore/trailing source order and authoritative output meter window");require(std::abs(outL[1023]-.8f)<1e-6f,"actual distinct target consumes accepted queue");
  const auto oldTaps=taps;const auto oldTelemetry=telemetry;liveTaps=reinterpret_cast<float*>(&context);const auto oldContext=context;component.process(contexts,contexts+1);require(!std::memcmp(&context,&oldContext,sizeof(context))&&taps==oldTaps&&!model->snapshotTelemetryStopped(telemetry.data(),12)&&!std::memcmp(telemetry.data(),oldTelemetry.data(),sizeof(telemetry)),"late tap/context alias rejects before render/history and no metadata/tap mutation");
  liveTaps=outL.data();component.process(contexts,contexts+1);require(taps==oldTaps&&!model->snapshotTelemetryStopped(telemetry.data(),12)&&!std::memcmp(telemetry.data(),oldTelemetry.data(),sizeof(telemetry)),"meter/audio overlap refuses whole callback before state/history");
  liveTaps=outL.data()+1020;const std::array<float,6> protectedSamples{outL[1020],outL[1021],outL[1022],outL[1023],outL[1024],outL[1025]};component.process(contexts,contexts+1);for(uint32_t i=0;i<6;++i)require(outL[1020+i]==protectedSamples[i],"partial late tap/audio overlap protects every known tap/sample/tail word");require(outL[0]==0&&taps==oldTaps&&!model->snapshotTelemetryStopped(telemetry.data(),12)&&!std::memcmp(telemetry.data(),oldTelemetry.data(),sizeof(telemetry)),"late partial meter/audio overlap silences independent prefix only without native history");
  liveTaps=reinterpret_cast<float*>(owner);component.process(contexts,contexts+1);require(owner->ready()&&taps==oldTaps&&owner->lastNativeProcessStatus()==0&&!owner->meterDiagnosticsStopped().nativeAttempted,"meter/whole owner overlap never overwrites ownership or original latest returned native status");liveTaps=taps.data();
  component.process(contexts,contexts+1);require(owner->meterDiagnosticsStopped().meter.published&&outL[1024]==77.f,"next valid block publishes fresh taps after structural faults");
  require(parameters->UpdateParameterNormalizedValue("p2",1.,AAX_eUpdateSource_Unspecified)==0,"actual canonical bypass intent");component.process(contexts,contexts+1);require(taps[2]==.5f&&taps[4]==(channels==2?.25f:0.f),"actual authoritative dry bypass meter taps");
  require(model->resetStopped()==0,"stopped meter reset");frames=0;component.process(contexts,contexts+1);for(uint32_t i=0;i<6;++i)require(taps[i]==0.f,"actual reset/zero frame follows native window semantics");
  require(parameters->Uninitialize()==0&&host.references==1&&host.registered.empty(),"actual SDK native cache/schema/controller retirement balanced");
 }
 registeredFaults(registered.descriptors[0].component);
 require(shutdown(startupHost.unknown())==0&&exit(),"actual SDK shutdown and module exit after borrowers");CFRelease(bundle);
 require(dlclose(module)==0,"actual metered module unload after all borrowed models");
}
}
int main(int argc,char** argv){try{require(argc==2,"actual module required");componentContracts(argv[1]);std::cout<<"PASS actual SDK AddMeters/metadata/factory Initialize mono/stereo native32/64 analytic taps, distinct concurrent SPSC restore/scalar, protected whole alias/history and retirement\n";}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
