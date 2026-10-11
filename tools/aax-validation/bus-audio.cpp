#include "RavetAaxAudio.hpp"
#include <cstdlib>
#include <cstring>
#include <iostream>
#include <new>
#include <stdexcept>
namespace {bool counting=false;uint64_t allocations=0,deletions=0;}
void *operator new(size_t n){if(counting)++allocations;if(auto *p=std::malloc(n?n:1))return p;throw std::bad_alloc();}
void *operator new[](size_t n){return ::operator new(n);}
void operator delete(void *p)noexcept{if(counting&&p)++deletions;std::free(p);}
void operator delete[](void *p)noexcept{::operator delete(p);}
void operator delete(void *p,size_t)noexcept{::operator delete(p);}
void operator delete[](void *p,size_t)noexcept{::operator delete(p);}
using namespace ravet::aax;
void require(bool value,const char *message){if(!value)throw std::runtime_error(message);}
void contracts(){
 for(bool reversed:{false,true}){
  AaxNativeAudioLayout profile{};profile.busCount=3;profile.maximumFrames=32;profile.inputCapacity=3;profile.outputCapacity=4;profile.buses[0]={2,2};profile.buses[reversed?2:1]={1,0};profile.inputFirst[reversed?2:1]=AaxNativeAudioLayout::Sidechain;profile.buses[reversed?1:2]={0,2};
  AaxNativeAudioMap map;require(map.prepare(profile),"explicit source-order main/input-only/output-only profile");std::array<std::array<float,34>,7> samples{};float *inputs[]={samples[0].data(),samples[1].data(),samples[2].data()},*outputs[]={samples[3].data(),samples[4].data(),samples[5].data(),samples[6].data()};AaxNativeAudioBlock block;
  require(map.map(inputs,outputs,32,2,block)&&block.busCount()==3,"whole prepared directional mapping");auto buses=block.buses();require(static_cast<const float *const*>(buses[reversed?2:1].inputs)[0]==inputs[2]&&static_cast<float *const*>(buses[reversed?1:2].outputs)[0]==outputs[2],"sidechain index and descriptor-ordered aux outputs independent of native bus ordinal");
  AaxNativeAudioBlock copy=block;require(copy.buses()[0].inputs!=block.buses()[0].inputs&&static_cast<const float *const*>(copy.buses()[0].inputs)[1]==inputs[1],"copied block retargets own stable native pointer arrays");
  std::array<unsigned char,sizeof(block)> prior{};std::memcpy(prior.data(),&block,sizeof(block));outputs[3]=outputs[0]+1;require(!map.map(inputs,outputs,32,2,block)&&!std::memcmp(prior.data(),&block,sizeof(block)),"late output partial overlap rejects without publishing any view");outputs[3]=samples[6].data();
  for(const int32_t index:{-1,0,1,3,INT32_MAX})require(!map.map(inputs,outputs,32,index,block)&&!std::memcmp(prior.data(),&block,sizeof(block)),"missing/overlapping/out-of-capacity sidechain index rejects before view mutation");
  require(!map.map(nullptr,outputs,32,2,block)&&!map.map(inputs,nullptr,32,2,block),"positive frames require declared flat pointer capacities");
  auto bad=profile;bad.buses[8]={0,1};const auto priorLayout=map.layout();require(!map.prepare(bad)&&!std::memcmp(&priorLayout,&map.layout(),sizeof(priorLayout)),"late invalid unused layout retains prior prepared shape");
  auto *ownedMetadata=reinterpret_cast<float*const*>(&map);require(!map.map(ownedMetadata,outputs,32,2,block),"whole mapper metadata alias checks before dereference");require(!map.map(inputs,reinterpret_cast<float*const*>(&block),32,2,block),"whole destination metadata alias checks before dereference");
  const AaxAudioProtectedRange protectedRange{reinterpret_cast<uintptr_t>(samples[6].data()),reinterpret_cast<uintptr_t>(samples[6].data()+32)};require(!map.map(inputs,outputs,32,2,block,&protectedRange,1),"declared foreign protected samples reject late map");for(auto &sample:samples)sample.fill(77.f);map.silence(inputs,outputs,33,&protectedRange,1);require(samples[3][0]==0.f&&samples[5][31]==0.f&&samples[3][32]==77.f&&samples[6][0]==77.f,"supported main/aux silence protects explicit owner/foreign range and backing tails");
  require(map.map(nullptr,nullptr,0,-1,block)&&block.busCount()==3&&block.buses()[0].input_channels==2&&!block.buses()[0].inputs&&!block.buses()[0].outputs,"frame0 explicit omission consumes no pointer metadata or dynamic selector");
 }
 AaxNativeAudioLayout maximum{};maximum.busCount=9;maximum.maximumFrames=1024;maximum.inputCapacity=9;maximum.outputCapacity=64;maximum.buses[0]={8,8};maximum.buses[1]={1,0};maximum.inputFirst[1]=AaxNativeAudioLayout::Sidechain;for(uint32_t i=2;i<9;++i)maximum.buses[i]={0,8};AaxNativeAudioMap mapper;require(mapper.prepare(maximum),"maximum9-bus/64-output profile");std::array<std::array<float,1025>,73> storage{};std::array<float*,9> inputs{};std::array<float*,64> outputs{};for(uint32_t i=0;i<inputs.size();++i)inputs[i]=storage[i].data();for(uint32_t i=0;i<outputs.size();++i)outputs[i]=storage[9+i].data();AaxNativeAudioBlock block;counting=true;
 for(uint32_t cycle=0;cycle<10000;++cycle){if(!mapper.map(inputs.data(),outputs.data(),1024,8,block)){counting=false;throw std::runtime_error("maximum prepared mapping");}outputs[63]=outputs[0]+1;if(mapper.map(inputs.data(),outputs.data(),1024,8,block)){counting=false;throw std::runtime_error("maximum late alias rejection");}outputs[63]=storage[72].data();}
 counting=false;require(!allocations&&!deletions,"10000 maximum/late-fault prepared maps no C++ heap");
}
int main(){try{contracts();std::cout<<"PASS prepared AAX9-bus main/detector/aux source-order mapping, full metadata/sample/alias preflight, previous-map retention/copy retarget, supported silence/tails and10000 max heap-counted cycles\n";return 0;}catch(const std::exception &error){std::cerr<<error.what()<<'\n';return 1;}}
