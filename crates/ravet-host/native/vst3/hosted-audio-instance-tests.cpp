#include "hosted-audio-instance-fixture.hpp"
using namespace ravet::vst3::processing_draft;
namespace {
struct Core{ravet::vst3::hosted_audio::ScratchBudget budget;};
struct Owned {
    Observation state;Factory factory{state};std::shared_ptr<Core> core=std::make_shared<Core>();
    ProcessingPoolKeepers<Core> processes{*core};ravet::vst3::parameters_draft::ParameterPoolKeepers parameters;ControllerHandlerKeepers handlers;
    std::unique_ptr<Instance> owner;
    Owned(){owner=Instance::allocate(metadata(),0,core,handlers,&core->budget);owner->initialize(factory,*Steinberg::owned(new HostApplication),metadata(),0);}
    void refresh(){owner->refreshParameters();owner->refreshProcessing(2,1);owner->refreshHostedAudio();}
    RavetVst3HostedAudioProfile profile(uint32_t bits) {
        RavetVst3HostedAudioProfile p{};p.struct_size=sizeof(p);p.sample_rate=48000;p.maximum_frames=8;p.sample_bits=bits;
        p.input_buses=uint32_t(state.audioInputCount);p.output_buses=uint32_t(state.audioOutputCount);
        for(uint32_t i=0;i<p.input_buses;++i)p.inputs[i]={state.audioInputs[i],uint32_t(i!=1),0};
        for(uint32_t i=0;i<p.output_buses;++i)p.outputs[i]={state.audioOutputs[i],uint32_t(i!=1),0};return p;
    }
    void prepare(const RavetVst3HostedAudioProfile& p){owner->prepareHostedAudio(p,parameters,processes,*core);}
    ~Owned(){if(owner->attached())owner->endAudio();owner->retire();owner->detachParameters(parameters);owner->detachProcessing(processes,*core);owner.reset();parameters.reclaim();processes.reclaim(*core,[](Core&){});handlers.reclaim();require(core->budget.bytes()==0);}
};
template<class S,class Block,class Input,class Output,class Boundary> void mixedMainAuxInactiveAndRichOutput(bool maximum=false) {
    Owned f;
    if(maximum){f.state.audioInputCount=f.state.audioOutputCount=1;f.state.audioInputs[0]=f.state.audioOutputs[0]=UINT64_MAX;}
    f.refresh();const auto p=f.profile(uint32_t(sizeof(S)*8));f.prepare(p);
    require(f.owner->hostedPrepared()&&f.owner->hostedTiming().latency_samples==19&&f.owner->hostedTiming().tail_kind==1&&f.owner->hostedTiming().tail_samples==37);
    const auto layout=f.owner->hostedLayout();require(layout.inputChannels==(maximum?64:4)&&layout.outputChannels==(maximum?64:2));
    std::array<std::array<S,10>,64> in{},out{};std::array<Input,64> inputs{};std::array<Output,64> outputs{};
    for(uint32_t ch=0;ch<layout.inputChannels;++ch){inputs[ch]={in[ch].data(),10,0};for(uint32_t frame=0;frame<10;++frame)in[ch][frame]=S(double(ch)+double(frame)*.125+1.00000000000001);}
    for(uint32_t ch=0;ch<layout.outputChannels;++ch){outputs[ch]={out[ch].data(),10,0};out[ch].fill(S(91));}
    Block block{sizeof(block),8,layout.inputChannels,layout.outputChannels,inputs.data(),outputs.data()};
    RavetVst3ParameterPoint point{1,0,.625};Record source{};source.struct_size=sizeof(source);source.type=Event::kNoteOnEvent;source.bus=1;source.channel=1;source.pitch=69;source.note_id=123;source.velocity=.75;
    RavetVst3ContextRecord context{};context.struct_size=sizeof(context);context.sample_rate=48000;context.state=RAVET_VST3_CONTEXT_TEMPO|RAVET_VST3_CONTEXT_MUSIC;context.tempo=123;context.project_time_music=8.25;
    std::array<Record,2> events{};std::array<uint8_t,16> payload{};std::array<uint32_t,2> ids{};std::array<RavetVst3ParameterPoint,2> points{};
    RavetVst3ProcessingOutputs output{};output.struct_size=sizeof(output);output.events=events.data();output.event_capacity=2;output.payload=payload.data();output.payload_capacity=16;
    output.parameter_ids=ids.data();output.parameter_queue_capacity=2;output.parameter_points=points.data();output.parameter_point_capacity=2;
    RavetVst3ProcessingReport report{};report.struct_size=sizeof(report);
    f.state.processing=[&](ProcessData& data){require(data.processContext&&data.processContext->tempo==123&&data.processContext->projectTimeMusic==8.25);
        Event event{};require(data.inputEvents->getEventCount()==1&&data.inputEvents->getEvent(0,event)==kResultOk&&event.noteOn.noteId==123);
        event.busIndex=0;require(data.outputEvents->addEvent(event)==kResultOk);int32 index=-1;auto* queue=data.outputParameterChanges->addParameterData(2,index);
        require(queue&&queue->addPoint(0,.25,index)==kResultOk);
    };
    f.owner->start();f.owner->beginAudio();
    std::thread worker([&]{
        for(uint32_t frames:{8u,0u,1u,7u}) {
            block.frames=frames;const auto rc=f.owner->processHostedBlock<S,Block,Boundary>(block,&point,1,&source,1,nullptr,0,&context,output,report);
            require(rc==RAVET_VST3_OK&&report.flags==51&&output.event_count==1&&events[0].note_id==123&&events[0].bus==0&&output.parameter_queue_count==1&&ids[0]==2&&points[0].value==.25);
            for(uint32_t ch=0;ch<layout.outputChannels;++ch){for(uint32_t frame=0;frame<frames;++frame){const double expected=double(in[ch][frame])*.625+(maximum?0:double(in[ch+2][frame])*.25);const double tolerance=8*double(std::numeric_limits<S>::epsilon())*std::max(1.,std::abs(expected));require(std::abs(double(out[ch][frame])-expected)<=tolerance);
                if constexpr(std::is_same_v<S,double>)if(ch==0&&frame==0)require(out[ch][frame]!=double(float(expected)));}require(out[ch][8]==S(91)&&out[ch][9]==S(91));}
        }
        const auto previous=report;block.frames=8;outputs[layout.outputChannels-1].reserved=1;const auto calls=f.state.processes;
        require(f.owner->processHostedBlock<S,Block,Boundary>(block,&point,1,&source,1,nullptr,0,&context,output,report)==RAVET_VST3_INVALID&&f.state.processes==calls&&std::memcmp(&report,&previous,sizeof(report))==0);outputs[layout.outputChannels-1].reserved=0;
    });worker.join();require(f.state.processes==4);f.owner->endAudio();require(f.owner->stop().native==kResultOk);
    if(!maximum){require(f.state.activeInputs[0]&&!f.state.activeInputs[1]&&f.state.activeInputs[2]&&f.state.activeOutputs[0]&&!f.state.activeOutputs[1]);}
}
void metadataAndGetterProvenance() {
    {Owned f;f.refresh();const auto saved=f.owner->hostedAudioCatalogue();auto invalid=f.profile(64);invalid.inputs[15].active=1;const auto calls=f.state.calls.size();
        bool local=false;try{f.prepare(invalid);}catch(const InstanceFailure& e){local=e.stage==Stage::Profile&&e.origin==FailureOrigin::Local;}
        require(local&&f.core->budget.bytes()==0&&f.state.calls.size()==calls&&std::memcmp(&saved,&f.owner->hostedAudioCatalogue(),sizeof(saved))==0);
        f.state.fail="precision64";bool returned=false;try{f.prepare(f.profile(64));}catch(const InstanceFailure& e){returned=e.stage==Stage::Precision&&e.returned&&e.native==4242;}
        require(returned&&f.core->budget.bytes()==0);
    }
    for(const char* phase:{"latency","tail"}) {Owned f;f.refresh();f.state.throwCall=phase;bool thrown=false;
        try{f.prepare(f.profile(64));}catch(const InstanceFailure& e){thrown=e.stage==(std::strcmp(phase,"latency")==0?Stage::Latency:Stage::Tail)&&e.origin==FailureOrigin::Thrown&&!e.returned&&e.native==0;}
        require(thrown&&f.core->budget.bytes()==0&&f.state.audioActivations.empty());f.state.throwCall.clear();
    }
    for(uint32_t tail:{0u,UINT32_MAX}){Owned f;f.refresh();f.state.tail=tail;f.prepare(f.profile(32));require(f.owner->hostedTiming().tail_kind==(tail?2u:0u)&&f.owner->hostedTiming().tail_samples==0);}
}
}
int main(){try{
    mixedMainAuxInactiveAndRichOutput<float,RavetVst3AudioBlock,RavetVst3InputChannel,RavetVst3OutputChannel,HostedBoundary32>();
    mixedMainAuxInactiveAndRichOutput<double,RavetVst3HostedAudioBlock64,RavetVst3HostedInputChannel64,RavetVst3HostedOutputChannel64,HostedBoundary64>();
    mixedMainAuxInactiveAndRichOutput<double,RavetVst3HostedAudioBlock64,RavetVst3HostedInputChannel64,RavetVst3HostedOutputChannel64,HostedBoundary64>(true);
    metadataAndGetterProvenance();std::cout<<"4 native hosted instance groups passed:32/64 main+aux inactive indices,64channels,rich output and getter provenance\n";
}catch(const InstanceFailure& e){std::cerr<<"InstanceFailure stage="<<int(e.stage)<<" native="<<e.native<<" origin="<<int(e.origin)<<"\n";return 1;}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
