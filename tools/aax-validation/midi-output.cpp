#include "RavetAaxMidiOutput.hpp"
#include <atomic>
#include <cstdlib>
#include <iostream>
#include <new>
#include <stdexcept>
static std::atomic<uint64_t> allocations{0};
void* operator new(size_t n){allocations.fetch_add(1);if(void* p=std::malloc(n?n:1))return p;throw std::bad_alloc();}
void* operator new[](size_t n){return ::operator new(n);}
void operator delete(void* p)noexcept{std::free(p);}
void operator delete[](void* p)noexcept{std::free(p);}
void operator delete(void* p,size_t)noexcept{std::free(p);}
void operator delete[](void* p,size_t)noexcept{std::free(p);}
namespace {
using Stage=ravet::aax::AaxMidiOutput<>;
void require(bool b,const char* text){if(!b)throw std::runtime_error(text);}
RavetNativeEventV1 note(uint32_t offset,uint32_t kind=RAVET_NATIVE_NOTE_ON,double velocity=1.){RavetNativeEventV1 e{};e.offset=offset;e.kind=kind;e.channel=15;e.key=127;e.value=velocity;return e;}
RavetNativeEventV1 midi(uint32_t offset,uint32_t bytes,uint32_t length){RavetNativeEventV1 e{};e.offset=offset;e.kind=RAVET_NATIVE_MIDI;e.words[0]=bytes;e.detail=length;return e;}
struct Node final:AAX_IMIDINode {
    Stage* stage=nullptr;
    std::array<AAX_CMidiPacket,1024> seen{};
    uint32_t calls=0,refuseAt=UINT32_MAX,throwAt=UINT32_MAX;
    bool reentry=false,stable=true;
    AAX_CMidiStream* GetNodeBuffer()override{throw std::runtime_error("output does not request input");}
    AAX_ITransport* GetTransport()override{throw std::runtime_error("no unversioned transport");}
    AAX_Result PostMIDIPacket(AAX_CMidiPacket* packet)override {
        const auto i=calls++;const auto original=*packet;
        if(i<seen.size())seen[i]=original;
        if(reentry){const auto e=note(0);stable=stable&&stage->stage(&e,1,1).code==Stage::StageCode::Busy&&stage->deliver(this).code==Stage::DeliveryCode::Busy&&!stage->invalidate()&&std::memcmp(&original,packet,sizeof(original))==0;}
        if(i==throwAt)throw 17;
        return i==refuseAt?AAX_Result(-7342):AAX_SUCCESS;
    }
    void reset(){calls=0;refuseAt=throwAt=UINT32_MAX;reentry=false;stable=true;}
};
void contracts(){
 Stage stage;Node node;node.stage=&stage;
 std::array<RavetNativeEventV1,1024> events{};
 events[0]=note(0,RAVET_NATIVE_NOTE_ON,0.);events[1]=note(1,RAVET_NATIVE_NOTE_ON,double(0.000001f));events[2]=note(1,RAVET_NATIVE_NOTE_OFF,double(.5f));events[3]=midi(3,0x7f00bf,3);events[4]=midi(3,0x7fcf,2);events[5]=midi(3,0x7fdf,2);events[6]=midi(3,0x7f7fef,3);events[7]=midi(3,0x7f7faf,3);
 require(bool(stage.stage(events.data(),8,4)),"whole supported subset stage");node.reentry=true;
 auto delivered=stage.deliver(&node);require(bool(delivered)&&delivered.attempted==8&&delivered.returned==8&&delivered.posted==8&&node.stable,"delivery counts and callback borrow reentry stability");
 require(node.seen[0].mData[0]==0x9f&&node.seen[0].mData[2]==0&&node.seen[1].mData[2]==1&&node.seen[2].mData[0]==0x8f&&node.seen[2].mData[2]==64,"velocity zero/tiny/half independent oracle");
 for(uint32_t i=0;i<8;++i)require(node.seen[i].mTimestamp==events[i].offset&&node.seen[i].mIsImmediate==0&&node.seen[i].mData[3]==0,"exact offsets and zero packet slack/immediate");
 require(stage.deliver(&node).code==Stage::DeliveryCode::NotReady&&node.calls==8,"successful batch consumed once");
 for(const auto invalid:{midi(3,0x7f01bf,3),midi(3,0xf8,1),note(3,RAVET_NATIVE_NOTE_ON,.1)}) {
  require(bool(stage.stage(events.data(),8,4)),"fresh batch before late fault");events[8]=invalid;require(!stage.stage(events.data(),9,4),"unsupported CC/system or noncanonical f64 late rejected");require(stage.deliver(&node).code==Stage::DeliveryCode::NotReady&&node.calls==8,"failed stage invalidates stale batch no partial posts");
 }
 auto identity=note(3);identity.note_id=1;require(!stage.stage(&identity,1,4),"nonzero hidden identity rejected");
 identity=note(3,RAVET_NATIVE_NOTE_OFF_SCOPED);identity.scope=RAVET_NATIVE_SCOPE_CHANNEL|RAVET_NATIVE_SCOPE_KEY;require(!stage.stage(&identity,1,4),"generic scoped release not stripped by encoder");
 identity=note(3,RAVET_NATIVE_NOTE_ON_WITH_ID);require(!stage.stage(&identity,1,4),"explicit identity zero kind rejected");
 node.reset();node.refuseAt=2;require(bool(stage.stage(events.data(),8,4)),"refusal input valid");delivered=stage.deliver(&node);
 require(delivered.code==Stage::DeliveryCode::Refused&&delivered.attempted==3&&delivered.returned==3&&delivered.posted==2&&delivered.hasStatus&&delivered.status==-7342,"late original refusal and partial irrevocable count");require(stage.deliver(&node).code==Stage::DeliveryCode::NotReady&&node.calls==3,"refusal no replay");
 node.reset();node.throwAt=0;require(bool(stage.stage(events.data(),8,4)),"throw input valid");delivered=stage.deliver(&node);require(delivered.code==Stage::DeliveryCode::Exception&&delivered.attempted==1&&!delivered.returned&&!delivered.posted&&!delivered.hasStatus,"first throw no invented SDK status");
 node.reset();node.throwAt=2;require(bool(stage.stage(events.data(),8,4)),"late throw input valid");delivered=stage.deliver(&node);require(delivered.code==Stage::DeliveryCode::Exception&&delivered.attempted==3&&delivered.returned==2&&delivered.posted==2&&delivered.hasStatus&&delivered.status==AAX_SUCCESS,"late throw retains last actual returned success, not unreturned latest status");require(stage.deliver(&node).code==Stage::DeliveryCode::NotReady&&node.calls==3,"late throw never replays preceding accepted posts");
 require(bool(stage.stage(nullptr,0,0))&&bool(stage.deliver(&node)),"empty zero-frame batch valid");
 require(bool(stage.stage(events.data(),8,4))&&stage.deliver(nullptr).code==Stage::DeliveryCode::InvalidNode&&stage.deliver(&node).code==Stage::DeliveryCode::NotReady,"null node consumes prepared readiness");
 for(uint32_t i=0;i<1024;++i)events[i]=note(i);
 node.reset();const auto before=allocations.load();
 for(uint32_t cycle=0;cycle<1000;++cycle){require(bool(stage.stage(events.data(),1024,1024)),"maximum stage");node.calls=0;require(bool(stage.deliver(&node)),"maximum delivery");events[1023].offset=1024;require(!stage.stage(events.data(),1024,1024)&&stage.deliver(&node).code==Stage::DeliveryCode::NotReady,"late maximum offset fault");events[1023].offset=1023;}
 require(allocations.load()==before,"1000 maximum and late-fault cycles allocate zero C++ heap");
}
}
int main(){try{contracts();std::cout<<"AAX MIDI stage/subset/refusal/reentry/max-heap contracts PASS\n";return 0;}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
