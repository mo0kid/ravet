#include "RavetAaxProcessor.hpp"
#include "RavetRawRouting.h"
#include <array>
#include <atomic>
#include <cmath>
#include <cstdlib>
#include <cstring>
#include <iostream>
#include <memory>
#include <new>
#include <stdexcept>
static std::atomic<bool> measuring{false};static std::atomic<size_t> allocations{0},retirements{0};
void* operator new(size_t n){if(measuring.load())++allocations;if(void* p=std::malloc(n?n:1))return p;throw std::bad_alloc();}void* operator new[](size_t n){return ::operator new(n);}void operator delete(void* p)noexcept{if(p&&measuring.load())++retirements;std::free(p);}void operator delete[](void* p)noexcept{::operator delete(p);}void operator delete(void* p,size_t)noexcept{::operator delete(p);}
namespace {
void require(bool good,const char* reason){if(!good)throw std::runtime_error(reason);}
template<uint32_t Bits>using Owner=ravet::aax::NativeAaxProcessor<2,32,false,1024,false,0,false,Bits,true>;
using Range=ravet::aax::AaxAudioProtectedRange;
struct Packets{std::array<AAX_CMidiPacket,1024> packets{};AAX_CMidiStream stream{0,packets.data()};void add(uint32_t frame,uint8_t status,uint8_t a=0,uint8_t b=0){const uint32_t size=status<0xf0?((status&0xe0)==0xc0?2:3):(status==0xf1||status==0xf3?2:status==0xf2?3:1);packets[stream.mBufferSize++]={frame,size,{status,a,b,0},0};}};
template<uint32_t Bits>void contracts(){
 auto owner=std::make_unique<Owner<Bits>>(ravet_raw_bus_routing_processor_v1(Bits),ravet_raw_bus_routing_audio_state_v1(Bits),ravet_raw_bus_routing_cached_parameters_v1(Bits),ravet_raw_bus_routing_state_v1(Bits),ravet_raw_bus_routing_telemetry_v1(Bits));require(owner->valid(),"raw exact selected native ownership");
 std::array<std::array<float,1026>,3> input{};std::array<std::array<float,1026>,4> output{};for(uint32_t i=0;i<1026;++i){input[0][i]=float(i)/16.f;input[1][i]=-float(i)/32.f;input[2][i]=.25f+float(i)/128.f;}for(auto& channel:output)channel.fill(77.f);
 const float* inputs[]={input[0].data(),input[1].data(),nullptr,nullptr,input[2].data()};float* outputs[]={output[0].data(),output[1].data(),output[2].data(),output[3].data()};Packets midi;
 std::array<RavetNativeTelemetryValueV1,10> before{},after{};
 for(bool reversed:{false,true}){
  auto layout=ravet::aax::AaxNativeAudioLayout::main(2,2,1024);layout.busCount=3;layout.inputCapacity=5;layout.outputCapacity=4;const auto detector=reversed?2u:1u,monitor=reversed?1u:2u;layout.buses[detector]={1,0};layout.inputFirst[detector]=ravet::aax::AaxNativeAudioLayout::Sidechain;layout.buses[monitor]={0,2};require(owner->prepareLayoutStopped(layout,48000.)==0,"raw aux exact stopped shape/source order");
  midi.stream.mBufferSize=0;typename Owner<Bits>::Reservation initial{};require(owner->stageParameter("p1",.25,initial)&&owner->commit(initial)&&owner->processRawLayout(inputs,outputs,0,&midi.stream,4)==0,"raw zero-frame complete global intent without frame advance");for(auto& channel:output)channel.fill(77.f);
  for(uint32_t s=0x80;s<=0xff;++s){if(s<0xf0||s==0xf1||s==0xf2||s==0xf3||s==0xf6||s==0xf7||s==0xf8||s==0xfa||s==0xfb||s==0xfc||s==0xfe||s==0xff)midi.add(0,uint8_t(s),69,127);}
  require(owner->processRawLayout(inputs,outputs,64,&midi.stream,4)==0&&!owner->snapshotTelemetryStopped(before.data(),10)&&before[4].samples==123&&before[2].samples==UINT64_C(15738453561344363854)&&before[3].samples==UINT64_C(3721297664),"actual canonical Rust both precisions receive all123 raw statuses/ordered bytes");for(uint32_t i=0;i<64;++i){require(output[0][i]==input[0][i]&&output[1][i]==input[1][i]&&output[2][i]==input[2][i]&&output[3][i]==input[2][i],"raw main/detector kernel exact directional routing");}require(output[0][64]==77.f&&output[3][64]==77.f,"raw routing active tails");
  typename Owner<Bits>::Reservation change{};require(owner->stageParameter("p1",.75,change)&&owner->commit(change),"pending physical Gain3 intent before late preflight");
  midi.stream.mBufferSize=0;midi.add(0,0xf8);midi.add(63,0xb0,74,128);require(owner->processRawLayout(inputs,outputs,64,&midi.stream,4)==1&&owner->ready()&&!owner->snapshotTelemetryStopped(after.data(),10)&&!std::memcmp(before.data(),after.data(),sizeof(before)),"late raw validation preserves native history/queued controls");require(output[0][0]==0.f&&output[3][63]==0.f&&output[3][64]==77.f,"late raw fault supported silence/tails");
  midi.stream.mBufferSize=1024;for(auto& packet:midi.packets)packet={0,1,{0xf8,0,0,0},0};require(owner->processRawLayout(inputs,outputs,64,&midi.stream,4)==1&&!owner->snapshotTelemetryStopped(after.data(),10)&&!std::memcmp(before.data(),after.data(),sizeof(before)),"raw1024 plus trailing control preflight before dequeue/history");
  midi.stream.mBufferSize=0;Range protectedSample{reinterpret_cast<uintptr_t>(input[2].data()),reinterpret_cast<uintptr_t>(input[2].data()+64)};require(owner->processRawLayout(inputs,outputs,64,&midi.stream,4,&protectedSample,1)==1&&owner->ready(),"known protected source sample rejects before native history");
  require(owner->processRawLayout(inputs,outputs,64,&midi.stream,5)==1&&owner->ready(),"late sidechain extent rejection retains prepared map/state");float* overlapping[]={output[0].data(),output[1].data(),output[2].data(),output[2].data()+1};require(owner->processRawLayout(inputs,overlapping,64,&midi.stream,4)==1&&owner->ready(),"late aux output partial alias before DSP");
  require(owner->processRawLayout(inputs,outputs,64,&midi.stream,4)==0&&!owner->snapshotTelemetryStopped(after.data(),10)&&after[0].samples==128&&after[5].value==3.&&after[9].samples==1,"healthy complete state/control followup proves retained history and once-only pending command");for(uint32_t i=0;i<64;++i)require(output[0][i]==input[0][i]*3.f&&output[2][i]==input[2][i]*3.f,"authoritative accepted pending gain applies to main and auxiliary");
  // Direct effect APIs cannot silently omit a required stream.
  const auto old=after;require(owner->processLayout(inputs,outputs,64,4)==1&&!owner->snapshotTelemetryStopped(after.data(),10)&&!std::memcmp(old.data(),after.data(),sizeof(old)),"legacy route refuses raw profile without dropping MIDI contract");
  require(owner->reset()==0,"raw stopped reset clears transient history");midi.stream.mBufferSize=1024;allocations=retirements=0;measuring=true;for(uint32_t cycle=0;cycle<100;++cycle)require(owner->processRawLayout(inputs,outputs,1024,&midi.stream,4)==0,"raw max repeated native rendering");measuring=false;require(!allocations&&!retirements&&!owner->snapshotTelemetryStopped(after.data(),10)&&after[0].samples==102400&&after[1].samples==102400,"maximum raw callbacks use no ordinary C++ new/delete");
 }
 require(owner->deactivate()==0,"raw native owned retirement preparation");
}
}
int main(){try{contracts<32>();contracts<64>();std::cout<<"PASS raw main/aux32/64 actual canonical routing, all123 ordered bytes, source order, late capacity/metadata/history/state and max no-C++allocation contracts\n";return 0;}catch(const std::exception& e){measuring=false;std::cerr<<e.what()<<'\n';return 1;}}
