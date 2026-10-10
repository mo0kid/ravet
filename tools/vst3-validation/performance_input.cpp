#include "RavetVst3PerformanceInput.hpp"
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
static bool check(bool okay,const char *message){if(!okay)std::fprintf(stderr,"VST3 performance input: %s\n",message);return okay;}
template<class T,class B> void malformed(T &out,B bits){volatile B source=bits;const B copy=source;std::memcpy(&out,&copy,sizeof(copy));}
uint64_t bits(const double &value){uint64_t out=0;std::memcpy(&out,&value,sizeof(out));return out;}
class List final:public Vst::IEventList {
public:
    std::array<E,1025> events{};int32 count=0,failure=-1;unsigned calls=0;
    tresult PLUGIN_API queryInterface(const TUID,void **out)override{if(out)*out=nullptr;return kNoInterface;}
    uint32 PLUGIN_API addRef()override{return 1;}
    uint32 PLUGIN_API release()override{return 1;}
    int32 PLUGIN_API getEventCount()override{return count;}
    tresult PLUGIN_API getEvent(int32 index,E &out)override{++calls;if(index==failure||index<0||index>=count||index>=1025)return kResultFalse;out=events[size_t(index)];return kResultOk;}
    tresult PLUGIN_API addEvent(E &)override{return kResultFalse;}
};
E note(bool on=true,int32 id=0){E e{};e.busIndex=2;e.sampleOffset=3;e.type=on?E::kNoteOnEvent:E::kNoteOffEvent;if(on)e.noteOn={15,127,0.f,.123456f,999,id};else e.noteOff={15,127,.123456f,id,0.f};return e;}
E expression(uint32 type,double value,int32 id=0){E e{};e.busIndex=2;e.sampleOffset=3;e.type=E::kNoteExpressionValueEvent;e.noteExpressionValue={type,id,value};return e;}
bool scalarMappings(){
    std::array<RavetNativeEventV1,2> out{};size_t count=91;
    for(bool on:{false,true})for(int32 id:{-1,0,std::numeric_limits<int32>::max(),-10000,-1000,std::numeric_limits<int32>::min()}){
        auto raw=note(on,id);for(float tuning:{-12000.f,-100.f,0.f,100.f,12000.f}){
            if(on)raw.noteOn.tuning=tuning;else raw.noteOff.tuning=tuning;
            if(!check(nativePerformanceInput(raw,2,8,UnknownExpressionPolicy::Reject,out,count)==PerformanceInputResult::Mapped&&count==size_t(tuning==0.f?1:2),"notes, optional ids, plugin negative IDs and cents expansion"))return false;
            const auto &n=out[(!on&&tuning!=0.f)?1:0];const auto expected=on?(id==-1?RAVET_NATIVE_NOTE_ON:RAVET_NATIVE_NOTE_ON_WITH_ID):(id==-1?RAVET_NATIVE_NOTE_OFF:RAVET_NATIVE_NOTE_OFF_WITH_ID);
            if(!check(n.kind==uint32_t(expected)&&n.channel==15&&n.key==127&&n.scope==0&&n.note_id==(id==-1?0:uint32_t(id))&&n.value==double(.123456f),"unquantized velocity and exact identity/canonical concrete fields"))return false;
            if(tuning!=0.f){const auto &x=out[on?1:0];if(!check(x.kind==RAVET_NATIVE_NOTE_EXPRESSION&&x.detail==2&&x.scope==uint32_t(id==-1?3:7)&&x.value==double(tuning)/100.&&x.offset==3,"On then tuning / tuning then Off stable expansion"))return false;}
            else if(!check(out[1].kind==0,"unused candidate canonical"))return false;
        }
    }
    for(uint32 type:{Vst::kVolumeTypeID,Vst::kPanTypeID,Vst::kTuningTypeID,Vst::kVibratoTypeID,Vst::kExpressionTypeID,Vst::kBrightnessTypeID})for(double value:{0.,.12345678912345678,.25,.5,1.}){
        auto raw=expression(type,value,-10000);if(!check(nativePerformanceInput(raw,2,8,UnknownExpressionPolicy::Reject,out,count)==PerformanceInputResult::Mapped&&count==1&&out[0].kind==RAVET_NATIVE_NOTE_EXPRESSION&&out[0].detail==type&&out[0].scope==4&&out[0].note_id==uint32_t(-10000),"all six standard normalized expression kinds and ID-only scope"))return false;
        const double expected=type==Vst::kVolumeTypeID?value*4.:type==Vst::kTuningTypeID?240.*(value-.5):value;
        if(!check(bits(out[0].value)==bits(expected),"explicit physical expression mapping"))return false;
    }
    auto raw=expression(Vst::kVolumeTypeID,0.);malformed(raw.noteExpressionValue.value,UINT64_C(0x8000000000000000));
    if(!check(nativePerformanceInput(raw,2,8,UnknownExpressionPolicy::Reject,out,count)==PerformanceInputResult::Mapped&&bits(out[0].value)==UINT64_C(0x8000000000000000),"signed zero mute retained"))return false;
    malformed(raw.noteExpressionValue.value,UINT64_C(1));
    if(!check(nativePerformanceInput(raw,2,8,UnknownExpressionPolicy::Reject,out,count)==PerformanceInputResult::Mapped&&bits(out[0].value)==4,"minimum subnormal volume maps exactly"))return false;
    raw={};raw.busIndex=2;raw.sampleOffset=3;raw.type=E::kPolyPressureEvent;raw.polyPressure={15,127,.123456f,-1};
    return check(nativePerformanceInput(raw,2,8,UnknownExpressionPolicy::Reject,out,count)==PerformanceInputResult::Mapped&&out[0].detail==6&&out[0].scope==3&&out[0].value==double(.123456f),"poly pressure preserves channel/key and f32 physical value");
}
bool rejection(){
    for(unsigned test=0;test<23;++test){auto raw=note();
        if(test==0)raw.noteOn.channel=-1;if(test==1)raw.noteOn.channel=16;if(test==2)raw.noteOn.pitch=-1;if(test==3)raw.noteOn.pitch=128;
        if(test==4)malformed(raw.noteOn.velocity,UINT32_C(0x7fc00001));if(test==5)malformed(raw.noteOn.velocity,UINT32_C(0x7f800000));if(test==6)raw.noteOn.velocity=-.1f;if(test==7)raw.noteOn.velocity=1.1f;
        if(test==8)malformed(raw.noteOn.tuning,UINT32_C(0x7fc00001));if(test==9)raw.noteOn.tuning=12001.f;if(test==10)raw.noteOn.tuning=-12001.f;
        if(test>=11&&test<=15){raw=expression(Vst::kVolumeTypeID,.5);if(test==11)raw.noteExpressionValue.noteId=-1;if(test==12)malformed(raw.noteExpressionValue.value,UINT64_C(0x7ff8000000000001));if(test==13)raw.noteExpressionValue.value=-.1;if(test==14)raw.noteExpressionValue.value=1.1;if(test==15)malformed(raw.noteExpressionValue.value,UINT64_C(0x7ff0000000000000));}
        if(test==16)raw.busIndex=-1;if(test==17)raw.sampleOffset=-1;if(test==18)raw.sampleOffset=8;
        if(test>=19){raw.type=test==19?E::kDataEvent:test==20?E::kNoteExpressionTextEvent:test==21?E::kNoteExpressionIntValueEvent:E::kLegacyMIDICCOutEvent;}
        std::array<RavetNativeEventV1,2> out{};out[0].kind=999;out[1].note_id=77;const auto before=out;size_t count=91;
        if(!check(nativePerformanceInput(raw,2,8,UnknownExpressionPolicy::Reject,out,count)==PerformanceInputResult::Rejected&&count==91&&std::memcmp(out.data(),before.data(),sizeof(out))==0,"complete malformed/unsupported rejection preserves output/count"))return false;
    }
    auto custom=expression(Vst::kCustomStart,.5);std::array<RavetNativeEventV1,2> out{};out[0].kind=999;size_t count=91;
    if(!check(nativePerformanceInput(custom,2,8,UnknownExpressionPolicy::Reject,out,count)==PerformanceInputResult::Rejected&&nativePerformanceInput(custom,2,8,UnknownExpressionPolicy::Ignore,out,count)==PerformanceInputResult::Ignored&&count==91&&out[0].kind==999,"explicit unknown policy preserves output"))return false;
    malformed(custom.noteExpressionValue.value,UINT64_C(0x7ff8000000000001));
    if(!check(nativePerformanceInput(custom,2,8,UnknownExpressionPolicy::Ignore,out,count)==PerformanceInputResult::Rejected,"unknown scalar preflight before filtering"))return false;
    custom.busIndex=1;return check(nativePerformanceInput(custom,2,8,UnknownExpressionPolicy::Reject,out,count)==PerformanceInputResult::Ignored&&out[0].kind==999,"unselected payload not dereferenced or interpreted");
}
bool lists(){
    List list;list.count=2;list.events[0]=note();list.events[0].noteOn.tuning=100.f;list.events[1]=expression(Vst::kVolumeTypeID,0.);
    std::array<RavetNativeEventV1,1024> out{};size_t used=0;const auto visitor=[&](const RavetNativeEventV1 &e){if(used>=out.size())return false;out[used++]=e;return true;};
    measuring=true;bool passed=true;
    for(unsigned cycle=0;cycle<10000;++cycle){used=0;list.calls=0;if(!visitValidatedPerformanceInput(&list,2,8,UnknownExpressionPolicy::Reject,visitor)||used!=3||list.calls!=2||out[0].kind!=RAVET_NATIVE_NOTE_ON_WITH_ID||out[1].detail!=2||out[2].value!=0.){passed=false;break;}}
    measuring=false;if(!check(passed&&news==0&&deletes==0,"10000 full list mappings preserve ties and use zero C++ heap/single getter reads"))return false;
    for(unsigned test=0;test<7;++test){list.count=2;list.failure=-1;list.events[1]=expression(Vst::kVolumeTypeID,0.);used=0;
        if(test==0)list.count=-1;if(test==1)list.count=1025;if(test==2)list.failure=1;if(test==3)list.events[1].sampleOffset=2;if(test==4)list.events[1].sampleOffset=8;if(test==5)list.events[1].type=E::kDataEvent;if(test==6)malformed(list.events[1].noteExpressionValue.value,UINT64_C(0x7ff8000000000001));
        if(!check(!visitValidatedPerformanceInput(&list,2,8,UnknownExpressionPolicy::Reject,visitor)&&used==0,"late raw failures/capacity/getters/order reject before ANY visitor"))return false;
    }
    list.failure=-1;list.count=1024;for(auto &e:list.events){e=note();e.sampleOffset=0;}
    used=0;if(!check(visitValidatedPerformanceInput(&list,2,0,UnknownExpressionPolicy::Reject,visitor)&&used==1024,"maximum count zero-frame notes"))return false;
    list.events[1023].noteOn.tuning=100.f;used=0;
    if(!check(!visitValidatedPerformanceInput(&list,2,0,UnknownExpressionPolicy::Reject,visitor)&&used==0,"expanded capacity fails atomically"))return false;
    list.count=2;list.events[0]=note();list.events[1]=note();used=0;const auto refusing=[&](const RavetNativeEventV1 &){return ++used<2;};
    return check(!visitValidatedPerformanceInput(&list,2,8,UnknownExpressionPolicy::Reject,refusing)&&used==2&&visitValidatedPerformanceInput(nullptr,2,0,UnknownExpressionPolicy::Reject,visitor)&&!visitValidatedPerformanceInput(nullptr,-1,0,UnknownExpressionPolicy::Reject,visitor),"explicit visitor failure retains prefix; null and invalid setup");
}
int main(){return scalarMappings()&&rejection()&&lists()?0:1;}
