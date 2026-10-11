#include "processing-instance-fixture.hpp"
using namespace ravet::vst3::processing_draft;
namespace {
struct Core {};
Observation observed(bool controller){Observation state;state.hasController=controller;return state;}
struct Owned {
    Observation state;Factory factory{state};std::shared_ptr<Core> core=std::make_shared<Core>();
    ProcessingPoolKeepers<Core> processes{*core};
    ravet::vst3::parameters_draft::ParameterPoolKeepers parameters;
    ControllerHandlerKeepers handlers;
    std::unique_ptr<Instance> owner=Instance::create(factory,*Steinberg::owned(new HostApplication),metadata(),0,core,handlers);
    explicit Owned(bool controller=true):state(observed(controller)){}
    void refresh(){owner->refreshParameters();owner->refreshProcessing(2,1);}
    void prepare(){owner->prepareProcessing({48000,8,1,1},parameters,processes,*core);}
    ~Owned(){
        if(owner->attached())owner->endAudio();
        (void)owner->retire();owner->detachParameters(parameters);owner->detachProcessing(processes,*core);
        parameters.reclaim();processes.reclaim(*core,[](Core&){});
    }
};
void completeBusMetadataProtection(){
    Owned f;f.state.eventInputCount=16;f.state.eventOutputCount=16;f.state.hasMapping=false;f.state.longEventNames=true;f.refresh();
    const auto& catalogue=f.owner->processingCatalogue();
    std::array<ravet::vst3::parameters_draft::Range,33> ranges{};
    require(catalogue.protectionRanges(ranges)==33);
    for(bool output:{false,true})for(uint32_t i=0;i<16;++i){const auto& name=catalogue.bus(output,i).name;
        require(name.size()==381&&!catalogue.externalRange(name.data(),name.capacity()+1,1)&&
            !f.owner->stateExternal(name.data(),name.capacity()+1,1)&&!f.owner->stateExternal(name.data()+name.size(),1,1));
        ravet::vst3::parameters_draft::Range value{};
        require(ravet::vst3::parameters_draft::range(name.data(),name.capacity()+1,1,1,value));
        require(std::any_of(ranges.begin(),ranges.end(),[&](const auto& r){return r.start==value.start&&r.end==value.end;}));
    }
    int external=0;require(f.owner->stateExternal(&external,sizeof(external),alignof(int)));
    f.prepare();require(f.owner->preparedProcessingPool());
}
void controllerlessEventContextProcessing(){
    Owned f(false);f.refresh();require(f.owner->parameterCatalogue().parameters().empty()&&f.owner->parametersCompatible());
    const auto& catalogue=f.owner->processingCatalogue();require(!catalogue.hasMidiMapping()&&catalogue.hasContextRequirements()&&f.state.mappingCalls==0);
    bool refused=false;try{(void)f.owner->parameterValue(1,RAVET_VST3_PARAMETER_READ,0);}catch(const InstanceFailure& e){refused=e.stage==Stage::Lifecycle;}
    require(refused);f.prepare();auto* pool=f.owner->preparedProcessingPool();require(pool);
    const auto inputs=catalogue.selectedBuses(false),outputs=catalogue.selectedBuses(true);
    Record source{};source.struct_size=sizeof(source);source.type=Event::kNoteOnEvent;source.bus=1;source.channel=1;source.pitch=69;source.velocity=.5f;
    f.state.processing=[&](ProcessData& data){require(data.inputEvents&&data.inputEvents->getEventCount()==1&&data.outputEvents&&data.outputParameterChanges&&data.outputParameterChanges->getParameterCount()==0);
        require(!data.inputParameterChanges||data.inputParameterChanges->getParameterCount()==0);};
    f.owner->start();f.owner->beginAudio();bool pass=false;
    std::thread worker([&]{for(uint32_t frames:{8u,0u,1u}){EventBatch* batch=nullptr;
        require(pool->prepare(frames,&source,1,nullptr,0,inputs,outputs,batch)==EventAdmission::Ok);
        bool threw=false;require(f.owner->process(frames,nullptr,&threw,batch)==kResultOk&&!threw&&batch->freeze()==0);
    }pass=true;});worker.join();require(pass&&f.state.processes==3);
    f.owner->endAudio();require(f.owner->stop().native==kResultOk&&f.state.count("handler")==0&&f.state.count("handler-clear")==0);
}
void copiedRefreshAndExplicitEventActivation(){
    Owned f;f.state.requirements=IProcessContextRequirements::kNeedTempo;
    f.refresh();const auto& catalogue=f.owner->processingCatalogue();
    require(f.state.mappingCalls==260&&catalogue.hasMidiMapping()&&catalogue.hasContextRequirements());
    require(catalogue.assignment(1,1,7)==1&&catalogue.assignment(1,1,8)==kNoParamId);
    require(catalogue.contextRequirements()==uint32(IProcessContextRequirements::kNeedTempo));
    require(f.state.eventActivations.empty());f.prepare();
    require(f.owner->preparedProcessingPool()&&f.processes.count(*f.core)==1);
    require(f.state.eventActivations==std::vector<std::array<int32,3>>({{kInput,1,1},{kOutput,0,1}}));
    f.owner->start();f.owner->beginAudio();f.owner->endAudio();require(f.owner->stop().native==kResultOk);
    f.prepare();
    require(f.state.eventActivations==std::vector<std::array<int32,3>>({{kInput,1,1},{kOutput,0,1},{kOutput,0,0},{kInput,1,0},{kInput,1,1},{kOutput,0,1}}));
    f.owner->refreshParameters();require(!f.owner->processingCompatible()&&!f.owner->preparedProcessingPool());
    f.owner->refreshProcessing(2,1);require(f.owner->processingCompatible());
}
void staleTopologyRefusesBeforeAllocationOrSetup(){
    Owned f;f.refresh();const auto calls=f.state.calls.size();
    f.state.eventInputCount=1;
    bool refused=false;try{f.prepare();}catch(const InstanceFailure& e){refused=e.stage==Stage::Buses&&!e.returned;}
    require(refused&&f.state.calls.size()==calls&&f.state.eventActivations.empty()&&f.processes.count(*f.core)==0);
    require(!f.owner->preparedProcessingPool());
    f.state.eventInputCount=2;f.prepare();require(f.owner->preparedProcessingPool());
}
void staleSuccessfulReprepareRetiresReadinessBeforeForeignInspection(){
    Owned f;f.refresh();f.prepare();auto* previous=f.owner->preparedProcessingPool();require(previous);
    const auto calls=f.state.calls.size();const auto activations=f.state.eventActivations;
    bool localRefused=false;
    try{f.owner->prepareProcessing({0,8,1,1},f.parameters,f.processes,*f.core);}
    catch(const InstanceFailure& e){localRefused=e.stage==Stage::Lifecycle;}
    require(localRefused&&f.owner->preparedProcessingPool()==previous&&f.state.calls.size()==calls&&f.state.eventActivations==activations);
    f.state.eventInputCount=1;bool refused=false;
    try{f.prepare();}catch(const InstanceFailure& e){refused=e.stage==Stage::Buses&&!e.returned;}
    require(refused&&!f.owner->preparedProcessingPool()&&f.processes.count(*f.core)==1&&f.state.calls.size()==calls&&f.state.eventActivations==activations);
    bool startRefused=false;try{f.owner->start();}catch(const InstanceFailure& e){startRefused=e.stage==Stage::Lifecycle;}
    require(startRefused&&f.state.calls.size()==calls&&f.state.eventActivations==activations);
    f.state.eventInputCount=2;f.prepare();require(f.owner->preparedProcessingPool()==previous);
    require(f.state.eventActivations==std::vector<std::array<int32,3>>({{kInput,1,1},{kOutput,0,1},{kOutput,0,0},{kInput,1,0},{kInput,1,1},{kOutput,0,1}}));
}
void activationFailureReverseOnceAndFirstProvenance(){
    for(bool throws:{false,true}) {
        Owned f;f.refresh();f.state.failedActivation=0;f.state.throwingActivation=throws;
        bool refused=false;try{f.prepare();}catch(const InstanceFailure& e){
            refused=true;require(e.stage==Stage::ActivateBus&&e.native==(throws?0:rejected)&&e.returned==!throws);
        }catch(const std::runtime_error&){refused=throws;}
        require(refused&&!f.owner->preparedProcessingPool());
        const auto expected=std::vector<std::array<int32,3>>({{kInput,1,1},{kOutput,0,1},{kOutput,0,0},{kInput,1,0}});
        require(f.state.eventActivations==expected);
        require(f.owner->retire().native==kResultOk&&f.state.eventActivations==expected);
        require(f.owner->retire().native==kResultOk&&f.state.eventActivations==expected);
    }
}
void actualProcessDataInputOutputParametersEventsAndContext(){
    Owned f;f.refresh();f.prepare();auto* pool=f.owner->preparedProcessingPool();require(pool);
    const auto inputs=f.owner->processingCatalogue().selectedBuses(false);
    const auto outputs=f.owner->processingCatalogue().selectedBuses(true);
    Record source{};source.struct_size=sizeof(source);source.type=Event::kNoteOnEvent;
    source.bus=1;source.offset=3;source.channel=1;source.pitch=69;source.note_id=777;source.velocity=.75f;
    Context wire{};wire.struct_size=sizeof(wire);wire.sample_rate=48000;wire.project_time_samples=-19;
    wire.state=RAVET_VST3_CONTEXT_PLAYING|RAVET_VST3_CONTEXT_TEMPO;wire.tempo=123;
    ProcessContext context{};require(sdkContext(wire,48000,context));
    f.state.processing=[&](ProcessData& data){
        require(data.processContext&&data.processContext->sampleRate==48000&&data.processContext->projectTimeSamples==-19);
        require(data.processContext->tempo==123&&data.processContext->state==wire.state);
        require(data.inputEvents&&data.outputEvents&&data.outputParameterChanges);
        require(data.inputEvents->getEventCount()==1);Event note{};
        require(data.inputEvents->getEvent(0,note)==kResultOk&&note.busIndex==1&&note.sampleOffset==int32(source.offset)&&note.noteOn.noteId==777);
        note.busIndex=0;note.type=Event::kNoteOffEvent;note.noteOff={1,69,.5f,-777,0};
        require(data.outputEvents->addEvent(note)==kResultOk);
        int32 index=-1;auto* queue=data.outputParameterChanges->addParameterData(2,index);
        require(queue&&queue->addPoint(data.numSamples?data.numSamples-1:0,.625,index)==kResultOk);
    };
    f.owner->start();f.owner->beginAudio();
    bool good=false;std::thread worker([&]{
        EventBatch* batch=nullptr;require(pool->prepare(8,&source,1,nullptr,0,inputs,outputs,batch)==EventAdmission::Ok);
        bool threw=false;require(f.owner->process(8,nullptr,&threw,batch,&context)==kResultOk&&!threw&&batch->freeze()==0);
        Record emitted[2]{};uint32_t ids[2]{};RavetVst3ParameterPoint points[2]{};
        RavetVst3ProcessingOutputs copied{};copied.struct_size=sizeof(copied);copied.events=emitted;copied.event_capacity=2;
        copied.parameter_ids=ids;copied.parameter_queue_capacity=2;copied.parameter_points=points;copied.parameter_point_capacity=2;
        require(batch->copyOutputCombined(copied)&&copied.event_count==1&&copied.parameter_queue_count==1&&copied.parameter_point_count==1);
        require(emitted[0].bus==0&&emitted[0].offset==3&&emitted[0].note_id==-777&&ids[0]==2&&points[0].offset==7&&points[0].value==.625);
        require(emitted[1].struct_size==0&&ids[1]==0&&points[1].id==0);
        source.offset=0;require(pool->prepare(0,&source,1,nullptr,0,inputs,outputs,batch)==EventAdmission::Ok);
        require(f.owner->process(0,nullptr,&threw,batch,&context)==kResultOk&&!threw&&batch->freeze()==0);
        require(batch->copyOutputCombined(copied)&&emitted[0].offset==0&&points[0].offset==0);
        good=true;
    });worker.join();require(good&&f.state.processes==2);
    f.owner->endAudio();require(f.owner->stop().native==kResultOk);
    require(f.owner->output(0)[0]==.25f);
}
}
int main(){try{
    completeBusMetadataProtection();std::cout<<"PASS all32 maximum UTF8 bus names, capacities/terminators and immutable numeric ownership\n";
    controllerlessEventContextProcessing();std::cout<<"PASS real controllerless SDK component empty parameter reconciliation and variable/zero event callbacks\n";
    copiedRefreshAndExplicitEventActivation();std::cout<<"PASS copied optional mapping/context and explicit selected event-bus preparation/reprepare\n";
    staleTopologyRefusesBeforeAllocationOrSetup();std::cout<<"PASS stale copied topology refusal before pool/setup and corrected valid continuation\n";
    staleSuccessfulReprepareRetiresReadinessBeforeForeignInspection();std::cout<<"PASS stale successful reprepare invalidates readiness, local rejection preserves it and valid retry retires duties once\n";
    activationFailureReverseOnceAndFirstProvenance();std::cout<<"PASS reverse attempted event-bus duties on return/throw, original first failure, no repeated retirement\n";
    actualProcessDataInputOutputParametersEventsAndContext();std::cout<<"PASS actual SDK ProcessData event/input/output-parameter/context fields and complete counted output/zero frames\n";
}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
