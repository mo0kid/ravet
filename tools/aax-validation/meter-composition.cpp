#include "RavetAaxFactoryRegistration.hpp"
#include "AAX_IComponentDescriptor.h"
#include "AAX_IPropertyMap.h"
#include "MeterControllerHost.hpp"
#include "TimingControllerHost.hpp"
#include "RavetAaxMeterRegistration.hpp"
#include "RavetRawRouting.h"
#include "RavetMidiSine.h"
#include "GainMeterParameters.hpp"
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
 int audioIn=-1,audioOut=-1,frames=-1,privateField=-1,privateSize=0,meterField=-1,midiField=-1,sidechainField=-1,auxField=-1; std::vector<AAX_CTypeID> meterIds;
 AAX_Result Clear()override{return AAX_SUCCESS;}
 AAX_Result AddAudioIn(AAX_CFieldIndex p)override{audioIn=p;return AAX_SUCCESS;}
 AAX_Result AddAudioOut(AAX_CFieldIndex p)override{audioOut=p;return AAX_SUCCESS;}
 AAX_Result AddAudioBufferLength(AAX_CFieldIndex p)override{frames=p;return AAX_SUCCESS;}
 AAX_Result AddSampleRate(AAX_CFieldIndex)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddClock(AAX_CFieldIndex)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddSideChainIn(AAX_CFieldIndex f)override{sidechainField=f;return 0;}
 AAX_Result AddDataInPort(AAX_CFieldIndex,uint32_t,AAX_EDataInPortType)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddAuxOutputStem(AAX_CFieldIndex f,int32_t,const char[])override{auxField=f;return 0;}
 AAX_Result AddPrivateData(AAX_CFieldIndex p,int32_t n,uint32_t)override{privateField=p;privateSize=n;return AAX_SUCCESS;}
 AAX_Result AddTemporaryData(AAX_CFieldIndex,uint32_t)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddDmaInstance(AAX_CFieldIndex,AAX_IDma::EMode)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddMeters(AAX_CFieldIndex f,const AAX_CTypeID* ids,uint32_t count)override{meterField=f;meterIds.assign(ids,ids+count);return AAX_SUCCESS;}
 AAX_Result AddMIDINode(AAX_CFieldIndex f,AAX_EMIDINodeType,const char[],uint32_t)override{midiField=f;return 0;}
 AAX_Result AddReservedField(AAX_CFieldIndex,uint32_t)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_IPropertyMap *NewPropertyMap()const override{return &properties;}
 AAX_IPropertyMap *DuplicatePropertyMap(AAX_IPropertyMap*)const override{return nullptr;}
 AAX_Result AddProcessProc_Native(AAX_CProcessProc p,AAX_IPropertyMap*,AAX_CInstanceInitProc i,AAX_CBackgroundProc,AAX_CSelector*)override{process=p;init=i;return AAX_SUCCESS;}
 AAX_Result AddProcessProc_TI(const char[],const char[],AAX_IPropertyMap*,const char[],const char[],AAX_CSelector*)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddProcessProc(AAX_IPropertyMap*,AAX_CSelector*,int32_t)override{return AAX_ERROR_UNIMPLEMENTED;}
};
struct Descriptor final : AAX_IEffectDescriptor {
 Component component; Properties properties; std::vector<std::pair<AAX_CTypeID,std::string>> meters; void *create=nullptr; int createCalls=0; AAX_Result createStatus=AAX_SUCCESS;
 AAX_IComponentDescriptor *NewComponentDescriptor()override{return &component;}
 AAX_Result AddComponent(AAX_IComponentDescriptor *p)override{return p==&component?AAX_SUCCESS:AAX_ERROR_INVALID_ARGUMENT;}
 AAX_Result AddName(const char*)override{return AAX_SUCCESS;}
 AAX_Result AddCategory(uint32_t)override{return AAX_SUCCESS;}
 AAX_Result AddCategoryBypassParameter(uint32_t,AAX_CParamID)override{return AAX_SUCCESS;}
 AAX_Result AddProcPtr(void *p,AAX_CProcPtrID kind)override{if(kind==kAAX_ProcPtrID_Create_EffectParameters){++createCalls;if(createStatus)return createStatus;create=p;}return AAX_SUCCESS;}
 AAX_IPropertyMap *NewPropertyMap()override{return &properties;}
 AAX_Result SetProperties(AAX_IPropertyMap*)override{return AAX_SUCCESS;}
 AAX_Result AddResourceInfo(AAX_EResourceType,const char*)override{return AAX_SUCCESS;}
 AAX_Result AddMeterDescription(AAX_CTypeID id,const char* name,AAX_IPropertyMap*)override{meters.emplace_back(id,name);return AAX_SUCCESS;}
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


using namespace ravet::aax;
struct Node final:AAX_IMIDINode{std::array<AAX_CMidiPacket,1024> packets{};AAX_CMidiStream stream{0,packets.data()};uint32_t reads=0;AAX_CMidiStream* GetNodeBuffer()override{++reads;return &stream;}AAX_Result PostMIDIPacket(AAX_CMidiPacket*)override{return AAX_ERROR_UNIMPLEMENTED;}AAX_ITransport* GetTransport()override{return nullptr;}};
template<class Model,bool Instrument=false,bool Aux=false>void describe(Collection& collection,const AaxPreparedMeters<1>& metadata){
 auto* d=collection.NewDescriptor();auto* c=d->NewComponentDescriptor();using C=typename Model::Context;
 require(c->AddPrivateData(AAX_FIELD_INDEX(C,owner),sizeof(typename Model::Owner*))==0&&c->AddAudioBufferLength(AAX_FIELD_INDEX(C,frames))==0&&c->AddAudioOut(AAX_FIELD_INDEX(C,outputs))==0,"prepared generic consumer actual typed SDK field registration");
 if constexpr(!Instrument)require(c->AddAudioIn(AAX_FIELD_INDEX(C,inputs))==0,"actual effect input registration");
 if constexpr(Aux)require(c->AddSideChainIn(AAX_FIELD_INDEX(C,sidechainFirst))==0&&c->AddAuxOutputStem(AAX_FIELD_INDEX(C,outputs),AAX_eStemFormat_Stereo,"Detector monitor")==0,"actual input-only sidechain/output-only aux registration");
 require(c->AddMIDINode(AAX_FIELD_INDEX(C,notes),AAX_eMIDINodeType_LocalInput,"MIDI Input",0xffff)==0,"actual metered input node registration");
 require(bool(registerAaxMeters(*d,*c,AAX_FIELD_INDEX(C,meterTaps),metadata))&&c->AddProcessProc_Native<C>(Model::process,c->NewPropertyMap(),nullptr,nullptr)==0&&d->AddComponent(c)==0&&collection.AddEffect("org.ravet.validation.meter-composition",d)==0,"actual AddMeters and registered native algorithm exact typed context");
}
template<uint32_t Bits,bool Raw>void instrument(){
 using M=NativeAaxEffectParameters<1,32,true,1024,false,false,0,false,Bits,Raw,1>;
 const AaxMeterBinding binding{701,7,"Acknowledged Level",AAX_eMeterType_Other,AAX_eMeterOrientation_Default,AAX_eMeterBallisticType_NoDecay,AaxMeterProjection::FiniteFloat};
 const auto* core=ravet_midi_sine_processor_v1(Bits);const auto* telemetry=ravet_midi_sine_telemetry_v1(Bits);AaxPreparedMeters<1> metadata(core,telemetry,&binding);require(metadata.valid(),"raw sine actual native32/64 immutable Level telemetry metadata");Collection registration;describe<M,true>(registration,metadata);MeterControllerHost host;host.effect="org.ravet.validation.meter-composition";host.input=AAX_eStemFormat_None;
 auto model=std::make_unique<M>(typename M::Identity{host.effect.c_str(),1,1,2,3,4},core,ravet_midi_sine_audio_state_v1(Bits),ravet_midi_sine_cached_parameters_v1(Bits),ravet_midi_sine_state_v1(Bits),telemetry,&binding);require(model->Initialize(host.unknown())==0,"actual SDK note-only/raw no-input metered model Initialize");typename M::Owner* owner=nullptr;require(model->ResetFieldData(0,&owner,sizeof(owner))==0,"actual no-input owner field");std::array<float,66> output{};output.fill(77);float* out[]={output.data()};std::array<float,2> taps{91,91};float* tap=taps.data();Node node;node.stream.mBufferSize=1;node.packets[0]={3,3,{0x90,69,64,0},0};int32_t frames=64;typename M::Context context{&owner,&frames,out,&node,&tap};void* contexts[]={&context};auto process=registration.descriptors[0].component.process;process(contexts,contexts+1);
 require(model->lastProcessStatus()==0&&taps[0]==.2f&&taps[1]==91&&node.reads==1&&output[64]==77,"real registered note-only/raw instrument native render then Level tap publication");
 for(uint32_t i=0;i<64;++i){const auto n=i<3?0:i-3;const auto analytic=i<3?0.:std::sin(6.283185307179586*440./48000.*n)*(n+1)/240.*double(.2f)*double(64.f/127.f)/16.;require(std::abs(output[i]-analytic)<1e-8,"independently predicted timed instrument output unaffected by meter composition");}
 std::array<RavetNativeTelemetryValueV1,23> before{},after{};require(!model->snapshotTelemetryStopped(before.data(),23),"instrument native voice/history snapshot");const auto reads=node.reads;std::array<std::byte,sizeof(AAX_IMIDINode)> base{};std::memcpy(base.data(),&node,base.size());tap=reinterpret_cast<float*>(&node);process(contexts,contexts+1);require(node.reads==reads&&!std::memcmp(base.data(),&node,base.size())&&taps[0]==.2f&&!model->snapshotTelemetryStopped(after.data(),23)&&!std::memcmp(before.data(),after.data(),sizeof(before)),"opt-in nonraw/raw meter tap/node-base overlap getter0 preserves known base/taps/native voice history");tap=taps.data();auto* priorSlot=context.meterTaps;context.meterTaps=reinterpret_cast<float**>(&node);process(contexts,contexts+1);require(node.reads==reads&&!std::memcmp(base.data(),&node,base.size())&&!model->snapshotTelemetryStopped(after.data(),23)&&!std::memcmp(before.data(),after.data(),sizeof(before)),"opt-in meter pointer slot/node base rejects before loading slot bytes or any getter/history");context.meterTaps=priorSlot;
 node.stream.mBufferSize=2;node.packets[1]={63,3,{0xb0,74,128,0},0};process(contexts,contexts+1);require(model->lastProcessStatus()==1&&taps[0]==.2f&&!model->snapshotTelemetryStopped(after.data(),23)&&!std::memcmp(before.data(),after.data(),sizeof(before)),"late packet invalidity retains meter/voice/history before native dispatch");
 require(model->Uninitialize()==0&&host.references==1,"metered no-input actual SDK/cache/model retirement");
}
template<uint32_t Bits>void aux(bool reversed){
 using M=NativeAaxEffectParameters<2,32,false,1024,false,true,0,false,Bits,true,1>;const AaxMeterBinding binding{601,6,"Acknowledged gain",AAX_eMeterType_Other,AAX_eMeterOrientation_Default,AAX_eMeterBallisticType_NoDecay,AaxMeterProjection::FiniteFloat};const auto* core=ravet_raw_bus_routing_processor_v1(Bits);const auto* telemetry=ravet_raw_bus_routing_telemetry_v1(Bits);AaxPreparedMeters<1> metadata(core,telemetry,&binding);Collection registration;describe<M,false,true>(registration,metadata);
 auto layout=AaxNativeAudioLayout::main(2,2,1024);layout.busCount=3;layout.inputCapacity=5;layout.outputCapacity=4;const auto detector=reversed?2u:1u,monitor=reversed?1u:2u;layout.buses[detector]={1,0};layout.inputFirst[detector]=AaxNativeAudioLayout::Sidechain;layout.buses[monitor]={0,2};MeterControllerHost host;host.effect="org.ravet.validation.meter-composition";host.input=host.output=AAX_eStemFormat_Stereo;
 auto model=std::make_unique<M>(typename M::Identity{host.effect.c_str(),2,1,2,3,4},core,ravet_raw_bus_routing_audio_state_v1(Bits),ravet_raw_bus_routing_cached_parameters_v1(Bits),ravet_raw_bus_routing_state_v1(Bits),typename M::PreparedProfile{layout,AAX_eStemFormat_Stereo,AAX_eStemFormat_Stereo},telemetry,&binding);require(model->Initialize(host.unknown())==0,"actual SDK multibus metered both native precisions Initialize");typename M::Owner* owner=nullptr;require(model->ResetFieldData(0,&owner,sizeof(owner))==0,"typed metered aux owner");
 std::array<std::array<float,66>,3> source{};source[0].fill(.5);source[1].fill(-.25);source[2].fill(.125);std::array<std::array<float,66>,4> output{};for(auto& x:output)x.fill(77);float* in[]={source[0].data(),source[1].data(),nullptr,nullptr,source[2].data()},*out[]={output[0].data(),output[1].data(),output[2].data(),output[3].data()};std::array<float,2> taps{91,91};float* tap=taps.data();Node node;node.stream.mBufferSize=1;node.packets[0]={0,1,{0xfa,0,0,0},0};int32_t frames=64,sidechain=4;typename M::Context context{&owner,&frames,in,out,&sidechain,&node,&tap};void* contexts[]={&context};auto process=registration.descriptors[0].component.process;require(model->UpdateParameterNormalizedValue("p1",.125,AAX_eUpdateSource_Unspecified)==0,"aux queued native Gain.5 intent");process(contexts,contexts+1);require(model->lastProcessStatus()==0&&taps[0]==.5f&&taps[1]==91,"registered raw aux finite-value meter from authoritative consumed state");for(uint32_t i=0;i<64;++i)require(output[0][i]==.25f&&output[1][i]==-.125f&&output[2][i]==.0625f&&output[3][i]==.0625f,"main/detector/input-only/output-only native routing both source orders preserved");require(output[3][64]==77,"aux sample backing tail");
 std::array<RavetNativeTelemetryValueV1,10> before{},after{};require(!model->snapshotTelemetryStopped(before.data(),10),"raw aux current observation");tap=out[3];process(contexts,contexts+1);require(taps[0]==.5f&&!model->snapshotTelemetryStopped(after.data(),10)&&!std::memcmp(before.data(),after.data(),sizeof(before)),"late meter/aux-output overlap preserves complete raw history and queued native state");tap=taps.data();process(contexts,contexts+1);require(model->lastProcessStatus()==0&&owner->meterDiagnosticsStopped().meter.published,"aux retained map followup successful publication");require(model->Uninitialize()==0&&host.references==1,"multibus actual SDK retirement balanced");
}
namespace candidateMetadata {
 inline const RavetNativeTelemetryEntryV1* actual=nullptr;inline uint32_t created=0,retired=0;
 void* create(){++created;return actual->create_cache();}void destroy(void* cache){++retired;actual->destroy_cache(cache);}
 int32_t info(const void* cache,uint32_t index,RavetNativeTelemetryInfoV1* out){if(index+1==actual->telemetry_count)return -733;return actual->info(cache,index,out);}
}
template<uint32_t Bits>void timing(){
 using M=NativeAaxEffectParameters<2,32,false,1024,false,false,0,true,Bits,true,1>;const AaxMeterBinding binding{601,6,"Acknowledged Level",AAX_eMeterType_Other,AAX_eMeterOrientation_Default,AAX_eMeterBallisticType_NoDecay,AaxMeterProjection::FiniteFloat};const auto* core=ravet_raw_delay8_processor_v1(Bits);const auto* telemetry=ravet_raw_delay8_telemetry_v1(Bits);AaxPreparedMeters<1> metadata(core,telemetry,&binding);Collection registration;describe<M>(registration,metadata);TimingControllerHost host;host.effect="org.ravet.validation.meter-composition";
 auto model=std::make_unique<M>(typename M::Identity{host.effect.c_str(),1,1,2,3,4},core,ravet_raw_delay8_audio_state_v1(Bits),ravet_raw_delay8_cached_parameters_v1(Bits),ravet_raw_delay8_state_v1(Bits),typename M::TimingProfile{8},telemetry,&binding);require(model->Initialize(host.unknown())==0,"actual initialized SDK timing8 metered32/64 owner");typename M::Owner* owner=nullptr;require(model->ResetFieldData(0,&owner,sizeof(owner))==0,"timed meter owner context");std::array<float,6> input{},output{};input[0]=1;output.fill(77);float* in[]={input.data()},*out[]={output.data()};std::array<float,2> taps{91,91};float* tap=taps.data();int32_t frames=4;Node node;typename M::Context context{&owner,&frames,in,out,&node,&tap};void* contexts[]={&context};auto process=registration.descriptors[0].component.process;process(contexts,contexts+1);require(taps[0]==1.f&&output[0]==0,"old Delay8 meter and real FIFO");
 typename M::Owner::TimingTables candidate{ravet_raw_delay16_processor_v1(Bits),ravet_raw_delay16_audio_state_v1(Bits),ravet_raw_delay16_cached_parameters_v1(Bits),ravet_raw_delay16_state_v1(Bits),ravet_raw_delay16_telemetry_v1(Bits)};
 candidateMetadata::actual=candidate.telemetry;candidateMetadata::created=candidateMetadata::retired=0;auto refusedMetadata=*candidate.telemetry;refusedMetadata.create_cache=candidateMetadata::create;refusedMetadata.destroy_cache=candidateMetadata::destroy;refusedMetadata.info=candidateMetadata::info;auto refusedCandidate=candidate;refusedCandidate.telemetry=&refusedMetadata;std::array<RavetNativeTelemetryValueV1,10> beforeCandidate{},afterCandidate{};require(!model->snapshotTelemetryStopped(beforeCandidate.data(),10),"current metered Delay8 before candidate metadata failure");require(model->stageTimingCandidateStopped(refusedCandidate)!=0&&owner->ready()&&!model->snapshotTelemetryStopped(afterCandidate.data(),10)&&!std::memcmp(beforeCandidate.data(),afterCandidate.data(),sizeof(beforeCandidate))&&host.latencyUpdates==0,"late candidate meter metadata failure retains exact old DSP history/state and no host request");const auto metadataFailure=model->candidateMeterDiagnosticsStopped();require(metadataFailure.infoAttempted&&metadataFailure.infoReturned&&metadataFailure.infoStatus==-733&&metadataFailure.cacheRetireReturned&&candidateMetadata::created==1&&candidateMetadata::retired==1,"original candidate meter metadata status and unavoidable temporary cache retirement retained separately");
 auto malformed=candidate;malformed.telemetry=nullptr;require(model->stageTimingCandidateStopped(malformed)!=0&&owner->ready(),"metered timing requires explicit candidate telemetry before current state/history mutation");require(model->stageTimingCandidateStopped(candidate)==0&&model->requestStagedTimingControl()==0,"compatible exact meter metadata candidate16 and host request");input[0]=0;process(contexts,contexts+1);require(model->timingStopped().currentLatency==8&&taps[0]==1.f&&output[0]==0,"pending candidate retains original Delay8 and meter source");host.signalLatency=16;require(model->NotificationReceived(AAX_eNotificationEvent_SignalLatencyChanged,nullptr,0)==0&&model->reconcileTimingControl()==0,"actual public SDK matching timing ACK");require(model->UpdateParameterNormalizedValue("p1",.5,AAX_eUpdateSource_Unspecified)==0,"queued Level across acknowledged candidate");process(contexts,contexts+1);require(model->lastProcessStatus()==0&&model->timingStopped().currentLatency==16&&taps[0]==.5f&&owner->meterDiagnosticsStopped().meter.published,"AUDIO candidate adoption uses exact compatible telemetry and migrated queued state");require(model->collectRetiredTimingControl()==0,"CONTROL-only old candidate/telemetry cache retirement");input[0]=1;process(contexts,contexts+1);input[0]=0;for(uint32_t i=0;i<3;++i){process(contexts,contexts+1);require(output[0]==0,"new Delay16 bounded FIFO remains exact");}process(contexts,contexts+1);require(output[0]==.5f&&taps[0]==.5f&&taps[1]==91,"independent delayed impulse after ACK native history and meter source");require(model->Uninitialize()==0&&host.references==1,"metered dynamic timing real SDK/native retirement");
}
}
int main(){try{instrument<32,false>();instrument<64,false>();instrument<32,true>();instrument<64,true>();aux<32>(false);aux<32>(true);aux<64>(false);aux<64>(true);timing<32>();timing<64>();std::cout<<"PASS real SDK metered32/64 note/raw instrument, detector/monitor bus directions/source order and exact timed candidate telemetry ACK/adoption/retirement\n";}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
