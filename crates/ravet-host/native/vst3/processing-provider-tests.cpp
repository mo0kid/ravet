#include "provider.cpp"
#include "RavetVst3State.h"
#include "processing-instance-fixture.hpp"
#include "public.sdk/source/vst/hosting/module.h"
#include "pluginterfaces/vst/ivsthostapplication.h"
#include <filesystem>
#include <map>
#include <unistd.h>
namespace {
struct RetainedFactory final:IPluginFactory3 {
    Observation state;
    Factory authored{state};
    uint32_t references=1,entries=0,exits=0,installed=0,cleared=0;
    FUnknown* host=nullptr;
    FUnknown* borrowedHost=nullptr;
    void probe(){require(borrowedHost);void* result=nullptr;require(borrowedHost->queryInterface(IHostApplication::iid,&result)==kResultOk&&result);static_cast<IHostApplication*>(result)->release();}
    tresult PLUGIN_API queryInterface(const TUID id,void** output)override {
        *output=nullptr;
        if(FUnknownPrivate::iidEqual(id,FUnknown::iid)||FUnknownPrivate::iidEqual(id,IPluginFactory::iid)||
           FUnknownPrivate::iidEqual(id,IPluginFactory2::iid)||FUnknownPrivate::iidEqual(id,IPluginFactory3::iid)) {
            *output=static_cast<IPluginFactory3*>(this);addRef();return kResultOk;
        }return kNoInterface;
    }
    uint32 PLUGIN_API addRef()override{return ++references;}
    uint32 PLUGIN_API release()override{return --references;}
    tresult PLUGIN_API getFactoryInfo(PFactoryInfo* out)override{*out={};std::strcpy(out->vendor,"RAVET");return kResultOk;}
    int32 PLUGIN_API countClasses()override{return 2;}
    tresult PLUGIN_API getClassInfo(int32 index,PClassInfo* out)override {
        *out={};out->cid[0]=char(index+1);out->cardinality=1;
        std::strcpy(out->category,index?"Component Controller Class":"Audio Module Class");std::strcpy(out->name,"State stream retention");return kResultOk;
    }
    tresult PLUGIN_API getClassInfo2(int32,PClassInfo2*)override{return kNotImplemented;}
    tresult PLUGIN_API getClassInfoUnicode(int32 index,PClassInfoW* out)override {
        *out={};out->cid[0]=char(index+1);out->cardinality=1;
        std::strcpy(out->category,index?"Component Controller Class":"Audio Module Class");out->name[0]='s';return kResultOk;
    }
    tresult PLUGIN_API createInstance(FIDString id,FIDString iid,void** output)override{return authored.createInstance(id,iid,output);}
    tresult PLUGIN_API setHostContext(FUnknown* value)override {
        if(value){require(!host);host=value;borrowedHost=value;host->addRef();++installed;probe();}
        else{require(host);probe();host->release();host=nullptr;++cleared;}
        return kNotImplemented;
    }
};
std::map<std::string,RetainedFactory*> modules;
class RetainedModule final:public VST3::Hosting::Module {
    RetainedFactory& owner;
public:
    explicit RetainedModule(RetainedFactory& from):owner(from){++owner.entries;owner.addRef();factory=VST3::Hosting::PluginFactory(owned<IPluginFactory>(&owner));}
    ~RetainedModule()noexcept override{owner.probe();factory=VST3::Hosting::PluginFactory(nullptr);++owner.exits;}
    bool load(const std::string&,std::string&)override{return true;}
};
}
namespace VST3::Hosting {
Module::Ptr Module::create(const std::string& path,std::string&){return std::make_shared<RetainedModule>(*modules.at(std::filesystem::canonical(path).string()));}
}
namespace {
using namespace ravet::vst3::processing_draft;
thread_local bool measured=false;
thread_local size_t allocations=0,deallocations=0;
struct Fixture {
    const RavetVst3ProviderV2* api=ravet_vst3_provider_v2();
    const RavetVst3ProcessingProviderV1* processing=ravet_vst3_processing_provider_v1();
    const RavetVst3ParameterProviderV1* parameters=ravet_vst3_parameter_provider_v1();
    RetainedFactory* factory=new RetainedFactory;
    RavetVst3Module* module=nullptr;RavetVst3Instance* child=nullptr;RavetVst3Audio* audio=nullptr;
    std::filesystem::path path;
    int32_t native=999;
    explicit Fixture(bool controller=true){
        require(metadata().classes[0].id[0]==1);
        char directory[]="/tmp/ravet-vst3-paired-provider-XXXXXX";require(mkdtemp(directory));path=directory;
        modules[std::filesystem::canonical(path).string()]=factory;factory->state.hasController=controller;
        auto text=path.string();require(api->open(reinterpret_cast<const uint8_t*>(text.data()),uint32_t(text.size()),&module,&native)==RAVET_VST3_OK);
        require(api->create(module,0,&child,&native)==RAVET_VST3_OK);
    }
    void prepare(){
        uint32_t count=999;RavetVst3ParameterReport parameterReport{};parameterReport.struct_size=sizeof(parameterReport);
        require(parameters->refresh(child,&count,&parameterReport)==RAVET_VST3_OK);
        RavetVst3ProcessingSelection selection{sizeof(selection),2,1,0};RavetVst3ProcessingInfo info{};info.struct_size=sizeof(info);
        RavetVst3ProcessingReport report{};report.struct_size=sizeof(report);
        require(processing->refresh(child,&selection,&info,&report)==RAVET_VST3_OK);
        require(info.input_bus_count==2&&info.output_bus_count==2&&info.selected_input_mask==2&&info.selected_output_mask==1);
        RavetVst3AudioProfile profile{sizeof(profile),16,1,1,48000};
        require(processing->prepare(child,&profile,&report)==RAVET_VST3_OK);
        require(api->start(child,&native)==RAVET_VST3_OK&&api->begin_audio(child,&audio,&native)==RAVET_VST3_OK);
    }
    ~Fixture(){
        if(audio)require(api->end_audio(audio,&native)==RAVET_VST3_OK);
        require(api->destroy(child,&native)==RAVET_VST3_OK);
        const auto result=api->close(module,&native);require(result==RAVET_VST3_OK||result==RAVET_VST3_PROCESS_PINNED);
        if(result==RAVET_VST3_OK)delete factory; // Pinned core borrows process-lifetime fixture factory.
        modules.erase(std::filesystem::canonical(path).string());std::filesystem::remove_all(path);
    }
};
struct Block {
    std::array<float,20> input{},output{};
    RavetVst3InputChannel in{input.data(),uint32_t(input.size()),0};
    RavetVst3OutputChannel out{output.data(),uint32_t(output.size()),0};
    RavetVst3AudioBlock block{sizeof(block),16,1,1,&in,&out};
    std::array<Record,12> events{};std::array<uint8_t,96> payload{};
    std::array<uint32_t,4> ids{};std::array<RavetVst3ParameterPoint,8> points{};
    RavetVst3ProcessingOutputs outputs{sizeof(outputs),0,events.data(),uint32_t(events.size()),777,
        payload.data(),uint32_t(payload.size()),778,ids.data(),uint32_t(ids.size()),779,
        points.data(),uint32_t(points.size()),780};
    RavetVst3ProcessingReport report{};
    Block(){input.fill(.75f);output.fill(9);payload.fill(0xa5);ids.fill(0xcccccccc);report.struct_size=sizeof(report);report.index=888;}
    int32_t call(Fixture& f,const RavetVst3ParameterPoint* parameters=nullptr,uint32_t parameterCount=0,
            const Record* inputEvents=nullptr,uint32_t eventCount=0,const uint8_t* bytes=nullptr,uint32_t byteCount=0,
            const RavetVst3ContextRecord* context=nullptr){
        return f.processing->process(f.audio,&block,parameters,parameterCount,inputEvents,eventCount,bytes,byteCount,context,&outputs,&report);
    }
};
Record note(uint32_t offset=0){Record r{};r.struct_size=sizeof(r);r.type=Event::kNoteOnEvent;r.bus=1;r.offset=offset;
    r.channel=1;r.pitch=69;r.note_id=401;r.velocity=.75f;r.tuning=-3.25f;r.note_length=100;r.flags=0xc001;r.ppq_position=-.125;return r;}
void metadataAndExactPair(){
    Fixture f;require(f.processing->instance_provider==f.api&&f.processing->version==1&&f.processing->struct_size==sizeof(*f.processing));
    uint32_t count=777;RavetVst3ParameterReport p{};p.struct_size=sizeof(p);require(f.parameters->refresh(f.child,&count,&p)==RAVET_VST3_OK&&count==2);
    RavetVst3ProcessingSelection selection{sizeof(selection),2,1,0};RavetVst3ProcessingInfo info{};info.struct_size=sizeof(info);
    RavetVst3ProcessingReport report{};report.struct_size=sizeof(report);
    require(f.processing->refresh(f.child,&selection,&info,&report)==RAVET_VST3_OK&&info.flags==3&&report.flags==0);
    RavetVst3EventBusRecord bus{};bus.struct_size=sizeof(bus);require(f.processing->bus_info(f.child,0,1,&bus)==RAVET_VST3_OK);
    require(bus.index==1&&bus.channels==2&&bus.bus_type==uint32_t(kAux)&&std::strcmp(bus.name,"y")==0);
    uint32_t id=888;require(f.processing->midi_assignment(f.child,1,1,7,&id)==RAVET_VST3_OK&&id==1);
    require(f.processing->midi_assignment(f.child,1,1,8,&id)==RAVET_VST3_OK&&id==UINT32_MAX);
    require(f.processing->midi_assignment(f.child,0,1,7,&id)==RAVET_VST3_INVALID&&id==UINT32_MAX);
    const auto previous=info;selection.input_mask=0x10000;require(f.processing->refresh(f.child,&selection,&info,&report)==RAVET_VST3_INVALID&&std::memcmp(&info,&previous,sizeof(info))==0);
}
void actualPairedTenKindsAndContext(uint32_t iterations=2,bool measure=false){
    Fixture f;f.prepare();Block b;
    std::array<Record,9> source{};for(size_t i=0;i<source.size();++i){source[i].struct_size=sizeof(Record);source[i].type=uint16_t(i);
        source[i].bus=1;source[i].offset=uint32_t(i);source[i].flags=0xc001;source[i].ppq_position=-.125+double(i);}
    source[0]=note();source[1].channel=1;source[1].pitch=70;source[1].note_id=-1;source[1].velocity=.125f;source[1].tuning=9.5f;
    source[2].payload_bytes=5;source[3].channel=1;source[3].pitch=71;source[3].note_id=401;source[3].velocity=.5f;
    source[4].note_id=401;source[4].expression_id=100007;source[4].value=.625;
    source[5].note_id=401;source[5].expression_id=100008;source[5].payload_offset=6;source[5].payload_bytes=6;
    source[6].root=69;source[6].bass=57;source[6].mask=65535;source[6].payload_offset=6;source[6].payload_bytes=6;
    source[7].root=60;source[7].mask=0xa5a5;source[7].payload_offset=6;source[7].payload_bytes=6;
    source[8].note_id=401;source[8].expression_id=100009;source[8].integer_value=UINT64_MAX;
    const std::array<uint16_t,6> bytes{{0xff00,0x80f0,0x00f7,0x0041,0xd83d,0xde00}};
    std::array<RavetVst3ParameterPoint,2> parameters{{{1,0,.375},{1,7,.625}}};
    RavetVst3ContextRecord context{};context.struct_size=sizeof(context);context.sample_rate=48000;context.project_time_samples=-9007199254740993LL;
    context.state=ContextFlags;
    context.tempo=123.25;context.continuous_time_samples=9007199254740993LL;context.project_time_music=-.125;
    context.system_time=-9007199254740995LL;context.bar_position_music=-8.5;
    context.cycle_start_music=-4.25;context.cycle_end_music=16.75;
    context.time_signature_numerator=7;context.time_signature_denominator=8;
    context.chord_key=255;context.chord_root=254;context.chord_mask=0xa55;
    context.smpte_offset_subframes=INT32_MIN;context.frames_per_second=29;
    context.frame_rate_flags=RAVET_VST3_FRAME_PULL_DOWN|RAVET_VST3_FRAME_DROP;context.samples_to_next_clock=-123456789;
    uint32_t expectedParameterCount=2;

    f.factory->state.processing=[&](ProcessData& data){
        require(data.inputEvents&&data.outputEvents&&data.inputParameterChanges&&data.outputParameterChanges&&data.processContext);
        const auto& c=*data.processContext;require(c.projectTimeSamples==context.project_time_samples&&c.continousTimeSamples==context.continuous_time_samples&&
            c.tempo==123.25&&c.projectTimeMusic==-.125&&c.sampleRate==48000&&c.state==context.state&&c.systemTime==context.system_time);
        require(c.barPositionMusic==-8.5&&c.cycleStartMusic==-4.25&&c.cycleEndMusic==16.75&&c.timeSigNumerator==7&&c.timeSigDenominator==8);
        require(c.chord.keyNote==255&&c.chord.rootNote==254&&c.chord.chordMask==0xa55&&c.smpteOffsetSubframes==INT32_MIN&&
            c.frameRate.framesPerSecond==29&&c.frameRate.flags==context.frame_rate_flags&&c.samplesToNextClock==-123456789);
        require(data.numInputs==1&&data.numOutputs==1&&data.inputs[0].numChannels==1&&data.outputs[0].numChannels==1);
        if(data.numSamples)require(data.inputs[0].channelBuffers32[0][0]==.75f);
        require(data.inputParameterChanges->getParameterCount()==1);
        auto* inputQueue=data.inputParameterChanges->getParameterData(0);require(inputQueue&&inputQueue->getParameterId()==1&&inputQueue->getPointCount()==int32(expectedParameterCount));
        for(uint32_t point=0;point<expectedParameterCount;++point){int32 offset=-99;double value=-99;require(inputQueue->getPoint(int32(point),offset,value)==kResultOk&&offset==int32(parameters[point].offset)&&value==parameters[point].value);}

        require(data.inputEvents->getEventCount()==9);
        for(int32 i=0;i<9;++i){Event e{};require(data.inputEvents->getEvent(i,e)==kResultOk&&e.type==uint16_t(i)&&e.busIndex==1);
            e.busIndex=0;if(e.type==Event::kNoteOnEvent)e.noteOn.noteId=-777;
            require(data.outputEvents->addEvent(e)==kResultOk);
        }
        Event legacy{};legacy.type=Event::kLegacyMIDICCOutEvent;legacy.busIndex=0;legacy.sampleOffset=data.numSamples?data.numSamples-1:0;legacy.midiCCOut={255,1,127,126};
        require(data.outputEvents->addEvent(legacy)==kResultOk);
        int32 index=-1;auto* empty=data.outputParameterChanges->addParameterData(1,index);require(empty&&index==0);
        auto* q=data.outputParameterChanges->addParameterData(2,index);require(q&&index==1&&q->addPoint(data.numSamples?data.numSamples-1:0,.8125,index)==kResultOk);
    };
    std::thread worker([&]{
        allocations=deallocations=0;measured=measure;
        for(uint32_t call=0;call<iterations;++call){
            const auto frames=measure?call%17:(call?0:16);b.block.frames=frames;
            for(size_t i=0;i<source.size();++i)source[i].offset=frames?uint32_t(i)*frames/9:0;
            expectedParameterCount=frames>1?2:1;parameters[1].offset=frames?frames-1:0;
            context.project_time_samples=-9007199254740993LL-int64_t(call);
            require(b.call(f,parameters.data(),expectedParameterCount,source.data(),9,reinterpret_cast<const uint8_t*>(bytes.data()),12,&context)==RAVET_VST3_OK);
            require(b.outputs.event_count==10&&b.outputs.payload_used==30&&b.outputs.parameter_queue_count==2&&b.outputs.parameter_point_count==1);
            require(b.ids[0]==1&&b.ids[1]==2&&b.ids[2]==0xcccccccc&&b.points[0].id==2&&b.points[0].offset==(frames?frames-1:0)&&b.points[0].value==.8125);
            require(b.events[0].note_id==-777&&b.events[0].channel==1&&b.events[0].pitch==69&&b.events[0].velocity==.75f&&b.events[0].tuning==-3.25f&&b.events[0].note_length==100);
            require(b.events[1].note_id==-1&&b.events[1].pitch==70&&b.events[1].velocity==.125f&&b.events[1].tuning==9.5f);
            require(b.events[3].pitch==71&&b.events[3].velocity==.5f&&b.events[4].expression_id==100007&&b.events[4].value==.625);
            require(b.events[5].expression_id==100008&&b.events[6].root==69&&b.events[6].bass==57&&b.events[6].mask==65535&&b.events[7].root==60&&b.events[7].mask==0xa5a5);
            require(b.events[8].integer_value==UINT64_MAX&&b.events[8].expression_id==100009&&b.events[9].controller==255&&b.events[9].midi_value==127&&b.events[9].midi_value2==126);
            require(b.events[9].offset==(frames?frames-1:0));
            for(size_t i=0;i<9;++i){require(b.events[i].type==i&&b.events[i].offset==source[i].offset&&b.events[i].flags==0xc001&&b.events[i].bus==0&&b.events[i].ppq_position==source[i].ppq_position);
                if(source[i].payload_bytes)require(std::memcmp(b.payload.data()+b.events[i].payload_offset,reinterpret_cast<const uint8_t*>(bytes.data())+source[i].payload_offset,source[i].payload_bytes)==0);
            }
            require(b.payload[5]==0&&b.payload[12]==0&&b.payload[13]==0&&b.payload[20]==0&&b.payload[21]==0&&b.payload[28]==0&&b.payload[29]==0);
            require(b.events[10].struct_size==0&&b.payload[30]==0xa5&&b.output[16]==9&&b.report.native_result==kResultOk&&b.report.flags==51);
            if(frames)require(b.output[0]==float(parameters[expectedParameterCount-1].value));
        }
        measured=false;if(measure)require(allocations==0&&deallocations==0);
    });worker.join();require(f.factory->state.processes==int(iterations));
    if(measure)std::cout<<"PASS10000 provider rich calls:9 input/10 output kinds,all13 context presence bits,queues/UTF16/data echo,frames0..16,0 measured C++ operator new/delete (not malloc/foreign SDK heap/deadlines)\n";

}
void localAdmissionPreservesEveryOutput(){
    Fixture f;f.prepare();Block b;const auto report=b.report;const auto outputs=b.outputs;
    const auto audio=b.output;const auto records=b.events;const auto bytes=b.payload;
    const auto source=note();RavetVst3ParameterPoint p{2,0,.5};
    RavetVst3ContextRecord context{};context.struct_size=sizeof(context);context.sample_rate=44100;
    std::thread worker([&]{
        require(b.call(f,&p,1)==RAVET_VST3_INVALID);require(b.call(f,nullptr,0,&source,1,nullptr,0,&context)==RAVET_VST3_INVALID);
        b.outputs.parameter_ids=reinterpret_cast<uint32_t*>(b.input.data()+16);require(b.call(f)==RAVET_VST3_INVALID);b.outputs=outputs;
        b.outputs.payload=reinterpret_cast<uint8_t*>(const_cast<Record*>(&source));require(b.call(f,nullptr,0,&source,1)==RAVET_VST3_INVALID);b.outputs=outputs;
        b.outputs.payload=reinterpret_cast<uint8_t*>(f.child->owner.get());require(b.call(f)==RAVET_VST3_INVALID);b.outputs=outputs;
        b.outputs.event_capacity=2049;require(b.call(f)==RAVET_VST3_INVALID);b.outputs=outputs;
        require(std::memcmp(&report,&b.report,sizeof(report))==0&&std::memcmp(&outputs,&b.outputs,sizeof(outputs))==0&&
            b.output==audio&&std::memcmp(records.data(),b.events.data(),sizeof(records))==0&&b.payload==bytes&&f.factory->state.processes==0);
        b.in.samples=b.output.data();require(b.call(f)==RAVET_VST3_OK&&b.output[0]==.25f&&b.output[16]==9); // admitted audio alias via precopy
    });worker.join();
}
void lateFailureProvenanceSilenceAndNoPublication(){
    for(int mode=0;mode<4;++mode){Fixture f;f.prepare();Block b;const auto original=b.outputs;const auto records=b.events;const auto bytes=b.payload;
        RavetVst3ParameterPoint parameter{1,0,.75};
        if(mode==1)f.factory->state.fail="process";
        if(mode==2)f.factory->state.throwProcess=true;
        if(mode==3)f.factory->state.protocolFault=true;
        f.factory->state.processing=[&](ProcessData& data){Event e{};e.type=Event::kNoteOnEvent;e.busIndex=0;e.noteOn={1,69,0,.5f,0,-777};require(data.outputEvents->addEvent(e)==kResultOk);};
        if(mode==0)b.outputs.event_capacity=0;
        const auto expected=b.outputs;
        std::thread worker([&]{const auto result=b.call(f,&parameter,1);require(result==(mode?RAVET_VST3_NATIVE:RAVET_VST3_CAPACITY));
            require(f.factory->state.dspValue==.75&&f.factory->state.processes==1&&std::all_of(b.output.begin(),b.output.begin()+16,[](float v){return v==0;})&&b.output[16]==9);
            require(std::memcmp(&b.outputs,&expected,sizeof(expected))==0&&std::memcmp(b.events.data(),records.data(),sizeof(records))==0&&b.payload==bytes);
            require((b.report.flags&(RAVET_VST3_PROCESSING_ATTEMPTED|RAVET_VST3_PROCESSING_HISTORY_MAY_HAVE_ADVANCED|RAVET_VST3_PROCESSING_INVALIDATED))==25&&!(b.report.flags&RAVET_VST3_PROCESSING_PUBLISHED));
            require(bool(b.report.flags&RAVET_VST3_PROCESSING_THROWN)==(mode==2)&&bool(b.report.flags&RAVET_VST3_PROCESSING_RETURNED)!=(mode==2));
            require(b.report.native_result==(mode==1?rejected:kResultOk));
            require(b.report.failure==uint32_t(mode==0?RAVET_VST3_PROCESSING_FAILURE_PUBLICATION:mode==1?RAVET_VST3_PROCESSING_FAILURE_NATIVE:mode==2?RAVET_VST3_PROCESSING_FAILURE_THROWN:RAVET_VST3_PROCESSING_FAILURE_PROTOCOL));
            const auto previous=b.report;require(b.call(f)==RAVET_VST3_LIFECYCLE&&std::memcmp(&b.report,&previous,sizeof(previous))==0&&f.factory->state.processes==1);
        });worker.join();(void)original;
    }
}
void reentryAndTenThousandNoHeapBlocks(){
    Fixture f;f.prepare();Block b,nested;const auto nestedReport=nested.report;
    f.factory->state.duringProcess=[&]{require(nested.call(f)==RAVET_VST3_LIFECYCLE&&std::memcmp(&nested.report,&nestedReport,sizeof(nestedReport))==0&&nested.output[0]==9);};
    std::thread worker([&]{measured=true;for(uint32_t i=0;i<10000;++i){b.block.frames=i%17;require(b.call(f)==RAVET_VST3_OK&&b.report.flags==51&&b.outputs.event_count==0&&b.outputs.parameter_queue_count==0&&b.output[16]==9);}measured=false;
        require(allocations==0&&deallocations==0);
    });worker.join();require(f.factory->state.processes==10000);
}
void retainedSecondPoolCancelsFirstReservation(){
    for(int mode=0;mode<4;++mode){Fixture f;f.prepare();Block b;std::array<FUnknown*,16> retained{};size_t retainedCount=0;
        f.factory->state.processing=[&](ProcessData& data){
            FUnknown* object=mode==0?static_cast<FUnknown*>(data.inputEvents):mode==1?static_cast<FUnknown*>(data.outputEvents):
                mode==2?static_cast<FUnknown*>(data.outputParameterChanges):nullptr;
            if(mode==3){int32 index=-1;object=data.outputParameterChanges->addParameterData(2,index);require(object);}
            require(retainedCount<16&&object->addRef()==2);retained[retainedCount++]=object;
        };
        std::thread worker([&]{for(size_t i=0;i<16;++i)require(b.call(f)==RAVET_VST3_OK);
            const auto previous=b.report;const auto output=b.output;require(b.call(f)==RAVET_VST3_CAPACITY&&f.factory->state.processes==16&&b.output==output&&std::memcmp(&b.report,&previous,sizeof(previous))==0);
            require(f.module->core->parameterKeepers->externalRange(&b,sizeof(b),alignof(Block))); // keeper unchanged/usable
            for(auto* object:retained)require(object->release()==1);
            f.factory->state.processing={};require(b.call(f)==RAVET_VST3_OK&&f.factory->state.processes==17);
        });worker.join();
    }
}
}
void* operator new(std::size_t size){if(measured)++allocations;if(auto* p=std::malloc(size?size:1))return p;throw std::bad_alloc();}
void* operator new[](std::size_t size){return ::operator new(size);}
void operator delete(void* pointer)noexcept{if(pointer&&measured)++deallocations;std::free(pointer);}
void operator delete[](void* pointer)noexcept{::operator delete(pointer);}
int main(){try{
    metadataAndExactPair();std::cout<<"PASS actual exact-paired CONTROL copied bus/mapping/selection metadata and rejection preservation\n";
    actualPairedTenKindsAndContext();actualPairedTenKindsAndContext(10000,true);std::cout<<"PASS actual provider paired parameters/all9 input/all10 output kinds, exact context and variable/zero publication\n";
    localAdmissionPreservesEveryOutput();std::cout<<"PASS whole caller capacity/alias/ID/context preflight, unchanged output/report and valid audio alias continuation\n";
    lateFailureProvenanceSilenceAndNoPublication();std::cout<<"PASS original native return/throw/protocol/late capacity, conservative history flags, silence and atomic nonpublication\n";
    reentryAndTenThousandNoHeapBlocks();std::cout<<"PASS reentry preservation and10000 actual variable/zero provider blocks with0 measured C++ operator new/delete (not malloc/foreign heap/deadline proof)\n";
    retainedSecondPoolCancelsFirstReservation();std::cout<<"PASS all4 retained event/output-parameter interfaces, exact16 slots, paired cancellation and valid released continuation\n";
}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
