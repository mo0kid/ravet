#include "RavetAaxProcessor.hpp"
#include "RavetPerformance.h"
#include <cmath>
#include <cstdlib>
#include <iostream>
#include <new>
#include <stdexcept>
namespace {bool measuring=false;uint64_t allocations=0,deletions=0;}
void *operator new(size_t n){if(measuring)++allocations;if(auto *p=std::malloc(n?n:1))return p;throw std::bad_alloc();}
void *operator new[](size_t n){return ::operator new(n);}
void operator delete(void *p)noexcept{if(measuring&&p)++deletions;std::free(p);}
void operator delete[](void *p)noexcept{::operator delete(p);}
void operator delete(void *p,size_t)noexcept{::operator delete(p);}
void operator delete[](void *p,size_t)noexcept{::operator delete(p);}
void require(bool okay,const char *message){if(!okay)throw std::runtime_error(message);}
struct Stream {
 std::array<AAX_CMidiPacket,1025> packets{};AAX_CMidiStream stream{0,packets.data()};
 void clear(){stream.mBufferSize=0;stream.mBuffer=packets.data();}
 void add(uint32_t at,std::initializer_list<uint8_t> bytes){auto &p=packets[stream.mBufferSize++];p={};p.mTimestamp=at;p.mLength=uint32_t(bytes.size());uint32_t i=0;for(auto b:bytes)p.mData[i++]=b;}
};
using Collector=ravet::aax::AaxNoteInput;
using Owner=ravet::aax::NativeAaxProcessor<1,4,true>;
Owner make(){return Owner(ravet_performance_processor_v1(32),ravet_performance_audio_state_v1(32),ravet_performance_audio_state_v1(32)->cached_parameters_entry(),ravet_performance_state_v1(32));}
void collector(){
 Stream source;std::array<RavetNativeEventV1,1024> events{};uint32_t count=77;
 source.add(0,{0x9f,69,1});source.add(1,{0x82,69,3});source.add(1,{0x90,70,0});source.packets[0].mIsImmediate=1;source.packets[0].mData[3]=255;
 require(Collector::convert(&source.stream,2,events.data(),1024,count)&&count==3,"timed native short notes and ignored unused byte/immediate flag");
 require(events[0].kind==RAVET_NATIVE_NOTE_ON&&events[0].channel==15&&events[0].value==double(1.f/127.f),"exact widened velocity and channel");
 require(events[1].kind==RAVET_NATIVE_NOTE_OFF_SCOPED&&events[1].scope==3&&events[1].channel==2&&events[1].value==double(3.f/127.f)&&events[2].value==0.,"exact scoped noteoff and MIDI zero convention");
 for(int fault=0;fault<13;++fault){
  source.clear();source.add(0,{0x90,69,127});source.add(1,{0x80,69,0});auto &last=source.packets[1];
  if(fault==0)last.mLength=0;if(fault==1)last.mLength=4;if(fault==2)last.mLength=2;if(fault==3)last.mData[0]=69;if(fault==4)last.mData[1]=128;
  if(fault==5)last.mData[0]=0xf0;if(fault==6){last.mData[0]=0xf7;last.mLength=1;}if(fault==7){last.mData[0]=0xf9;last.mLength=1;}
  if(fault==8)last.mTimestamp=2;if(fault==9){source.packets[0].mTimestamp=1;last.mTimestamp=0;}if(fault==10)last.mIsImmediate=2;
  if(fault==11)source.stream.mBufferSize=1025;if(fault==12)source.stream.mBuffer=nullptr;
  events[0].kind=999;count=77;require(!Collector::convert(&source.stream,2,events.data(),1024,count)&&events[0].kind==999&&count==77,"late malformed whole stream leaves output/count untouched");
 }
 source.clear();for(auto message:{std::initializer_list<uint8_t>{0xb0,1,127},{0xc0,1},{0xd0,1},{0xe0,1,127},{0xa0,69,127},{0xf1,1},{0xf2,1,2},{0xf3,1},{0xf6},{0xf8},{0xfa},{0xfb},{0xfc},{0xfe},{0xff}})source.add(0,message);
 require(Collector::convert(&source.stream,0,nullptr,0,count)&&count==0,"defined non-note messages validate then omit without inventing semantics");
 source.clear();source.add(0,{0x90,69,127});require(Collector::convert(&source.stream,0,events.data(),1,count)&&count==1,"collector validates zero-frame0 timestamp independently of processor admission");
 require(!Collector::convert(&source.stream,1,nullptr,0,count),"note capacity preflight");
 auto *alias=reinterpret_cast<RavetNativeEventV1*>(source.packets.data());require(!Collector::convert(&source.stream,1,alias,1,count),"known output/input range alias rejected");
 source.clear();for(uint32_t i=0;i<1024;++i)source.add(i,{0x90,uint8_t(i%128),127});
 measuring=true;for(uint32_t i=0;i<10000;++i){if(!Collector::convert(&source.stream,1024,events.data(),1024,count)||count!=1024){measuring=false;throw std::runtime_error("max raw collector cycle");}source.packets[1023].mData[1]=128;if(Collector::convert(&source.stream,1024,events.data(),1024,count)){measuring=false;throw std::runtime_error("max late raw fault");}source.packets[1023].mData[1]=127;}measuring=false;
 require(!allocations&&!deletions,"10000 maximum/late-fault collection cycles no C++ heap");
}
void instrument(){
 for(uint32_t channels:{1u,2u}){
  auto owner=make(),oracle=make();require(owner.valid()&&owner.prepare(channels,48000)==0&&oracle.prepare(channels,48000)==0,"exact originating Performance no-input setup");
  std::array<std::array<float,1026>,2> a{},b{};for(auto &ch:a)ch.fill(77.f);for(auto &ch:b)ch.fill(77.f);float *out[]={a[0].data(),a[1].data()},*expected[]={b[0].data(),b[1].data()};Stream source;
  source.add(3,{0x90,69,64});require(owner.processNotes(out,240,&source.stream)==0&&oracle.processNotes(expected,240,&source.stream)==0,"actual native accepts collected canonical short notes");
  for(uint32_t i=0;i<240;++i){if(i<3)require(a[0][i]==0.f,"timed leading silence");else{const auto n=i-3;const auto analytic=std::sin(6.283185307179586*440./48000.*n)*(n+1)/240.*double(.2f)*double(64.f/127.f)/16.;require(std::abs(double(a[0][i])-analytic)<1e-8,"independent timed frequency/velocity/attack analytic sample");}if(channels==2)require(a[1][i]==a[0][i],"stereo native centered note matches mono");}require(a[0][240]==77.f,"render backing tail");
  Owner::Reservation pending{},other{};require(owner.stageParameter("p1",.75,pending)&&owner.commit(pending)&&oracle.stageParameter("p1",.75,other)&&oracle.commit(other),"queued Level before late malformed note");
  source.clear();source.add(0,{0x90,70,127});source.add(20,{0x90,70,128});require(owner.processNotes(out,64,&source.stream)==1&&owner.ready(),"whole late invalid note fails before control/voice advancement");
  source.clear();float *overlap[]={a[0].data(),a[0].data()+1};if(channels==2)require(owner.processNotes(overlap,64,&source.stream)==1,"late output overlap before pending Level/DSP");
  require(owner.processNotes(out,0,&source.stream)==1,"zero-frame pending global controls reject before state transfer");
  require(owner.processNotes(out,64,&source.stream)==0&&oracle.processNotes(expected,64,&source.stream)==0,"independent successful voice-phase history oracle");for(uint32_t ch=0;ch<channels;++ch)require(!std::memcmp(a[ch].data(),b[ch].data(),64*sizeof(float)),"late faults preserve phase and source-ordered pending Level");
  const auto before=source.stream;require(owner.processNotes(out,64,reinterpret_cast<const AAX_CMidiStream*>(&owner))==1,"owner-overlapping header rejects before dereference/clear");require(source.stream.mBuffer==before.mBuffer,"external stream remains stable");
  source.add(0,{0x90,69,127});const auto packets=source.packets;float *metadata[]={reinterpret_cast<float*>(source.packets.data()),a[1].data()};require(owner.processNotes(metadata,64,&source.stream)==1&&!std::memcmp(packets.data(),source.packets.data(),sizeof(packets)),"MIDI sample alias is rejected without clearing borrowed packets");
  require(owner.reset()==0,"checked reset removes voice history retains Level");source.clear();source.add(0,{0x90,69,127});source.add(0,{0x90,69,0});require(owner.processNotes(out,64,&source.stream)==0&&a[0][63]==0.f,"equal-offset note then zero release source order");
  require(owner.reset()==0,"source-order fresh history");source.clear();source.add(0,{0x90,69,0});source.add(0,{0x90,69,127});require(owner.processNotes(out,64,&source.stream)==0&&std::abs(a[0][63])>0.f,"reverse equal-offset order starts note");
  std::array<uint8_t,8208> state{};require(owner.save(state.data(),owner.stateBytes())&&owner.stateBytes()==24&&!std::memcmp(state.data(),"RAVETPS\0",8),"complete native Level state no fabricated bypass");
  require(owner.deactivate()==0&&!owner.ready()&&owner.prepare(channels,48000)==0,"checked stopped retirement/fresh instrument setup");
  source.clear();source.add(0,{0x90,69,127});source.add(20,{0x80,69,0});
  measuring=true;for(uint32_t i=0;i<1000;++i){Owner::Reservation queued{};if(!owner.stageParameter("p1",i%2?.25:.75,queued)||!owner.commit(queued)||owner.processNotes(out,64,&source.stream)){measuring=false;throw std::runtime_error("prepared instrument control/note cycle");}source.packets[1].mData[1]=128;if(owner.processNotes(out,64,&source.stream)!=1){measuring=false;throw std::runtime_error("prepared instrument late fault cycle");}source.packets[1].mData[1]=69;}measuring=false;require(!allocations&&!deletions,"prepared instrument valid/late-fault/control cycles no C++ heap");
 }
 using Small=ravet::aax::NativeAaxProcessor<1,4,true,4>;Small owner(ravet_performance_processor_v1(32),ravet_performance_audio_state_v1(32),ravet_performance_audio_state_v1(32)->cached_parameters_entry(),ravet_performance_state_v1(32));require(owner.prepare(1,48000)==0,"small combined capacity profile");
 std::array<uint8_t,8208> bytes{};require(owner.save(bytes.data(),owner.stateBytes()),"restore initial Level");Small::Reservation r{};require(owner.stageRestore(bytes.data(),owner.stateBytes(),r)&&owner.commit(r)&&owner.stageParameter("p1",.75,r)&&owner.commit(r),"queued complete restore then trailing Level");
 Stream source;for(int i=0;i<4;++i)source.add(0,{0x90,uint8_t(60+i),127});std::array<float,66> samples{};samples.fill(77.f);float *out[]={samples.data()};require(owner.processNotes(out,64,&source.stream)==1&&owner.ready()&&samples[0]==0.f&&samples[64]==77.f,"combined effective trailing controls/notes capacity before applyState/dequeue");
 source.clear();source.add(0,{0x90,69,127});require(owner.processNotes(out,64,&source.stream)==0,"retained complete restore/trailing Level consumes once with fitting notes");const auto analytic=std::sin(6.283185307179586*440./48000.*63)*64/240.*double(.75f)/16.;require(std::abs(double(samples[63])-analytic)<1e-8,"combined capacity rejection preserved queued Level and empty voice history");
}
int main(){try{collector();instrument();std::cout<<"PASS AAX bounded atomic short-note collection, authoritative native instrument timing/state/order/capacity, late-fault phase oracle and 10000 maximum collector plus 2000 prepared instrument allocation-counted cycles\n";return 0;}catch(const std::exception &e){std::cerr<<e.what()<<'\n';return 1;}}
