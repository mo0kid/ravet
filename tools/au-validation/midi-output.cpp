#include "RavetAuMidiOutput.hpp"
#include "RavetPerformance.h"
#include <cstdlib>
#include <iostream>
#include <new>
#include <stdexcept>
namespace allocation {bool active=false;size_t created=0,retired=0;}
void *operator new(size_t bytes){if(allocation::active)++allocation::created;if(auto p=std::malloc(bytes?bytes:1))return p;throw std::bad_alloc();}
void *operator new[](size_t bytes){return ::operator new(bytes);}
void operator delete(void *p)noexcept{if(p&&allocation::active)++allocation::retired;std::free(p);}
void operator delete[](void *p)noexcept{::operator delete(p);}
void operator delete(void *p,size_t)noexcept{::operator delete(p);}
void operator delete[](void *p,size_t)noexcept{::operator delete(p);}
namespace {
void require(bool value,const char *text){if(!value)throw std::runtime_error(text);}
using Message=ravet::native::NativeMidiMessage;
using Owner=ravet::au::NativeAuMidiOutput<>;
using Rejection=ravet::au::MidiOutputRejection;
RavetNativeEventV1 midi(uint32_t status,uint32_t count,uint32_t data1=0,uint32_t data2=0){RavetNativeEventV1 out{};out.kind=RAVET_NATIVE_MIDI;out.detail=count;out.words[0]=status|(data1<<8)|(data2<<16);return out;}
RavetNativeEventV1 note(uint32_t kind=RAVET_NATIVE_NOTE_ON,double velocity=double(.5f)){RavetNativeEventV1 out{};out.kind=kind;out.channel=15;out.key=127;out.value=velocity;return out;}
void foreign(double &value,uint64_t pattern){volatile uint64_t source=pattern;const uint64_t bits=source;std::memcpy(&value,&bits,sizeof(value));}
const RavetNativeProcessorEntryV1 *entry=nullptr;void *core=nullptr;
bool canonical(const RavetNativeEventV1 &event){const RavetNativeProcessorBusV1 bus{0,1,nullptr,nullptr};RavetNativeProcessorResultV1 result{};auto zero=event;zero.offset=0;return !entry->process(core,&bus,1,0,&zero,1,nullptr,nullptr,0,&result)&&result.status==0;}
uint32_t decode(const MIDIPacketList *list,std::array<Message,1024> &output){
    const MIDIPacket *packet=&list->packet[0];uint32_t count=0;
    for(uint32_t i=0;i<list->numPackets;++i){uint32_t at=0;while(at<packet->length){const auto status=packet->data[at];uint32_t size=0;
        if(status>=0x80&&status<=0xef)size=(status>>4)==0xc||(status>>4)==0xd?2:3;
        else if(status==0xf1||status==0xf3)size=2;else if(status==0xf2)size=3;
        else if(status==0xf6||status==0xf8||status==0xfa||status==0xfb||status==0xfc||status==0xfe||status==0xff)size=1;
        if(!size||size>packet->length-at||count==output.size()||packet->timeStamp>65535)return UINT32_MAX;
        auto &message=output[count++];message={};message.offset=uint32_t(packet->timeStamp);message.size=size;for(uint32_t j=0;j<size;++j)message.bytes[j]=packet->data[at+j];at+=size;
    }packet=MIDIPacketNext(packet);}
    return count;
}
bool sameMessages(const std::array<Message,1024> &left,const std::array<Message,1024> &right,uint32_t count){for(uint32_t i=0;i<count;++i)if(left[i].offset!=right[i].offset||left[i].size!=right[i].size||left[i].bytes!=right[i].bytes)return false;return true;}
size_t packetExtent(const MIDIPacketList *list){const auto *packet=&list->packet[0];for(uint32_t i=0;i<list->numPackets;++i)packet=MIDIPacketNext(packet);return size_t(reinterpret_cast<const uint8_t *>(packet)-reinterpret_cast<const uint8_t *>(list));}
struct Host {
    Owner *owner=nullptr;uint32_t calls=0,count=0,index=0;const AudioTimeStamp *timestamp=nullptr;OSStatus status=noErr;bool reenter=false,stable=true;
    std::array<Message,1024> messages{};
};
OSStatus callback(void *user,const AudioTimeStamp *time,UInt32 index,const MIDIPacketList *list){auto &host=*static_cast<Host *>(user);++host.calls;host.timestamp=time;host.index=index;host.count=decode(list,host.messages);
    if(host.reenter){std::array<uint8_t,16388> before{};const auto bytes=packetExtent(list);host.stable=host.stable&&bytes<=before.size();if(bytes>before.size())return -1;std::memcpy(before.data(),list,bytes);auto raw=midi(0xf8,1);const AUMIDIOutputCallbackStruct again{callback,user};
        host.stable=host.stable&&!host.owner->stage(&raw,1,0)&&host.owner->deliver(again,time,index).reason==Rejection::Busy;
        host.stable=host.stable&&!std::memcmp(before.data(),list,bytes);
    }return host.status;
}
void scalars(){
    Message message{};
    for(uint32_t status=0x80;status<0xf0;++status){const uint32_t count=(status>>4)==0xc||(status>>4)==0xd?2:3;auto raw=midi(status,count,127,count==3?126:0);require(canonical(raw)&&ravet::native::nativeMidiOutput(raw,0,message)&&message.bytes[0]==status&&message.bytes[1]==127&&message.size==count,"all channel MIDI short forms canonical/exact");}
    for(uint32_t status:{0xf1,0xf2,0xf3,0xf6,0xf8,0xfa,0xfb,0xfc,0xfe,0xff}){const uint32_t count=status==0xf2?3:status==0xf1||status==0xf3?2:1;auto raw=midi(status,count,count>1?17:0,count>2?29:0);require(canonical(raw)&&ravet::native::nativeMidiOutput(raw,0,message)&&message.bytes[0]==status&&message.size==count,"all defined system short forms canonical/exact");}
    for(uint32_t kind:{uint32_t(RAVET_NATIVE_NOTE_ON),uint32_t(RAVET_NATIVE_NOTE_OFF)})for(float value:{0.f,.000001f,.5f,1.f}){auto raw=note(kind,double(value));require(canonical(raw)&&ravet::native::nativeMidiOutput(raw,0,message),"native exactf32 note acceptance");const uint8_t velocity=value==0.f?0:value==.5f?64:value==1.f?127:kind==RAVET_NATIVE_NOTE_ON?1:0;require(message.bytes[0]==(kind==RAVET_NATIVE_NOTE_ON?0x9f:0x8f)&&message.bytes[1]==127&&message.bytes[2]==velocity,"explicit MIDI7 nearest/ties-up positiveOn minimum1/exactzero convention");}
    auto signedZero=note();foreign(signedZero.value,UINT64_C(0x8000000000000000));require(canonical(signedZero)&&ravet::native::nativeMidiOutput(signedZero,0,message)&&!message.bytes[2],"signedzero used velocity canonical; wirezeroOff convention explicit");
    auto retained=message;for(uint32_t test=0;test<25;++test){auto raw=midi(0x90,3,64,100);
        if(test==0)raw.parameter_id=1;if(test==1)raw.note_id=1;if(test==2)raw.scope=1;if(test==3)raw.channel=1;if(test==4)raw.key=1;if(test==5)raw.value=-0.;if(test==6)raw.detail=0;if(test==7)raw.detail=4;if(test==8)raw.words[1]=1;if(test==9)raw.words[0]|=0x1000000;
        if(test==10)raw.words[0]=0x00908090;if(test==11)raw.words[0]=0x1000c0;if(test>=12){raw=note();if(test==12)raw.value=.1;if(test==13)foreign(raw.value,UINT64_C(0x7ff8000000000001));if(test==14)foreign(raw.value,UINT64_C(0x7ff0000000000000));if(test==15)raw.value=-1.;if(test==16)raw.value=1.1;if(test==17)raw.channel=16;if(test==18)raw.key=128;if(test==19)raw.words[3]=1;if(test==20)raw.detail=1;if(test==21)raw.kind=RAVET_NATIVE_NOTE_ON_WITH_ID;if(test==22)raw.kind=RAVET_NATIVE_NOTE_OFF_SCOPED;if(test==23)raw.kind=RAVET_NATIVE_NOTE_END;if(test==24)raw.kind=RAVET_NATIVE_UMP;}
        require(!ravet::native::nativeMidiOutput(raw,0,message)&&!std::memcmp(&message,&retained,sizeof(message)),"malformed/unrepresentable native fields/scalars reject atomically");
        if(test<21)require(!canonical(raw),"authoritative native codec rejects malformed canonical fields");
    }
    for(uint32_t status:{0x00,0x7f,0xf0,0xf4,0xf5,0xf7,0xf9,0xfd}){auto raw=midi(status,1);require(!ravet::native::nativeMidiOutput(raw,0,message),"running status/SysEx/undefined reject");}
}
void nativeCompletion(){
    auto *processor=entry->create();RavetNativeProcessorConfigV1 config{};config.version=1;config.maximum_frames=64;config.maximum_events=1024;config.maximum_output_events=16;config.bus_count=1;config.sample_rate=48000.;config.buses[0]={0,1};require(processor&&!entry->prepare(processor,&config),"actual completion-output owner");
    const RavetNativeProcessorBusV1 bus{0,1,nullptr,nullptr};std::array<RavetNativeEventV1,16> output{};RavetNativeProcessorResultV1 result{};
    auto on=note();on.kind=RAVET_NATIVE_NOTE_ON_WITH_ID;on.note_id=UINT32_MAX;require(!entry->process(processor,&bus,1,0,&on,1,nullptr,output.data(),output.size(),&result)&&!result.event_count,"actual identified voice starts");
    RavetNativeEventV1 choke{};choke.kind=RAVET_NATIVE_NOTE_CHOKE;choke.scope=7;choke.note_id=on.note_id;choke.channel=on.channel;choke.key=on.key;require(!entry->process(processor,&bus,1,0,&choke,1,nullptr,output.data(),output.size(),&result)&&result.event_count==1&&output[0].kind==RAVET_NATIVE_NOTE_END&&output[0].note_id==UINT32_MAX,"actual native generated completion retains full identity");
    Owner owner;Host host;host.owner=&owner;AudioTimeStamp time{};AUMIDIOutputCallbackStruct installed{callback,&host};require(!owner.stage(output.data(),result.event_count,0)&&owner.deliver(installed,&time,0).reason==Rejection::NotStaged&&!host.calls,"actual native completion is unsupported, never becomes invented MIDI NoteOff");
    require(!entry->deactivate(processor),"completion owner stop");entry->destroy(processor);
}
void delivery(){
    ravet::au::NativeAuMidiOutput<1> tiny;auto minimum=note();require(tiny.stage(&minimum,1,0)&&tiny.count()==1&&tiny.packets()->numPackets==1&&tiny.packets()->packet[0].length==3&&tiny.packets()->packet[0].data[2]==64,"minimum template bound uses complete correctly sized CoreMIDI packet storage");require(!tiny.stage(&minimum,2,0)&&!tiny.ready()&&tiny.count()==1,"minimum bound rejects complete count before read and invalidates readiness");
    Owner owner;Host host;host.owner=&owner;host.reenter=true;AUMIDIOutputCallbackStruct installed{callback,&host};AudioTimeStamp time{};time.mSampleTime=-123.5;time.mFlags=kAudioTimeStampSampleTimeValid;
    std::array<RavetNativeEventV1,1025> records{};std::array<Message,1024> expected{};
    for(uint32_t i=0;i<1024;++i){records[i]=i%2?note(RAVET_NATIVE_NOTE_OFF,double(.5f)):midi(0x90,3,i%128,100);records[i].offset=i/2;require(ravet::native::nativeMidiOutput(records[i],1024,expected[i]),"expected maximum messages");}
    require(owner.stage(records.data(),1024,1024)&&owner.count()==1024&&owner.ready(),"maximum full stage");auto outcome=owner.deliver(installed,&time,7);require(outcome.reason==Rejection::None&&outcome.attempted==1024&&host.calls==1&&host.timestamp==&time&&host.index==7&&host.count==1024&&host.stable&&sameMessages(expected,host.messages,1024),"single full host callback exact timestamp/index/bytes/order/reentrancy");require(owner.deliver(installed,&time,7).reason==Rejection::NotStaged&&host.calls==1,"successful delivery cannot replay");
    const auto oldCount=owner.count();std::array<Message,1024> previous{};require(decode(owner.packets(),previous)==1024,"retained consumed packet observation");
    for(uint32_t test=0;test<7;++test){auto bad=records;size_t count=1024;uint32_t frames=1024;if(test==0)bad[1023].kind=RAVET_NATIVE_NOTE_END;if(test==1)bad[1023].value=.1;if(test==2)bad[1023].offset=1024;if(test==3)bad[1023].offset=0;if(test==4)count=1025;if(test==5)frames=65537;if(test==6)count=1;
        require(owner.stage(records.data(),1024,1024),"fresh prior stage");const auto source=test==6?reinterpret_cast<const RavetNativeEventV1 *>(UINTPTR_MAX-(UINTPTR_MAX%alignof(RavetNativeEventV1))-8):bad.data();
        require(!owner.stage(source,count,frames)&&!owner.ready()&&owner.count()==oldCount,"complete late/range/count rejection invalidates ready but retains count");std::array<Message,1024> after{};require(decode(owner.packets(),after)==1024&&sameMessages(previous,after,1024)&&owner.deliver(installed,&time,7).reason==Rejection::NotStaged&&host.calls==1,"fault retains packet bytes with no stale callback");
    }
    require(owner.stage(records.data(),1,1024),"fresh refusal stage");host.status=-77;outcome=owner.deliver(installed,&time,2);require(outcome.reason==Rejection::HostRejected&&outcome.hostStatus==-77&&outcome.attempted==1&&host.calls==2&&!owner.ready()&&owner.deliver(installed,&time,2).reason==Rejection::NotStaged,"host refusal consumed once with status/unknown acceptance");host.status=0;
    AUMIDIOutputCallbackStruct missing{};require(owner.stage(records.data(),1,1024)&&owner.deliver(missing,&time,0).reason==Rejection::MissingCallback&&!owner.ready(),"missing callback consumes current block");require(owner.stage(records.data(),1,1024)&&owner.deliver(installed,nullptr,0).reason==Rejection::MissingCallback&&!owner.ready(),"missing timestamp consumes current block");require(owner.stage(nullptr,0,0)&&owner.deliver(missing,nullptr,UINT32_MAX).reason==Rejection::None&&host.calls==2,"empty list requires no host/time/output guessing");
    bool okay=true;host.reenter=false;allocation::created=allocation::retired=0;allocation::active=true;for(uint32_t cycle=0;cycle<2000;++cycle){okay=owner.stage(records.data(),1024,1024)&&owner.deliver(installed,&time,7).reason==Rejection::None&&okay;const auto kind=records[1023].kind;records[1023].kind=RAVET_NATIVE_NOTE_END;okay=!owner.stage(records.data(),1024,1024)&&owner.deliver(installed,&time,7).reason==Rejection::NotStaged&&okay;records[1023].kind=kind;}allocation::active=false;require(okay&&!allocation::created&&!allocation::retired,"2000 maximum accepted/late-fault packet deliveries no counted ordinary C++heap");
}
}
int main(){try{entry=ravet_performance_processor_v1(32);core=entry->create();RavetNativeProcessorConfigV1 config{};config.version=1;config.maximum_frames=64;config.maximum_events=1024;config.bus_count=1;config.sample_rate=48000.;config.buses[0]={0,1};require(core&&!entry->prepare(core,&config),"actual native canonical validator");scalars();nativeCompletion();delivery();require(!entry->deactivate(core),"validator stop");entry->destroy(core);std::cout<<"PASS AU native short MIDI canonicalization/whole packet/host delivery contracts\n";return 0;}catch(const std::exception &error){std::cerr<<error.what()<<'\n';return 1;}}
