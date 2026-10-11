#include "RavetAaxFactoryRegistration.hpp"
#include "pluginterfaces/base/ipluginbase.h"
#include "public.sdk/source/vst/aaxwrapper/aaxwrapper_description.h"
#include <CoreFoundation/CoreFoundation.h>
#include <filesystem>
#include <functional>
#include "AAX_IComponentDescriptor.h"
#include "AAX_IPropertyMap.h"
#include "TransportControllerHost.hpp"
#include "MidiParameters.hpp"

#include "AAX_CString.h"
#include <array>
#include <atomic>
#include <cstdlib>
#include <cmath>
#include <dlfcn.h>
#include <iostream>
#include <map>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>
#include <thread>
static std::atomic<bool> measuring{false};static std::atomic<size_t> allocationCount{0},deletionCount{0};
void *operator new(size_t size){if(measuring.load(std::memory_order_relaxed))++allocationCount;if(auto *p=std::malloc(size?size:1))return p;throw std::bad_alloc();}
void operator delete(void *p)noexcept{if(p&&measuring.load(std::memory_order_relaxed))++deletionCount;std::free(p);}
void operator delete(void *p,size_t)noexcept{::operator delete(p);}
void *operator new[](size_t size){return ::operator new(size);}void operator delete[](void *p)noexcept{::operator delete(p);}
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
 int audioIn=-1,audioOut=-1,frames=-1,privateField=-1,privateSize=0,midi=-1;AAX_EMIDINodeType midiType=AAX_eMIDINodeType_LocalOutput;uint32_t mask=0;std::string midiName;
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
 AAX_Result AddMIDINode(AAX_CFieldIndex p,AAX_EMIDINodeType type,const char name[],uint32_t channels)override{midi=p;midiType=type;midiName=name;mask=channels;return AAX_SUCCESS;}
 AAX_Result AddReservedField(AAX_CFieldIndex,uint32_t)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_IPropertyMap *NewPropertyMap()const override{return &properties;}
 AAX_IPropertyMap *DuplicatePropertyMap(AAX_IPropertyMap*)const override{return nullptr;}
 AAX_Result AddProcessProc_Native(AAX_CProcessProc p,AAX_IPropertyMap*,AAX_CInstanceInitProc i,AAX_CBackgroundProc,AAX_CSelector*)override{process=p;init=i;return AAX_SUCCESS;}
 AAX_Result AddProcessProc_TI(const char[],const char[],AAX_IPropertyMap*,const char[],const char[],AAX_CSelector*)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddProcessProc(AAX_IPropertyMap*,AAX_CSelector*,int32_t)override{return AAX_ERROR_UNIMPLEMENTED;}
};
struct Descriptor final : AAX_IEffectDescriptor {
 uint32_t category=0;Component component; Properties properties; void *create=nullptr; int createCalls=0; AAX_Result createStatus=AAX_SUCCESS;
 AAX_IComponentDescriptor *NewComponentDescriptor()override{return &component;}
 AAX_Result AddComponent(AAX_IComponentDescriptor *p)override{return p==&component?AAX_SUCCESS:AAX_ERROR_INVALID_ARGUMENT;}
 AAX_Result AddName(const char*)override{return AAX_SUCCESS;}
 AAX_Result AddCategory(uint32_t c)override{category=c;return AAX_SUCCESS;}
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
struct Node final:AAX_IMIDINode{
 std::array<AAX_CMidiPacket,1024> packets{};AAX_CMidiStream stream{0,packets.data()};uint32_t reads=0;std::function<void()> onRead;
 AAX_CMidiStream *GetNodeBuffer()override{++reads;if(onRead)onRead();return &stream;}
 AAX_Result PostMIDIPacket(AAX_CMidiPacket*)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_ITransport *GetTransport()override{throw std::runtime_error("no unversioned transport request");}
 void add(uint32_t frame,uint8_t status,uint8_t key,uint8_t velocity){packets[stream.mBufferSize++]={frame,3,{status,key,velocity,0},0};}
};
using Model=RavetMidiAaxParameters;
void componentContracts(const char *path){
 void *module=dlopen(path,RTLD_NOW|RTLD_LOCAL);if(!module)throw std::runtime_error(dlerror());
 using Enter=bool(*)(CFBundleRef);using Exit=bool(*)();auto entry=reinterpret_cast<Enter>(dlsym(module,"bundleEntry"));auto exit=reinterpret_cast<Exit>(dlsym(module,"bundleExit"));const auto directory=std::filesystem::path(path).parent_path().parent_path().parent_path().string();auto url=CFURLCreateFromFileSystemRepresentation(nullptr,reinterpret_cast<const UInt8*>(directory.data()),directory.size(),true);auto bundle=url?CFBundleCreate(nullptr,url):nullptr;if(url)CFRelease(url);require(entry&&exit&&bundle&&entry(bundle),"actual external SDK module bundleEntry initialization");
 TransportControllerHost startupHost;using Startup=AAX_Result(*)(IACFUnknown*);auto startup=reinterpret_cast<Startup>(dlsym(module,"ACFStartup"));auto shutdown=reinterpret_cast<Startup>(dlsym(module,"ACFShutdown"));require(startup&&shutdown&&startup(startupHost.unknown())==AAX_SUCCESS,"actual external AAX SDK startup proxy hook");
 using Compatibility=AAX_Effect_Desc*(*)();auto compatibility=reinterpret_cast<Compatibility>(dlsym(module,"_Z25AAXWrapper_GetDescriptionv"));auto *compat=compatibility?compatibility():nullptr;require(compat&&compat->mPluginDesc&&!std::strcmp(compat->mPluginDesc[0].mEffectID,"org.ravet.midisine.mono")&&!std::strcmp(compat->mPluginDesc[1].mEffectID,"org.ravet.midisine.stereo")&&!compat->mPluginDesc[2].mEffectID,"actual external bootstrap description exact distinct mono/stereo identity");
 using Factory=Steinberg::IPluginFactory*(*)();auto bootstrap=reinterpret_cast<Factory>(dlsym(module,"GetPluginFactory"));require(bootstrap,"actual external SDK bootstrap export");auto *empty=bootstrap();require(empty&&empty->countClasses()==0,"actual AAX-only bootstrap exposes zero unrelated VST3 classes");empty->release();
 using Version=AAX_Result(*)(uint64_t*);auto version=reinterpret_cast<Version>(dlsym(module,"ACFGetSDKVersion"));uint64_t sdkVersion=0;require(version&&version(&sdkVersion)==AAX_SUCCESS&&sdkVersion,"actual external SDK version startup hook");
 using Describe=AAX_Result(*)(AAX_ICollection*);auto describe=reinterpret_cast<Describe>(dlsym(module,"_Z21GetEffectDescriptionsP15AAX_ICollection"));require(describe,"actual instrument native descriptor export");
 Collection refused;refused.descriptors[0].createStatus=AAX_ERROR_UNIMPLEMENTED;require(describe(&refused)==AAX_ERROR_UNIMPLEMENTED&&refused.effects.empty()&&refused.used==1,"first original instrument registration refusal retained");
 Collection registered;require(describe(&registered)==AAX_SUCCESS&&registered.effects.size()==2,"actual instrument mono/stereo public registration");
 for(uint32_t channels:{1u,2u}){
  auto &descriptor=registered.descriptors[channels-1];auto &component=descriptor.component;TransportControllerHost host,referenceHost;
  host.effect=referenceHost.effect=channels==1?"org.ravet.midisine.mono":"org.ravet.midisine.stereo";host.input=referenceHost.input=AAX_eStemFormat_None;host.output=referenceHost.output=channels==1?AAX_eStemFormat_Mono:AAX_eStemFormat_Stereo;
  require(registered.effects[channels-1].first==host.effect&&descriptor.category==AAX_ePlugInCategory_SWGenerators&&component.audioIn==-1&&component.audioOut==2&&component.frames==1&&component.privateField==0&&component.privateSize==sizeof(void*)&&component.midi==3&&component.midiType==AAX_eMIDINodeType_LocalInput&&component.mask==0xffff&&component.midiName=="MIDI Input"&&component.process&&!component.init,"actual exact instrument context/MIDI/category descriptor");
  require(component.properties.values.at(AAX_eProperty_InputStemFormat)==AAX_CPropertyValue(AAX_eStemFormat_None)&&component.properties.values.at(AAX_eProperty_OutputStemFormat)==AAX_CPropertyValue(host.output)&&component.properties.values.at(AAX_eProperty_CanBypass)==0&&component.properties.values.at(AAX_eProperty_Constraint_MultiMonoSupport)==0&&component.properties.values.at(AAX_eProperty_PlugInID_Native)==(channels==1?0x52764d31:0x52764d32)&&!component.properties.values.count(AAX_eProperty_PlugInID_AudioSuite),"instrument no bypass/no AudioSuite/explicit independent native IDs");
  using Create=AAX_IEffectParameters*(AAX_CALLBACK*)();auto factory=reinterpret_cast<Create>(descriptor.create);std::unique_ptr<AAX_IEffectParameters> parameters(factory()),reference(factory());require(parameters&&reference,"actual instrument exported factories");
  require(parameters->Initialize(host.unknown())==AAX_SUCCESS&&reference->Initialize(referenceHost.unknown())==AAX_SUCCESS&&host.registered.size()==1,"actual real SDK instrument initialization and native Level-only objects");
  void *owner=nullptr,*referenceOwner=nullptr;require(parameters->ResetFieldData(0,&owner,sizeof(owner))==AAX_SUCCESS&&reference->ResetFieldData(0,&referenceOwner,sizeof(referenceOwner))==AAX_SUCCESS,"actual instrument checked ready context owners");auto *published=static_cast<Model::Owner*>(owner);
  require(published->lastNativeProcessStatus()==1,"fresh actual initialized raw owner exposes documented numeric never-returned sentinel1");
  std::array<std::array<float,1026>,2> a{},b{};for(auto &ch:a)ch.fill(77.f);for(auto &ch:b)ch.fill(77.f);float *out[]={a[0].data(),a[1].data()},*expected[]={b[0].data(),b[1].data()};Node node,referenceNode;int32_t frames=240;void *context[4]={&owner,&frames,out,&node},*referenceContext[4]={&referenceOwner,&frames,expected,&referenceNode};void *contexts[]={context},*referenceContexts[]={referenceContext};auto *typedModel=static_cast<Model*>(parameters.get());auto *typedReference=static_cast<Model*>(reference.get());
  frames=0;node.add(0,0x90,72,127);referenceNode.add(0,0x90,72,127);node.packets[0].mIsImmediate=referenceNode.packets[0].mIsImmediate=1;
  require(parameters->UpdateParameterNormalizedValue("p1",.3,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS&&reference->UpdateParameterNormalizedValue("p1",.3,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS,"queue acknowledged controller intent before raw zero-frame note");
  component.process(contexts,contexts+1);component.process(referenceContexts,referenceContexts+1);std::array<RavetNativeTelemetryValueV1,23> zeroObservation{};
  require(published->lastProcessStatus()==0&&!typedModel->snapshotTelemetryStopped(zeroObservation.data(),23)&&zeroObservation[0].samples==0&&(zeroObservation[5].samples>>16)==1&&(zeroObservation[5].samples&65535)==73&&zeroObservation[6].value==double(.3f)&&a[0][0]==77.f&&a[0][1025]==77.f,"raw zero-frame queued controller/note state applies without sample/frame advance or tail writes");
  node.stream.mBufferSize=referenceNode.stream.mBufferSize=0;node.add(0,0x80,72,31);referenceNode.add(0,0x80,72,31);component.process(contexts,contexts+1);component.process(referenceContexts,referenceContexts+1);
  require(!typedModel->snapshotTelemetryStopped(zeroObservation.data(),23)&&zeroObservation[0].samples==0&&(zeroObservation[5].samples>>16)==3&&a[0][0]==77.f&&a[0][1025]==77.f,"raw zero-frame note-off enters release without advancing phase or audio");
  require(typedModel->resetStopped()==AAX_SUCCESS&&typedReference->resetStopped()==AAX_SUCCESS&&parameters->UpdateParameterNormalizedValue("p1",.2,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS&&reference->UpdateParameterNormalizedValue("p1",.2,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS,"explicit stopped reset and acknowledged initial analytic Level");
  node.stream.mBufferSize=referenceNode.stream.mBufferSize=0;frames=240;
  node.add(3,0x90,69,64);referenceNode.add(3,0x90,69,64);component.process(contexts,contexts+1);component.process(referenceContexts,referenceContexts+1);
  require(node.reads==3&&published->lastProcessStatus()==0&&published->transportSnapshotStopped().available()==127&&published->transportSnapshotStopped().transport().sample_position==123,"actual registered instrument initialized SDK proxy supplies native transport");for(auto count:host.transportCalls)require(count==3,"actual factory transport getter once per successful block");
  for(uint32_t i=0;i<240;++i){const auto n=i<3?0:i-3;const auto analytic=i<3?0.:std::sin(6.283185307179586*440./48000.*n)*(n+1)/240.*double(.2f)*double(64.f/127.f)/16.;require(std::abs(double(a[0][i])-analytic)<1e-8,"actual registered instrument independent analytic MIDI audio timing");if(channels==2)require(a[1][i]==a[0][i],"actual registered stereo center");}require(a[0][240]==77.f,"actual active backing tail");
  std::array<RavetNativeTelemetryValueV1,23> rawObservation{};require(!typedModel->snapshotTelemetryStopped(rawObservation.data(),23)&&rawObservation[4].samples==1&&rawObservation[0].samples==240,"actual registered exact native raw telemetry confirms first note");
  const auto successfulRaw=typedModel->rawDiagnosticsStopped();require(successfulRaw.nativeAttempted&&successfulRaw.nativeReturned&&successfulRaw.samplesTransferred,"established successful raw block immediately before early callback metadata fault");auto* earlyFrame=static_cast<int32_t*>(context[1]);const auto earlyReads=node.reads;context[1]=published;component.process(contexts,contexts+1);context[1]=earlyFrame;const auto earlyFault=typedModel->rawDiagnosticsStopped();require(published->lastProcessStatus()==1&&published->lastNativeProcessStatus()==0&&node.reads==earlyReads&&earlyFault.error==Model::Owner::PrecisionError::Preflight&&!earlyFault.nativeAttempted&&!earlyFault.nativeReturned&&!earlyFault.stateAttempted&&!earlyFault.commandsConsumed&&!earlyFault.samplesTransferred,"success immediately followed by admitted early frame alias publishes fresh refusal diagnostics before getter/history mutation");if constexpr(RAVET_AAX_NATIVE_SAMPLE_BITS==64){const auto d=typedModel->precisionDiagnosticsStopped();require(d.error==Model::Owner::PrecisionError::Preflight&&!d.nativeAttempted&&!d.nativeReturned&&!d.samplesTransferred,"early raw64 metadata failure also clears prior precision success");}
  std::array<RavetNativeTelemetryValueV1,23> afterEarly{};require(!typedModel->snapshotTelemetryStopped(afterEarly.data(),23)&&!std::memcmp(afterEarly.data(),rawObservation.data(),sizeof(afterEarly)),"immediate early frame refusal preserves all authoritative frame/voice/request telemetry");
  std::array<RavetNativeTelemetryValueV1,24> snapshotRange{};for(auto& value:snapshotRange)value.samples=777;const auto oldSnapshotRange=snapshotRange;require(typedModel->snapshotTelemetryStopped(snapshotRange.data(),0)==1&&typedModel->snapshotTelemetryStopped(snapshotRange.data(),22)==1&&!std::memcmp(snapshotRange.data(),oldSnapshotRange.data(),sizeof(snapshotRange)),"short/zero stopped telemetry capacity rejects before all borrowed output mutation");require(typedModel->snapshotTelemetryStopped(snapshotRange.data(),24)==0&&snapshotRange[0].samples==240&&snapshotRange[23].samples==777,"complete telemetry prefix succeeds and longer capacity preserves tail");require(typedModel->snapshotTelemetryStopped(reinterpret_cast<RavetNativeTelemetryValueV1*>(parameters.get()),23)==1&&typedModel->snapshotTelemetryStopped(reinterpret_cast<RavetNativeTelemetryValueV1*>(published),23)==1,"known model/owner telemetry output aliases refuse before native snapshot");

  // Every defined short class reaches the actual registered processor. Keep
  // this matrix before the new independent note/history oracle below.
  node.stream.mBufferSize=referenceNode.stream.mBufferSize=0;frames=128;
  for(uint32_t status=0x80;status<=0xff;++status){uint32_t n=0;if(status<0xf0)n=(status&0xe0)==0xc0?2:3;else if(status==0xf1||status==0xf3)n=2;else if(status==0xf2)n=3;else if(status==0xf6||status==0xf7||status==0xf8||status==0xfa||status==0xfb||status==0xfc||status==0xfe||status==0xff)n=1;if(!n)continue;AAX_CMidiPacket packet{0,n,{uint8_t(status),69,127,255},0};node.packets[node.stream.mBufferSize++]=packet;referenceNode.packets[referenceNode.stream.mBufferSize++]=packet;}
  component.process(contexts,contexts+1);component.process(referenceContexts,referenceContexts+1);require(published->lastProcessStatus()==0&&!typedModel->snapshotTelemetryStopped(rawObservation.data(),23)&&rawObservation[4].samples==123&&rawObservation[2].samples==UINT64_C(15738453561344363854)&&rawObservation[3].samples==UINT64_C(3721297664),"all123 defined status/channel/system payloads reach actual initialized native consumer");
  node.stream.mBufferSize=referenceNode.stream.mBufferSize=0;frames=64;
  node.add(0,0xb0,0,7);node.add(0,0xb0,32,11);node.packets[node.stream.mBufferSize++]={0,2,{0xc0,2,255,255},0};node.add(3,0x90,69,127);referenceNode.packets=node.packets;referenceNode.stream.mBufferSize=node.stream.mBufferSize;
  component.process(contexts,contexts+1);component.process(referenceContexts,referenceContexts+1);
  require(!typedModel->snapshotTelemetryStopped(rawObservation.data(),23)&&rawObservation[4].samples==4&&rawObservation[7].samples==1+((7*128+11)<<8|2)&&rawObservation[6].value==double(.2f),"exact raw program/bank request observed without replacing acknowledged global Level");
  require(a[0][20]!=0.f,"live actual note before subsequent late fault/history oracle");
  node.stream.mBufferSize=0;frames=0;node.add(0,0xb0,0,0);node.add(0,0xb0,32,0);node.packets[node.stream.mBufferSize++]={0,2,{0xc0,2,255,255},0};referenceNode.packets=node.packets;referenceNode.stream.mBufferSize=node.stream.mBufferSize;
  component.process(contexts,contexts+1);component.process(referenceContexts,referenceContexts+1);
  require(!typedModel->snapshotTelemetryStopped(rawObservation.data(),23)&&rawObservation[7].samples==3&&rawObservation[6].value==double(.2f),"known Full bank0/program2 request remains exact without replacing acknowledged Level");frames=64;
  bool liveRefused=false;node.stream.mBufferSize=referenceNode.stream.mBufferSize=0;
  node.onRead=[&]{std::array<RavetNativeTelemetryValueV1,23> output{};output[0].samples=777;const auto old=output;const auto refs=host.references,reads=node.reads;require(typedModel->snapshotTelemetryStopped(output.data(),23)==1&&!std::memcmp(output.data(),old.data(),sizeof(old)),"live stopped snapshot rejects without borrowed-output mutation");require(typedModel->resetStopped()==AAX_ERROR_INVALID_ARGUMENT&&typedModel->reprepareStopped()==AAX_ERROR_INVALID_ARGUMENT&&typedModel->Uninitialize()==AAX_ERROR_INVALID_ARGUMENT&&host.references==refs,"raw admitted callback rejects lifecycle before host/resource mutation");component.process(contexts,contexts+1);require(node.reads==reads&&published->ready(),"nested raw callback Busy before getter/history/output mutation");liveRefused=true;};
  component.process(contexts,contexts+1);node.onRead={};component.process(referenceContexts,referenceContexts+1);require(liveRefused&&!typedModel->snapshotTelemetryStopped(rawObservation.data(),23)&&rawObservation[0].samples==496,"post-join observer succeeds after live/nested rejection");

  node.add(7,0xe0,0,127);referenceNode.packets=node.packets;referenceNode.stream.mBufferSize=node.stream.mBufferSize;
  component.process(contexts,contexts+1);component.process(referenceContexts,referenceContexts+1);
  const float bendSemitones=(8064.f/8191.f)*2.f;const double bentFrequency=440.*std::pow(2.,double(bendSemitones)/12.);
  for(uint32_t i=0;i<64;++i){const auto cycles=(125.+std::min(i,7u))*440./48000.+(i>7?(i-7)*bentFrequency/48000.:0.);const auto analytic=std::sin(6.283185307179586*cycles)*(126.+i)/240.*double(.2f)/16.;require(std::abs(double(a[0][i])-analytic)<1e-8,"actual raw bend preserves phase and changes pitch at exact offset");}
  node.stream.mBufferSize=0;node.add(0,0xb0,64,127);node.add(7,0x80,69,31);referenceNode.packets=node.packets;referenceNode.stream.mBufferSize=node.stream.mBufferSize;
  component.process(contexts,contexts+1);component.process(referenceContexts,referenceContexts+1);
  require(!typedModel->snapshotTelemetryStopped(rawObservation.data(),23)&&(rawObservation[5].samples>>16)==2&&a[0][63]!=0.f,"actual raw sustain holds note-off voice and audible history");
  node.stream.mBufferSize=0;node.add(0,0xb0,64,0);referenceNode.packets=node.packets;referenceNode.stream.mBufferSize=node.stream.mBufferSize;
  component.process(contexts,contexts+1);component.process(referenceContexts,referenceContexts+1);
  require(!typedModel->snapshotTelemetryStopped(rawObservation.data(),23)&&(rawObservation[5].samples>>16)==3&&a[0][63]!=0.f,"actual raw pedal release enters finite release without resetting phase");

  node.stream.mBufferSize=referenceNode.stream.mBufferSize=1024;for(auto &packet:node.packets)packet={0,1,{0xf8,0,0,0},0};referenceNode.packets=node.packets;
  const auto oldCount=rawObservation[1].samples;allocationCount=deletionCount=0;measuring=true;
  for(uint32_t cycle=0;cycle<100;++cycle){component.process(contexts,contexts+1);component.process(referenceContexts,referenceContexts+1);}
  measuring=false;require(allocationCount==0&&deletionCount==0&&published->lastProcessStatus()==0&&!typedModel->snapshotTelemetryStopped(rawObservation.data(),23)&&rawObservation[1].samples==oldCount+102400&&rawObservation[4].samples==1024,"actual maximum raw packet callbacks preserve capacity with zero ordinary C++ new/delete");
  node.stream.mBufferSize=referenceNode.stream.mBufferSize=0;

  node.add(3,0x91,69,127);referenceNode.add(3,0x91,69,127);component.process(contexts,contexts+1);component.process(referenceContexts,referenceContexts+1);
  require(std::abs(double(a[0][63])-std::sin(6.283185307179586*440./48000.*60)*61/240.*double(.2f)/16.)<1e-8&&a[0][63]!=0.f,"fresh independent live voice after maximum loops keeps subsequent rejected-block phase oracle substantive");
  node.stream.mBufferSize=referenceNode.stream.mBufferSize=0;

  require(!typedModel->snapshotTelemetryStopped(rawObservation.data(),23),"stopped live voice history before multi-Context callbacks");const auto beforeMultiple=rawObservation[0].samples;const auto multipleReads=node.reads;void* repeatedContexts[]={context,context};component.process(repeatedContexts,repeatedContexts+2);component.process(referenceContexts,referenceContexts+1);component.process(referenceContexts,referenceContexts+1);require(node.reads==multipleReads+2&&published->lastProcessStatus()==0,"same owner appears twice in one registered callback and independently re-admits sequential contexts");for(uint32_t ch=0;ch<channels;++ch)require(!std::memcmp(a[ch].data(),b[ch].data(),64*sizeof(float)),"repeated same-owner callback retains exact independent live voice advancement");void* separateContexts[]={context,referenceContext};const auto separateReads=node.reads,referenceReads=referenceNode.reads;component.process(separateContexts,separateContexts+2);require(node.reads==separateReads+1&&referenceNode.reads==referenceReads+1&&published->lastProcessStatus()==0&&!typedModel->snapshotTelemetryStopped(rawObservation.data(),23)&&rawObservation[0].samples==beforeMultiple+192,"two separate actual initialized owners process one shared callback list with exact frame history");for(uint32_t ch=0;ch<channels;++ch)require(!std::memcmp(a[ch].data(),b[ch].data(),64*sizeof(float)),"separate-owner multi-Context callback preserves independent phase/audio");
  require(parameters->SetParameterNormalizedValue("p1",.75)==AAX_SUCCESS&&host.requests==1,"instrument SDK host request alone");require(parameters->UpdateParameterNormalizedValue("p1",.75,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS&&reference->UpdateParameterNormalizedValue("p1",.75,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS,"instrument actual acknowledged Level before malformed note");
  uint32_t size=0;AAX_CTypeID chunkID=0;require(parameters->GetChunkIDFromIndex(0,&chunkID)==AAX_SUCCESS&&parameters->GetChunkSize(chunkID,&size)==AAX_SUCCESS&&size==24,"actual complete Level-only native state metadata");alignas(AAX_SPlugInChunk) std::array<uint8_t,128> storage{};auto *chunk=new(storage.data()) AAX_SPlugInChunk{};chunk->fSize=size;chunk->fChunkID=chunkID;chunk->fManufacturerID=0x52617674;chunk->fProductID=0x52764d53;chunk->fPlugInID=channels==1?0x52764d31:0x52764d32;require(parameters->GetChunk(chunkID,chunk)==AAX_SUCCESS&&chunk->fVersion==1&&!std::memcmp(chunk->fData,"RAVETMS\0",8),"actual queued-before-render canonical native Level save");float savedLevel=0;std::memcpy(&savedLevel,chunk->fData+20,4);require(savedLevel==.75f,"accepted pending Level present in complete chunk");
  const float liveRestoredLevel=.3f;std::memcpy(chunk->fData+20,&liveRestoredLevel,4);
  const auto requestsBeforeLive=host.requests;AAX_Result liveUpdate=0,liveRestore=0,liveRequest=0;node.onRead=[&]{liveUpdate=parameters->UpdateParameterNormalizedValue("p1",.4,AAX_eUpdateSource_Unspecified);liveRestore=parameters->SetChunk(chunkID,chunk);liveRequest=parameters->SetParameterNormalizedValue("p1",.4);};component.process(contexts,contexts+1);node.onRead={};require(liveUpdate==AAX_SUCCESS&&liveRestore==AAX_SUCCESS&&liveRequest==AAX_SUCCESS&&host.requests==requestsBeforeLive+1,"raw admitted AUDIO accepts serialized CONTROL SPSC automation/complete restore and original host request status");require(reference->UpdateParameterNormalizedValue("p1",.4,AAX_eUpdateSource_Unspecified)==0&&reference->SetChunk(chunkID,chunk)==0,"independent healthy reference applies matching effective source order");component.process(referenceContexts,referenceContexts+1);for(uint32_t ch=0;ch<channels;++ch)require(!std::memcmp(a[ch].data(),b[ch].data(),64*sizeof(float)),"accepted live restore supersedes earlier scalar without voice/history reset or unacknowledged request application");require(!typedModel->snapshotTelemetryStopped(rawObservation.data(),23)&&rawObservation[6].value==double(liveRestoredLevel),"actual live distinct complete Level.3 restore is consumed rather than ignored/drained");
  const float concurrentRestoredLevel=.6f;std::memcpy(chunk->fData+20,&concurrentRestoredLevel,4);std::array<RavetNativeTelemetryValueV1,23> beforeConcurrent{};require(!typedModel->snapshotTelemetryStopped(beforeConcurrent.data(),23)&&beforeConcurrent[6].value==double(liveRestoredLevel),"joined before getter-blocked publication observes prior acknowledged Level.3");
  std::atomic<bool> concurrentEntered{false},concurrentRelease{false};node.onRead=[&]{concurrentEntered.store(true,std::memory_order_release);while(!concurrentRelease.load(std::memory_order_acquire))std::this_thread::yield();};std::thread audioWorker([&]{component.process(contexts,contexts+1);});while(!concurrentEntered.load(std::memory_order_acquire))std::this_thread::yield();
  const auto concurrentUpdate=parameters->UpdateParameterNormalizedValue("p1",.4,AAX_eUpdateSource_Unspecified),concurrentRestore=parameters->SetChunk(chunkID,chunk),concurrentRequest=parameters->SetParameterNormalizedValue("p1",.4);const auto concurrentLifecycle=typedModel->resetStopped();concurrentRelease.store(true,std::memory_order_release);audioWorker.join();node.onRead={};
  require(concurrentUpdate==0&&concurrentRestore==0&&concurrentRequest==0&&concurrentLifecycle==AAX_ERROR_INVALID_ARGUMENT&&published->lastProcessStatus()==0,"actual concurrent CONTROL accepts SPSC scalar/restore/request while AUDIO admission retains stopped-lifecycle refusal");require(reference->UpdateParameterNormalizedValue("p1",.4,AAX_eUpdateSource_Unspecified)==0&&reference->SetChunk(chunkID,chunk)==0,"concurrent reference effective complete-state source order");component.process(referenceContexts,referenceContexts+1);for(uint32_t ch=0;ch<channels;++ch)require(!std::memcmp(a[ch].data(),b[ch].data(),64*sizeof(float)),"actual concurrent accepted state/control preserves authoritative independent voice phase/audio");require(!typedModel->snapshotTelemetryStopped(rawObservation.data(),23)&&rawObservation[6].value==double(concurrentRestoredLevel)&&rawObservation[6].value!=beforeConcurrent[6].value&&rawObservation[0].samples==beforeConcurrent[0].samples+64,"joined getter admission consumes distinct Level.6 restore and advances one exact block after prior Level.3");std::memcpy(chunk->fData+20,&savedLevel,4);

  require(parameters->SetChunk(chunkID,chunk)==AAX_SUCCESS&&reference->SetChunk(chunkID,chunk)==AAX_SUCCESS,"queue complete restore before late malformed transport");frames=64;a[0][64]=77.f;node.stream.mBufferSize=0;host.looping=true;host.loopStart=INT64_MAX-1;host.loopEnd=INT64_MAX;component.process(contexts,contexts+1);require(published->lastProcessStatus()==1&&published->ready()&&a[0][0]==0.f&&a[0][64]==77.f&&a[0][240]==77.f,"actual registered transport rejects collapsed loop before queued restore/voice advancement, supported prefix silence/tail");const auto transportFailure=published->transportResultStopped();require(transportFailure.fault==ravet::aax::AaxTransportFault::InvalidProjection&&transportFailure.getter==ravet::aax::AaxTransportGetter::Loop&&transportFailure.returned==127&&transportFailure.statuses[5]==AAX_SUCCESS,"actual factory original successful SDK status preserved beside local projection fault");host.looping=false;
  frames=64;node.stream.mBufferSize=0;node.add(0,0x90,70,127);node.add(20,0x90,70,128);component.process(contexts,contexts+1);require(published->lastProcessStatus()==1&&published->lastNativeProcessStatus()==0&&typedModel->rawDiagnosticsStopped().error==Model::Owner::PrecisionError::Preflight&&!typedModel->rawDiagnosticsStopped().nativeAttempted&&published->ready()&&a[0][0]==0.f,"actual registered local late malformed packet preserves last returned native status, ready intent and voice history");node.stream.mBufferSize=referenceNode.stream.mBufferSize=0;
  require(parameters->UpdateParameterNormalizedValue("p1",.6,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS&&reference->UpdateParameterNormalizedValue("p1",.6,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS,"effective trailing Level command after pending complete restore");
  std::array<RavetNativeTelemetryValueV1,23> beforeCapacity{},afterCapacity{};require(!typedModel->snapshotTelemetryStopped(beforeCapacity.data(),23),"read authoritative history before raw+control overflow");const auto beforeGetters=host.transportCalls;
  node.stream.mBufferSize=1024;for(auto &packet:node.packets)packet={0,1,{0xf8,0,0,0},0};
  frames=0;const auto zeroOutput=a;node.packets.back().mData[0]=0xf9;component.process(contexts,contexts+1);
  require(published->lastProcessStatus()==1&&a==zeroOutput&&host.transportCalls==beforeGetters&&!typedModel->snapshotTelemetryStopped(afterCapacity.data(),23)&&!std::memcmp(beforeCapacity.data(),afterCapacity.data(),sizeof(beforeCapacity)),"zero-frame late malformed raw packet preserves queued state/history and all audio/tails");
  node.packets.back().mData[0]=0xf8;component.process(contexts,contexts+1);
  require(published->lastProcessStatus()==1&&a==zeroOutput&&host.transportCalls==beforeGetters&&!typedModel->snapshotTelemetryStopped(afterCapacity.data(),23)&&!std::memcmp(beforeCapacity.data(),afterCapacity.data(),sizeof(beforeCapacity)),"zero-frame combined raw+pending-controller capacity preflights before state/dequeue/history");frames=64;component.process(contexts,contexts+1);
  require(published->lastProcessStatus()==1&&published->ready()&&host.transportCalls==beforeGetters&&!typedModel->snapshotTelemetryStopped(afterCapacity.data(),23)&&!std::memcmp(beforeCapacity.data(),afterCapacity.data(),sizeof(beforeCapacity)),"ALL1024 nonnote raw packets plus trailing control reject before transport/apply/dequeue/native history");node.stream.mBufferSize=0;
  if(channels==2){float *overlap[]={a[0].data(),a[0].data()+1};context[2]=overlap;component.process(contexts,contexts+1);require(published->lastProcessStatus()==1,"actual late output partial overlap before intent/DSP");context[2]=out;}
  alignas(Model::Context) std::array<std::byte,1024> protectedContextStorage{};alignas(AAX_IMIDINode) std::array<std::byte,1024> protectedFrameStorage{};
  auto *protectedFrames=new(protectedFrameStorage.data()) int32_t(frames);auto *typedOwner=published;auto *guardedContext=new(protectedContextStorage.data()) Model::Context{&typedOwner,protectedFrames,out,&node};void *guardedContexts[]={guardedContext};
  // Known public node spans must be admitted before any virtual getter. Keep
  // an actual live voice and pending restore/control here; the matching healthy
  // reference below proves neither rejected block consumed intent nor phase.
  const auto beforeNodeReads=node.reads;const auto beforeNodeGetters=host.transportCalls;
  for(auto* metadataNode:{reinterpret_cast<AAX_IMIDINode*>(&typedOwner),reinterpret_cast<AAX_IMIDINode*>(guardedContext),reinterpret_cast<AAX_IMIDINode*>(guardedContexts),reinterpret_cast<AAX_IMIDINode*>(out),reinterpret_cast<AAX_IMIDINode*>(protectedFrames)}){require(reinterpret_cast<uintptr_t>(metadataNode)%alignof(AAX_IMIDINode)==0,"aligned live metadata alias exercises overlap refusal independently of alignment gate");guardedContext->notes=metadataNode;const auto preservedContext=*guardedContext;const auto preservedOutputPointers=std::array<float*,2>{out[0],out[1]};const auto preservedFrame=*protectedFrames;const auto preservedContextPointer=guardedContexts[0];component.process(guardedContexts,guardedContexts+1);require(!std::memcmp(guardedContext,&preservedContext,sizeof(preservedContext))&&out[0]==preservedOutputPointers[0]&&out[1]==preservedOutputPointers[1]&&*protectedFrames==preservedFrame&&guardedContexts[0]==preservedContextPointer,"aligned fake node aliases preserve each exact known Context/frame/list/output-pointer metadata span");require(node.reads==beforeNodeReads&&typedOwner==published&&published->lastProcessStatus()==1&&published->ready()&&host.transportCalls==beforeNodeGetters,"raw known owner-pointer/Context/Context-array/output-array node aliases reject before virtual dispatch");}
  guardedContext->notes=reinterpret_cast<AAX_IMIDINode*>(published);
  component.process(guardedContexts,guardedContexts+1);
  require(published->lastProcessStatus()==1&&published->ready()&&node.reads==beforeNodeReads&&host.transportCalls==beforeNodeGetters&&!typedModel->snapshotTelemetryStopped(afterCapacity.data(),23)&&!std::memcmp(beforeCapacity.data(),afterCapacity.data(),sizeof(beforeCapacity)),"actual raw owner-aliased node rejects before getter/transport/dequeue/history");
  std::array<std::byte,sizeof(AAX_IMIDINode)+alignof(AAX_IMIDINode)> invalidNodeBytes{};std::memcpy(invalidNodeBytes.data(),static_cast<const void*>(&node),invalidNodeBytes.size());
  guardedContext->notes=reinterpret_cast<AAX_IMIDINode*>(reinterpret_cast<uintptr_t>(&node)+alignof(float));out[0]=reinterpret_cast<float*>(guardedContext->notes);
  component.process(guardedContexts,guardedContexts+1);
  require(node.reads==beforeNodeReads&&published->lastProcessStatus()==1&&published->ready()&&!std::memcmp(invalidNodeBytes.data(),static_cast<const void*>(&node),invalidNodeBytes.size()),"actual raw unaligned node rejects without virtual dispatch or clearing bounded node bytes");out[0]=a[0].data();
  guardedContext->notes=nullptr;component.process(guardedContexts,guardedContexts+1);
  require(node.reads==beforeNodeReads&&published->lastProcessStatus()==1&&published->ready(),"actual raw null node rejects without virtual dispatch");
  guardedContext->notes=&node;
  std::array<std::byte,sizeof(AAX_IMIDINode)> originalNodeBase{};std::memcpy(originalNodeBase.data(),static_cast<const void*>(static_cast<AAX_IMIDINode*>(&node)),originalNodeBase.size());
  out[0]=reinterpret_cast<float*>(static_cast<AAX_IMIDINode*>(&node));if(channels==2){a[1][0]=91.f;a[1][64]=77.f;}
  component.process(guardedContexts,guardedContexts+1);
  require(!std::memcmp(originalNodeBase.data(),static_cast<const void*>(static_cast<AAX_IMIDINode*>(&node)),originalNodeBase.size())&&node.reads==beforeNodeReads+1&&published->lastProcessStatus()==1&&published->ready()&&host.transportCalls==beforeNodeGetters&&!typedModel->snapshotTelemetryStopped(afterCapacity.data(),23)&&!std::memcmp(beforeCapacity.data(),afterCapacity.data(),sizeof(beforeCapacity)),"actual raw writable sample/node-base overlap preserves virtual metadata and native pending intent/history");
  if(channels==2)require(a[1][0]==0.f&&a[1][64]==77.f,"raw node-alias refusal silences independently safe channel prefix and preserves tail");out[0]=a[0].data();
  const auto readsBeforeSelf=node.reads;guardedContext->frames=reinterpret_cast<int32_t*>(published);component.process(guardedContexts,guardedContexts+1);require(published->lastProcessStatus()==1&&published->ready()&&node.reads==readsBeforeSelf&&*protectedFrames==frames,"default instrument self-aliased frame metadata rejects before dereference or MIDI getter");const auto metadataDiagnostic=typedModel->rawDiagnosticsStopped();require(metadataDiagnostic.error==Model::Owner::PrecisionError::Preflight&&!metadataDiagnostic.nativeAttempted&&!metadataDiagnostic.nativeReturned&&!metadataDiagnostic.commandsConsumed&&!metadataDiagnostic.samplesTransferred&&typedModel->lastNativeProcessStatus()==0,"admitted early frame metadata refusal publishes fresh raw preflight flags and preserves last actual native return");if constexpr(RAVET_AAX_NATIVE_SAMPLE_BITS==64){const auto precisionDiagnostic=typedModel->precisionDiagnosticsStopped();require(precisionDiagnostic.error==Model::Owner::PrecisionError::Preflight&&!precisionDiagnostic.nativeAttempted&&!precisionDiagnostic.nativeReturned&&!precisionDiagnostic.samplesTransferred,"selected64 early callback metadata refusal replaces prior success precision flags");}guardedContext->frames=protectedFrames;
  out[0]=reinterpret_cast<float*>(&typedOwner);component.process(guardedContexts,guardedContexts+1);require(typedOwner==published&&published->lastProcessStatus()==1,"default instrument late sample/owner-pointer alias preserves ownership word");
  out[0]=reinterpret_cast<float*>(protectedFrames);component.process(guardedContexts,guardedContexts+1);require(*protectedFrames==frames&&published->lastProcessStatus()==1&&published->ready(),"default instrument late sample/frame metadata overlap preserves frame and queued complete state");out[0]=reinterpret_cast<float*>(guardedContext);const auto originalContext=*guardedContext;component.process(guardedContexts,guardedContexts+1);require(!std::memcmp(guardedContext,&originalContext,sizeof(originalContext))&&published->lastProcessStatus()==1&&published->ready(),"default instrument late sample/context overlap preserves callback metadata and voice history");out[0]=a[0].data();
  component.process(contexts,contexts+1);component.process(referenceContexts,referenceContexts+1);for(uint32_t ch=0;ch<channels;++ch)require(!std::memcmp(a[ch].data(),b[ch].data(),64*sizeof(float)),"actual independent successful callback phase oracle proves rejected block did not advance");
  require(parameters->UpdateParameterNormalizedValue("p1",.5,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS&&parameters->SetChunk(chunkID,chunk)==AAX_SUCCESS,"actual complete restore supersedes older scalar intent");const auto first=chunk->fData[23];chunk->fData[23]=char(0x7f);require(parameters->SetChunk(chunkID,chunk)==AAX_ERROR_INVALID_ARGUMENT,"actual malformed last native value preflight");chunk->fData[23]=first;
  frames=1025;component.process(contexts,contexts+1);require(published->lastProcessStatus()==1&&a[0][1023]==0.f&&a[0][1024]==77.f,"actual excessive-frame supported silence and tail");frames=64;component.process(contexts,contexts+1);
  require(parameters->Uninitialize()==AAX_SUCCESS&&reference->Uninitialize()==AAX_SUCCESS&&host.references==1&&host.registered.empty(),"actual instrument SDK/native checked retirement");require(parameters->Initialize(host.unknown())==AAX_SUCCESS&&parameters->ResetFieldData(0,&owner,sizeof(owner))==AAX_SUCCESS,"actual fresh initialized instrument retains accepted Level intent");node.stream.mBufferSize=0;node.add(0,0x90,69,127);component.process(contexts,contexts+1);const auto analytic=std::sin(6.283185307179586*440./48000.*63)*64/240.*double(.75f)/16.;require(std::abs(double(a[0][63])-analytic)<1e-8,"fresh instrument has reset voice history and retained accepted complete Level");require(parameters->Uninitialize()==AAX_SUCCESS,"actual final native instrument retirement");
 }
 require(shutdown(startupHost.unknown())==AAX_SUCCESS&&startupHost.references==1&&exit(),"actual SDK shutdown/module exit balance startup lifetimes");CFRelease(bundle);
 dlclose(module);
}
}
int main(int argc,char **argv){try{require(argc==2,"supply actual instrument module");componentContracts(argv[1]);std::cout<<"PASS actual registered AAX transport capture and late-fault queued restore/voice history, native instrument factories, None/mono/stereo SDK lifecycle, MIDI descriptor/timed analytic audio, Level chunk intent, independent late-fault phase oracle and retirement\n";return 0;}catch(const std::exception &e){std::cerr<<e.what()<<'\n';return 1;}}
