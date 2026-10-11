#define RAVET_VST3_EVENT_LIST_TESTS
#include "BoundedEventLists.hpp"
#include <algorithm>
#include <cstdlib>
#include <iostream>
#include <memory>
#include <new>
#include <stdexcept>
#include <tuple>
using namespace Steinberg;
using namespace Steinberg::Vst;
using namespace ravet::vst3::processing_draft;
namespace ravet::vst3::processing_draft {
struct EventListTestAccess {
    static auto& inputRefs(EventBatch& batch){return batch.input_.refs;}
};
}
namespace {
thread_local bool measured=false;
thread_local size_t allocations=0,deallocations=0;
void require(bool good,const char* message){if(!good){std::cerr<<"FAIL "<<message<<'\n';throw std::runtime_error(message);}}
EventBuses buses(){EventBuses b{};b[0]=16;b[2]=2;return b;}
Record note(uint32_t offset=0){Record r{};r.struct_size=sizeof(r);r.type=Event::kNoteOnEvent;
    r.offset=offset;r.channel=15;r.pitch=69;r.note_id=401;r.velocity=.75f;r.tuning=-3.25f;r.note_length=100;
    r.flags=Event::kIsLive|Event::kUserReserved1|Event::kUserReserved2;r.ppq_position=-.125;return r;}
std::unique_ptr<EventPool> pool(){return std::make_unique<EventPool>();}
auto fields(const Record& r){return std::tie(r.struct_size,r.type,r.flags,r.bus,r.offset,r.ppq_position,
    r.channel,r.pitch,r.note_id,r.note_length,r.tuning,r.velocity,r.expression_id,r.data_type,r.value,
    r.integer_value,r.root,r.bass,r.mask,r.controller,r.midi_value,r.midi_value2,r.payload_bytes,r.reserved);}
bool exact(const Record& wanted,const Event& event,const uint8_t* payload){
    Record got{};NativePayload data{};return extract(event,got,data)&&fields(got)==fields(wanted)&&
        (!data.size||std::memcmp(data.bytes,payload+wanted.payload_offset,data.size)==0)&&
        (!data.text||(reinterpret_cast<uintptr_t>(data.bytes)%2==0&&data.bytes[data.size]==0&&data.bytes[data.size+1]==0));
}
std::array<Record,9> allInputs(){
    std::array<Record,9> records{};
    for(size_t i=0;i<records.size();++i){records[i].struct_size=sizeof(Record);records[i].type=uint16_t(i);
        records[i].offset=uint32_t(i);records[i].ppq_position=-.125+double(i);records[i].flags=0xc001;}
    records[0]=note();records[1].channel=1;records[1].pitch=70;records[1].note_id=-1;records[1].velocity=.125f;records[1].tuning=9.5f;
    records[2].payload_bytes=5;
    records[3].channel=2;records[3].pitch=71;records[3].note_id=401;records[3].velocity=.5f;
    records[4].note_id=401;records[4].expression_id=100007;records[4].value=.625;
    records[5].note_id=401;records[5].expression_id=100008;records[5].payload_offset=5;records[5].payload_bytes=6;
    records[6].root=69;records[6].bass=57;records[6].mask=65535;records[6].payload_offset=5;records[6].payload_bytes=6;
    records[7].root=60;records[7].mask=0xa5a5;records[7].payload_offset=5;records[7].payload_bytes=6;
    records[8].note_id=401;records[8].expression_id=100009;records[8].integer_value=UINT64_MAX;
    return records;
}
std::array<uint8_t,11> payload(){std::array<uint8_t,11> bytes{{0xf0,0xf8,0x90,1,0xf7}};
    const uint16_t text[]{0x41,0xd83d,0xde00};std::memcpy(bytes.data()+5,text,6);return bytes;}
void allKindsOwnedDispatchAndWholeCopy(){
    auto owner=pool();auto input=allInputs();const auto wanted=input;
    auto bytes=payload();const auto wantedBytes=bytes;auto in=buses(),out=buses();EventBatch* batch=nullptr;
    require(owner->prepare(32,input.data(),input.size(),bytes.data(),bytes.size(),in,out,batch)==EventAdmission::Ok,"all nine actual SDK input kinds admitted");
    // Original CONTROL/caller sources may disappear after the complete copy.
    input[0].pitch=2;std::fill(bytes.begin(),bytes.end(),0xee);in.fill(0);out.fill(0);
    auto& reader=batch->input();auto& writer=batch->output();
    require(reader.getEventCount()==9&&writer.getEventCount()==0,"SDK directional list counts");
    auto expected=wanted;
    for(int32 i=0;i<9;++i){
        Event event{};require(reader.getEvent(i,event)==kResultOk&&exact(wanted[size_t(i)],event,wantedBytes.data()),"exact common/scalar/opaque/Unicode SDK input dispatch");
        if(i==0){event.noteOn.noteId=INT32_MIN;expected[0].note_id=INT32_MIN;}
        if(i==3){event.polyPressure.noteId=-1234;expected[3].note_id=-1234;}
        if(i==8){event.noteExpressionIntValue.noteId=-10000;expected[8].note_id=-10000;}
        require(writer.addEvent(event)==kResultOk,"output echo copies valid input-owned payload");
        Event copied{};require(writer.getEvent(i,copied)==kResultOk&&exact(expected[size_t(i)],copied,wantedBytes.data()),"exact copied signed output identity/integer/metadata");
        NativePayload a{},b{};Record ar{},br{};require(extract(event,ar,a)&&extract(copied,br,b),"payload identities");
        if(a.size)require(a.bytes!=b.bytes,"output payload owns separate storage");
    }
    Event legacy{};legacy.type=Event::kLegacyMIDICCOutEvent;legacy.busIndex=2;legacy.sampleOffset=9;
    legacy.ppqPosition=9.25;legacy.midiCCOut={255,1,127,126};
    require(writer.addEvent(legacy)==kResultOk&&writer.getEventCount()==10,"actual SDK legacy CC output accepted only on selected bus/channel");
    require(batch->freeze()==0,"ten kind completion");
    std::array<Record,10> records{};std::array<uint8_t,128> packed{};uint32_t count=99,used=99;
    require(batch->copyOutput(records.data(),records.size(),packed.data(),packed.size(),count,used)&&count==10,"whole native wire output publication");
    for(size_t i=0;i<records.size();++i)require(validate(records[i],packed.data(),used,32,buses(),true),"copied output is complete valid wire");
    for(size_t i=0;i<9;++i)require(fields(records[i])==fields(expected[i])&&
        (!records[i].payload_bytes||std::memcmp(packed.data()+records[i].payload_offset,wantedBytes.data()+wanted[i].payload_offset,records[i].payload_bytes)==0),"independent wire/payload oracle");
    require(records[9].controller==255&&records[9].midi_value==127&&records[9].midi_value2==126&&records[9].bus==2,"legacy unsigned domain survives signed SDK fields");
    Event untouched{};untouched.busIndex=123;require(!reader.getEventCount()&&reader.getEvent(0,untouched)==kInvalidArgument&&untouched.busIndex==123&&batch->failureKind()==0,"frozen list cannot mutate output or completed diagnostic");
}
void wholeInputAdmissionAndSourceRanges(){
    auto owner=pool();auto records=allInputs();const auto bytes=payload();const auto profile=buses();EventBatch* batch=nullptr;
    require(owner->prepare(32,records.data(),records.size(),bytes.data(),bytes.size(),profile,profile,batch)==EventAdmission::Ok,"atomic setup");
    auto* original=batch;EventBatch* result=reinterpret_cast<EventBatch*>(uintptr_t{0x1234});
    require(owner->prepare(32,records.data(),records.size(),bytes.data(),bytes.size(),profile,profile,result)==EventAdmission::Busy,"nested process is busy");
    records.back().expression_id=UINT32_MAX;
    require(owner->prepare(32,records.data(),records.size(),bytes.data(),bytes.size(),profile,profile,result)==EventAdmission::Invalid&&result==reinterpret_cast<EventBatch*>(uintptr_t{0x1234}),"late malformed input rejected before pool/handle mutation");
    require(original->input().getEventCount()==9&&original->freeze()==0,"active original batch preserved by rejected preflight");
    auto one=note();one.note_id=-2;
    require(owner->prepare(1,&one,1,nullptr,0,profile,profile,result)==EventAdmission::Invalid,"reserved negative input identity rejected");
    one=note();one.offset=1;require(owner->prepare(1,&one,1,nullptr,0,profile,profile,result)==EventAdmission::Invalid,"offset boundary rejects");
    require(owner->prepare(1,nullptr,1,nullptr,0,profile,profile,result)==EventAdmission::Invalid,"null records");
    require(owner->prepare(1,reinterpret_cast<const Record*>(UINTPTR_MAX-7),1,nullptr,0,profile,profile,result)==EventAdmission::Invalid,"overflowed wire address");
    require(owner->prepare(1,reinterpret_cast<const Record*>(owner.get()),1,nullptr,0,profile,profile,result)==EventAdmission::Invalid,"whole pool protected input");
    require(owner->prepare(1,&one,2049,nullptr,0,profile,profile,result)==EventAdmission::Capacity,"event capacity checked before read");
    one=note();auto& inside=*reinterpret_cast<EventBatch**>(owner.get());
    require(owner->prepare(1,&one,1,nullptr,0,profile,profile,inside)==EventAdmission::Invalid,"whole pool protected handle publication");
    auto badProfile=profile;badProfile[15]=17;
    require(owner->prepare(1,&one,1,nullptr,0,badProfile,profile,result)==EventAdmission::Invalid,"complete copied bus profile validated");
    std::array<Record,2> order{note(1),note(0)};
    require(owner->prepare(2,order.data(),2,nullptr,0,profile,profile,result)==EventAdmission::Invalid,"global input order");
    auto empty=note();empty.type=Event::kNoteExpressionTextEvent;empty.expression_id=88;empty.note_id=401;
    std::array<Record,2048> texts{};texts.fill(empty);const std::array<uint8_t,32> manyText{{'A',0}};
    for(auto& record:texts)record.payload_bytes=32;
    require(owner->prepare(0,texts.data(),texts.size(),manyText.data(),manyText.size(),profile,profile,result)==EventAdmission::Invalid,"embedded NUL text rejects before aggregate copy");
    std::array<uint16_t,16> fullText{};fullText.fill('A');
    require(owner->prepare(0,texts.data(),texts.size(),reinterpret_cast<const uint8_t*>(fullText.data()),32,profile,profile,result)==EventAdmission::Capacity,"aggregate Unicode terminators participate in capacity");
    require(result==reinterpret_cast<EventBatch*>(uintptr_t{0x1234})&&!owner->observedRetention(),"all admission failures preserve caller output and lifetime state");
    require(owner->prepare(0,nullptr,0,nullptr,0,profile,profile,batch)==EventAdmission::Ok&&batch->freeze()==0,"empty zero-frame phase");
}
void SDKProtocolFaultsAndNoPartialPublication(){
    auto owner=pool();const auto profile=buses();auto one=note();EventBatch* batch=nullptr;
    auto start=[&]{require(owner->prepare(8,&one,1,nullptr,0,profile,profile,batch)==EventAdmission::Ok,"protocol phase");};
    start();Event event{};require(batch->input().getEvent(0,event)==kResultOk,"protocol event");
    require(batch->input().addEvent(event)==kInvalidArgument&&batch->freeze()==3,"immutable input add first fault");
    start();event.busIndex=999;require(batch->input().getEvent(-1,event)==kInvalidArgument&&event.busIndex==999&&batch->freeze()==1,"SDK index leaves caller event unchanged");
    start();require(batch->input().getEvent(0,*reinterpret_cast<Event*>(owner.get()))==kInvalidArgument&&batch->freeze()==1,"SDK output cannot overwrite pool");
    start();void* queried=reinterpret_cast<void*>(uintptr_t{0x5678});
    require(batch->input().queryInterface(nullptr,&queried)==kInvalidArgument&&queried==reinterpret_cast<void*>(uintptr_t{0x5678})&&batch->freeze()==1,"bad QI leaves output unchanged");
    start();require(batch->input().getEvent(0,event)==kResultOk&&batch->output().addEvent(event)==kResultOk,"first output before late fault");
    event.sampleOffset=-1;require(batch->output().addEvent(event)==kInvalidArgument&&batch->freeze()==1,"late malformed scalar faults whole process");
    std::array<Record,2> destination{};destination[0].pitch=777;std::array<uint8_t,8> bytes{};bytes.fill(0xa5);uint32_t count=77,used=88;
    require(!batch->copyOutput(destination.data(),destination.size(),bytes.data(),bytes.size(),count,used)&&
        destination[0].pitch==777&&count==77&&used==88&&bytes[0]==0xa5,"late fault cannot publish even earlier valid output");
    start();std::thread worker([&]{require(batch->input().getEventCount()==0&&batch->output().addEvent(event)==kInvalidArgument,"wrong-thread SDK operations refuse");});worker.join();
    require(batch->freeze()==2,"wrong-thread fault distinct");
    start();require(batch->input().release()==1&&batch->freeze()==1&&!owner->hasForeignReferences(),"borrowed release never removes permanent owner");
    start();auto& refs=EventListTestAccess::inputRefs(*batch);refs.store(UINT32_MAX-1);
    queried=nullptr;require(batch->input().queryInterface(IEventList::iid,&queried)==kResultOk&&queried==&batch->input()&&refs.load()==UINT32_MAX,"last representable QI reference");
    queried=reinterpret_cast<void*>(uintptr_t{0x5678});require(batch->input().queryInterface(IEventList::iid,&queried)==kInternalError&&!queried,"QI saturation no false pointer publication");
    refs.store(1);require(batch->freeze()==1,"saturation diagnostic; injected references restored for test cleanup");
}
void SDKPayloadAdmissionEchoAndAggregateLimits(){
    auto owner=pool();const auto profile=buses();auto records=allInputs();const auto source=payload();EventBatch* batch=nullptr;
    auto start=[&]{require(owner->prepare(32,records.data(),records.size(),source.data(),source.size(),profile,profile,batch)==EventAdmission::Ok,"payload phase");};
    auto bad=[&](Event& event,uint32_t kind){const auto status=batch->output().addEvent(event);const auto failure=batch->freeze();
        if(status==kResultOk||failure!=kind)std::cerr<<"payload type="<<event.type<<" status="<<status<<" actual="<<failure<<" expected="<<kind<<'\n';
        require(status!=kResultOk&&failure==kind,"native payload refusal and freeze");};
    start();Event data{};data.type=Event::kDataEvent;data.data={1,DataEvent::kMidiSysEx,reinterpret_cast<const uint8_t*>(owner.get())};bad(data,1);
    start();data.data.bytes=reinterpret_cast<const uint8_t*>(UINTPTR_MAX);data.data.size=2;bad(data,1);
    start();data.data.bytes=nullptr;data.data.size=1;bad(data,1);
    start();std::array<TChar,2> invalidText{{TChar('A'),TChar('B')}};Event text{};text.type=Event::kNoteExpressionTextEvent;
    text.noteExpressionText={88,401,1,invalidText.data()};bad(text,1);
    start();std::array<TChar,2> loneSurrogate{{TChar(0xd800),TChar(0)}};text.noteExpressionText.text=loneSurrogate.data();bad(text,1);
    start();require(batch->input().getEvent(7,text)==kResultOk,"last owned input text");
    data.data={1,DataEvent::kMidiSysEx,reinterpret_cast<const uint8_t*>(text.scale.text)+8};bad(data,1);
    start();require(batch->input().getEvent(2,data)==kResultOk&&batch->output().addEvent(data)==kResultOk,"opaque owned input payload echo");
    require(batch->output().getEvent(0,data)==kResultOk&&batch->output().addEvent(data)==kResultOk&&batch->freeze()==0,"duplicate prior output payload is copied before append");
    start();std::array<uint8_t,65536> maximum{};maximum.front()=0xf0;maximum[32768]=0xf8;maximum.back()=0xf7;
    data.sampleOffset=0;data.type=Event::kDataEvent;data.data={uint32_t(maximum.size()),DataEvent::kMidiSysEx,maximum.data()};
    require(batch->output().addEvent(data)==kResultOk,"maximum counted opaque payload admitted");
    const TChar nul=0;text.sampleOffset=0;text.type=Event::kNoteExpressionTextEvent;text.noteExpressionText={88,401,0,&nul};bad(text,4);
    start();auto event=sdkEvent(note(),source.data());
    for(size_t i=0;i<2048;++i)require(batch->output().addEvent(event)==kResultOk,"maximum scalar output events");
    require(batch->output().addEvent(event)==kInternalError&&batch->freeze()==4,"2049th output event capacity fault");
    start();event.sampleOffset=1;require(batch->output().addEvent(event)==kResultOk,"output ordered prefix");event.sampleOffset=0;bad(event,1);
}
void RetainedSixteenBatchesAndWorkerRelease(){
    auto owner=pool();const auto profile=buses();std::array<IEventList*,16> inputs{},outputs{};
    std::array<const uint8_t*,16> pointers{};std::array<uint8_t,16> expected{};EventBatch* batch=nullptr;
    Record record{};record.struct_size=sizeof(record);record.type=Event::kDataEvent;record.payload_bytes=1;
    for(size_t i=0;i<16;++i){uint8_t byte=uint8_t(i+17);expected[i]=byte;
        require(owner->prepare(0,&record,1,&byte,1,profile,profile,batch)==EventAdmission::Ok,"distinct unretained prepared slot");
        inputs[i]=&batch->input();outputs[i]=&batch->output();Event event{};
        require(inputs[i]->getEvent(0,event)==kResultOk&&event.data.bytes[0]==byte,"retained payload original");pointers[i]=event.data.bytes;
        require(inputs[i]->addRef()==2&&outputs[i]->addRef()==2&&batch->freeze()==0,"both directions separately retained");
    }
    EventBatch* result=reinterpret_cast<EventBatch*>(uintptr_t{0x1234});
    require(owner->prepare(0,nullptr,0,nullptr,0,profile,profile,result)==EventAdmission::Capacity&&result==reinterpret_cast<EventBatch*>(uintptr_t{0x1234}),"sixteen retained slots exhausted without overwrite");
    for(size_t i=0;i<16;++i)require(pointers[i][0]==expected[i]&&inputs[i]->getEventCount()==0,"frozen payload remains owned while SDK reads refuse");
    std::thread worker([&]{for(size_t i=0;i<16;++i){void* retained=nullptr;
        require(inputs[i]->queryInterface(FUnknown::iid,&retained)==kResultOk&&retained==inputs[i],"worker frozen QI available");
        require(static_cast<IEventList*>(retained)->release()==2&&inputs[i]->release()==1,"worker reference release preserves permanent owner");
        require(outputs[i]->release()==1,"worker output release no destruction");}});worker.join();
    require(!owner->hasForeignReferences()&&owner->observedRetention(),"sticky retention evidence survives final worker release");
    require(owner->prepare(0,nullptr,0,nullptr,0,profile,profile,batch)==EventAdmission::Ok&&batch->freeze()==0,"released pool slots reusable on later audio phase");
}
void ReusedPayloadPaddingAndTerminatorBounds(){
    auto owner=pool();const auto profile=buses();EventBatch* batch=nullptr;
    Record input[2]{};for(auto& r:input)r.struct_size=sizeof(r);
    input[0].type=Event::kDataEvent;input[0].payload_bytes=2;
    const uint8_t old[]{0x11,0x42};
    require(owner->prepare(1,input,1,old,2,profile,profile,batch)==EventAdmission::Ok,"seed input/output storage");
    auto* reused=batch;Event data{},text{};
    require(batch->input().getEvent(0,data)==kResultOk&&batch->output().addEvent(data)==kResultOk&&batch->freeze()==0,"seed prior-cycle payload byte");
    input[0].payload_bytes=1;input[1].type=Event::kNoteExpressionTextEvent;
    input[1].payload_offset=1;input[1].payload_bytes=2;
    const uint8_t fresh[]{0x55,'A',0};
    require(owner->prepare(1,input,2,fresh,3,profile,profile,batch)==EventAdmission::Ok&&batch==reused,"reuse same poisoned batch");
    require(batch->input().getEvent(0,data)==kResultOk&&batch->input().getEvent(1,text)==kResultOk,"read repacked input");
    const auto* owned=reinterpret_cast<const uint8_t*>(text.noteExpressionText.text);
    require(owned[-1]==0&&owned[0]=='A'&&owned[1]==0&&owned[2]==0&&owned[3]==0,"input padding and UTF16 terminator initialized");
    require(batch->output().addEvent(data)==kResultOk&&batch->output().addEvent(text)==kResultOk&&batch->freeze()==0,"output repacking completed");
    Record copied[2]{};uint8_t packed[8];std::memset(packed,0xa5,sizeof(packed));uint32_t count=9,used=9;
    require(batch->copyOutput(copied,2,packed,8,count,used)&&count==2&&used==6&&copied[1].payload_offset==2,"whole reused prefix published");
    const uint8_t expected[]{0x55,0,'A',0,0,0};
    require(std::memcmp(packed,expected,6)==0&&packed[6]==0xa5&&packed[7]==0xa5,"output padding deterministic with untouched suffix");
    std::array<uint8_t,65535> maximum{};maximum.fill(0x42);
    input[0].payload_bytes=65533;input[1].payload_offset=0;input[1].payload_bytes=0;
    require(owner->prepare(1,input,2,maximum.data(),maximum.size(),profile,profile,batch)==EventAdmission::Ok,"input alignment plus empty terminator exact65536 bound");
    require(batch->input().getEvent(0,data)==kResultOk&&batch->input().getEvent(1,text)==kResultOk,"boundary SDK events");
    owned=reinterpret_cast<const uint8_t*>(text.noteExpressionText.text);
    require(owned[-1]==0&&owned[0]==0&&owned[1]==0,"boundary input padding/terminator");
    require(batch->output().addEvent(data)==kResultOk&&batch->output().addEvent(text)==kResultOk&&batch->freeze()==0,"output exact65536 boundary");
    std::array<uint8_t,65536> full{};
    require(batch->copyOutput(copied,2,full.data(),full.size(),count,used)&&used==65536&&copied[1].payload_offset==65534&&
        full[65532]==0x42&&full[65533]==0&&full[65534]==0&&full[65535]==0,"all final capacity bytes initialized");
    input[0].payload_bytes=65535;auto* unchanged=batch;
    require(owner->prepare(1,input,2,maximum.data(),maximum.size(),profile,profile,batch)==EventAdmission::Capacity&&batch==unchanged,"input alignment/terminator overflow before mutation");
    require(owner->prepare(1,nullptr,0,nullptr,0,profile,profile,batch)==EventAdmission::Ok,"output boundary failure phase");
    data.data={65535,DataEvent::kMidiSysEx,maximum.data()};
    const TChar empty=0;text.noteExpressionText.text=&empty;
    const auto dataStatus=batch->output().addEvent(data),textStatus=batch->output().addEvent(text);
    const auto failure=batch->freeze();
    require(dataStatus==kResultOk&&textStatus==kInternalError&&failure==4,"output alignment/terminator overflow faults whole batch");
    packed[0]=0xa5;count=9;used=9;
    require(!batch->copyOutput(copied,2,packed,8,count,used)&&packed[0]==0xa5&&count==9&&used==9,"late boundary refusal publishes no prior valid prefix");
}
void WholeCopyAliasesAndHeapFreePhases(){
    auto owner=pool();const auto profile=buses();auto record=note();EventBatch* batch=nullptr;
    require(owner->prepare(1,&record,1,nullptr,0,profile,profile,batch)==EventAdmission::Ok,"publication setup");
    Event event{};require(batch->input().getEvent(0,event)==kResultOk&&batch->output().addEvent(event)==kResultOk&&batch->freeze()==0,"publication completed");
    std::array<Record,2> destination{};destination[0].pitch=77;std::array<uint8_t,16> bytes{};uint32_t count=7,used=8;
    require(!batch->copyOutput(destination.data(),0,bytes.data(),bytes.size(),count,used)&&destination[0].pitch==77&&count==7&&used==8,"too-small complete output unchanged");
    require(!batch->copyOutput(destination.data(),2,reinterpret_cast<uint8_t*>(destination.data()),16,count,used)&&destination[0].pitch==77,"overlapping output resources unchanged");
    require(!batch->copyOutput(destination.data(),2,bytes.data(),bytes.size(),count,count)&&count==7,"overlapping publication counts unchanged");
    require(!batch->copyOutput(reinterpret_cast<Record*>(owner.get()),2,bytes.data(),bytes.size(),count,used),"whole pool protected caller publication");
    // Warm up local thread identity before measuring the ordinary C++ heap.
    (void)std::this_thread::get_id();bool good=true;allocations=deallocations=0;measured=true;
    for(size_t i=0;i<10000;++i){const uint32_t frames=uint32_t(i%17);
        good&=owner->prepare(frames,&record,1,nullptr,0,profile,profile,batch)==EventAdmission::Ok;
        good&=batch->input().getEvent(0,event)==kResultOk&&batch->output().addEvent(event)==kResultOk;
        good&=batch->freeze()==0;
        good&=batch->copyOutput(destination.data(),2,bytes.data(),bytes.size(),count,used)&&count==1&&used==0&&destination[0].pitch==69;
    }
    measured=false;require(good&&allocations==0&&deallocations==0,"10000 variable/zero actual SDK list phases no ordinary C++new/delete");
}
}
void* operator new(size_t size){if(measured)++allocations;if(void* p=std::malloc(size?size:1))return p;throw std::bad_alloc();}
void* operator new[](size_t size){return ::operator new(size);}
void operator delete(void* pointer)noexcept{if(measured&&pointer)++deallocations;std::free(pointer);}
void operator delete[](void* pointer)noexcept{::operator delete(pointer);}
int main(){try{allKindsOwnedDispatchAndWholeCopy();wholeInputAdmissionAndSourceRanges();SDKProtocolFaultsAndNoPartialPublication();
    SDKPayloadAdmissionEchoAndAggregateLimits();RetainedSixteenBatchesAndWorkerRelease();ReusedPayloadPaddingAndTerminatorBounds();WholeCopyAliasesAndHeapFreePhases();
    std::cout<<"PASS actual SDK event lists:7 groups,all10kinds,owned UTF16/SysEx,deterministic reused padding,signed IDs,uint64 expressions,whole publication,16 retained slots,10000 measured phases\n";
    return 0;}catch(const std::exception& error){measured=false;std::cerr<<error.what()<<'\n';return 1;}}
