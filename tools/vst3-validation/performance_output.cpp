#include "RavetVst3PerformanceInput.hpp"
#include "RavetVst3PerformanceOutput.hpp"
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <new>
namespace { bool measuring=false;size_t news=0,deletes=0; }
void *operator new(size_t bytes){if(measuring)++news;if(auto *p=std::malloc(bytes?bytes:1))return p;throw std::bad_alloc();}
void operator delete(void *p)noexcept{if(p&&measuring)++deletes;std::free(p);}
void operator delete(void *p,size_t)noexcept{::operator delete(p);}
using namespace Steinberg;
using E=Vst::Event;
using namespace ravet::vst3;
static bool check(bool okay,const char *message){if(!okay)std::fprintf(stderr,"VST3 performance output: %s\n",message);return okay;}
template<class T,class B>void malformed(T &out,B bits){volatile B source=bits;const B copy=source;std::memcpy(&out,&copy,sizeof(out));}
uint64_t bits(const double &value){uint64_t out=0;std::memcpy(&out,&value,sizeof(out));return out;}
class Host final:public Vst::IEventList {
public:
    std::array<E,1024> events{};size_t accepted=0,limit=1024,calls=0;bool mutate=false;
    tresult PLUGIN_API queryInterface(const TUID,void **out)override{if(out)*out=nullptr;return kNoInterface;}
    uint32 PLUGIN_API addRef()override{return 1;}
    uint32 PLUGIN_API release()override{return 1;}
    int32 PLUGIN_API getEventCount()override{return int32(accepted);}
    tresult PLUGIN_API getEvent(int32 index,E &out)override{if(index<0||size_t(index)>=accepted)return kResultFalse;out=events[size_t(index)];return kResultOk;}
    tresult PLUGIN_API addEvent(E &in)override{++calls;if(accepted>=limit)return kResultFalse;events[accepted++]=in;if(mutate)in.sampleOffset=999;return kResultOk;}
};
RavetNativeEventV1 note(uint32_t kind=RAVET_NATIVE_NOTE_ON){RavetNativeEventV1 e{};e.offset=3;e.kind=kind;e.channel=15;e.key=127;e.value=double(.123456f);return e;}
RavetNativeEventV1 expression(uint32_t detail,double value,uint32_t id=0){RavetNativeEventV1 e{};e.offset=3;e.kind=RAVET_NATIVE_NOTE_EXPRESSION;e.scope=4;e.detail=detail;e.note_id=id;e.value=value;return e;}
bool mappings(){
    E out{};
    for(uint32_t kind:{RAVET_NATIVE_NOTE_ON,RAVET_NATIVE_NOTE_OFF,RAVET_NATIVE_NOTE_ON_WITH_ID,RAVET_NATIVE_NOTE_OFF_WITH_ID,RAVET_NATIVE_NOTE_OFF_SCOPED})for(uint32_t id:{0u,UINT32_C(0x7fffffff),UINT32_C(0x80000000),uint32_t(-10000),uint32_t(-1000)}){
        auto raw=note(kind);const bool identified=kind==RAVET_NATIVE_NOTE_ON_WITH_ID||kind==RAVET_NATIVE_NOTE_OFF_WITH_ID||kind==RAVET_NATIVE_NOTE_OFF_SCOPED;
        if(identified)raw.note_id=id;if(kind==RAVET_NATIVE_NOTE_OFF_SCOPED)raw.scope=7;
        if(!check(nativePerformanceOutput(raw,2,8,out)&&out.busIndex==2&&out.sampleOffset==3&&out.flags==0&&out.ppqPosition==0.,"all supported note tags and SDK metadata defaults"))return false;
        const bool on=kind==RAVET_NATIVE_NOTE_ON||kind==RAVET_NATIVE_NOTE_ON_WITH_ID;
        int32 expectedId=-1;if(identified)std::memcpy(&expectedId,&id,sizeof(id));
        if(!check(out.type==(on?E::kNoteOnEvent:E::kNoteOffEvent)&&(on?out.noteOn.noteId:out.noteOff.noteId)==expectedId,"note identity bit mapping"))return false;
        // Decode through the independent input mapper to validate canonical values.
        std::array<RavetNativeEventV1,2> decoded{};size_t count=0;
        if(!check(nativePerformanceInput(out,2,8,UnknownExpressionPolicy::Reject,decoded,count)==PerformanceInputResult::Mapped&&count==1&&decoded[0].channel==15&&decoded[0].key==127&&decoded[0].value==raw.value&&decoded[0].note_id==raw.note_id,"note scalar/address roundtrip"))return false;
    }
    for(uint32_t detail=0;detail<6;++detail)for(double value:{0.,.12345678912345678,1.}){
        auto raw=expression(detail,detail==0?value*4.:detail==2?value*240.-120.:value,uint32_t(-10000));
        if(!check(nativePerformanceOutput(raw,2,8,out)&&out.type==E::kNoteExpressionValueEvent&&out.noteExpressionValue.typeId==detail&&out.noteExpressionValue.noteId==-10000,"six standard expression output kinds"))return false;
        const double expected=detail==0?raw.value/4.:detail==2?raw.value/240.+.5:raw.value;
        if(!check(bits(out.noteExpressionValue.value)==bits(expected),"explicit normalized physical mapping"))return false;
    }
    auto raw=expression(0,0.);malformed(raw.value,UINT64_C(0x8000000000000000));
    if(!check(nativePerformanceOutput(raw,2,8,out)&&bits(out.noteExpressionValue.value)==UINT64_C(0x8000000000000000),"signed zero volume output"))return false;
    malformed(raw.value,UINT64_C(4));if(!check(nativePerformanceOutput(raw,2,8,out)&&bits(out.noteExpressionValue.value)==1,"representable minimum normalized subnormal"))return false;
    for(uint64_t encoded:{UINT64_C(0),UINT64_C(1),UINT64_C(2),UINT64_C(3),UINT64_C(4),UINT64_C(0x0010000000000001),UINT64_C(0x0010000000000004),UINT64_C(0x0020000000000001),UINT64_C(0x0020000000000002),UINT64_C(0x0030000000000001)}) {
        raw=expression(0,0.);malformed(raw.value,encoded);
        const bool expected=encoded==0||encoded==4||encoded==UINT64_C(0x0010000000000004)||encoded==UINT64_C(0x0020000000000002)||encoded==UINT64_C(0x0030000000000001);
        if(!check(nativePerformanceOutput(raw,2,8,out)==expected,"volume normalization exactness at subnormal and normal exponent boundaries"))return false;
    }
    for(uint32_t scope:{3u,7u}){raw=expression(6,double(.123456f));raw.scope=scope;raw.channel=15;raw.key=127;raw.note_id=scope==7?uint32_t(-10000):0;
        if(!check(nativePerformanceOutput(raw,2,8,out)&&out.type==E::kPolyPressureEvent&&out.polyPressure.channel==15&&out.polyPressure.pitch==127&&out.polyPressure.pressure==.123456f&&out.polyPressure.noteId==(scope==7?-10000:-1),"exact f32 pressure and scoped address"))return false;
    }
    raw=note();malformed(raw.value,UINT64_C(0x8000000000000000));
    if(!check(nativePerformanceOutput(raw,2,8,out)&&out.type==E::kNoteOnEvent&&out.noteOn.velocity==0.f,"velocity-zero remains On"))return false;
    uint32_t velocityBits=0;std::memcpy(&velocityBits,&out.noteOn.velocity,sizeof(velocityBits));
    return check(velocityBits==UINT32_C(0x80000000),"signed zero velocity retained");
}
bool rejection(){
    for(unsigned test=0;test<28;++test){auto raw=note();
        if(test==0)raw.parameter_id=1;if(test==1)raw.words[3]=1;if(test==2)raw.channel=16;if(test==3)raw.key=128;if(test==4)raw.scope=1;if(test==5)raw.detail=1;if(test==6)raw.note_id=1;
        if(test==7)raw.value=.1;if(test==8)malformed(raw.value,UINT64_C(0x7ff8000000000001));if(test==9)malformed(raw.value,UINT64_C(0x7ff0000000000000));if(test==10)raw.value=-.1;if(test==11)raw.value=1.1;
        if(test==12){raw.kind=RAVET_NATIVE_NOTE_ON_WITH_ID;raw.note_id=UINT32_MAX;}if(test==13)raw.kind=RAVET_NATIVE_NOTE_CHOKE;if(test==14)raw.kind=RAVET_NATIVE_NOTE_END;if(test==15)raw.kind=RAVET_NATIVE_MIDI;if(test==16)raw.kind=RAVET_NATIVE_NOTE_OFF_SCOPED;
        if(test>=17){raw=expression(0,2.);if(test==17)raw.scope=7;if(test==18)raw.channel=1;if(test==19)raw.key=1;if(test==20)raw.note_id=UINT32_MAX;if(test==21)raw.value=4.1;if(test==22)raw.value=-.1;if(test==23)raw.detail=7;if(test==24)malformed(raw.value,UINT64_C(1));if(test==25)malformed(raw.value,UINT64_C(3));if(test==26){raw.detail=6;raw.scope=3;raw.value=.1;}if(test==27){raw.detail=2;raw.value=120.1;}}
        E out{};out.busIndex=999;out.type=77;const auto before=out;
        if(!check(!nativePerformanceOutput(raw,2,8,out)&&std::memcmp(&out,&before,sizeof(out))==0,"unsupported/noncanonical/scalar/identity/scope rejection leaves SDK output unchanged"))return false;
    }
    E out{};auto raw=note();return check(!nativePerformanceOutput(raw,-1,8,out)&&!nativePerformanceOutput(raw,2,-1,out)&&!nativePerformanceOutput(raw,2,65537,out)&&!nativePerformanceOutput(raw,2,0,out),"configuration/timing bounds");
}
bool delivery(){
    Host host;std::array<RavetNativeEventV1,1025> events{};for(auto &e:events){e=note();e.offset=0;}
    const auto before=events;host.mutate=true;bool passed=true;measuring=true;
    for(unsigned cycle=0;cycle<1000;++cycle){host.accepted=host.calls=0;const auto result=deliverNativePerformanceOutput(events.data(),1024,&host,2,0);
        if(result.reason!=PerformanceOutputRejection::None||result.delivered!=1024||result.undelivered||host.calls!=1024||std::memcmp(events.data(),before.data(),sizeof(events))!=0){passed=false;break;}}
    measuring=false;if(!check(passed&&news==0&&deletes==0,"1000 maximum zero-frame output lists use zero C++ heap and isolate host mutation"))return false;
    for(unsigned test=0;test<5;++test){host.accepted=host.calls=0;events=before;size_t count=2;
        if(test==0)events[1].kind=RAVET_NATIVE_NOTE_END;if(test==1)events[1].value=.1;if(test==2){events[0].offset=1;events[1].offset=0;}if(test==3)events[1].offset=8;if(test==4)count=1025;
        const auto result=deliverNativePerformanceOutput(events.data(),count,&host,2,8);
        if(!check(result.reason==PerformanceOutputRejection::Unrepresentable&&result.delivered==0&&result.undelivered==count&&host.calls==0,"complete output preflight before ANY host call"))return false;
    }
    events=before;for(size_t limit=0;limit<3;++limit){host.accepted=host.calls=0;host.limit=limit;
        const auto result=deliverNativePerformanceOutput(events.data(),3,&host,2,8);
        if(!check(result.reason==PerformanceOutputRejection::HostRejected&&result.delivered==limit&&result.undelivered==3-limit&&host.accepted==limit&&host.calls==limit+1,"exact host prefix refusal counts without rollback/retry"))return false;
    }
    const auto empty=deliverNativePerformanceOutput(nullptr,0,nullptr,2,0),missing=deliverNativePerformanceOutput(events.data(),1,nullptr,2,8),null=deliverNativePerformanceOutput(nullptr,1,&host,2,8);
    return check(empty.reason==PerformanceOutputRejection::None&&missing.reason==PerformanceOutputRejection::Unrepresentable&&null.reason==PerformanceOutputRejection::Unrepresentable,"null/empty explicit delivery");
}
int main(){return mappings()&&rejection()&&delivery()?0:1;}
