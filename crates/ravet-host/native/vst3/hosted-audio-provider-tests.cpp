#include "provider.cpp"
#include "RavetVst3State.h"
#include "hosted-audio-instance-fixture.hpp"
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
    const RavetVst3HostedAudioProviderV1* hosted=ravet_vst3_hosted_audio_provider_v1();
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
    void prepare(uint32_t bits){
        uint32_t count=999;RavetVst3ParameterReport parameterReport{};parameterReport.struct_size=sizeof(parameterReport);
        require(parameters->refresh(child,&count,&parameterReport)==RAVET_VST3_OK);
        RavetVst3ProcessingSelection selection{sizeof(selection),2,1,0};RavetVst3ProcessingInfo info{};info.struct_size=sizeof(info);
        RavetVst3ProcessingReport report{};report.struct_size=sizeof(report);
        require(processing->refresh(child,&selection,&info,&report)==RAVET_VST3_OK);
        require(info.input_bus_count==2&&info.output_bus_count==2&&info.selected_input_mask==2&&info.selected_output_mask==1);
        RavetVst3HostedAudioInfo audioInfo{};audioInfo.struct_size=sizeof(audioInfo);
        RavetVst3HostedAudioReport audioReport{};audioReport.struct_size=sizeof(audioReport);
        require(hosted->instance_origin==api&&hosted->processing_origin==processing&&hosted->version==1&&hosted->struct_size==sizeof(*hosted));
        require(hosted->refresh_audio(child,&audioInfo,&audioReport)==RAVET_VST3_OK);
        RavetVst3HostedAudioProfile profile{};profile.struct_size=sizeof(profile);profile.maximum_frames=16;profile.sample_rate=48000;profile.sample_bits=bits;
        profile.input_buses=audioInfo.input_buses;profile.output_buses=audioInfo.output_buses;
        for(uint32_t i=0;i<profile.input_buses;++i)profile.inputs[i]={factory->state.audioInputs[i],uint32_t(i!=1),0};
        for(uint32_t i=0;i<profile.output_buses;++i)profile.outputs[i]={factory->state.audioOutputs[i],uint32_t(i!=1),0};
        require(hosted->prepare(child,&profile,&audioReport)==RAVET_VST3_OK&&audioReport.failure==0&&audioReport.native_result==0);
        RavetVst3HostedAudioTiming timing{};timing.struct_size=sizeof(timing);
        require(hosted->timing(child,&timing,&audioReport)==RAVET_VST3_OK&&timing.latency_samples==19&&timing.tail_kind==1&&timing.tail_samples==37);
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
template<class Sample,class NativeBlock,class Input,class Output> struct Block {
    std::array<std::array<Sample,20>,64> input{},output{};
    std::array<Input,64> in{};std::array<Output,64> out{};
    NativeBlock block{sizeof(block),16,4,2,in.data(),out.data()};
    std::array<Record,12> events{};std::array<uint8_t,96> payload{};
    std::array<uint32_t,4> ids{};std::array<RavetVst3ParameterPoint,8> points{};
    RavetVst3ProcessingOutputs outputs{sizeof(outputs),0,events.data(),uint32_t(events.size()),777,
        payload.data(),uint32_t(payload.size()),778,ids.data(),uint32_t(ids.size()),779,
        points.data(),uint32_t(points.size()),780};
    RavetVst3ProcessingReport report{};
    explicit Block(bool maximum=false){if(maximum)block.input_channels=block.output_channels=64;
        for(size_t i=0;i<64;++i){in[i]={input[i].data(),20,0};out[i]={output[i].data(),20,0};
            input[i].fill(Sample(double(i)*.0625+.75+1e-14));output[i].fill(Sample(9));}
        payload.fill(0xa5);ids.fill(0xcccccccc);report.struct_size=sizeof(report);report.index=888;
    }
    int32_t call(Fixture& f,const RavetVst3ParameterPoint* parameters=nullptr,uint32_t parameterCount=0,
            const Record* inputEvents=nullptr,uint32_t eventCount=0,const uint8_t* bytes=nullptr,uint32_t byteCount=0,
            const RavetVst3ContextRecord* context=nullptr){
        if constexpr(std::is_same_v<Sample,float>)return f.hosted->process32(f.audio,&block,parameters,parameterCount,inputEvents,eventCount,bytes,byteCount,context,&outputs,&report);
        else return f.hosted->process64(f.audio,&block,parameters,parameterCount,inputEvents,eventCount,bytes,byteCount,context,&outputs,&report);
    }
};
Record note(uint32_t offset=0){Record r{};r.struct_size=sizeof(r);r.type=Event::kNoteOnEvent;r.bus=1;r.offset=offset;
    r.channel=1;r.pitch=69;r.note_id=401;r.velocity=.75f;r.tuning=-3.25f;r.note_length=100;r.flags=0xc001;r.ppq_position=-.125;return r;}
template<class Sample,class NativeBlock,class Input,class Output>
void actualPairedTenKindsAndContext(bool maximum=false){
    constexpr uint32_t iterations=10000;constexpr bool measure=true;
    Fixture f;if(maximum){f.factory->state.audioInputCount=f.factory->state.audioOutputCount=1;f.factory->state.audioInputs[0]=f.factory->state.audioOutputs[0]=UINT64_MAX;}
    f.prepare(uint32_t(sizeof(Sample)*8));Block<Sample,NativeBlock,Input,Output> b(maximum);
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
        require(data.numInputs==(maximum?1:3)&&data.numOutputs==(maximum?1:2)&&data.inputs[0].numChannels==(maximum?64:2)&&data.outputs[0].numChannels==(maximum?64:2));
        if(data.numSamples){if constexpr(std::is_same_v<Sample,float>)require(data.inputs[0].channelBuffers32[0][0]==b.input[0][0]);else require(data.inputs[0].channelBuffers64[0][0]==b.input[0][0]);}
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
            require(b.events[10].struct_size==0&&b.payload[30]==0xa5&&b.output[0][16]==9&&b.report.native_result==kResultOk&&b.report.flags==51);
            if(frames)for(uint32_t ch=0;ch<b.block.output_channels;++ch) {
                const double expected=double(b.input[ch][0])*parameters[expectedParameterCount-1].value+(maximum?0:double(b.input[ch+2][0])*.25);
                const double tolerance=8*double(std::numeric_limits<Sample>::epsilon())*std::max(1.,std::abs(expected));
                require(std::abs(double(b.output[ch][0])-expected)<=tolerance&&b.output[ch][16]==Sample(9));
                if constexpr(std::is_same_v<Sample,double>)if(ch==0)require(b.output[ch][0]!=double(float(expected)));
            }
        }
        measured=false;if(measure)require(allocations==0&&deallocations==0);
    });worker.join();require(f.factory->state.processes==int(iterations));
    if(measure)std::cout<<"PASS10000 hosted"<<sizeof(Sample)*8<<"bit"<<(maximum?"64channels":"main/aux/inactive")<<" provider rich calls:9 input/10 output kinds,all13 context presence bits,queues/UTF16/data echo,frames0..16,0 measured C++ operator new/delete (not malloc/foreign SDK heap/deadlines)\n";

}

}
void* operator new(std::size_t size){if(measured)++allocations;if(auto* p=std::malloc(size?size:1))return p;throw std::bad_alloc();}
void* operator new[](std::size_t size){return ::operator new(size);}
void operator delete(void* pointer)noexcept{if(pointer&&measured)++deallocations;std::free(pointer);}
void operator delete[](void* pointer)noexcept{::operator delete(pointer);}
int main(){try{
    actualPairedTenKindsAndContext<float,RavetVst3AudioBlock,RavetVst3InputChannel,RavetVst3OutputChannel>();
    actualPairedTenKindsAndContext<double,RavetVst3HostedAudioBlock64,RavetVst3HostedInputChannel64,RavetVst3HostedOutputChannel64>();
    actualPairedTenKindsAndContext<double,RavetVst3HostedAudioBlock64,RavetVst3HostedInputChannel64,RavetVst3HostedOutputChannel64>(true);
    std::cout<<"3 actual hosted provider rich workload groups passed\n";
}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
