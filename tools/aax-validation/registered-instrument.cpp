#include "RavetAaxFactoryRegistration.hpp"
#include "AAX_IComponentDescriptor.h"
#include "AAX_IPropertyMap.h"
#include "TransportControllerHost.hpp"
#include "PerformanceParameters.hpp"

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
 std::array<AAX_CMidiPacket,1024> packets{};AAX_CMidiStream stream{0,packets.data()};uint32_t reads=0;
 AAX_CMidiStream *GetNodeBuffer()override{++reads;return &stream;}
 AAX_Result PostMIDIPacket(AAX_CMidiPacket*)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_ITransport *GetTransport()override{throw std::runtime_error("no unversioned transport request");}
 void add(uint32_t frame,uint8_t status,uint8_t key,uint8_t velocity){packets[stream.mBufferSize++]={frame,3,{status,key,velocity,0},0};}
};
using Model=RavetPerformanceAaxParameters;
void componentContracts(const char *path){
 void *module=dlopen(path,RTLD_NOW|RTLD_LOCAL);if(!module)throw std::runtime_error(dlerror());
 using Describe=AAX_Result(*)(AAX_ICollection*);auto describe=reinterpret_cast<Describe>(dlsym(module,"_Z21GetEffectDescriptionsP15AAX_ICollection"));require(describe,"actual instrument native descriptor export");
 Collection refused;refused.descriptors[0].createStatus=AAX_ERROR_UNIMPLEMENTED;require(describe(&refused)==AAX_ERROR_UNIMPLEMENTED&&refused.effects.empty()&&refused.used==1,"first original instrument registration refusal retained");
 Collection registered;require(describe(&registered)==AAX_SUCCESS&&registered.effects.size()==2,"actual instrument mono/stereo public registration");
 for(uint32_t channels:{1u,2u}){
  auto &descriptor=registered.descriptors[channels-1];auto &component=descriptor.component;TransportControllerHost host,referenceHost;
  host.effect=referenceHost.effect=channels==1?"org.ravet.performance.mono":"org.ravet.performance.stereo";host.input=referenceHost.input=AAX_eStemFormat_None;host.output=referenceHost.output=channels==1?AAX_eStemFormat_Mono:AAX_eStemFormat_Stereo;
  require(registered.effects[channels-1].first==host.effect&&descriptor.category==AAX_ePlugInCategory_SWGenerators&&component.audioIn==-1&&component.audioOut==2&&component.frames==1&&component.privateField==0&&component.privateSize==sizeof(void*)&&component.midi==3&&component.midiType==AAX_eMIDINodeType_LocalInput&&component.mask==0xffff&&component.midiName=="MIDI Input"&&component.process&&!component.init,"actual exact instrument context/MIDI/category descriptor");
  require(component.properties.values.at(AAX_eProperty_InputStemFormat)==AAX_CPropertyValue(AAX_eStemFormat_None)&&component.properties.values.at(AAX_eProperty_OutputStemFormat)==AAX_CPropertyValue(host.output)&&component.properties.values.at(AAX_eProperty_CanBypass)==0&&component.properties.values.at(AAX_eProperty_Constraint_MultiMonoSupport)==0&&component.properties.values.at(AAX_eProperty_PlugInID_Native)==(channels==1?0x52765031:0x52765032)&&!component.properties.values.count(AAX_eProperty_PlugInID_AudioSuite),"instrument no bypass/no AudioSuite/explicit independent native IDs");
  using Create=AAX_IEffectParameters*(AAX_CALLBACK*)();auto factory=reinterpret_cast<Create>(descriptor.create);std::unique_ptr<AAX_IEffectParameters> parameters(factory()),reference(factory());require(parameters&&reference,"actual instrument exported factories");
  require(parameters->Initialize(host.unknown())==AAX_SUCCESS&&reference->Initialize(referenceHost.unknown())==AAX_SUCCESS&&host.registered.size()==1,"actual real SDK instrument initialization and native Level-only objects");
  void *owner=nullptr,*referenceOwner=nullptr;require(parameters->ResetFieldData(0,&owner,sizeof(owner))==AAX_SUCCESS&&reference->ResetFieldData(0,&referenceOwner,sizeof(referenceOwner))==AAX_SUCCESS,"actual instrument checked ready context owners");auto *published=static_cast<Model::Owner*>(owner);
  std::array<std::array<float,1026>,2> a{},b{};for(auto &ch:a)ch.fill(77.f);for(auto &ch:b)ch.fill(77.f);float *out[]={a[0].data(),a[1].data()},*expected[]={b[0].data(),b[1].data()};Node node,referenceNode;int32_t frames=240;void *context[4]={&owner,&frames,out,&node},*referenceContext[4]={&referenceOwner,&frames,expected,&referenceNode};void *contexts[]={context},*referenceContexts[]={referenceContext};node.add(3,0x90,69,64);referenceNode.add(3,0x90,69,64);component.process(contexts,contexts+1);component.process(referenceContexts,referenceContexts+1);
  if constexpr(RAVET_AAX_NATIVE_SAMPLE_BITS==64){const auto precision=published->precisionDiagnosticsStopped();require(precision.error==Model::Owner::PrecisionError::None&&precision.nativeAttempted&&precision.nativeReturned&&precision.nativeStatus==0&&precision.samplesTransferred&&published->lastNativeProcessStatus()==0,"actual distributed Performance factory traverses authoritative native64 DSP and complete host-f32 conversion");}
  require(node.reads==1&&published->lastProcessStatus()==0&&published->transportSnapshotStopped().available()==127&&published->transportSnapshotStopped().transport().sample_position==123,"actual registered instrument initialized SDK proxy supplies native transport");for(auto count:host.transportCalls)require(count==1,"actual factory transport getter once per successful block");
  for(uint32_t i=0;i<240;++i){const auto n=i<3?0:i-3;const auto analytic=i<3?0.:std::sin(6.283185307179586*440./48000.*n)*(n+1)/240.*double(.2f)*double(64.f/127.f)/16.;require(std::abs(double(a[0][i])-analytic)<1e-8,"actual registered instrument independent analytic MIDI audio timing");if(channels==2)require(a[1][i]==a[0][i],"actual registered stereo center");}require(a[0][240]==77.f,"actual active backing tail");
  require(parameters->SetParameterNormalizedValue("p1",.75)==AAX_SUCCESS&&host.requests==1,"instrument SDK host request alone");require(parameters->UpdateParameterNormalizedValue("p1",.75,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS&&reference->UpdateParameterNormalizedValue("p1",.75,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS,"instrument actual acknowledged Level before malformed note");
  uint32_t size=0;AAX_CTypeID chunkID=0;require(parameters->GetChunkIDFromIndex(0,&chunkID)==AAX_SUCCESS&&parameters->GetChunkSize(chunkID,&size)==AAX_SUCCESS&&size==24,"actual complete Level-only native state metadata");alignas(AAX_SPlugInChunk) std::array<uint8_t,128> storage{};auto *chunk=new(storage.data()) AAX_SPlugInChunk{};chunk->fSize=size;chunk->fChunkID=chunkID;chunk->fManufacturerID=0x52617674;chunk->fProductID=0x52765073;chunk->fPlugInID=channels==1?0x52765031:0x52765032;require(parameters->GetChunk(chunkID,chunk)==AAX_SUCCESS&&chunk->fVersion==1&&!std::memcmp(chunk->fData,"RAVETPS\0",8),"actual queued-before-render canonical native Level save");float savedLevel=0;std::memcpy(&savedLevel,chunk->fData+20,4);require(savedLevel==.75f,"accepted pending Level present in complete chunk");
  require(parameters->SetChunk(chunkID,chunk)==AAX_SUCCESS&&reference->SetChunk(chunkID,chunk)==AAX_SUCCESS,"queue complete restore before late malformed transport");frames=64;node.stream.mBufferSize=0;host.looping=true;host.loopStart=INT64_MAX-1;host.loopEnd=INT64_MAX;component.process(contexts,contexts+1);require(published->lastProcessStatus()==1&&published->ready()&&a[0][0]==0.f&&a[0][64]!=0.f&&a[0][240]==77.f,"actual registered transport rejects collapsed loop before queued restore/voice advancement, supported prefix silence/tail");const auto transportFailure=published->transportResultStopped();require(transportFailure.fault==ravet::aax::AaxTransportFault::InvalidProjection&&transportFailure.getter==ravet::aax::AaxTransportGetter::Loop&&transportFailure.returned==127&&transportFailure.statuses[5]==AAX_SUCCESS,"actual factory original successful SDK status preserved beside local projection fault");host.looping=false;
  frames=64;node.stream.mBufferSize=0;node.add(0,0x90,70,127);node.add(20,0x90,70,128);component.process(contexts,contexts+1);require(published->lastProcessStatus()==1&&published->ready()&&a[0][0]==0.f,"actual registered late malformed packet preserves ready owner/intent/voice history");node.stream.mBufferSize=referenceNode.stream.mBufferSize=0;
  if(channels==2){float *overlap[]={a[0].data(),a[0].data()+1};context[2]=overlap;component.process(contexts,contexts+1);require(published->lastProcessStatus()==1,"actual late output partial overlap before intent/DSP");context[2]=out;}
  alignas(Model::Context) std::array<std::byte,1024> protectedContextStorage{};alignas(int32_t) std::array<std::byte,1024> protectedFrameStorage{};
  auto *protectedFrames=new(protectedFrameStorage.data()) int32_t(frames);auto *typedOwner=published;auto *guardedContext=new(protectedContextStorage.data()) Model::Context{&typedOwner,protectedFrames,out,&node};void *guardedContexts[]={guardedContext};
  const auto readsBeforeSelf=node.reads;guardedContext->frames=reinterpret_cast<int32_t*>(published);component.process(guardedContexts,guardedContexts+1);require(published->lastProcessStatus()==1&&published->ready()&&node.reads==readsBeforeSelf&&*protectedFrames==frames,"default instrument self-aliased frame metadata rejects before dereference or MIDI getter");guardedContext->frames=protectedFrames;
  out[0]=reinterpret_cast<float*>(&typedOwner);component.process(guardedContexts,guardedContexts+1);require(typedOwner==published&&published->lastProcessStatus()==1,"default instrument late sample/owner-pointer alias preserves ownership word");
  out[0]=reinterpret_cast<float*>(protectedFrames);component.process(guardedContexts,guardedContexts+1);require(*protectedFrames==frames&&published->lastProcessStatus()==1&&published->ready(),"default instrument late sample/frame metadata overlap preserves frame and queued complete state");out[0]=reinterpret_cast<float*>(guardedContext);const auto originalContext=*guardedContext;component.process(guardedContexts,guardedContexts+1);require(!std::memcmp(guardedContext,&originalContext,sizeof(originalContext))&&published->lastProcessStatus()==1&&published->ready(),"default instrument late sample/context overlap preserves callback metadata and voice history");out[0]=a[0].data();
  component.process(contexts,contexts+1);component.process(referenceContexts,referenceContexts+1);for(uint32_t ch=0;ch<channels;++ch)require(!std::memcmp(a[ch].data(),b[ch].data(),64*sizeof(float)),"actual independent successful callback phase oracle proves rejected block did not advance");
  require(parameters->UpdateParameterNormalizedValue("p1",.5,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS&&parameters->SetChunk(chunkID,chunk)==AAX_SUCCESS,"actual complete restore supersedes older scalar intent");const auto first=chunk->fData[23];chunk->fData[23]=char(0x7f);require(parameters->SetChunk(chunkID,chunk)==AAX_ERROR_INVALID_ARGUMENT,"actual malformed last native value preflight");chunk->fData[23]=first;
  frames=1025;component.process(contexts,contexts+1);require(published->lastProcessStatus()==1&&a[0][1023]==0.f&&a[0][1024]==77.f,"actual excessive-frame supported silence and tail");frames=64;component.process(contexts,contexts+1);
  require(parameters->Uninitialize()==AAX_SUCCESS&&reference->Uninitialize()==AAX_SUCCESS&&host.references==1&&host.registered.empty(),"actual instrument SDK/native checked retirement");require(parameters->Initialize(host.unknown())==AAX_SUCCESS&&parameters->ResetFieldData(0,&owner,sizeof(owner))==AAX_SUCCESS,"actual fresh initialized instrument retains accepted Level intent");node.stream.mBufferSize=0;node.add(0,0x90,69,127);component.process(contexts,contexts+1);const auto analytic=std::sin(6.283185307179586*440./48000.*63)*64/240.*double(.75f)/16.;require(std::abs(double(a[0][63])-analytic)<1e-8,"fresh instrument has reset voice history and retained accepted complete Level");require(parameters->Uninitialize()==AAX_SUCCESS,"actual final native instrument retirement");
 }
 dlclose(module);
}
}
int main(int argc,char **argv){try{require(argc==2,"supply actual instrument module");componentContracts(argv[1]);std::cout<<"PASS actual registered AAX transport capture and late-fault queued restore/voice history, native instrument factories, None/mono/stereo SDK lifecycle, MIDI descriptor/timed analytic audio, Level chunk intent, independent late-fault phase oracle and retirement\n";return 0;}catch(const std::exception &e){std::cerr<<e.what()<<'\n';return 1;}}
