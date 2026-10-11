#include "RavetAaxPrecision.hpp"
#include <atomic>
#include <cstdlib>
#include <iostream>
#include <memory>
#include <new>
#include <stdexcept>
static std::atomic<uint64_t> allocations{0},destructions{0};
void* operator new(size_t bytes){allocations.fetch_add(1);if(void* value=std::malloc(bytes?bytes:1))return value;throw std::bad_alloc();}
void* operator new[](size_t bytes){return ::operator new(bytes);}
void operator delete(void* value)noexcept{destructions.fetch_add(1);std::free(value);}void operator delete[](void* value)noexcept{::operator delete(value);}void operator delete(void* value,size_t)noexcept{::operator delete(value);}void operator delete[](void* value,size_t)noexcept{::operator delete(value);}
void require(bool value,const char* message){if(!value)throw std::runtime_error(message);}
int main(){try{
 using namespace ravet::aax;using Planes=std::array<std::array<float,1025>,64>;auto planes=std::make_unique<Planes>();auto bridge=std::make_unique<AaxNativeDoubleAudio>();std::array<const float*,64> inputs{};std::array<float*,64> outputs{};for(uint32_t channel=0;channel<64;++channel){(*planes)[channel].fill(float(channel+1));inputs[channel]=(*planes)[channel].data();outputs[channel]=(*planes)[(channel+1)%64].data();}
 AaxNativeAudioLayout layout{};layout.busCount=9;layout.maximumFrames=1024;layout.inputCapacity=layout.outputCapacity=64;layout.buses[0]={8,8};for(uint32_t bus=1;bus<9;++bus){layout.buses[bus]={7,7};layout.inputFirst[bus]=8+(bus-1)*7;}
 AaxNativeAudioMap map;AaxNativeAudioBlock block;require(map.prepare(layout)&&bridge->prepare(layout),"maximum prepared bridge source-order geometry");const auto address=reinterpret_cast<uintptr_t>(bridge.get());AaxAudioProtectedRange protection{address,address+sizeof(*bridge)};const auto allocated=allocations.load(),freed=destructions.load();AaxNativeDoubleAudio::Failure fault;
 auto matches=[&](uint32_t completed){for(uint32_t channel=0;channel<64;++channel){const float expected=float((channel+64-completed%64)%64+1);for(uint32_t frame=0;frame<1024;++frame)if((*planes)[channel][frame]!=expected)return false;if((*planes)[channel][1024]!=float(channel+1))return false;}return true;};
 for(uint32_t phase=0;phase<1000;++phase){require(map.map(inputs.data(),outputs.data(),1024,-1,block,&protection,1)&&bridge->capture(block),"maximum aliased full input capture");for(uint32_t bus=0;bus<9;++bus){const auto input=static_cast<const double*const*>(bridge->buses()[bus].inputs);const auto output=static_cast<double*const*>(bridge->buses()[bus].outputs);for(uint32_t channel=0;channel<bridge->buses()[bus].output_channels;++channel)std::copy_n(input[channel],1024,output[channel]);}require(bridge->check(fault)&&bridge->copy(block),"maximum whole converted copy");require(matches(phase+1),"independent whole maximum-channel rotation and all active sample/backing-tail oracle");if(phase%100==0){require(bridge->capture(block),"maximum late fault capture");auto late=static_cast<double*const*>(bridge->buses()[8].outputs);uint64_t bits=UINT64_C(0x7fefffffffffffff);std::memcpy(late[6]+1023,&bits,8);const auto sentinel=(*planes)[0][0];require(!bridge->check(fault)&&fault.bus==8&&fault.channel==6&&fault.frame==1023&&!bridge->copy(block)&&(*planes)[0][0]==sentinel&&matches(phase+1),"maximum late output overflow commits no host prefix");}}
 require(allocations.load()==allocated&&destructions.load()==freed,"1000 maximum map/capture/whole narrow/copy plus late rejection phases have zero ordinary C++ heap work");for(uint32_t channel=0;channel<64;++channel)require((*planes)[channel][1024]==float(channel+1),"maximum output backing tails preserved");std::cout<<"Maximum9bus64channel1024frame1000cycles/latefault/zeroCppheap PASS\n";return 0;
}catch(const std::exception& error){std::cerr<<error.what()<<'\n';return 1;}}
