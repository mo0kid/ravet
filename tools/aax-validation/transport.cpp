#include "RavetAaxTransport.hpp"
#include "RavetNativeClient.hpp"
#include "RavetGain.h"
#include <cstdlib>
#include <iostream>
#include <limits>
#include <new>
#include <stdexcept>
namespace {bool measuring=false;uint64_t allocations=0,deletions=0;}
void *operator new(size_t n){if(measuring)++allocations;if(auto *p=std::malloc(n?n:1))return p;throw std::bad_alloc();}
void *operator new[](size_t n){return ::operator new(n);}
void operator delete(void *p)noexcept{if(measuring&&p)++deletions;std::free(p);}
void operator delete[](void *p)noexcept{::operator delete(p);}
void operator delete(void *p,size_t)noexcept{::operator delete(p);}
void operator delete[](void *p,size_t)noexcept{::operator delete(p);}
using namespace ravet::aax;
constexpr uint32_t bit(AaxTransportGetter getter){return 1u<<uint32_t(getter);}
void require(bool yes,const char *text){if(!yes)throw std::runtime_error(text);}
struct Proxy final:AAX_ITransport {
 std::array<AAX_Result,7> statuses{};mutable std::array<uint32_t,7> calls{};
 bool playing=true,looping=false;double tempo=120.;int32_t numerator=4,denominator=4;int64_t ticks=960000,samples=123,start=0,end=3840000;uint32_t resolution=960000;int throwGetter=-1;uint32_t unwritten=0;
 AAX_Result before(AaxTransportGetter getter)const{const auto i=uint32_t(getter);++calls[i];if(int(i)==throwGetter)throw 17;return statuses[i];}
 bool writes(AaxTransportGetter getter)const{return !(unwritten&bit(getter));}
 AAX_Result GetCurrentTempo(double *v)const override{auto s=before(AaxTransportGetter::Tempo);if(writes(AaxTransportGetter::Tempo))*v=tempo;return s;}
 AAX_Result GetCurrentMeter(int32_t *n,int32_t *d)const override{auto s=before(AaxTransportGetter::Meter);if(writes(AaxTransportGetter::Meter)){*n=numerator;*d=denominator;}return s;}
 AAX_Result IsTransportPlaying(bool *v)const override{auto s=before(AaxTransportGetter::Playing);if(writes(AaxTransportGetter::Playing))*v=playing;return s;}
 AAX_Result GetCurrentTickPosition(int64_t *v)const override{auto s=before(AaxTransportGetter::Ticks);if(writes(AaxTransportGetter::Ticks))*v=ticks;return s;}
 AAX_Result GetCurrentLoopPosition(bool *v,int64_t *a,int64_t *b)const override{auto s=before(AaxTransportGetter::Loop);if(writes(AaxTransportGetter::Loop)){*v=looping;*a=start;*b=end;}return s;}
 AAX_Result GetCurrentNativeSampleLocation(int64_t *v)const override{auto s=before(AaxTransportGetter::Samples);if(writes(AaxTransportGetter::Samples))*v=samples;return s;}
 AAX_Result GetTicksPerQuarter(uint32_t *v)const override{auto s=before(AaxTransportGetter::Resolution);if(writes(AaxTransportGetter::Resolution))*v=resolution;return s;}
 AAX_Result GetCustomTickPosition(int64_t*,int64_t)const override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result GetBarBeatPosition(int32_t*,int32_t*,int64_t*,int64_t)const override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result GetCurrentTicksPerBeat(uint32_t*)const override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result GetTimelineSelectionStartPosition(int64_t*)const override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result GetTimeCodeInfo(AAX_EFrameRate*,int32_t*)const override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result GetFeetFramesInfo(AAX_EFeetFramesRate*,int64_t*)const override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result IsMetronomeEnabled(int32_t*)const override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result GetHDTimeCodeInfo(AAX_EFrameRate*,int64_t*)const override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result RequestTransportStart()override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result RequestTransportStop()override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result GetTimelineSelectionEndPosition(int64_t*)const override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result GetKeySignature(int64_t,uint32_t*)const override{return AAX_ERROR_UNIMPLEMENTED;}
};
void nativeAccept(const AaxTransportSnapshot &snapshot){
 ravet::native::NativeProcessorClient processor(ravet_native_processor_v1(32));RavetNativeProcessorConfigV1 config{};config.version=1;config.maximum_frames=1024;config.maximum_events=1;config.bus_count=1;config.sample_rate=48000;config.buses[0]={1,1};require(processor.prepare(config)==0,"authoritative actual Gain native configured");RavetNativeProcessorBusV1 bus{1,1,nullptr,nullptr};RavetNativeProcessorResultV1 report{};require(processor.process(&bus,1,0,nullptr,0,snapshot.transport().present?&snapshot.transport():nullptr,nullptr,0,report)==0,"projected snapshot authoritatively accepted by Rust native transport validation");
}
void contracts(){
 Proxy host;AaxTransportSnapshot snapshot;auto result=aaxTransport(&host,snapshot);require(bool(result)&&snapshot.attempted()==127&&snapshot.returned()==127&&snapshot.available()==127,"all independent getters attempted once and exact availability");for(const auto calls:host.calls)require(calls==1,"single getter calls");require(snapshot.transport().sample_position==123&&snapshot.transport().beat_position==1.&&snapshot.transport().tempo==120.&&snapshot.transport().signature_numerator==4,"canonical known native values");nativeAccept(snapshot);
 for(const int64_t position:{INT64_MIN,INT64_MAX}){host.samples=position;require(bool(aaxTransport(&host,snapshot))&&snapshot.transport().sample_position==position,"full exact signed-i64 native samples without floating narrowing");nativeAccept(snapshot);}
 for(const int64_t ticks:{-(INT64_C(1)<<53)-1,-(INT64_C(1)<<53),-(INT64_C(1)<<53)+1,(INT64_C(1)<<53)-1,(INT64_C(1)<<53),(INT64_C(1)<<53)+1,INT64_MIN,INT64_MAX}){host.ticks=ticks;require(bool(aaxTransport(&host,snapshot))&&snapshot.ticks()==ticks&&snapshot.transport().beat_position==double(ticks)/double(host.resolution),"exact full raw ticks and explicitly rounded full-range f64 quarter projection");nativeAccept(snapshot);}
 host=Proxy{};host.playing=false;result=aaxTransport(&host,snapshot);require(bool(result)&&!(snapshot.attempted()&bit(AaxTransportGetter::Samples))&&host.calls[6]==0&&!(snapshot.transport().present&RAVET_TRANSPORT_SAMPLES)&&!(snapshot.transport().flags&RAVET_TRANSPORT_FLAG_PLAYING),"known stopped sample position is unavailable, not invented selector");
 host.statuses[0]=AAX_ERROR_UNIMPLEMENTED;result=aaxTransport(&host,snapshot);require(bool(result)&&snapshot.status(AaxTransportGetter::Playing)==AAX_ERROR_UNIMPLEMENTED&&!(snapshot.available()&1)&&!(snapshot.transport().present&RAVET_TRANSPORT_PLAYING)&&!(snapshot.attempted()&bit(AaxTransportGetter::Samples)),"unknown playing preserves original status and conditional sample absence");nativeAccept(snapshot);
 for(int failed=0;failed<7;++failed){host=Proxy{};host.statuses[failed]=AAX_ERROR_NULL_OBJECT;result=aaxTransport(&host,snapshot);require(bool(result)&&snapshot.status(AaxTransportGetter(failed))==AAX_ERROR_NULL_OBJECT&&!(snapshot.available()&(1u<<failed)),"getter error independent original status retained");nativeAccept(snapshot);}
 for(int profile=0;profile<3;++profile){host=Proxy{};host.statuses.fill(AAX_ERROR_UNIMPLEMENTED);if(profile==0){host.statuses[0]=host.statuses[6]=0;}if(profile==1)host.statuses[2]=0;if(profile==2)host.statuses[1]=host.statuses[3]=host.statuses[4]=0;require(bool(aaxTransport(&host,snapshot)),"independent sample/signature/beat-tempo tuple capture");nativeAccept(snapshot);}
 host=Proxy{};host.looping=true;require(bool(aaxTransport(&host,snapshot))&&(snapshot.transport().present&RAVET_TRANSPORT_BEAT_LOOP)&&snapshot.configuredLoop()&&snapshot.loopStartTicks()==0&&snapshot.loopEndTicks()==3840000,"configured loop raw sidecar and native canonical matching beat range");nativeAccept(snapshot);
 host.statuses[3]=AAX_ERROR_UNIMPLEMENTED;require(bool(aaxTransport(&host,snapshot))&&(snapshot.transport().flags&RAVET_TRANSPORT_FLAG_LOOP_ACTIVE)&&!(snapshot.transport().present&RAVET_TRANSPORT_BEAT_LOOP),"configured loop without known beat timeline has no invented native endpoints");nativeAccept(snapshot);
 host=Proxy{};require(bool(aaxTransport(&host,snapshot)),"retained known snapshot");const auto original=snapshot.transport();
 for(int fault=0;fault<11;++fault){host=Proxy{};if(fault==0)host.tempo=0.;if(fault==1){uint64_t bits=UINT64_C(0x7ff0000000000000);std::memcpy(&host.tempo,&bits,8);}if(fault==2){uint64_t bits=UINT64_C(0x7ff8000000000001);std::memcpy(&host.tempo,&bits,8);}if(fault==3)host.numerator=-1;if(fault==4)host.denominator=65536;if(fault==5)host.resolution=0;if(fault==6){host.looping=true;host.start=host.end;}if(fault==7){host.looping=true;host.start=INT64_MAX-1;host.end=INT64_MAX;}if(fault>=8)host.unwritten=bit(AaxTransportGetter(fault==8?1:fault==9?2:4));result=aaxTransport(&host,snapshot);require(!result&&result.fault==(fault==7?AaxTransportFault::InvalidProjection:AaxTransportFault::InvalidHostData)&&!std::memcmp(&snapshot.transport(),&original,sizeof(original)),"successful malformed/collapsed or detectable unwritten data rejects whole projection output-last");}
 for(int throwing=0;throwing<7;++throwing){host=Proxy{};host.throwGetter=throwing;result=aaxTransport(&host,snapshot);require(!result&&result.fault==AaxTransportFault::ForeignException&&result.getter==AaxTransportGetter(throwing)&&(result.attempted&(1u<<throwing))&&!(result.returned&(1u<<throwing))&&!std::memcmp(&snapshot.transport(),&original,sizeof(original)),"foreign exception separate from original returned SDK statuses, no partial snapshot");}
 require(bool(aaxTransport(nullptr,snapshot))&&snapshot.attempted()==0&&snapshot.available()==0&&snapshot.transport().present==0,"missing proxy no attempted status or invented context");nativeAccept(snapshot);
 host=Proxy{};host.looping=true;measuring=true;for(int i=0;i<10000;++i){if(!aaxTransport(&host,snapshot)){measuring=false;throw std::runtime_error("prepared maximal transport capture");}host.end=host.start;if(aaxTransport(&host,snapshot)){measuring=false;throw std::runtime_error("late loop failure capture");}host.end=3840000;}measuring=false;require(!allocations&&!deletions,"10000 full/late-fault fixed transport cycles no C++ heap");
}
int main(){try{contracts();std::cout<<"PASS synthetic public-SDK AAX transport independent status/availability, exact i64 samples/raw ticks+rounded PPQ, canonical Rust acceptance, atomic malformed/throw projections and10000 heap-counted capture cycles\n";return 0;}catch(const std::exception &e){std::cerr<<e.what()<<'\n';return 1;}}
