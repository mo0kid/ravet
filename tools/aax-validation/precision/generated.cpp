#include "RavetAaxFactoryRegistration.hpp"
#include "AAX_IComponentDescriptor.h"
#include "AAX_IPropertyMap.h"
#include "TransportControllerHost.hpp"
#include "RavetAaxEffectParameters.hpp"
#include "RavetNoteRouting.h"

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
#include <atomic>
#include <thread>
#include <cstdlib>
#include <new>
static std::atomic<uint64_t> allocations{0};
void* operator new(size_t n){allocations.fetch_add(1);if(void* p=std::malloc(n?n:1))return p;throw std::bad_alloc();}
void* operator new[](size_t n){return ::operator new(n);}
void operator delete(void* p)noexcept{std::free(p);}
void operator delete[](void* p)noexcept{std::free(p);}
void operator delete(void* p,size_t)noexcept{std::free(p);}
void operator delete[](void* p,size_t)noexcept{std::free(p);}
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
 std::vector<std::pair<int,AAX_EMIDINodeType>> nodes;AAX_Result nodeStatus=AAX_SUCCESS;
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
 AAX_Result AddMIDINode(AAX_CFieldIndex field,AAX_EMIDINodeType type,const char[],uint32_t mask)override{if(mask!=0xffff)return AAX_ERROR_INVALID_ARGUMENT;nodes.emplace_back(field,type);return nodeStatus;}
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
using Model=ravet::aax::NativeAaxEffectParameters<1,32,true,1024,false,false,1024,false,64>;
RavetNativeProcessorEntryV1 core{};RavetNativeParameterEntryV1 parametersTable{};RavetNativeCachedParameterEntryV1 cache{};RavetNativeAudioStateEntryV1 audio{};RavetNativeStateEntryV1 state{};RavetNativeTelemetryEntryV1 telemetry{};
bool precisionOverflow=false;
void *lastHandle=nullptr;uint32_t nativeCalls=0,lastCapacity=0,lastCount=0,applyCalls=0;float lastApplied=0.f;bool corrupt=false;
const RavetNativeProcessorEntryV1 *coreOrigin(){return &core;}
const RavetNativeParameterEntryV1 *parameterOrigin(){return &parametersTable;}
const RavetNativeCachedParameterEntryV1 *cacheOrigin(){return &cache;}
int32_t recordProcess(void *p,const RavetNativeProcessorBusV1 *b,uint32_t count,uint32_t frames,const RavetNativeEventV1 *e,uint32_t n,const RavetNativeTransportV1 *t,RavetNativeEventV1 *out,uint32_t capacity,RavetNativeProcessorResultV1 *r){lastHandle=p;++nativeCalls;lastCapacity=capacity;const auto status=ravet_note_routing_processor_v1(64)->process(p,b,count,frames,e,n,t,out,capacity,r);lastCount=r->event_count;if(precisionOverflow&&!status&&frames){auto samples=static_cast<double*const*>(b[count-1].outputs);const uint64_t bits=UINT64_C(0x7fefffffffffffff);std::memcpy(samples[b[count-1].output_channels-1]+frames-1,&bits,8);}if(corrupt&&!status&&r->event_count){auto& late=out[r->event_count-1];late.kind=RAVET_NATIVE_NOTE_OFF_SCOPED;late.scope=RAVET_NATIVE_SCOPE_CHANNEL|RAVET_NATIVE_SCOPE_KEY;}return status;}
int32_t recordApply(void* handle,const void* cached,const RavetNativeParameterValueV1* values,uint32_t count){++applyCalls;if(count==1)lastApplied=values[0].value;return ravet_note_routing_audio_state_v1(64)->apply(handle,cached,values,count);}
void initializeTables(){
 require(ravet_note_routing_processor_v1(64)&&ravet_note_routing_audio_state_v1(64)&&ravet_note_routing_state_v1(64)&&ravet_note_routing_telemetry_v1(64),"actual complete native producer exports");
 core=*ravet_note_routing_processor_v1(64);core.process=recordProcess;
 audio=*ravet_note_routing_audio_state_v1(64);audio.apply=recordApply;cache=*audio.cached_parameters_entry();parametersTable=*cache.parameters_entry();
 parametersTable.processor_entry=coreOrigin;cache.parameters_entry=parameterOrigin;audio.cached_parameters_entry=cacheOrigin;
 state=*ravet_note_routing_state_v1(64);state.parameters_entry=parameterOrigin;telemetry=*ravet_note_routing_telemetry_v1(64);telemetry.processor_entry=coreOrigin;
}
constexpr const char *identities[]={"org.ravet.validation.noterouting.mono","org.ravet.validation.noterouting.stereo"};
template<unsigned Channels>AAX_IEffectParameters *AAX_CALLBACK create(){try{return new Model({identities[Channels-1],Channels,0x52617674,0x52764e74,0x52764e30+Channels,0x52764e53},&core,&audio,&cache,&state);}catch(...){return nullptr;}}
using Create=AAX_IEffectParameters*(AAX_CALLBACK*)();
AAX_Result describe(AAX_ICollection& host,unsigned channels){
 auto* descriptor=host.NewDescriptor();if(!descriptor)return AAX_ERROR_NULL_OBJECT;
 auto status=descriptor->AddName("RAVET Note Routing validation");if(status)return status;
 status=descriptor->AddProcPtr(reinterpret_cast<void*>(channels==1?create<1>:create<2>),kAAX_ProcPtrID_Create_EffectParameters);if(status)return status;
 auto* component=descriptor->NewComponentDescriptor();if(!component)return AAX_ERROR_NULL_OBJECT;
 using Context=Model::Context;
 status=component->AddPrivateData(AAX_FIELD_INDEX(Context,owner),sizeof(Model::Owner*));if(status)return status;
 status=component->AddAudioBufferLength(AAX_FIELD_INDEX(Context,frames));if(status)return status;
 status=component->AddAudioOut(AAX_FIELD_INDEX(Context,outputs));if(status)return status;
 status=component->AddMIDINode(AAX_FIELD_INDEX(Context,notes),AAX_eMIDINodeType_LocalInput,"Note input",0xffff);if(status)return status;
 status=component->AddMIDINode(AAX_FIELD_INDEX(Context,generated),AAX_eMIDINodeType_LocalOutput,"Generated notes",0xffff);if(status)return status;
 status=component->AddProcessProc_Native(reinterpret_cast<AAX_CProcessProc>(Model::process),component->NewPropertyMap());if(status)return status;
 status=descriptor->AddComponent(component);if(status)return status;return host.AddEffect(identities[channels-1],descriptor);
}
struct Node final:AAX_IMIDINode{
 std::array<AAX_CMidiPacket,1024> input{},posted{};AAX_CMidiStream stream{0,input.data()};uint32_t reads=0,calls=0,refuseAt=UINT32_MAX,throwAt=UINT32_MAX;
 void(*hook)(void*)=nullptr;void* opaque=nullptr;bool hookRead=false,hookPost=false,stable=true;
 AAX_CMidiStream* GetNodeBuffer()override{++reads;if(hookRead)hook(opaque);return &stream;}
 AAX_ITransport* GetTransport()override{throw std::runtime_error("no unversioned transport");}
 AAX_Result PostMIDIPacket(AAX_CMidiPacket* p)override{const auto original=*p;const auto i=calls++;if(i<posted.size())posted[i]=original;if(hookPost){hook(opaque);stable=stable&&std::memcmp(&original,p,sizeof(original))==0;}if(i==throwAt)throw 7;return i==refuseAt?AAX_Result(-7342):AAX_SUCCESS;}
 void add(uint32_t offset,uint8_t status,uint8_t key,uint8_t velocity){input[stream.mBufferSize++]={offset,3,{status,key,velocity,0},0};}
 void clear(){stream.mBufferSize=0;calls=0;refuseAt=throwAt=UINT32_MAX;hookRead=hookPost=false;stable=true;}
};
struct Reentry{Component* component;Model::Context* context;void** contexts;Node* input;bool stable=true;uint32_t nested=0;
 static void call(void* data){auto& self=*static_cast<Reentry*>(data);const auto calls=nativeCalls,reads=self.input->reads;std::array<float,1026> sentinel{};sentinel.fill(77.f);float* scratch[]={sentinel.data(),sentinel.data()};auto* original=self.context->outputs;self.context->outputs=scratch;self.component->process(self.contexts,self.contexts+1);self.context->outputs=original;++self.nested;self.stable=self.stable&&nativeCalls==calls&&self.input->reads==reads&&sentinel[0]==77.f&&sentinel[1025]==77.f;}
};
struct ConcurrentAdmission {
 std::atomic<bool> entered{false},release{false};
 static void block(void* data){auto& self=*static_cast<ConcurrentAdmission*>(data);self.entered.store(true,std::memory_order_release);while(!self.release.load(std::memory_order_acquire))std::this_thread::yield();}
};
void canonicalScopedAcceptance(){
 const auto* table=ravet_note_routing_processor_v1(64);auto* handle=table->create();require(handle,"independent canonical probe handle");
 RavetNativeProcessorConfigV1 config{};config.version=1;config.maximum_frames=8;config.maximum_events=1;config.maximum_output_events=1024;config.bus_count=1;config.sample_rate=48000.;config.buses[0]={0,1};require(table->prepare(handle,&config)==0,"independent probe prepare");
 std::array<double,9> samples{};double* outputs[]={samples.data()};RavetNativeProcessorBusV1 bus{0,1,nullptr,outputs};std::array<RavetNativeEventV1,1024> generated{};RavetNativeEventV1 scoped{};scoped.kind=RAVET_NATIVE_NOTE_OFF_SCOPED;scoped.offset=3;scoped.channel=3;scoped.key=72;scoped.scope=RAVET_NATIVE_SCOPE_CHANNEL|RAVET_NATIVE_SCOPE_KEY;scoped.value=double(float(64)/127.f);RavetNativeProcessorResultV1 report{};
 require(table->process(handle,&bus,1,8,&scoped,1,nullptr,generated.data(),1024,&report)==0&&report.event_count==1&&generated[0].kind==RAVET_NATIVE_NOTE_OFF&&generated[0].offset==3,"authoritative Rust canonical validation accepts the exact scoped-release record rejected by format stage");
 const auto* meter=ravet_note_routing_telemetry_v1(64);auto* cached=meter->create_cache();RavetNativeTelemetryValueV1 before{},after{};require(cached&&meter->snapshot(handle,cached,&before,1)==0&&before.samples==8,"independent native history before capacity rejection");
 require(table->process(handle,&bus,1,8,nullptr,0,nullptr,generated.data(),1,&report)==1&&report.event_count==0&&meter->snapshot(handle,cached,&after,1)==0&&after.samples==before.samples,"full prepared output capacity required even an empty burst, before native DSP history");
 require(table->process(handle,&bus,1,8,&scoped,1,nullptr,generated.data(),1,&report)==1&&report.event_count==0&&meter->snapshot(handle,cached,&after,1)==0&&after.samples==before.samples,"produced one-event expectation cannot replace prepared capacity");
 meter->destroy_cache(cached);table->destroy(handle);
}
void contracts(){
 canonicalScopedAcceptance();initializeTables();Collection rejected;rejected.descriptors[0].component.nodeStatus=-8123;require(describe(rejected,1)==-8123&&rejected.effects.empty()&&!rejected.descriptors[0].component.process,"original output registration refusal before effect/algorithm publication");
 Collection registered;require(describe(registered,1)==AAX_SUCCESS&&describe(registered,2)==AAX_SUCCESS&&registered.effects.size()==2,"actual registered mono/stereo generated profiles");
 auto* telemetryCache=telemetry.create_cache();require(telemetryCache,"actual telemetry schema");
 for(unsigned channels:{1u,2u}){
  auto& descriptor=registered.descriptors[channels-1];auto& component=descriptor.component;
  require(component.audioIn==-1&&component.audioOut==2&&component.privateField==0&&component.frames==1&&component.privateSize==sizeof(Model::Owner*)&&component.nodes==std::vector<std::pair<int,AAX_EMIDINodeType>>{{3,AAX_eMIDINodeType_LocalInput},{4,AAX_eMIDINodeType_LocalOutput}}&&component.process,"actual SDK input/output fields and unchanged required owner layout");
  TransportControllerHost host;host.effect=identities[channels-1];host.input=AAX_eStemFormat_None;host.output=channels==1?AAX_eStemFormat_Mono:AAX_eStemFormat_Stereo;
  std::unique_ptr<AAX_IEffectParameters> parameters(reinterpret_cast<Create>(descriptor.create)());require(parameters&&parameters->Initialize(host.unknown())==AAX_SUCCESS&&host.references==4&&host.registered.size()==1,"registered factory real SDK Initialize Level-only native producer");
  Model::Owner* owner=nullptr;require(parameters->ResetFieldData(0,&owner,sizeof(owner))==AAX_SUCCESS&&owner,"ready registered owner published");
  std::array<float,1026> left{},right{};left.fill(77.f);right.fill(77.f);float* outputs[]={left.data(),right.data()};int32_t frames=8;Node input,output;
  Model::Context context{&owner,&frames,outputs,&input,&output};void* contexts[]={&context};auto invoke=[&]{component.process(contexts,contexts+1);};
  Reentry reentry{&component,&context,contexts,&input};input.hook=output.hook=Reentry::call;input.opaque=output.opaque=&reentry;input.hookRead=true;output.hookPost=true;
  input.add(0,0x93,72,127);input.add(3,0x83,72,64);input.add(7,0x93,72,0);invoke();
  require(owner->lastProcessStatus()==0&&lastCapacity==1024&&lastCount==3&&output.calls==3&&output.posted[0].mTimestamp==0&&output.posted[0].mData[0]==0x93&&output.posted[0].mData[2]==127&&output.posted[1].mTimestamp==3&&output.posted[1].mData[0]==0x83&&output.posted[1].mData[2]==64&&output.posted[2].mTimestamp==7&&output.posted[2].mData[0]==0x83&&output.posted[2].mData[2]==0&&output.stable&&reentry.stable&&reentry.nested==4,"actual Rust output byte/offset/release oracle and registered input-getter/output-post reentry before getter/DSP/silence");
  require(left[0]==0.f&&left[7]==0.f&&left[8]==77.f&&(channels==1||right[7]==0.f),"silent reference supported prefix/tails");
  auto samples=[&]{RavetNativeTelemetryValueV1 v{};require(lastHandle&&telemetry.snapshot(lastHandle,telemetryCache,&v,1)==0,"actual stopped Rust history telemetry");return v.samples;};
  require(samples()==8,"actual native frame counter after one valid call");input.clear();output.clear();
  require(parameters->UpdateParameterNormalizedValue("p1",.5,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS,"queued Level .5");
  uint32_t bytes=0;require(parameters->GetChunkSize(0x52764e53,&bytes)==AAX_SUCCESS&&bytes==24,"complete Level codec size");alignas(AAX_SPlugInChunk) std::array<uint8_t,128> chunkBytes{};auto* chunk=new(chunkBytes.data())AAX_SPlugInChunk{};chunk->fSize=int32_t(bytes);chunk->fChunkID=0x52764e53;chunk->fManufacturerID=0x52617674;chunk->fProductID=0x52764e74;chunk->fPlugInID=0x52764e30+channels;require(parameters->GetChunk(chunk->fChunkID,chunk)==AAX_SUCCESS,"save complete accepted Level intent");
  // SDK control producer runs concurrently with a registered AUDIO callback
  // held inside its foreign input getter. Availability must reject without
  // SDK-manager/native/queue mutation, while another callback admission is Busy.
  ConcurrentAdmission concurrent;input.hook=ConcurrentAdmission::block;input.opaque=&concurrent;input.hookRead=true;
  const auto concurrentCalls=nativeCalls;const auto concurrentHistory=samples();double originalNormalized=-1.;require(parameters->GetParameterNormalizedValue("p1",&originalNormalized)==AAX_SUCCESS&&originalNormalized==.5,"complete control intent before concurrent AUDIO");
  std::thread worker([&]{invoke();});struct JoinedOnExit{ConcurrentAdmission& gate;std::thread& worker;~JoinedOnExit(){gate.release.store(true,std::memory_order_release);if(worker.joinable())worker.join();}} joined{concurrent,worker};while(!concurrent.entered.load(std::memory_order_acquire))std::this_thread::yield();
  require(owner->processingAudio(),"registered worker callback holds atomic admission before MIDI getter returns");
  require(parameters->UpdateParameterNormalizedValue("p1",.75,AAX_eUpdateSource_Unspecified)==AAX_ERROR_INVALID_ARGUMENT&&parameters->SetChunk(chunk->fChunkID,chunk)==AAX_ERROR_INVALID_ARGUMENT,"concurrent scalar/complete restore reject during active admission without mutation");
  auto rejectedAdmission=owner->admitGeneratedAudio();require(!rejectedAdmission&&owner->processingAudio(),"second actual owner admission CAS rejects without clearing outer lease");
  double retainedNormalized=-1.;require(parameters->GetParameterNormalizedValue("p1",&retainedNormalized)==AAX_SUCCESS&&retainedNormalized==originalNormalized,"concurrent rejected controls preserve SDK normalized manager intent");
  concurrent.release.store(true,std::memory_order_release);worker.join();require(!owner->processingAudio()&&owner->lastProcessStatus()==0&&nativeCalls==concurrentCalls+1&&samples()==concurrentHistory+8,"worker actual Rust processing completes and releases admission for CONTROL");
  input.clear();output.clear();input.hook=output.hook=Reentry::call;input.opaque=output.opaque=&reentry;
  require(parameters->UpdateParameterNormalizedValue("p1",.75,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS&&parameters->SetChunk(chunk->fChunkID,chunk)==AAX_SUCCESS&&parameters->UpdateParameterNormalizedValue("p1",.25,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS,"pending complete recall supersedes old controls, trailing Level retained");
  input.add(0,0x93,72,127);input.add(7,0xf9,72,1);const auto calls=nativeCalls,applies=applyCalls;const auto before=samples();invoke();require(owner->lastProcessStatus()==1&&nativeCalls==calls&&applyCalls==applies&&samples()==before&&!output.calls&&owner->ready(),"malformed last MIDI before complete apply/dequeue/DSP");input.stream.mBufferSize=1;
  context.generated=nullptr;const auto reads=input.reads;invoke();require(owner->lastProcessStatus()==1&&input.reads==reads&&nativeCalls==calls&&applyCalls==applies&&samples()==before,"known null output node rejects before input getter/state/DSP");
  const auto originalPacket=input.input[0];const auto originalHeader=input.stream;outputs[0]=reinterpret_cast<float*>(&input);invoke();require(input.input[0].mTimestamp==originalPacket.mTimestamp&&input.input[0].mData[0]==originalPacket.mData[0]&&input.stream.mBufferSize==originalHeader.mBufferSize&&input.stream.mBuffer==originalHeader.mBuffer&&input.reads==reads&&nativeCalls==calls&&applyCalls==applies,"known invalid output node cannot silence live valid input node metadata");outputs[0]=left.data();context.generated=&output;
  outputs[0]=reinterpret_cast<float*>(&input.stream);require(owner->processGeneratedNotes(outputs,frames,&input.stream,nullptr)==1&&input.stream.mBufferSize==originalHeader.mBufferSize&&input.stream.mBuffer==originalHeader.mBuffer&&nativeCalls==calls&&applyCalls==applies,"direct null output node preflights borrowed stream header before silence");outputs[0]=left.data();
  outputs[0]=reinterpret_cast<float*>(&frames);invoke();require(frames==8&&nativeCalls==calls&&applyCalls==applies&&samples()==before&&!output.calls,"live frame output overlap rejected before state/DSP");outputs[0]=left.data();
  invoke();require(owner->lastProcessStatus()==0&&applyCalls==applies+1&&lastApplied==.5f&&owner->outputDiagnosticsStopped().stateAttempted&&owner->outputDiagnosticsStopped().stateStatus==0&&samples()==before+8&&output.calls==1&&output.posted[0].mData[2]==32&&lastCapacity==1024,"valid next alone applies retained complete/trailing Level, full capacity even one generated");input.clear();output.clear();
  require(parameters->UpdateParameterNormalizedValue("p1",.5,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS,"queued control before maximum notes");frames=1024;for(uint32_t i=0;i<1024;++i)input.add(i,0x93,uint8_t(i%128),127);const auto capacityCalls=nativeCalls;const auto capacityFrames=samples();invoke();require(owner->lastProcessStatus()==1&&nativeCalls==capacityCalls&&samples()==capacityFrames&&!output.calls,"combined effective control/notes capacity before DSP and global queue");input.stream.mBufferSize=1023;invoke();require(owner->lastProcessStatus()==0&&samples()==capacityFrames+1024&&output.calls==1023&&output.posted[0].mData[2]==64&&lastCapacity==1024,"retained control plus1023 actual generated records, exact capacity");
  output.clear();input.stream.mBufferSize=1024;invoke();require(owner->lastProcessStatus()==0&&output.calls==1024&&lastCount==1024&&output.posted[1023].mTimestamp==1023,"maximum real native generated burst");
  const auto allocationBefore=allocations.load(),maximumHistory=samples();
  for(uint32_t cycle=0;cycle<1000;++cycle){output.calls=0;invoke();require(owner->lastProcessStatus()==0&&output.calls==1024,"repeated maximum actual initialized native output");}
  require(allocations.load()==allocationBefore&&samples()==maximumHistory+1000*1024,"maximum actual owner/model callbacks allocate zero C++ heap and advance exact Rust history");
  input.clear();output.clear();frames=8;input.add(0,0x93,72,127);input.add(3,0x83,72,64);require(parameters->UpdateParameterNormalizedValue("p1",.25,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS,"control consumed even late output encoding failure");corrupt=true;const auto history=samples();invoke();corrupt=false;auto diag=owner->outputDiagnosticsStopped();require(owner->lastProcessStatus()==1&&owner->lastNativeProcessStatus()==0&&diag.nativeAttempted&&diag.nativeReturned&&diag.nativeStatus==0&&diag.reportCount==2&&!diag.stage&&!output.calls&&samples()==history+8&&owner->ready(),"late unsupported scoped output after actual Rust success advances history/consumes intent but posts none; exact native0 distinct");
  output.refuseAt=1;invoke();diag=owner->outputDiagnosticsStopped();require(owner->lastProcessStatus()==1&&owner->lastNativeProcessStatus()==0&&diag.delivery.attempted==2&&diag.delivery.returned==2&&diag.delivery.posted==1&&diag.delivery.hasStatus&&diag.delivery.status==-7342&&output.calls==2&&output.posted[0].mData[2]==32,"actual late original SDK refusal preserves advanced Level and exact native status");
  input.clear();output.clear();invoke();diag=owner->outputDiagnosticsStopped();require(owner->lastProcessStatus()==0&&lastCapacity==1024&&!lastCount&&!output.calls&&diag.delivery.attempted==0,"fresh empty native call passes full capacity and never replays refused notes");
  input.add(0,0x93,72,127);output.throwAt=0;invoke();diag=owner->outputDiagnosticsStopped();require(owner->lastProcessStatus()==1&&diag.nativeStatus==0&&diag.delivery.attempted==1&&!diag.delivery.returned&&!diag.delivery.hasStatus,"actual host throw attempted/notreturned without invented SDK code");
  input.clear();output.clear();input.add(0,0x93,72,127);input.add(3,0x83,72,64);output.throwAt=1;invoke();diag=owner->outputDiagnosticsStopped();require(owner->lastProcessStatus()==1&&owner->lastNativeProcessStatus()==0&&diag.delivery.attempted==2&&diag.delivery.returned==1&&diag.delivery.posted==1&&diag.delivery.hasStatus&&diag.delivery.status==AAX_SUCCESS,"actual later throw retains last returned success and distinguishes unreturned latest attempt");
  input.clear();output.clear();invoke();require(owner->lastProcessStatus()==0&&!output.calls&&!lastCount,"actual empty fresh block never replays accepted posts after later throw");
  input.clear();output.clear();input.add(0,0x93,72,127);const auto beforePrecisionFault=samples();precisionOverflow=true;invoke();precisionOverflow=false;const auto precision=owner->precisionDiagnosticsStopped();diag=owner->outputDiagnosticsStopped();require(owner->lastProcessStatus()==1&&!owner->ready()&&owner->lastNativeProcessStatus()==0&&precision.nativeReturned&&precision.nativeStatus==0&&precision.error==Model::Owner::PrecisionError::NarrowOverflow&&precision.commandsConsumed&&!precision.samplesTransferred&&diag.nativeReturned&&diag.nativeStatus==0&&!diag.stage&&!output.calls&&samples()==beforePrecisionFault+8,"whole native64 narrowing overflow AFTER actual DSP/event production blocks ALL host MIDI/audio publication, retainsnative0/history and poisons readiness");const auto poisonedCalls=nativeCalls;invoke();require(nativeCalls==poisonedCalls&&!output.calls&&samples()==beforePrecisionFault+8,"no stale generated MIDI on rejected poisoned follow-up");auto* typed=static_cast<Model*>(parameters.get());require(typed->reprepareStopped()==0,"fresh stopped generated64 native preparation recovers precision fault");input.clear();output.clear();invoke();require(owner->lastProcessStatus()==0&&!output.calls&&!lastCount&&samples()==8,"fresh64 prepare resets transient history and cannot replay previously unposted native events");
  input.clear();output.clear();frames=0;context.outputs=nullptr;invoke();require(owner->lastProcessStatus()==0&&lastCapacity==1024&&!lastCount&&!output.calls,"zero-frame omitted audio and empty input preserve full generated capacity");
  require(parameters->Uninitialize()==AAX_SUCCESS&&host.references==1&&host.registered.empty(),"real SDK/native retirement releases initialized proxies");
 }
 telemetry.destroy_cache(telemetryCache);
}
}
int main(){try{contracts();std::cout<<"Actual registered AAX input/output native NoteRouting contracts PASS\n";return 0;}catch(const std::exception& error){std::cerr<<error.what()<<'\n';return 1;}}
