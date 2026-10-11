#include "RavetLv2MidiOutput.hpp"
#include <array>
#include <atomic>
#include <cstdlib>
#include <iostream>
#include <memory>
#include <new>
#include <stdexcept>
std::atomic<size_t> allocations{0},deallocations{0};std::atomic<bool> measuring{false};
void* operator new(size_t n){if(measuring.load(std::memory_order_relaxed))++allocations;if(void* p=std::malloc(n?n:1))return p;throw std::bad_alloc();}
void operator delete(void* p)noexcept{if(measuring.load(std::memory_order_relaxed))++deallocations;std::free(p);}
void* operator new[](size_t n){return ::operator new(n);}void operator delete[](void* p)noexcept{::operator delete(p);}
namespace {
void require(bool v,const char* m){if(!v)throw std::runtime_error(m);}
using Output=ravet::native::Lv2MidiOutput<1024>;constexpr ravet::native::Lv2MidiOutputTypes types{1,2,3,4};
struct Host {alignas(8)std::array<uint8_t,Output::maximumBytes+64> bytes{};void reset(uint32_t body=Output::maximumBodyBytes,uint32_t type=1){bytes.fill(0x5a);LV2_Atom atom{body,type};std::memcpy(bytes.data(),&atom,8);}};
RavetNativeEventV1 note(uint32_t offset=0){RavetNativeEventV1 e{};e.offset=offset;e.kind=RAVET_NATIVE_NOTE_ON;e.channel=2;e.key=60;e.value=.5;return e;}
RavetNativeEventV1 midi(uint8_t status,uint32_t length){RavetNativeEventV1 e{};e.kind=RAVET_NATIVE_MIDI;e.detail=length;e.words[0]=status;if(length>1)e.words[0]|=uint32_t(61)<<8;if(length>2)e.words[0]|=uint32_t(63)<<16;return e;}
void check(const Host& host,uint32_t count,uint8_t status=0x92,uint8_t velocity=64){
 LV2_Atom_Sequence seq{};std::memcpy(&seq,host.bytes.data(),sizeof(seq));require(seq.atom.type==2&&seq.atom.size==8+24*count&&seq.body.unit==4&&!seq.body.pad,"complete typed Sequence/body byte capacity");
 for(uint32_t i=0;i<count;++i){LV2_Atom_Event e{};size_t pos=16+24*i;std::memcpy(&e,host.bytes.data()+pos,16);require(e.time.frames==0&&e.body.type==3&&e.body.size==3,"sample-timed event/type/payload");require(host.bytes[pos+16]==status&&host.bytes[pos+17]==60&&host.bytes[pos+18]==velocity,"independent note/channel/key velocity encoding");for(uint32_t j=19;j<24;++j)require(host.bytes[pos+j]==0,"deterministic padded payload");}
 for(size_t i=16+24*count;i<host.bytes.size();++i)require(host.bytes[i]==0x5a,"unused output backing tail");
}
void tests(){
 auto output=std::make_unique<Output>(types);Host host{};Output::Admission admitted;std::array<RavetNativeEventV1,1024> events{};events.fill(note());
 host.reset();require(output->admit(host.bytes.data(),nullptr,0,admitted)&&admitted.complete(),"full configured worst-case admitted");require(output->stage(events.data(),1024,1)&&output->publish(admitted),"actual full1024 staging/publication");check(host,1024);require(!output->publish(admitted)&&!output->readyStopped(),"consumed publication never replays");
 for(auto kind:{RAVET_NATIVE_NOTE_ON,RAVET_NATIVE_NOTE_OFF}){events[0]=note();events[0].kind=kind;host.reset();require(output->admit(host.bytes.data(),nullptr,0,admitted)&&output->stage(events.data(),1,0)&&output->publish(admitted),"zero-frame offsets0 complete ordinary notes");check(host,1,kind==RAVET_NATIVE_NOTE_ON?0x92:0x82);}
 for(float value:{0.f,std::numeric_limits<float>::denorm_min(),1.f}){events[0]=note();events[0].value=double(value);host.reset();require(output->admit(host.bytes.data(),nullptr,0,admitted)&&output->stage(events.data(),1,1)&&output->publish(admitted),"velocity zero/positive-minimum/full canonical boundary");check(host,1,0x92,value==0.f?0:(value==1.f?127:1));}
 const std::array<uint8_t,17> statuses{0x80,0x90,0xa0,0xb0,0xc0,0xd0,0xe0,0xf1,0xf2,0xf3,0xf6,0xf8,0xfa,0xfb,0xfc,0xfe,0xff};
 for(auto status:statuses){uint32_t length=status<0xf0?((status&0xe0)==0xc0?2:3):(status==0xf1||status==0xf3?2:(status==0xf2?3:1));events[0]=midi(status,length);host.reset();require(output->admit(host.bytes.data(),nullptr,0,admitted)&&output->stage(events.data(),1,1)&&output->publish(admitted),"all supported channel/system raw short classes");LV2_Atom_Event e{};std::memcpy(&e,host.bytes.data()+16,16);require(e.body.size==length&&host.bytes[32]==status&&(!length||host.bytes[33]==(length>1?61:0)),"raw bytes and deterministic padding retained");}
 events.fill(note());host.reset();require(output->admit(host.bytes.data(),nullptr,0,admitted)&&output->stage(events.data(),1024,1),"prior complete staged bytes");std::array<uint8_t,Output::maximumBytes> prior{};std::memcpy(prior.data(),output->bytesStopped(),prior.size());
 events.back().words[3]=1;require(!output->stage(events.data(),1024,1)&&!output->readyStopped()&&std::memcmp(prior.data(),output->bytesStopped(),prior.size())==0,"late malformed record invalidates readiness but retains prior bytes");require(!output->publish(admitted)&&output->empty(admitted),"failed stage may publish only safely admitted complete empty Sequence");check(host,0);require(!output->empty(admitted),"empty write consumes admission");events.back().words[3]=0;
 for(auto bad:{0xf0,0xf4,0xf5,0xf7,0xf9,0xfd}){events[0]=midi(uint8_t(bad),1);require(!output->stage(events.data(),1,1),"unsupported/undefined/SystemExclusive short output rejects");}
 events[0]=note();volatile uint64_t nanSource=UINT64_C(0x7ff8000000000000);uint64_t nanBits=nanSource;std::memcpy(&events[0].value,&nanBits,8);require(!output->stage(events.data(),1,1),"nonfinite native scalar rejects under fast math");events[0]=note();events[0].value=.1;require(!output->stage(events.data(),1,1),"lossy noncanonical f32 widened velocity rejects");events[0]=note(1);require(!output->stage(events.data(),1,1),"late-frame offset rejects");events[0]=note(1);events[1]=note(0);require(!output->stage(events.data(),2,2),"source order rejects before publication");events.fill(note());
 host.reset(8);require(!output->admit(host.bytes.data(),nullptr,0,admitted)&&admitted.canEmpty()&&!admitted.complete(),"short Chunk admits only complete16-byte empty sequence");require(output->empty(admitted),"capacity refusal safely empties");check(host,0);
 for(uint32_t size:{0u,7u}){host.reset(size);auto priorHost=host.bytes;require(!output->admit(host.bytes.data(),nullptr,0,admitted)&&!admitted.canEmpty()&&!output->empty(admitted)&&priorHost==host.bytes,"insufficient empty capacity preserves every host byte");}
 host.reset(Output::maximumBodyBytes,2);auto priorHost=host.bytes;require(!output->admit(host.bytes.data(),nullptr,0,admitted)&&!output->empty(admitted)&&priorHost==host.bytes,"previous output Sequence is not next-run Chunk capacity");
 host.reset();const auto first=reinterpret_cast<uintptr_t>(host.bytes.data());ravet::native::Lv2ProtectedRange span{first+Output::maximumBytes-1,first+Output::maximumBytes};priorHost=host.bytes;require(!output->admit(host.bytes.data(),&span,1,admitted)&&!output->empty(admitted)&&priorHost==host.bytes,"late protected alias whole configured capacity rejects with no write");
 require(!output->admit(host.bytes.data()+1,nullptr,0,admitted),"eight-byte Atom alignment");require(!output->admit(&admitted,nullptr,0,admitted),"admission metadata cannot be foreign output");require(!output->admit(output.get(),nullptr,0,admitted),"owned staging metadata cannot be foreign output");
 Output wrong({1,1,3,4});require(!wrong.valid()&&!wrong.admit(host.bytes.data(),nullptr,0,admitted),"CONTROL URID collisions invalid");
 host.reset();require(output->admit(host.bytes.data(),nullptr,0,admitted)&&output->stage(events.data(),1,1),"fresh valid token");Output::Admission newer;require(output->admit(host.bytes.data(),nullptr,0,newer)&&!output->publish(admitted),"superseded admission cannot replay");require(!admitted.first()&&!admitted.last(),"superseded known ranges never describe a fresh block");output->beginBlock();require(!newer.first()&&!newer.last()&&!output->empty(newer),"early owner rejection invalidates readiness and known-range token");
 allocations=0;deallocations=0;measuring=true;bool okay=true;
 for(uint32_t i=0;i<10000;++i){host.reset();okay&=output->admit(host.bytes.data(),nullptr,0,admitted)&&output->stage(events.data(),1024,1)&&output->publish(admitted);host.reset();okay&=output->admit(host.bytes.data(),nullptr,0,admitted);events.back().words[3]=1;okay&=!output->stage(events.data(),1024,1)&&output->empty(admitted);events.back().words[3]=0;}
 measuring=false;require(okay&&allocations==0&&deallocations==0,"10000 actual maximum-output/late-fault cycles ordinary C++ heap-free");
}
}
int main(){try{tests();std::cout<<"PASS LV2 full configured Chunk capacity/alignment/aliases, exact staged short classes/order/velocity/empty/stale tails and10000 max1024 ordinary-C++zeroheap cycles\n";return 0;}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
