#include "RavetVst3Provider.h"
#include <array>
#include <algorithm>
#include <cmath>
#include <iostream>
#include <stdexcept>
#include <thread>
#include <string>
#include <cstdlib>
#include <new>
namespace {
thread_local bool measured=false;
thread_local uint64_t allocations=0,deallocations=0;
}
void* operator new(std::size_t bytes){if(measured)++allocations;if(auto* p=std::malloc(bytes?bytes:1))return p;throw std::bad_alloc();}
void* operator new[](std::size_t bytes){return ::operator new(bytes);}
void operator delete(void* p)noexcept{if(p&&measured)++deallocations;std::free(p);}
void operator delete[](void* p)noexcept{::operator delete(p);}
void operator delete(void* p,std::size_t)noexcept{::operator delete(p);}
void operator delete[](void* p,std::size_t)noexcept{::operator delete(p);}
using namespace std;
static void require(bool x){if(!x)throw runtime_error("v2 actual owner/block contract");}
int main(int argc,char** argv){
 try{
  require(argc==3);auto* api=ravet_vst3_provider_v2();require(api&&api->version==2&&api->struct_size==sizeof(*api));
  for(int selected=1;selected<argc;++selected){
   string path=argv[selected];RavetVst3Module* parent=nullptr;int32_t native=999;
   require(api->open(reinterpret_cast<const uint8_t*>(path.data()),uint32_t(path.size()),&parent,&native)==RAVET_VST3_OK);
   require(ravet_vst3_provider_v1()->close(parent,&native)==RAVET_VST3_INVALID);
   RavetVst3FactoryRecord factory{};factory.struct_size=sizeof(factory);require(api->factory(parent,&factory)==RAVET_VST3_OK);
   uint32_t index=0;for(;index<factory.class_count;++index){RavetVst3ClassRecord info{};info.struct_size=sizeof(info);require(api->class_info(parent,index,&info)==RAVET_VST3_OK);if(string(info.category)=="Audio Module Class")break;}
   require(index<factory.class_count);RavetVst3Instance* child=nullptr;
   require(api->create(parent,index,&child,&native)==RAVET_VST3_OK);
   require(api->close(parent,&native)==RAVET_VST3_CHILDREN);
   bool wrongThread=false;thread wrong([&]{RavetVst3Instance* absent=nullptr;int32_t result=999;wrongThread=api->create(parent,index,&absent,&result)==RAVET_VST3_THREAD&&!absent&&result==999;});wrong.join();require(wrongThread);
   for(uint32_t channels:{1u,2u}){
    RavetVst3AudioProfile profile{sizeof(profile),64,selected==1?channels:0u,channels,48000.};require(api->prepare(child,&profile,&native)==RAVET_VST3_OK);require(api->start(child,&native)==RAVET_VST3_OK);
    RavetVst3Audio* audio=nullptr;require(api->begin_audio(child,&audio,&native)==RAVET_VST3_OK);
    require(api->destroy(child,&native)==RAVET_VST3_LIFECYCLE);require(api->stop(child,&native)==RAVET_VST3_LIFECYCLE);
    array<array<float,70>,2> inputs{},outputs{};array<RavetVst3InputChannel,2> in{};array<RavetVst3OutputChannel,2> out{};
    for(uint32_t ch=0;ch<channels;++ch){for(uint32_t f=0;f<70;++f)inputs[ch][f]=float(f+ch)/128.f;outputs[ch].fill(77.f);in[ch]={inputs[ch].data(),70,0};out[ch]={outputs[ch].data(),70,0};}
    RavetVst3AudioBlock block{sizeof(block),64,profile.input_channels,channels,profile.input_channels?in.data():nullptr,out.data()};
    bool passed=false;thread worker([&]{
     allocations=deallocations=0;measured=true;int32_t diagnostic=999;auto invalid=block;invalid.frames=65;
     require(api->process(audio,&invalid,&diagnostic)==RAVET_VST3_INVALID&&diagnostic==999&&outputs[0][0]==77.f);
     out[channels-1].capacity=63;require(api->process(audio,&block,&diagnostic)==RAVET_VST3_INVALID&&diagnostic==999&&outputs[0][0]==77.f);out[channels-1].capacity=70;
     require(api->process(audio,&block,reinterpret_cast<int32_t*>(outputs[0].data()))==RAVET_VST3_INVALID&&outputs[0][0]==77.f);
     auto original=out[0].samples;out[0].samples=reinterpret_cast<float*>(out.data());require(api->process(audio,&block,&diagnostic)==RAVET_VST3_INVALID&&diagnostic==999);out[0].samples=original;
     if(channels==2){auto previous=out[1].samples;out[1].samples=outputs[0].data()+1;require(api->process(audio,&block,&diagnostic)==RAVET_VST3_INVALID&&diagnostic==999&&outputs[0][0]==77.f);out[1].samples=previous;}
     for(int n=0;n<10000;++n)require(api->process(audio,&block,&diagnostic)==RAVET_VST3_OK&&diagnostic==0);
     for(uint32_t ch=0;ch<channels;++ch){for(uint32_t f=0;f<64;++f)require(abs(outputs[ch][f]-(selected==1?inputs[ch][f]:0.f))<1e-6f);for(uint32_t f=64;f<70;++f)require(outputs[ch][f]==77.f);}
     // Matched input/output aliases use complete input pre-copy before rendering.
     if(selected==1){for(uint32_t ch=0;ch<channels;++ch)out[ch].samples=inputs[ch].data();require(api->process(audio,&block,&diagnostic)==RAVET_VST3_OK);}
     measured=false;require(allocations==0&&deallocations==0);passed=true;
    });worker.join();require(passed);require(api->end_audio(audio,&native)==RAVET_VST3_OK);require(api->stop(child,&native)==RAVET_VST3_OK);
    require(api->start(child,&native)==RAVET_VST3_OK);audio=nullptr;require(api->begin_audio(child,&audio,&native)==RAVET_VST3_OK);
    bool caught=false;thread joined([&]{try{block.frames=0;require(api->process(audio,&block,&native)==RAVET_VST3_OK);throw runtime_error("caller exception");}catch(...){caught=true;}});joined.join();require(caught);require(api->end_audio(audio,&native)==RAVET_VST3_OK);require(api->stop(child,&native)==RAVET_VST3_OK);
   }
   require(api->destroy(child,&native)==RAVET_VST3_OK);require(api->close(parent,&native)==RAVET_VST3_OK);
  }
  cout<<"PASS actual v2 Gain/Performance parent/child/borrowed AUDIO ownership, main-thread rejection,10000 prepared caller blocks each with zero measured C++ new/delete including first block and invalid preflight, whole late map/alias/diagnostic rejection, tails, input aliases and joined caller exception\n";
 }catch(const exception& e){cerr<<e.what()<<'\n';return 1;}
}
