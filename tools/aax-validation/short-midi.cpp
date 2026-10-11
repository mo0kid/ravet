#include "RavetAaxShortMidiInput.hpp"
#include "RavetMidiSine.h"
#include "RavetPerformance.h"
#include <array>
#include <cstring>
#include <cmath>
#include <iostream>
#include <stdexcept>
#include <atomic>
#include <cstdlib>
static std::atomic<bool> measure{false};static std::atomic<size_t> allocations{0};
void *operator new(size_t size){if(measure.load())++allocations;if(auto p=std::malloc(size))return p;throw std::bad_alloc();}
void operator delete(void *p)noexcept{std::free(p);}void operator delete(void *p,size_t)noexcept{std::free(p);}
void *operator new[](size_t size){return ::operator new(size);}void operator delete[](void *p)noexcept{::operator delete(p);}
void require(bool okay,const char *message){if(!okay)throw std::runtime_error(message);}
uint32_t length(uint8_t status){if(status>=0x80&&status<0xf0)return (status&0xe0)==0xc0?2:3;if(status==0xf1||status==0xf3)return 2;if(status==0xf2)return 3;return status==0xf6||status==0xf7||status==0xf8||status==0xfa||status==0xfb||status==0xfc||status==0xfe||status==0xff?1:0;}
using Collector=ravet::aax::AaxShortMidiInput;
struct Fixture{
 std::array<AAX_CMidiPacket,1024> packets{};AAX_CMidiStream stream{0,packets.data()};std::array<RavetNativeEventV1,1024> output{};
 void add(uint8_t status,uint32_t offset=0){const auto size=length(status);packets[stream.mBufferSize++]={offset,size,{status,69,127,0xff},0};}
};
template<class S>void authoritative(uint32_t bits,Fixture &fixture){
 const auto *core=ravet_midi_sine_processor_v1(bits);const auto *telemetry=ravet_midi_sine_telemetry_v1(bits);require(core&&core->sample_bits==bits&&telemetry&&telemetry->processor_entry()==core,"exact paired raw native table/telemetry");
 const auto *parameter=ravet_midi_sine_parameters_v1(bits);const auto *cached=ravet_midi_sine_cached_parameters_v1(bits);const auto *audio=ravet_midi_sine_audio_state_v1(bits);const auto *state=ravet_midi_sine_state_v1(bits);
 require(parameter&&cached&&audio&&state&&parameter->processor_entry()==core&&cached->parameters_entry()==parameter&&audio->cached_parameters_entry()==cached&&state->parameters_entry()==parameter,"all cross-codegen MIDI native companion exact table identities");
 const auto *oldCore=ravet_performance_processor_v1(bits);const auto *oldParameter=ravet_performance_parameters_v1(bits);const auto *oldCached=ravet_performance_cached_parameters_v1(bits);const auto *oldAudio=ravet_performance_audio_state_v1(bits);const auto *oldState=ravet_performance_state_v1(bits);
 require(oldCore&&oldParameter&&oldCached&&oldAudio&&oldState&&oldParameter->processor_entry()==oldCore&&oldCached->parameters_entry()==oldParameter&&oldAudio->cached_parameters_entry()==oldCached&&oldState->parameters_entry()==oldParameter,"unchanged Performance all companion exact identities32/64");
 auto *owner=core->create();auto *oracle=core->create();auto *cache=telemetry->create_cache();require(owner&&oracle&&cache,"native control ownership");
 RavetNativeProcessorConfigV1 config{};config.version=1;config.maximum_frames=64;config.maximum_events=1024;config.bus_count=1;config.sample_rate=48000.;config.buses[0]={0,1};require(!core->prepare(owner,&config)&&!core->prepare(oracle,&config),"actual native32/64 instrument prepare");
 std::array<S,66> samples{},expected{};samples.fill(S(77));expected.fill(S(77));S *out[]={samples.data()},*ref[]={expected.data()};RavetNativeProcessorBusV1 bus{0,1,nullptr,out},reference{0,1,nullptr,ref};RavetNativeProcessorResultV1 result{};
 uint32_t count=0;require(Collector::convert(&fixture.stream,64,fixture.output.data(),1024,count)&&count==fixture.stream.mBufferSize,"complete defined status conversion");
 require(!core->process(owner,&bus,1,64,fixture.output.data(),count,nullptr,nullptr,0,&result)&&result.status==0,"all defined short classes canonical Rust native processing");require(!core->process(oracle,&reference,1,64,fixture.output.data(),count,nullptr,nullptr,0,&result),"independent native history oracle");
 std::array<RavetNativeTelemetryValueV1,23> observed{},saved{},check{};require(!telemetry->snapshot(owner,cache,observed.data(),23)&&observed[4].samples==count&&observed[2].samples==UINT64_C(15738453561344363854)&&observed[3].samples==UINT64_C(3721297664),"authoritative consumer receives exact ordered raw payload and all classes");
 RavetNativeEventV1 start{};start.kind=RAVET_NATIVE_MIDI;start.detail=3;start.words[0]=0x7f4590;start.offset=3;
 require(!core->process(owner,&bus,1,64,&start,1,nullptr,nullptr,0,&result)&&!core->process(oracle,&reference,1,64,&start,1,nullptr,nullptr,0,&result)&&std::abs(double(samples[20]))>1e-7,"live voice history before late decoder fault");require(!telemetry->snapshot(owner,cache,saved.data(),23),"save live history telemetry");
 std::array<RavetNativeEventV1,2> malformed{start,fixture.output[count-1]};malformed[0].words[0]=0x7f4890;malformed[0].offset=0;malformed[1].words[0]=0xf0;require(core->process(owner,&bus,1,64,malformed.data(),2,nullptr,nullptr,0,&result)==1&&result.status==2,"malformed late native raw canonical rejection");require(!telemetry->snapshot(owner,cache,check.data(),23)&&!std::memcmp(check.data(),saved.data(),sizeof(saved)),"Rust decoder rejection precedes native history/telemetry");
 require(!core->process(owner,&bus,1,64,nullptr,0,nullptr,nullptr,0,&result)&&!core->process(oracle,&reference,1,64,nullptr,0,nullptr,nullptr,0,&result)&&samples==expected,"followup independent native history equality after rejection");require(samples[64]==S(77)&&samples[65]==S(77),"native backing tails");
 core->destroy(owner);core->destroy(oracle);telemetry->destroy_cache(cache);
}
int main(){try{
 Fixture fixture;uint32_t defined=0;
 for(uint32_t status=0;status<256;++status){const auto size=length(uint8_t(status));if(size){fixture.add(uint8_t(status));++defined;}
  for(uint32_t n=0;n<=4;++n){AAX_CMidiPacket packet{0,n,{uint8_t(status),69,127,255},0};AAX_CMidiStream stream{1,&packet};RavetNativeEventV1 output{};output.offset=321;const auto old=output;uint32_t count=991;
   const bool accepted=Collector::convert(&stream,64,&output,1,count);require(accepted==(size&&n==size),"exhaustive status/length matrix");
   if(accepted){require(count==1&&output.kind==RAVET_NATIVE_MIDI&&output.detail==size&&output.channel==0&&output.key==0&&output.value==0.,"raw fields canonical");uint32_t packed=0;for(uint32_t b=0;b<size;++b)packed|=uint32_t(packet.mData[b])<<(b*8);require(output.words[0]==packed&&output.words[1]==0&&output.words[2]==0&&output.words[3]==0,"exact wire payload");}
   else require(count==991&&!std::memcmp(&old,&output,sizeof(old)),"late status rejection keeps output/count");
  }
 }
 require(defined==123&&fixture.stream.mBufferSize==123,"all channel/system defined statuses accounted");
 authoritative<float>(32,fixture);authoritative<double>(64,fixture);
 AAX_CMidiPacket end{0,1,{0xf7,255,255,255},0};AAX_CMidiStream endStream{1,&end};uint32_t count=999;require(!ravet::aax::AaxNoteInput::convert(&endStream,64,fixture.output.data(),1024,count)&&count==999,"legacy note-only standalone F7 remains rejected");require(Collector::convert(&endStream,0,fixture.output.data(),1024,count)&&count==1,"standalone complete F7 maps EndOfExclusive at zero frames");
 fixture.stream.mBufferSize=1024;for(auto &packet:fixture.packets)packet={0,1,{0xf8,0,0,0},0};
 count=17;measure=true;for(uint32_t i=0;i<1000;++i){require(Collector::convert(&fixture.stream,1024,fixture.output.data(),1024,count)&&count==1024,"maximum raw list conversion");fixture.packets.back().mData[0]=0xf9;require(!Collector::convert(&fixture.stream,1024,fixture.output.data(),1024,count)&&count==1024&&fixture.output.back().words[0]==0xf8,"maximum late undefined status atomicity");fixture.packets.back().mData[0]=0xf8;}measure=false;require(allocations==0,"maximum prepared collector cycles allocate no ordinary C++ heap");
 fixture.packets.back().mTimestamp=1024;require(!Collector::convert(&fixture.stream,1024,fixture.output.data(),1024,count),"late frame bound");fixture.packets.back().mTimestamp=0;for(uint8_t invalid:{uint8_t(2),uint8_t(0x80),uint8_t(0xff)}){fixture.packets.back().mIsImmediate=invalid;const auto before=fixture.output;const auto beforeCount=count;require(!Collector::convert(&fixture.stream,1024,fixture.output.data(),1024,count)&&count==beforeCount&&!std::memcmp(before.data(),fixture.output.data(),sizeof(before)),"late immediate exact bool0/1 including80/ff preserves whole output/count");}fixture.packets.back().mIsImmediate=0;
 AAX_CMidiPacket immediate{7,1,{0xf8,0,0,0},1};AAX_CMidiStream immediateStream{1,&immediate};require(Collector::convert(&immediateStream,8,fixture.output.data(),1024,count)&&count==1&&fixture.output[0].offset==7,"defined immediate bool1 accepted with authoritative valid timed offset");const auto priorOutput=fixture.output[0];count=991;require(!Collector::convert(&immediateStream,0,fixture.output.data(),1024,count)&&count==991&&!std::memcmp(&priorOutput,&fixture.output[0],sizeof(priorOutput)),"immediate bool does not waive nonzero offset at zero frames");immediate.mTimestamp=0;require(Collector::convert(&immediateStream,0,fixture.output.data(),1024,count)&&fixture.output[0].offset==0,"SDK timestamp0 compatibility immediate delivery accepted at zero frames");std::array<AAX_CMidiPacket,2> decreasing{{{7,1,{0xf8,0,0,0},0},{3,1,{0xf8,0,0,0},1}}};AAX_CMidiStream decreasingStream{2,decreasing.data()};const auto savedCount=count;const auto savedOutput=fixture.output;require(!Collector::convert(&decreasingStream,8,fixture.output.data(),1024,count)&&count==savedCount&&!std::memcmp(fixture.output.data(),savedOutput.data(),sizeof(savedOutput)),"late decreasing timestamp remains whole-output/count atomic with immediate bool1");count=1024;
 require(!Collector::convert(&fixture.stream,1024,fixture.output.data(),1023,count)&&count==1024,"raw nonnote packets included in capacity");
 auto &aliased=*reinterpret_cast<uint32_t*>(&fixture.output[0]);const auto old=aliased;require(!Collector::convert(&fixture.stream,1024,fixture.output.data(),1024,aliased)&&aliased==old,"count/output alias preflight");
 // Structural invalidity must leave the complete output/count untouched.
 AAX_CMidiPacket one{0,1,{0xf8,0,0,0},0};AAX_CMidiStream valid{1,&one};const auto structuralOutput=fixture.output;count=887;
 auto rejects=[&](const AAX_CMidiStream* borrowed,RavetNativeEventV1* result){require(!Collector::convert(borrowed,64,result,1024,count)&&count==887&&!std::memcmp(structuralOutput.data(),fixture.output.data(),sizeof(structuralOutput)),"structural null/alignment/stream-packet-output alias refusal preserves output/count");};
 rejects(nullptr,fixture.output.data());AAX_CMidiStream nullPackets{1,nullptr};rejects(&nullPackets,fixture.output.data());
 alignas(AAX_CMidiStream) std::array<std::byte,sizeof(AAX_CMidiStream)+8> unalignedHeader{};std::memcpy(unalignedHeader.data()+1,&valid,sizeof(valid));rejects(reinterpret_cast<AAX_CMidiStream*>(unalignedHeader.data()+1),fixture.output.data());
 alignas(AAX_CMidiPacket) std::array<std::byte,sizeof(AAX_CMidiPacket)+8> unalignedPacket{};std::memcpy(unalignedPacket.data()+1,&one,sizeof(one));AAX_CMidiStream misalignedPackets{1,reinterpret_cast<AAX_CMidiPacket*>(unalignedPacket.data()+1)};rejects(&misalignedPackets,fixture.output.data());
 AAX_CMidiStream headerPacketAlias{1,reinterpret_cast<AAX_CMidiPacket*>(&valid)};rejects(&headerPacketAlias,fixture.output.data());
 AAX_CMidiStream packetOutputAlias{1,reinterpret_cast<AAX_CMidiPacket*>(fixture.output.data())};rejects(&packetOutputAlias,fixture.output.data());
 rejects(&valid,reinterpret_cast<RavetNativeEventV1*>(reinterpret_cast<uintptr_t>(fixture.output.data())+1));rejects(&valid,nullptr);
 std::cout<<"PASS exhaustive123 statuses/native32/64 canonical classes/legacyF7/whole rejection/max1000 zero C++ allocation\n";return 0;
 }catch(const std::exception &e){std::cerr<<e.what()<<'\n';return 1;}}
