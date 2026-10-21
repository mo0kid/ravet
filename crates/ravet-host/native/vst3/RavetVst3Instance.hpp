#ifndef RAVET_VST3_INSTANCE_DRAFT_HPP
#define RAVET_VST3_INSTANCE_DRAFT_HPP
#include "RavetVst3Catalogue.hpp"
#include "RavetVst3Provider.h"
#include "RavetVst3State.hpp"
#include "RavetVst3Parameters.h"
#include "ParameterCatalogue.hpp"
#include "ParameterPoolKeepers.hpp"
#include "ProcessingCatalogue.hpp"
#include "ProcessingPoolKeepers.hpp"
#include "ProcessingBoundary.hpp"
#include "ControllerHandlerKeepers.hpp"
#include "HostedAudioScratch.hpp"
#include "HostedAudioCatalogue.hpp"
#include <limits>
#include "pluginterfaces/vst/ivstaudioprocessor.h"
#include "pluginterfaces/vst/ivsteditcontroller.h"
#include "pluginterfaces/vst/ivstmessage.h"
#include "pluginterfaces/vst/vstspeaker.h"
#include <pthread.h>
#include <cmath>
#include <atomic>
#include <thread>
#include <vector>

namespace ravet::vst3::draft {
enum class Stage { Create, Query, ControllerId, Initialize, Connect, Buses,
    Arrangement, Precision, Setup, ActivateBus, Activate, Start, Process,
    Stop, Deactivate, Handler, Disconnect, Terminate, Thread, Profile, Lifecycle, Latency, Tail };
enum class FailureOrigin { Local, Returned, Thrown };
struct InstanceFailure {
    Stage stage; Steinberg::tresult native; bool returned=false;
    FailureOrigin origin=FailureOrigin::Local;
};
template<class Call> auto sdkCall(Stage stage,Call call)->decltype(call()) {
    try{return call();}catch(...){throw InstanceFailure{stage,0,false,FailureOrigin::Thrown};}
}
template<class Call> void sdkAccepted(Stage stage,Call call) {
    const auto status=sdkCall(stage,call);
    if(status!=Steinberg::kResultOk)throw InstanceFailure{stage,status,true,FailureOrigin::Returned};
}
inline void accepted(Steinberg::tresult result, Stage stage,bool returned=true) {
    if (result != Steinberg::kResultOk) throw InstanceFailure{stage,result,returned,returned?FailureOrigin::Returned:FailureOrigin::Local};
}
template<class Interface> Steinberg::IPtr<Interface> queried(Steinberg::FUnknown& from, bool optional=false) {
    Interface* pointer=nullptr;
    Steinberg::tresult status;bool returned=false;
    try { status=from.queryInterface(Interface::iid,reinterpret_cast<void**>(&pointer));returned=true; }
    catch (...) {auto owner=Steinberg::owned(pointer);throw InstanceFailure{Stage::Query,0,false,FailureOrigin::Thrown};}
    auto owner=Steinberg::owned(pointer); // valid unexpected pointers are released on failure
    // Absent: kNoInterface per the SDK; JUCE's wrappers answer an interface
    // they do not implement with kResultFalse and no pointer.
    if (optional && !pointer && (status==Steinberg::kNoInterface||status==Steinberg::kResultFalse)) return nullptr;
    accepted(status,Stage::Query,returned);
    if (!pointer) throw InstanceFailure{Stage::Query,Steinberg::kInternalError};
    return owner;
}
template<class Interface,class Call> Steinberg::IPtr<Interface> created(Call call) {
    Interface* pointer=nullptr;
    Steinberg::tresult status;bool returned=false;
    try { status=call(reinterpret_cast<void**>(&pointer));returned=true; }
    catch (...) {auto owner=Steinberg::owned(pointer);throw InstanceFailure{Stage::Create,0,false,FailureOrigin::Thrown};}
    auto owner=Steinberg::owned(pointer);
    accepted(status,Stage::Create,returned);
    if (!pointer) throw InstanceFailure{Stage::Create,Steinberg::kInternalError};
    return owner;
}
struct AudioProfile { double rate=48000; uint32_t maximumFrames=1024, inputs=2, outputs=2; };
// Internal CONTROL-owned instance. The provider reserves/pins the native core
// BEFORE create for non-discardable factories and retains state-stream owners;
// this helper does not load/scan or publish raw factories.
class Instance {
    using Result=Steinberg::tresult;
    std::shared_ptr<void> core_; // host and image survive all interface release
    std::array<uint8_t,16> classId_{};
    ControllerHandlerKeepers* const handlers_;
    ControllerHandler* const handler_;
    std::thread::id main_=std::this_thread::get_id(), audio_{};
    Steinberg::IPtr<Steinberg::Vst::IComponent> component_;
    Steinberg::IPtr<Steinberg::Vst::IAudioProcessor> processor_;
    Steinberg::IPtr<Steinberg::Vst::IEditController> controller_;
    Steinberg::IPtr<Steinberg::Vst::IConnectionPoint> componentPoint_,controllerPoint_;
    bool componentInitialized_=false, controllerInitialized_=false;
    bool handlerAttempted_=false;
    bool toController_=false,toComponent_=false,inputActive_=false,outputActive_=false;
    bool active_=false,running_=false,attached_=false,ready_=false,fault_=false;
    AudioProfile profile_;
    hosted_audio::CopiedCatalogue hostedCatalogue_;
    hosted_audio::PackedLayout hostedLayout_;
    RavetVst3HostedAudioProfile hostedProfile_{};
    RavetVst3HostedAudioTiming hostedTiming_{};
    bool hostedCatalogueCompatible_=false,hostedMode_=false;
    uint16_t hostedInputDuties_=0,hostedOutputDuties_=0;
    // Core lifetime is retained by core_ until after both MAIN-owned typed
    // scratch owners release their exact requested extents. The local budget
    // keeps standalone internal fixtures usable without inventing a core.
    hosted_audio::ScratchBudget localScratchBudget_;
    hosted_audio::ScratchBudget* const scratchBudget_;
    hosted_audio::Scratch currentScratch_,pendingScratch_;
    std::array<float*,2> inputPointers_{},outputPointers_{};
    parameters_draft::ParameterCatalogue parameterCatalogue_;
    parameters_draft::Pool* parameterPool_=nullptr;
    bool parametersCompatible_=false;
    processing_draft::ProcessingCatalogue processingCatalogue_;
    processing_draft::EventPool* processingPool_=nullptr;
    bool processingCompatible_=false,processingReady_=false;
    uint16_t eventInputActive_=0,eventOutputActive_=0;
    // One deactivation duty per attempted event activation. The SDK may have
    // changed state before a refusal/throw; retire each attempt once in reverse.
    void verifyProcessingTopology() {
        using namespace Steinberg::Vst;
        for(auto direction:{kInput,kOutput}) {
            int32_t count=0;
            try{count=component_->getBusCount(kEvent,direction);}
            catch(...){throw InstanceFailure{Stage::Buses,0,false,FailureOrigin::Thrown};}
            const auto copied=direction==kInput?processingCatalogue_.inputCount():processingCatalogue_.outputCount();
            if(count<0||uint32_t(count)!=copied)throw InstanceFailure{Stage::Buses,Steinberg::kInvalidArgument,false};
            for(int32_t bus=0;bus<count;++bus) {
                BusInfo info{};Result status=Steinberg::kInternalError;bool returned=false;
                try{status=component_->getBusInfo(kEvent,direction,bus,info);returned=true;}catch(...){throw InstanceFailure{Stage::Buses,0,false,FailureOrigin::Thrown};}
                accepted(status,Stage::Buses,returned);
                const auto& expected=processingCatalogue_.bus(direction==kOutput,uint32_t(bus));
                if(info.mediaType!=kEvent||info.direction!=direction||info.channelCount!=int32_t(expected.channels)||
                   info.busType!=int32_t(expected.type)||info.flags!=expected.flags)
                    throw InstanceFailure{Stage::Buses,Steinberg::kInvalidArgument,false};
                try{if(ravet::vst3::text(info.name,true)!=expected.name)throw InstanceFailure{Stage::Buses,Steinberg::kInvalidArgument,false};}
                catch(const Failure&){throw InstanceFailure{Stage::Buses,Steinberg::kInvalidArgument,false};}
            }
        }
    }
    void activateEventBus(Steinberg::Vst::BusDirection direction,int32_t bus) {
        Result status=Steinberg::kInternalError;bool returned=false;
        try{status=component_->activateBus(Steinberg::Vst::kEvent,direction,bus,true);returned=true;}catch(...){throw InstanceFailure{Stage::ActivateBus,0,false,FailureOrigin::Thrown};}
        accepted(status,Stage::ActivateBus,returned);
    }
    void deactivateEventBuses(InstanceFailure& first) noexcept {
        using namespace Steinberg::Vst;
        for(auto direction:{kOutput,kInput}) {
            auto& mask=direction==kInput?eventInputActive_:eventOutputActive_;
            for(int32_t bus=15;bus>=0;--bus)if(mask&(1u<<bus)) {
                mask=uint16_t(mask&~(1u<<bus));
                cleanup([&]{return component_->activateBus(kEvent,direction,bus,false);},Stage::Deactivate,first);
            }
        }
    }
    hosted_audio::CopiedCatalogue inspectHostedAudio() {
        using namespace Steinberg::Vst;
        hosted_audio::CopiedCatalogue candidate;
        for(auto direction:{kInput,kOutput}) {
            const auto count=sdkCall(Stage::Buses,[&]{return component_->getBusCount(kAudio,direction);});
            if(count<0||count>16)throw InstanceFailure{Stage::Buses,0};
            const bool output=direction==kOutput;auto& entries=output?candidate.outputs:candidate.inputs;
            (output?candidate.outputCount:candidate.inputCount)=uint32_t(count);unsigned mains=0;
            for(int32_t index=0;index<count;++index) {
                BusInfo info{};sdkAccepted(Stage::Buses,[&]{return component_->getBusInfo(kAudio,direction,index,info);});
                if(info.mediaType!=kAudio||info.direction!=direction||info.channelCount<0||info.channelCount>64||
                   (info.busType!=kMain&&info.busType!=kAux)||(info.flags&~uint32_t(BusInfo::kDefaultActive))||
                   (info.busType==kMain&&++mains>1))throw InstanceFailure{Stage::Buses,0};
                SpeakerArrangement arrangement{};
                sdkAccepted(Stage::Arrangement,[&]{return processor_->getBusArrangement(direction,index,arrangement);});
                if(std::bitset<64>(arrangement).count()!=uint32_t(info.channelCount))throw InstanceFailure{Stage::Arrangement,0};
                std::string name;
                try{name=ravet::vst3::text(info.name,true);}catch(const Failure&){throw InstanceFailure{Stage::Buses,0};}
                if(name.size()>=sizeof(entries[0].name))throw InstanceFailure{Stage::Buses,0};
                auto& record=entries[uint32_t(index)];record.struct_size=sizeof(record);record.index=uint32_t(index);
                record.channels=uint32_t(info.channelCount);record.bus_type=uint32_t(info.busType);record.flags=info.flags;
                record.arrangement=arrangement;std::copy(name.begin(),name.end(),record.name);
            }
        }
        return candidate;
    }
    void deactivateHostedBuses(InstanceFailure& first)noexcept {
        using namespace Steinberg::Vst;
        for(auto direction:{kOutput,kInput}) {
            auto& mask=direction==kInput?hostedInputDuties_:hostedOutputDuties_;
            for(int32_t bus=15;bus>=0;--bus)if(mask&(1u<<bus)) {
                mask=uint16_t(mask&~(1u<<bus));
                cleanup([&]{return component_->activateBus(kAudio,direction,bus,false);},Stage::Deactivate,first);
            }
        }
    }
    void main() const {
        if (std::this_thread::get_id()!=main_ || !pthread_main_np())
            throw InstanceFailure{Stage::Thread,Steinberg::kInvalidArgument};
    }
    template<class Call> static void cleanup(Call call,Stage stage,InstanceFailure& first) noexcept {
        Result status=Steinberg::kInternalError;bool returned=false;
        try {status=call();returned=true;} catch (...) {
            if(first.native==Steinberg::kResultOk&&first.origin==FailureOrigin::Local)
                first={stage,0,false,FailureOrigin::Thrown};
            return;
        }
        if(first.native==Steinberg::kResultOk&&first.origin==FailureOrigin::Local&&status!=Steinberg::kResultOk)
            first={stage,status,returned,FailureOrigin::Returned};
    }
    static bool validProfile(AudioProfile p) {
        return parameters_draft::finite(p.rate)&&p.rate>0&&p.rate<=768000&&p.maximumFrames>0&&p.maximumFrames<=65536&&
            (p.outputs==1||p.outputs==2)&&(p.inputs==0||p.inputs==p.outputs);
    }
    void inspectBuses(bool arranged) {
        using namespace Steinberg::Vst;
        for (auto type:{kAudio,kEvent}) for (auto direction:{kInput,kOutput}) {
            const auto count=sdkCall(Stage::Buses,[&]{return component_->getBusCount(type,direction);});
            if (count<0||count>16) throw InstanceFailure{Stage::Buses,Steinberg::kInvalidArgument};
            if (type==kAudio && count!=(direction==kInput?(profile_.inputs?1:0):1))
                throw InstanceFailure{Stage::Buses,Steinberg::kInvalidArgument};
            for (int index=0;index<count;++index) {
                BusInfo info{}; sdkAccepted(Stage::Buses,[&]{return component_->getBusInfo(type,direction,index,info);});
                if (info.mediaType!=type||info.direction!=direction||info.channelCount<1||info.channelCount>64||
                    (info.busType!=kMain&&info.busType!=kAux)||
                    (info.flags&~uint32_t(BusInfo::kDefaultActive|BusInfo::kIsControlVoltage))||
                    (type==kEvent&&(info.channelCount>16||(info.flags&BusInfo::kIsControlVoltage))))
                    throw InstanceFailure{Stage::Buses,Steinberg::kInvalidArgument};
                try { (void)ravet::vst3::text(info.name,true); }
                catch (const Failure&) { throw InstanceFailure{Stage::Buses,Steinberg::kInvalidArgument}; }
                // The first profile owns ordinary main audio only. Event buses
                // are enumerated but remain inactive until event adoption.
                if (type==kAudio && (info.busType!=kMain||(info.flags&BusInfo::kIsControlVoltage)))
                    throw InstanceFailure{Stage::Buses,Steinberg::kInvalidArgument};
                if (arranged&&type==kAudio) {
                    const auto expected=direction==kInput?profile_.inputs:profile_.outputs;
                    SpeakerArrangement actual{};
                    sdkAccepted(Stage::Arrangement,[&]{return processor_->getBusArrangement(direction,index,actual);});
                    if (info.channelCount!=static_cast<int>(expected)||actual!=(expected==1?SpeakerArr::kMono:SpeakerArr::kStereo))
                        throw InstanceFailure{Stage::Arrangement,Steinberg::kInvalidArgument};
                }
            }
        }
    }
public:
    Instance(const Instance&)=delete;
    Instance& operator=(const Instance&)=delete;
    explicit Instance(std::shared_ptr<void> core,ControllerHandlerKeepers& handlers,
            hosted_audio::ScratchBudget* budget=nullptr):core_(std::move(core)),handlers_(&handlers),handler_(&handlers.attach()),
            scratchBudget_(budget?budget:&localScratchBudget_){}
    static std::unique_ptr<Instance> create(Steinberg::IPluginFactory& factory,
            Steinberg::FUnknown& host,const Catalogue& catalogue,uint32_t index,std::shared_ptr<void> core,ControllerHandlerKeepers& handlers) {
        auto owner=allocate(catalogue,index,std::move(core),handlers);owner->initialize(factory,host,catalogue,index);return owner;
    }
    static std::unique_ptr<Instance> allocate(const Catalogue& catalogue,uint32_t index,std::shared_ptr<void> core,ControllerHandlerKeepers& handlers,hosted_audio::ScratchBudget* budget=nullptr) {
        if(!pthread_main_np())throw InstanceFailure{Stage::Thread,Steinberg::kInvalidArgument};
        if(!core||index>=catalogue.classes.size()||catalogue.classes[index].category!="Audio Module Class")throw InstanceFailure{Stage::Profile,Steinberg::kInvalidArgument};
        auto owner=std::make_unique<Instance>(std::move(core),handlers,budget);owner->classId_=catalogue.classes[index].id;return owner;
    }
    void initialize(Steinberg::IPluginFactory& factory,Steinberg::FUnknown& host,const Catalogue& catalogue,uint32_t index) {
        using namespace Steinberg;
        using namespace Steinberg::Vst;
        if (!pthread_main_np()) throw InstanceFailure{Stage::Thread,kInvalidArgument};
        if (!core_||index>=catalogue.classes.size()||catalogue.classes[index].category!="Audio Module Class")
            throw InstanceFailure{Stage::Profile,kInvalidArgument};
        auto* owner=this;
        owner->component_=created<IComponent>([&](void** output){return factory.createInstance(
            reinterpret_cast<const char*>(catalogue.classes[index].id.data()),IComponent::iid,output);});
        auto* raw=owner->component_.get();
        owner->processor_=queried<IAudioProcessor>(*raw);
        TUID controllerId{};
        const auto controllerStatus=sdkCall(Stage::ControllerId,[&]{return raw->getControllerClassId(controllerId);});
        if (controllerStatus!=kResultOk&&controllerStatus!=kResultFalse&&controllerStatus!=kNoInterface&&controllerStatus!=kNotImplemented)
            accepted(controllerStatus,Stage::ControllerId);
        owner->controller_=queried<IEditController>(*raw,true);
        if (owner->controller_) {
            auto componentIdentity=queried<FUnknown>(*raw);
            auto controllerIdentity=queried<FUnknown>(*owner->controller_);
            if (componentIdentity.get()!=controllerIdentity.get()) throw InstanceFailure{Stage::Query,kInvalidArgument};
        } else if (controllerStatus==kResultOk) {
            const auto advertised=std::find_if(catalogue.classes.begin(),catalogue.classes.end(),[&](const auto& item){
                return std::memcmp(item.id.data(),controllerId,16)==0&&item.category=="Component Controller Class";
            });
            if (advertised==catalogue.classes.end()) throw InstanceFailure{Stage::ControllerId,kInvalidArgument};
            owner->controller_=created<IEditController>([&](void** output){return factory.createInstance(
                controllerId,IEditController::iid,output);});
            auto* controller=owner->controller_.get();
            auto componentIdentity=queried<FUnknown>(*raw);
            auto controllerIdentity=queried<FUnknown>(*controller);
            if (componentIdentity.get()!=controllerIdentity.get()) {
                // Initialize component first; a distinct controller has its own
                // lifecycle. Failed initialize never acquires terminate duty.
                sdkAccepted(Stage::Initialize,[&]{return raw->initialize(&host);}); owner->componentInitialized_=true;
                sdkAccepted(Stage::Initialize,[&]{return controller->initialize(&host);}); owner->controllerInitialized_=true;
            }
        } else if (controllerStatus!=kResultFalse&&controllerStatus!=kNoInterface&&controllerStatus!=kNotImplemented) {
            accepted(controllerStatus,Stage::ControllerId);
        }
        if (!owner->componentInitialized_) {
            sdkAccepted(Stage::Initialize,[&]{return raw->initialize(&host);}); owner->componentInitialized_=true;
        }
        if (owner->controller_) {
            owner->handlerAttempted_=true;
            sdkAccepted(Stage::Handler,[&]{return owner->controller_->setComponentHandler(owner->handler_);});
        }
        if (owner->controllerInitialized_) {
            owner->componentPoint_=queried<IConnectionPoint>(*raw,true);
            owner->controllerPoint_=queried<IConnectionPoint>(*owner->controller_,true);
            if (bool(owner->componentPoint_)!=bool(owner->controllerPoint_)) throw InstanceFailure{Stage::Connect,kNoInterface};
            if (owner->componentPoint_) {
                sdkAccepted(Stage::Connect,[&]{return owner->componentPoint_->connect(owner->controllerPoint_);}); owner->toController_=true;
                sdkAccepted(Stage::Connect,[&]{return owner->controllerPoint_->connect(owner->componentPoint_);}); owner->toComponent_=true;
            }
        }
    }
    // All state operations are synchronous CONTROL work on an initialized,
    // stopped, detached, non-faulted child. No raw SDK interface escapes.
    bool stateAllowed()const {
        main();return componentInitialized_&&component_&&!attached_&&!active_&&!running_&&!fault_;
    }
    bool stateIdentity(const RavetVst3StateRecord& record)const {
        main();return std::memcmp(classId_.data(),record.class_id,16)==0&&
            record.controller_mode<=RAVET_VST3_CONTROLLER_STATE&&
            bool(controller_)==(record.controller_mode!=RAVET_VST3_CONTROLLER_ABSENT)&&
            record.component_size<=state_draft::BoundedStateStream::MaximumBytes&&
            record.controller_size<=state_draft::BoundedStateStream::MaximumBytes&&
            (record.controller_mode==RAVET_VST3_CONTROLLER_STATE||!record.controller_size);
    }
    bool stateExternal(const void* pointer,size_t bytes,size_t alignment)const noexcept {
        Range requested{},owner{};
        if(!range(pointer,bytes,1,alignment,requested)||!range(this,1,sizeof(*this),alignof(Instance),owner)||overlaps(requested,owner))return false;
        if(!currentScratch_.external(pointer,bytes,alignment)||!pendingScratch_.external(pointer,bytes,alignment))return false;
        return parameterCatalogue_.externalRange(pointer,bytes,alignment)&&
            processingCatalogue_.externalRange(pointer,bytes,alignment);
    }
    static constexpr size_t MaximumOwnedRanges=41;
    template<class Append> bool appendOwnedRanges(Append append)const noexcept {
        if(!append(this,sizeof(*this),alignof(Instance))||
           !append(handler_,sizeof(ControllerHandler),alignof(ControllerHandler)))return false;
        // At most four full typed extents replace the former four per-channel
        // vectors; pending ownership remains visible during foreign reentry.
        if(!currentScratch_.appendRanges(append)||!pendingScratch_.appendRanges(append))return false;
        std::array<parameters_draft::Range,2> parameters{};
        std::array<parameters_draft::Range,33> events{};
        if(!parameterCatalogue_.protectionRanges(parameters))return false;
        for(auto r:parameters)if(!append(reinterpret_cast<const void*>(r.start),r.end-r.start,1))return false;
        const auto count=processingCatalogue_.protectionRanges(events);if(!count)return false;
        for(size_t i=0;i<count;++i)if(!append(reinterpret_cast<const void*>(events[i].start),events[i].end-events[i].start,1))return false;
        return true;
    }
    bool captureState(state_draft::StateStreamKeepers& keepers,uint8_t* componentBytes,size_t componentCapacity,
            uint8_t* controllerBytes,size_t controllerCapacity,RavetVst3StateRecord& record,RavetVst3StateReport& report) {
        if(!stateAllowed())throw InstanceFailure{Stage::Lifecycle,Steinberg::kInvalidArgument};
        return state_draft::StateTransaction::capture(*component_,controller_.get(),classId_.data(),keepers,
            componentBytes,componentCapacity,controllerBytes,controllerCapacity,record,report);
    }
    bool restoreState(state_draft::StateStreamKeepers& keepers,const RavetVst3StateRecord& record,
            const uint8_t* componentBytes,const uint8_t* controllerBytes,RavetVst3StateReport& report) {
        if(!stateAllowed())throw InstanceFailure{Stage::Lifecycle,Steinberg::kInvalidArgument};
        if(!stateIdentity(record))throw InstanceFailure{Stage::Profile,Steinberg::kInvalidArgument};
        const bool success=state_draft::StateTransaction::restore(*component_,controller_.get(),keepers,
            record,componentBytes,controllerBytes,report);
        ready_=false;
        parametersCompatible_=false;
        processingCompatible_=false;processingReady_=false;
        if(!success){fault_=true;report.flags|=RAVET_VST3_STATE_POISONED;}
        return success;
    }
    void refreshParameters() {
        if(!stateAllowed())throw InstanceFailure{Stage::Lifecycle,Steinberg::kInvalidArgument};
        parametersCompatible_=false;
        processingCompatible_=false;processingReady_=false;
        // A component may deliberately have no edit controller. Its reconciled
        // parameter catalogue is empty; event/context processing still works.
        parameters_draft::ParameterCatalogue candidate;
        if(controller_){
            candidate=parameters_draft::ParameterCatalogue::inspect(*controller_);
            // Validate current reads as well as metadata before reconciliation.
            for(const auto& p:candidate.parameters())(void)candidate.read(*controller_,p.id);
        }
        parameterCatalogue_=std::move(candidate);parametersCompatible_=true;
    }
    const auto& parameterCatalogue()const {
        if(!stateAllowed())throw InstanceFailure{Stage::Lifecycle,Steinberg::kInvalidArgument};
        return parameterCatalogue_;
    }
    double parameterValue(uint32_t id,uint32_t operation,double input) {
        if(!stateAllowed()||!controller_)throw InstanceFailure{Stage::Lifecycle,Steinberg::kInvalidArgument};
        switch(operation){
            case RAVET_VST3_PARAMETER_READ:return parameterCatalogue_.read(*controller_,id);
            case RAVET_VST3_PARAMETER_PLAIN:return parameterCatalogue_.plain(*controller_,id,input);
            case RAVET_VST3_PARAMETER_NORMALIZE:return parameterCatalogue_.normalize(*controller_,id,input);
            case RAVET_VST3_PARAMETER_EDIT:
                try {parameterCatalogue_.edit(*controller_,id,input);}
                catch(const parameters_draft::ParameterFailure& e){
                    if(e.kind==parameters_draft::ParameterErrorKind::Native||e.kind==parameters_draft::ParameterErrorKind::Thrown)parametersCompatible_=false;
                    throw;
                }
                return input;
            default:throw InstanceFailure{Stage::Profile,Steinberg::kInvalidArgument};
        }
    }
    std::string formatParameter(uint32_t id,double value) {
        if(!stateAllowed()||!controller_)throw InstanceFailure{Stage::Lifecycle,Steinberg::kInvalidArgument};
        return parameterCatalogue_.format(*controller_,id,value);
    }
    double parseParameter(uint32_t id,const Steinberg::Vst::String128& text) {
        if(!stateAllowed()||!controller_)throw InstanceFailure{Stage::Lifecycle,Steinberg::kInvalidArgument};
        return parameterCatalogue_.parse(*controller_,id,text);
    }
    bool parametersCompatible()const noexcept{return parametersCompatible_;}
    void prepareParameters(AudioProfile profile,parameters_draft::ParameterPoolKeepers& keepers) {
        main();
        if(!stateAllowed()||!parametersCompatible_||!validProfile(profile))
            throw InstanceFailure{Stage::Lifecycle,Steinberg::kInvalidArgument};
        if(!parameterPool_)parameterPool_=&keepers.attach();
        prepare(profile);
    }
    void detachParameters(parameters_draft::ParameterPoolKeepers& keepers) {
        main();if(attached_)throw InstanceFailure{Stage::Lifecycle,Steinberg::kInvalidArgument};
        if(parameterPool_){keepers.detach(*parameterPool_);parameterPool_=nullptr;}
    }
    void refreshProcessing(uint16_t selectedInput,uint16_t selectedOutput) {
        main();
        if(!stateAllowed()||!parametersCompatible_)throw InstanceFailure{Stage::Lifecycle,Steinberg::kInvalidArgument};
        processingCompatible_=false;processingReady_=false;
        std::array<uint32_t,1024> ids{};size_t count=0;
        for(const auto& p:parameterCatalogue_.parameters())if(parameterCatalogue_.writable(p.id))ids[count++]=p.id;
        auto mapping=controller_?queried<Steinberg::Vst::IMidiMapping>(*controller_,true):nullptr;
        auto requirements=queried<Steinberg::Vst::IProcessContextRequirements>(*processor_,true);
        auto candidate=processing_draft::ProcessingCatalogue::inspect(*component_,mapping.get(),requirements.get(),
            ids.data(),count,selectedInput,selectedOutput);
        processingCatalogue_=std::move(candidate);processingCompatible_=true;
    }
    const auto& processingCatalogue()const {
        if(!stateAllowed())throw InstanceFailure{Stage::Lifecycle,Steinberg::kInvalidArgument};
        return processingCatalogue_;
    }
    bool processingCompatible()const noexcept{return processingCompatible_&&parametersCompatible_;}
    auto* preparedProcessingPool()const noexcept{return processingReady_?processingPool_:nullptr;}
    template<class Core> void prepareProcessing(AudioProfile profile,
            parameters_draft::ParameterPoolKeepers& parameters,
            processing_draft::ProcessingPoolKeepers<Core>& processes,Core& core) {
        main();
        if(!stateAllowed()||!processingCompatible()||!validProfile(profile))
            throw InstanceFailure{Stage::Lifecycle,Steinberg::kInvalidArgument};
        // An admitted reprepare may observe changed foreign topology. Retire
        // prior readiness before that inspection; local invalid profiles above
        // leave the previous preparation intact. Activation duties stay owned
        // until a valid retry or retirement deactivates them once.
        ready_=false;processingReady_=false;
        verifyProcessingTopology(); // Before any pool allocation or native setup.
        if(!processingPool_)processingPool_=&processes.attach(core);
        std::array<uint32_t,1024> ids{};size_t count=0;
        // Output parameter queues may describe every copied known ID, including
        // readonly output meters; input mapping still admits writable IDs only.
        for(const auto& p:parameterCatalogue_.parameters())ids[count++]=p.id;
        if(!processingPool_->configureParameters(ids.data(),count))
            throw InstanceFailure{Stage::Lifecycle,Steinberg::kInvalidArgument};
        prepareParameters(profile,parameters);
        try {
            verifyProcessingTopology(); // Arrangements/setup must not stale event metadata.
            for(auto direction:{Steinberg::Vst::kInput,Steinberg::Vst::kOutput}) {
                const auto selected=processingCatalogue_.selectedBuses(direction==Steinberg::Vst::kOutput);
                auto& mask=direction==Steinberg::Vst::kInput?eventInputActive_:eventOutputActive_;
                for(uint32_t bus=0;bus<selected.size();++bus)if(selected[bus]) {
                    mask=uint16_t(mask|(1u<<bus));
                    activateEventBus(direction,int32_t(bus));
                }
            }
            processingReady_=true;
        } catch (...) {
            auto original=std::current_exception();
            InstanceFailure cleanupFailure{Stage::Deactivate,Steinberg::kResultOk};
            deactivateEventBuses(cleanupFailure);
            ready_=false;processingReady_=false;fault_=true;
            std::rethrow_exception(original);
        }
    }
    template<class Core> void detachProcessing(processing_draft::ProcessingPoolKeepers<Core>& keepers,Core& core) {
        main();if(attached_)throw InstanceFailure{Stage::Lifecycle,Steinberg::kInvalidArgument};
        if(processingPool_){keepers.detach(core,*processingPool_);processingPool_=nullptr;}
    }
    void prepare(AudioProfile profile) {
        using namespace Steinberg::Vst;
        main();
        if (attached_||active_||running_||fault_) throw InstanceFailure{Stage::Lifecycle,Steinberg::kInvalidArgument};
        if (!validProfile(profile)) throw InstanceFailure{Stage::Profile,Steinberg::kInvalidArgument};
        // Stage planar scratch before foreign setup; metadata inspection and
        // its bounded text allocation remain on CONTROL.
        pendingScratch_.prepare(*scratchBudget_,32,profile.maximumFrames,profile.inputs,profile.outputs,false);
        struct PendingCleanup {hosted_audio::Scratch& scratch;~PendingCleanup(){scratch.clear();}} pendingCleanup{pendingScratch_};
        ready_=false;processingReady_=false;
        InstanceFailure deactivation{Stage::Deactivate,Steinberg::kResultOk};
        deactivateEventBuses(deactivation);deactivateHostedBuses(deactivation);hostedMode_=false;
        if(deactivation.origin!=FailureOrigin::Local||deactivation.native!=Steinberg::kResultOk){fault_=true;throw deactivation;}
        profile_=profile;
        try {
            inspectBuses(false);
            auto in=profile.inputs==1?SpeakerArr::kMono:SpeakerArr::kStereo;
            auto out=profile.outputs==1?SpeakerArr::kMono:SpeakerArr::kStereo;
            sdkAccepted(Stage::Precision,[&]{return processor_->canProcessSampleSize(kSample32);});
            sdkAccepted(Stage::Arrangement,[&]{return processor_->setBusArrangements(profile.inputs?&in:nullptr,profile.inputs?1:0,&out,1);});
            inspectBuses(true);
            ProcessSetup setup{}; setup.processMode=kRealtime; setup.symbolicSampleSize=kSample32;
            setup.sampleRate=profile.rate; setup.maxSamplesPerBlock=static_cast<int>(profile.maximumFrames);
            sdkAccepted(Stage::Setup,[&]{return processor_->setupProcessing(setup);});
            if (profile.inputs) { sdkAccepted(Stage::ActivateBus,[&]{return component_->activateBus(kAudio,kInput,0,true);}); inputActive_=true; }
            sdkAccepted(Stage::ActivateBus,[&]{return component_->activateBus(kAudio,kOutput,0,true);}); outputActive_=true;
            currentScratch_=std::move(pendingScratch_);
            for (uint32_t ch=0;ch<2;++ch) {inputPointers_[ch]=currentScratch_.input<float>(ch);outputPointers_[ch]=currentScratch_.output<float>(ch);}
            ready_=true;
        } catch (...) { fault_=true; throw; }
    }
    void refreshHostedAudio() {
        main();if(!stateAllowed())throw InstanceFailure{Stage::Lifecycle,0};
        hostedCatalogueCompatible_=false;ready_=false;processingReady_=false;
        auto candidate=inspectHostedAudio();hostedCatalogue_=candidate;hostedCatalogueCompatible_=true;
    }
    const auto& hostedAudioCatalogue()const {
        if(!stateAllowed()||!hostedCatalogueCompatible_)throw InstanceFailure{Stage::Lifecycle,0};return hostedCatalogue_;
    }
    bool hostedPrepared()const noexcept{return hostedMode_&&ready_&&processingReady_&&!fault_;}
    bool prepared()const noexcept{return ready_&&!fault_;}
    const auto& hostedLayout()const noexcept{return hostedLayout_;}
    const auto& hostedTiming()const {
        main();if(attached_||!hostedPrepared())throw InstanceFailure{Stage::Lifecycle,0};return hostedTiming_;
    }
    hosted_audio::PackedLayout preflightHostedAudio(const RavetVst3HostedAudioProfile& requested)const {
        main();
        if(!stateAllowed()||!processingCompatible()||!hostedCatalogueCompatible_)throw InstanceFailure{Stage::Lifecycle,0};
        hosted_audio::PackedLayout candidate;
        if(!hosted_audio::validProfile(requested)||requested.input_buses!=hostedCatalogue_.inputCount||
           requested.output_buses!=hostedCatalogue_.outputCount||!hosted_audio::requestedLayout(requested,candidate))
            throw InstanceFailure{Stage::Profile,0};
        const size_t bytes=size_t(candidate.inputChannels+candidate.outputChannels)*requested.maximum_frames*(requested.sample_bits/8);
        if(!scratchBudget_->canAdmit(bytes))throw std::length_error("hosted audio containing-core scratch capacity");
        return candidate;
    }
    template<class Core> void prepareHostedAudio(const RavetVst3HostedAudioProfile requested,
            parameters_draft::ParameterPoolKeepers& parameters,
            processing_draft::ProcessingPoolKeepers<Core>& processes,Core& core) {
        using namespace Steinberg::Vst;
        const auto candidateLayout=preflightHostedAudio(requested);
        std::array<uint32_t,16> inputs{},outputs{};
        std::array<SpeakerArrangement,16> inputArrangements{},outputArrangements{};
        for(bool output:{false,true})for(uint32_t i=0;i<hostedCatalogue_.count(output);++i) {
            const auto value=(output?requested.outputs:requested.inputs)[i].arrangement;
            (output?outputs:inputs)[i]=uint32_t(std::bitset<64>(value).count());
            (output?outputArrangements:inputArrangements)[i]=value;
        }
        pendingScratch_.prepare(*scratchBudget_,requested.sample_bits,requested.maximum_frames,candidateLayout.inputChannels,candidateLayout.outputChannels);
        struct PendingCleanup {hosted_audio::Scratch& scratch;~PendingCleanup(){scratch.clear();}} pendingCleanup{pendingScratch_};
        ready_=false;processingReady_=false;
        try {
            const auto before=inspectHostedAudio();
            if(std::memcmp(&before,&hostedCatalogue_,sizeof(before)))throw InstanceFailure{Stage::Buses,0};
            verifyProcessingTopology();
            if(!parameterPool_)parameterPool_=&parameters.attach();
            if(!processingPool_)processingPool_=&processes.attach(core);
            std::array<uint32_t,1024> ids{};size_t count=0;
            for(const auto& p:parameterCatalogue_.parameters())ids[count++]=p.id;
            if(!processingPool_->configureParameters(ids.data(),count))throw InstanceFailure{Stage::Lifecycle,0};
            InstanceFailure deactivate{Stage::Deactivate,Steinberg::kResultOk};
            deactivateEventBuses(deactivate);deactivateHostedBuses(deactivate);
            if(outputActive_){outputActive_=false;cleanup([&]{return component_->activateBus(kAudio,kOutput,0,false);},Stage::Deactivate,deactivate);}
            if(inputActive_){inputActive_=false;cleanup([&]{return component_->activateBus(kAudio,kInput,0,false);},Stage::Deactivate,deactivate);}
            if(deactivate.origin!=FailureOrigin::Local||deactivate.native!=Steinberg::kResultOk)throw deactivate;
            const auto sampleSize=requested.sample_bits==32?kSample32:kSample64;
            sdkAccepted(Stage::Precision,[&]{return processor_->canProcessSampleSize(sampleSize);});
            sdkAccepted(Stage::Arrangement,[&]{return processor_->setBusArrangements(
                requested.input_buses?inputArrangements.data():nullptr,int32_t(requested.input_buses),
                requested.output_buses?outputArrangements.data():nullptr,int32_t(requested.output_buses));});
            const auto after=inspectHostedAudio();
            if(after.inputCount!=before.inputCount||after.outputCount!=before.outputCount)throw InstanceFailure{Stage::Buses,0};
            for(bool output:{false,true})for(uint32_t i=0;i<after.count(output);++i) {
                const auto& previous=before.bus(output,i);const auto& actual=after.bus(output,i);
                const auto& request=(output?requested.outputs:requested.inputs)[i];
                if(actual.bus_type!=previous.bus_type||actual.flags!=previous.flags||actual.arrangement!=request.arrangement||
                   actual.channels!=(output?outputs:inputs)[i])throw InstanceFailure{Stage::Arrangement,0};
            }
            ProcessSetup setup{};setup.processMode=kRealtime;setup.symbolicSampleSize=sampleSize;
            setup.sampleRate=requested.sample_rate;setup.maxSamplesPerBlock=int32_t(requested.maximum_frames);
            sdkAccepted(Stage::Setup,[&]{return processor_->setupProcessing(setup);});
            // A setup callback cannot silently change the negotiated snapshot.
            const auto finalAudio=inspectHostedAudio();
            if(std::memcmp(&finalAudio,&after,sizeof(after)))throw InstanceFailure{Stage::Buses,0};
            verifyProcessingTopology();
            RavetVst3HostedAudioTiming timing{};timing.struct_size=sizeof(timing);
            timing.latency_samples=sdkCall(Stage::Latency,[&]{return processor_->getLatencySamples();});
            const auto tail=sdkCall(Stage::Tail,[&]{return processor_->getTailSamples();});
            timing.tail_kind=tail==0?0u:tail==UINT32_MAX?2u:1u;timing.tail_samples=timing.tail_kind==1?tail:0;
            for(auto direction:{kInput,kOutput}) {
                const bool output=direction==kOutput;auto& duty=output?hostedOutputDuties_:hostedInputDuties_;
                for(uint32_t bus=0;bus<after.count(output);++bus) {
                    const bool enabled=(output?requested.outputs:requested.inputs)[bus].active;
                    if(enabled)duty=uint16_t(duty|(1u<<bus)); // acquire BEFORE foreign attempt
                    sdkAccepted(Stage::ActivateBus,[&]{return component_->activateBus(kAudio,direction,int32_t(bus),enabled);});
                }
            }
            for(auto direction:{kInput,kOutput}) {
                const auto selected=processingCatalogue_.selectedBuses(direction==kOutput);
                auto& duty=direction==kOutput?eventOutputActive_:eventInputActive_;
                for(uint32_t bus=0;bus<selected.size();++bus)if(selected[bus]) {
                    duty=uint16_t(duty|(1u<<bus));activateEventBus(direction,int32_t(bus));
                }
            }
            const auto admittedAudio=inspectHostedAudio();
            if(std::memcmp(&admittedAudio,&after,sizeof(after)))throw InstanceFailure{Stage::Buses,0};
            verifyProcessingTopology();
            currentScratch_=std::move(pendingScratch_);hostedProfile_=requested;hostedLayout_=candidateLayout;
            hostedCatalogue_=after;hostedTiming_=timing;hostedMode_=true;
            profile_={requested.sample_rate,requested.maximum_frames,candidateLayout.inputChannels,candidateLayout.outputChannels};
            inputPointers_.fill(nullptr);outputPointers_.fill(nullptr);
            ready_=true;processingReady_=true;
        }catch(...) {
            // History/native state cannot be rolled back. Existing scratch and
            // every attempted activation duty remain owned until retirement.
            fault_=true;ready_=false;processingReady_=false;throw;
        }
    }
    template<class Sample> Result processHosted(uint32_t frames,Steinberg::Vst::IParameterChanges& changes,
            processing_draft::EventBatch& events,Steinberg::Vst::ProcessContext* context,bool& attempted,bool& threw)noexcept {
        using namespace Steinberg::Vst;
        attempted=false;threw=false;
        constexpr uint32_t bits=uint32_t(sizeof(Sample)*8);
        if(!hostedPrepared()||!attached_||!running_||bits!=hostedProfile_.sample_bits||frames>hostedProfile_.maximum_frames)return Steinberg::kInvalidArgument;
        if(audio_==std::thread::id{})audio_=std::this_thread::get_id();
        if(audio_!=std::this_thread::get_id())return Steinberg::kInvalidArgument;
        // Fixed stack metadata; no array position disappears for an inactive
        // native bus. Inactive buses retain channels/pointer arrays, with every
        // sample pointer null. Only explicit active indices use packed scratch.
        std::array<AudioBusBuffers,16> nativeInputs{},nativeOutputs{};
        std::array<std::array<Sample*,64>,16> inputPointers{},outputPointers{};
        for(bool output:{false,true}) {
            auto& buffers=output?nativeOutputs:nativeInputs;auto& pointers=output?outputPointers:inputPointers;
            const auto& layout=output?hostedLayout_.outputs:hostedLayout_.inputs;
            const auto count=output?hostedLayout_.outputBuses:hostedLayout_.inputBuses;
            for(uint32_t i=0;i<count;++i) {
                const auto& bus=layout[i];buffers[i].numChannels=int32_t(bus.channels);
                if constexpr(std::is_same_v<Sample,float>)buffers[i].channelBuffers32=pointers[i].data();
                else buffers[i].channelBuffers64=pointers[i].data();
                if(bus.active)for(uint32_t ch=0;ch<bus.channels;++ch) {
                    pointers[i][ch]=output?currentScratch_.output<Sample>(bus.offset+ch):currentScratch_.input<Sample>(bus.offset+ch);
                    if(output)std::fill_n(pointers[i][ch],frames,Sample(0));
                }
            }
        }
        ProcessData data{};data.processMode=kRealtime;data.symbolicSampleSize=bits==32?kSample32:kSample64;
        data.numSamples=int32_t(frames);data.numInputs=int32_t(hostedLayout_.inputBuses);data.numOutputs=int32_t(hostedLayout_.outputBuses);
        data.inputs=hostedLayout_.inputBuses?nativeInputs.data():nullptr;data.outputs=hostedLayout_.outputBuses?nativeOutputs.data():nullptr;
        data.inputParameterChanges=&changes;data.outputParameterChanges=&events.outputParameters();data.inputEvents=&events.input();data.outputEvents=&events.output();data.processContext=context;
        Result status=Steinberg::kInternalError;attempted=true;
        try{status=processor_->process(data);}catch(...){threw=true;}
        if(status!=Steinberg::kResultOk||threw)fault_=true;
        for(uint32_t i=0;i<hostedLayout_.outputBuses;++i) {
            const auto& bus=hostedLayout_.outputs[i];if(!bus.active)continue;
            for(uint32_t ch=0;ch<bus.channels;++ch)if(fault_||(nativeOutputs[i].silenceFlags&(uint64_t{1}<<ch)))
                std::fill_n(currentScratch_.output<Sample>(bus.offset+ch),frames,Sample(0));
        }
        return status;
    }
    void start() {
        main(); if (attached_||active_||running_||!ready_||fault_) throw InstanceFailure{Stage::Lifecycle,Steinberg::kInvalidArgument};
        try {
            if(hostedMode_)active_=true;
            sdkAccepted(Stage::Activate,[&]{return component_->setActive(true);}); active_=true;
            if(hostedMode_)running_=true;
            sdkAccepted(Stage::Start,[&]{return processor_->setProcessing(true);}); running_=true;
        } catch (...) { fault_=true; throw; }
    }
    void beginAudio() {main();if (!running_||fault_||attached_)throw InstanceFailure{Stage::Lifecycle,Steinberg::kInvalidArgument};attached_=true;audio_={};}
    void endAudio() {main();if (!attached_)throw InstanceFailure{Stage::Lifecycle,Steinberg::kInvalidArgument};attached_=false;audio_={};}
    // The caller exclusively scopes this capability and joins before endAudio.
    // No audio capability owns native interfaces or has lifecycle in its Drop.
    Result process(uint32_t frames,Steinberg::Vst::IParameterChanges* changes=nullptr,bool* threw=nullptr,
            processing_draft::EventBatch* events=nullptr,Steinberg::Vst::ProcessContext* context=nullptr,
            bool* attempted=nullptr) noexcept {
        if(attempted)*attempted=false;
        if(hostedMode_)return Steinberg::kInvalidArgument;
        if ((events||context)&&(!processingReady_||!processingCompatible()))return Steinberg::kInvalidArgument;
        if (!attached_||!running_||fault_||frames>profile_.maximumFrames) return Steinberg::kInvalidArgument;
        if (audio_==std::thread::id{}) audio_=std::this_thread::get_id();
        if (audio_!=std::this_thread::get_id()) return Steinberg::kInvalidArgument;
        using namespace Steinberg::Vst;
        AudioBusBuffers in{},out{}; in.numChannels=static_cast<int>(profile_.inputs);out.numChannels=static_cast<int>(profile_.outputs);
        in.channelBuffers32=inputPointers_.data();out.channelBuffers32=outputPointers_.data();
        for(uint32_t ch=0;ch<profile_.outputs;++ch) std::fill_n(outputPointers_[ch],frames,0.f);
        ProcessData data{};data.processMode=kRealtime;data.symbolicSampleSize=kSample32;data.numSamples=static_cast<int>(frames);
        data.numInputs=profile_.inputs?1:0;data.numOutputs=1;data.inputs=profile_.inputs?&in:nullptr;data.outputs=&out;
        data.inputParameterChanges=changes;
        data.inputEvents=events?&events->input():nullptr;
        data.outputEvents=events?&events->output():nullptr;
        data.outputParameterChanges=events?&events->outputParameters():nullptr;
        data.processContext=context;
        Result result=Steinberg::kInternalError;
        if(attempted)*attempted=true;
        try{result=processor_->process(data);}catch(...){if(threw)*threw=true;}
        if (result!=Steinberg::kResultOk) {fault_=true;for(uint32_t ch=0;ch<profile_.outputs;++ch)std::fill_n(outputPointers_[ch],frames,0.f);}
        // The plugin can mark output silence without filling its writable data.
        for(uint32_t ch=0;ch<profile_.outputs;++ch)if(out.silenceFlags&(uint64_t{1}<<ch))std::fill_n(outputPointers_[ch],frames,0.f);
        return result;
    }
    struct Range { uintptr_t first=0,last=0; };
    static bool range(const void* pointer,size_t count,size_t width,size_t alignment,Range& out) noexcept {
        if(count>std::numeric_limits<uintptr_t>::max()/width)return false;
        const auto bytes=count*width,start=reinterpret_cast<uintptr_t>(pointer);
        if(bytes&&(!pointer||start%alignment||bytes>std::numeric_limits<uintptr_t>::max()-start))return false;
        out={start,start+bytes};return true;
    }
    static bool overlaps(Range a,Range b) noexcept {return a.first<a.last&&b.first<b.last&&a.first<b.last&&b.first<a.last;}
    // Trusted ranges must actually contain their declared elements. Protect whole
    // borrowed metadata and prepared owner/scratch, permit input/output alias via
    // pre-copy, reject any intersecting active output prefixes before DSP/writes.
    bool validateBlock(const RavetVst3AudioBlock& block,const void* extra=nullptr,size_t extraBytes=0)const noexcept {
        if(hostedMode_||!attached_||!running_||fault_||block.struct_size!=sizeof(block)||block.frames>profile_.maximumFrames||
           block.input_channels!=profile_.inputs||block.output_channels!=profile_.outputs||
           (!block.input_channels&&block.inputs))return false;
        std::array<Range,4> protectedRanges{};
        if(!range(&block,1,sizeof(block),alignof(RavetVst3AudioBlock),protectedRanges[0])||
           !range(block.inputs,block.input_channels,sizeof(RavetVst3InputChannel),alignof(RavetVst3InputChannel),protectedRanges[1])||
           !range(block.outputs,block.output_channels,sizeof(RavetVst3OutputChannel),alignof(RavetVst3OutputChannel),protectedRanges[2])||
           !range(extra,extraBytes,1,1,protectedRanges[3]))return false;
        for(uint32_t ch=0;ch<profile_.inputs;++ch){Range full{};const auto& v=block.inputs[ch];
            if(v.reserved||v.capacity<block.frames||!range(v.samples,v.capacity,sizeof(float),alignof(float),full))return false;
        }
        std::array<Range,2> destinations{};Range owner{};
        if(!range(this,1,sizeof(*this),alignof(Instance),owner))return false;
        for(uint32_t ch=0;ch<profile_.outputs;++ch){Range full{};const auto& v=block.outputs[ch];
            if(v.reserved||v.capacity<block.frames||!range(v.samples,v.capacity,sizeof(float),alignof(float),full)||
               !range(v.samples,block.frames,sizeof(float),alignof(float),destinations[ch]))return false;
            if(overlaps(destinations[ch],owner))return false;
            for(const auto r:protectedRanges)if(overlaps(destinations[ch],r))return false;
            for(uint32_t prior=0;prior<ch;++prior)if(overlaps(destinations[ch],destinations[prior]))return false;
            auto excludes=[&](const void* p,size_t bytes,size_t alignment){Range scratch{};
                return range(p,bytes,1,alignment,scratch)&&!overlaps(destinations[ch],scratch)&&!overlaps(protectedRanges[3],scratch);};
            if(!currentScratch_.appendRanges(excludes)||!pendingScratch_.appendRanges(excludes))return false;
        }
        // The eventual diagnostic write must not alter borrowed records or any
        // active input/output storage. Invalid blocks preserve all these bytes.
        const auto diagnostic=protectedRanges[3];
        for(size_t i=0;i<3;++i)if(overlaps(diagnostic,protectedRanges[i]))return false;
        if(overlaps(diagnostic,owner))return false;
        for(uint32_t ch=0;ch<profile_.inputs;++ch){Range input{};
            (void)range(block.inputs[ch].samples,block.frames,sizeof(float),alignof(float),input);
            if(overlaps(diagnostic,input))return false;
        }
        return true;
    }
    Result processBlock(const RavetVst3AudioBlock& block,const void* extra=nullptr,size_t extraBytes=0) noexcept {
        if(!validateBlock(block,extra,extraBytes))return Steinberg::kInvalidArgument;
        if(audio_!=std::thread::id{}&&audio_!=std::this_thread::get_id())return Steinberg::kInvalidArgument;
        for(uint32_t ch=0;ch<profile_.inputs;++ch)if(block.frames)std::copy_n(block.inputs[ch].samples,block.frames,inputPointers_[ch]);
        const auto status=process(block.frames);
        // Known native faults silence supported caller prefixes via the scratch;
        // invalid shape/aliases made no caller or plugin writes above.
        for(uint32_t ch=0;ch<profile_.outputs;++ch)if(block.frames)std::copy_n(outputPointers_[ch],block.frames,block.outputs[ch].samples);
        return status;
    }
    int32_t processParameterBlock(const RavetVst3AudioBlock& block,
            const parameters_draft::Point* points,uint32_t count,RavetVst3ParameterReport& report) noexcept {
        using namespace parameters_draft;
        if(!parametersCompatible_||!parameterPool_||
           (audio_!=std::thread::id{}&&audio_!=std::this_thread::get_id()))return RAVET_VST3_LIFECYCLE;
        if(block.input_channels>2||block.output_channels>2||
           !parameterPool_->externalRange(&block,sizeof(block),alignof(RavetVst3AudioBlock))||
           !parameterPool_->externalRange(block.inputs,block.input_channels*sizeof(RavetVst3InputChannel),alignof(RavetVst3InputChannel))||
           !parameterPool_->externalRange(block.outputs,block.output_channels*sizeof(RavetVst3OutputChannel),alignof(RavetVst3OutputChannel))||
           !stateExternal(&report,sizeof(report),alignof(RavetVst3ParameterReport))||
           !stateExternal(block.inputs,block.input_channels*sizeof(RavetVst3InputChannel),alignof(RavetVst3InputChannel))||
           !stateExternal(block.outputs,block.output_channels*sizeof(RavetVst3OutputChannel),alignof(RavetVst3OutputChannel))||
           !validateBlock(block,&report,sizeof(report)))return RAVET_VST3_INVALID;
        parameters_draft::Range source{},diagnostic{};
        if(count>Batch::MaximumPoints||!parameters_draft::range(points,count,sizeof(Point),alignof(Point),source)||
           !parameters_draft::range(&report,1,sizeof(report),alignof(RavetVst3ParameterReport),diagnostic)||
           parameters_draft::overlaps(source,diagnostic)||!stateExternal(points,count*sizeof(Point),alignof(Point)))return RAVET_VST3_INVALID;
        // Whole source/outputs/borrowed records/protected pool validation before
        // pool admission, input copy, scratch clear or native DSP invocation.
        if(!parameterPool_->externalRange(points,count*sizeof(Point),alignof(Point))||
           !parameterPool_->externalRange(&report,sizeof(report),alignof(RavetVst3ParameterReport))||
           !parameterPool_->externalRange(&block,sizeof(block),alignof(RavetVst3AudioBlock))||
           !parameterPool_->externalRange(block.inputs,block.input_channels*sizeof(RavetVst3InputChannel),alignof(RavetVst3InputChannel))||
           !parameterPool_->externalRange(block.outputs,block.output_channels*sizeof(RavetVst3OutputChannel),alignof(RavetVst3OutputChannel)))return RAVET_VST3_INVALID;
        for(uint32_t ch=0;ch<block.output_channels;++ch){
            parameters_draft::Range active{};if(!parameters_draft::range(block.outputs[ch].samples,block.frames,sizeof(float),alignof(float),active)||
                parameters_draft::overlaps(active,source)||!stateExternal(block.outputs[ch].samples,block.frames*sizeof(float),alignof(float))||
                !parameterPool_->externalRange(block.outputs[ch].samples,size_t(block.outputs[ch].capacity)*sizeof(float),alignof(float)))return RAVET_VST3_INVALID;
        }
        for(uint32_t ch=0;ch<block.input_channels;++ch)if(
            !parameterPool_->externalRange(block.inputs[ch].samples,size_t(block.inputs[ch].capacity)*sizeof(float),alignof(float)))return RAVET_VST3_INVALID;
        for(uint32_t p=0;p<count;++p)if(!parameterCatalogue_.writable(points[p].id))return RAVET_VST3_INVALID;
        Batch* batch=nullptr;const auto admission=parameterPool_->prepare(block.frames,points,count,batch);
        if(admission!=Admission::Ok)return admission==Admission::Capacity?RAVET_VST3_CAPACITY:
            admission==Admission::Busy?RAVET_VST3_LIFECYCLE:RAVET_VST3_INVALID;
        for(uint32_t ch=0;ch<profile_.inputs;++ch)if(block.frames)std::copy_n(block.inputs[ch].samples,block.frames,inputPointers_[ch]);
        bool threw=false;const auto result=process(block.frames,batch,&threw);const auto failure=batch->freeze();
        if(failure){fault_=true;for(uint32_t ch=0;ch<profile_.outputs;++ch)std::fill_n(outputPointers_[ch],block.frames,0.f);}
        if(failure||threw||result!=Steinberg::kResultOk)parametersCompatible_=false;
        for(uint32_t ch=0;ch<profile_.outputs;++ch)if(block.frames)std::copy_n(outputPointers_[ch],block.frames,block.outputs[ch].samples);
        report={sizeof(report),8,failure,0,uint32_t(RAVET_VST3_PARAMETER_ATTEMPTED|
            (threw?RAVET_VST3_PARAMETER_THROWN:RAVET_VST3_PARAMETER_RETURNED)|
            (parametersCompatible_?0:RAVET_VST3_PARAMETER_INVALIDATED)),result};
        return result==Steinberg::kResultOk&&!failure?RAVET_VST3_OK:RAVET_VST3_NATIVE;
    }
    // Native processing companion: paired immutable input reservations and
    // atomic owned event/parameter output publication. Outer provider adds its
    // exact core/facade/keeper exclusions to this pure boundary before entry.
    int32_t processCompleteBlock(const RavetVst3AudioBlock& block,
            const parameters_draft::Point* points,uint32_t pointCount,
            const RavetVst3EventRecord* events,uint32_t eventCount,
            const uint8_t* payload,uint32_t payloadBytes,const RavetVst3ContextRecord* context,
            RavetVst3ProcessingOutputs& outputs,RavetVst3ProcessingReport& report,
            const parameters_draft::Range* outerRanges=nullptr,size_t outerCount=0) noexcept {
        using namespace processing_draft;
        if(!processingReady_||!processingCompatible()||!parameterPool_||!processingPool_||
           (audio_!=std::thread::id{}&&audio_!=std::this_thread::get_id()))return RAVET_VST3_LIFECYCLE;
        ProcessingBoundary boundary;
        if(!boundary.admit(block,points,pointCount,events,eventCount,payload,payloadBytes,context,outputs,report,
            [&](const void* p,size_t bytes,size_t alignment){return stateExternal(p,bytes,alignment)&&
                parameterPool_->externalRange(p,bytes,alignment)&&processingPool_->externalRange(p,bytes,alignment);})||
           !validateBlock(block,&report,sizeof(report))||outerCount>128||(!outerRanges&&outerCount))return RAVET_VST3_INVALID;
        for(uint32_t p=0;p<pointCount;++p)if(!parameterCatalogue_.writable(points[p].id))return RAVET_VST3_INVALID;
        Steinberg::Vst::ProcessContext nativeContext{};
        if(context&&!sdkContext(*context,profile_.rate,nativeContext))return RAVET_VST3_INVALID;
        // Numeric snapshots protect immutable owner/scratch/catalogue and both
        // pools from SDK callback destination aliases. No owner pointer is
        // dereferenced by an escaped list after this Instance retires.
        const auto callerRangeCount=boundary.count();
        auto protect=[&](const void* p,size_t bytes,size_t alignment){parameters_draft::Range r{};
            return parameters_draft::range(p,bytes,1,alignment,r)&&boundary.append(r);};
        if(!protect(this,sizeof(*this),alignof(Instance))||
           !protect(parameterPool_,sizeof(*parameterPool_),alignof(parameters_draft::Pool))||
           !protect(processingPool_,sizeof(*processingPool_),alignof(EventPool)))return RAVET_VST3_CAPACITY;
        if(!currentScratch_.appendRanges(protect)||!pendingScratch_.appendRanges(protect))return RAVET_VST3_CAPACITY;
        std::array<parameters_draft::Range,2> parameterMetadata{};
        std::array<parameters_draft::Range,33> eventMetadata{};
        if(!parameterCatalogue_.protectionRanges(parameterMetadata))return RAVET_VST3_INVALID;
        for(auto r:parameterMetadata)if(!boundary.append(r))return RAVET_VST3_CAPACITY;
        const auto metadataCount=processingCatalogue_.protectionRanges(eventMetadata);
        if(!metadataCount)return RAVET_VST3_INVALID;
        for(size_t i=0;i<metadataCount;++i)if(!boundary.append(eventMetadata[i]))return RAVET_VST3_CAPACITY;
        for(size_t i=0;i<outerCount;++i){
            // Whole original caller storage must exclude every outer owner.
            if(!boundary.appendOwner(outerRanges[i],callerRangeCount))return RAVET_VST3_INVALID;
        }
        parameters_draft::Pool::Plan parameterPlan;EventPool::Plan eventPlan;
        const auto parameterAdmission=parameterPool_->preflight(block.frames,points,pointCount,parameterPlan,boundary.ranges(),boundary.count());
        if(parameterAdmission!=parameters_draft::Admission::Ok)return parameterAdmission==parameters_draft::Admission::Capacity?RAVET_VST3_CAPACITY:RAVET_VST3_INVALID;
        const auto inputBuses=processingCatalogue_.selectedBuses(false),outputBuses=processingCatalogue_.selectedBuses(true);
        const auto eventAdmission=processingPool_->preflight(block.frames,events,eventCount,payload,payloadBytes,inputBuses,outputBuses,eventPlan,boundary.ranges(),boundary.count());
        if(eventAdmission!=EventAdmission::Ok)return eventAdmission==EventAdmission::Capacity?RAVET_VST3_CAPACITY:RAVET_VST3_INVALID;
        parameters_draft::Pool::Reservation parameterReservation;EventPool::Reservation eventReservation;
        const auto reserveParameter=parameterPool_->reserve(parameterReservation);
        if(reserveParameter!=parameters_draft::Admission::Ok)return reserveParameter==parameters_draft::Admission::Capacity?RAVET_VST3_CAPACITY:RAVET_VST3_LIFECYCLE;
        const auto reserveEvent=processingPool_->reserve(eventReservation);
        if(reserveEvent!=EventAdmission::Ok)return reserveEvent==EventAdmission::Capacity?RAVET_VST3_CAPACITY:RAVET_VST3_LIFECYCLE;
        // Both reservations succeeded: activation is nonthrowing and performs
        // the first queue/input mutation. A failed second reservation above
        // cancels the first without copying input or clearing caller outputs.
        auto& parameterBatch=parameterPool_->activate(parameterPlan,parameterReservation);
        auto& eventBatch=processingPool_->activate(eventPlan,eventReservation);
        for(uint32_t ch=0;ch<profile_.inputs;++ch)if(block.frames)std::copy_n(block.inputs[ch].samples,block.frames,inputPointers_[ch]);
        bool threw=false,attempted=false;
        const auto native=process(block.frames,&parameterBatch,&threw,&eventBatch,context?&nativeContext:nullptr,&attempted);
        const auto parameterFailure=parameterBatch.freeze(),eventFailure=eventBatch.freeze();
        RavetVst3ProcessingReport staged{};staged.struct_size=sizeof(staged);
        staged.stage=RAVET_VST3_PROCESSING_STAGE_PROCESS;staged.index=UINT32_MAX;
        staged.input_parameter_failure=parameterFailure;staged.output_event_failure=eventFailure;
        if(attempted){staged.flags=RAVET_VST3_PROCESSING_ATTEMPTED|RAVET_VST3_PROCESSING_HISTORY_MAY_HAVE_ADVANCED|
            (threw?RAVET_VST3_PROCESSING_THROWN:RAVET_VST3_PROCESSING_RETURNED);if(!threw)staged.native_result=native;}
        bool success=attempted&&!threw&&native==Steinberg::kResultOk&&!parameterFailure&&!eventFailure;
        if(!success)staged.failure=threw?RAVET_VST3_PROCESSING_FAILURE_THROWN:
            parameterFailure||eventFailure?RAVET_VST3_PROCESSING_FAILURE_PROTOCOL:RAVET_VST3_PROCESSING_FAILURE_NATIVE;
        if(success&&!eventBatch.copyOutputCombined(outputs)){
            success=false;staged.stage=RAVET_VST3_PROCESSING_STAGE_PUBLICATION;
            staged.failure=RAVET_VST3_PROCESSING_FAILURE_PUBLICATION;
        }
        if(success){staged.flags|=RAVET_VST3_PROCESSING_PUBLISHED;
            staged.event_count=outputs.event_count;staged.payload_used=outputs.payload_used;
            staged.parameter_queue_count=outputs.parameter_queue_count;staged.parameter_point_count=outputs.parameter_point_count;
        }else{
            fault_=true;ready_=false;processingReady_=false;processingCompatible_=false;parametersCompatible_=false;
            staged.flags|=RAVET_VST3_PROCESSING_INVALIDATED;
            for(uint32_t ch=0;ch<profile_.outputs;++ch)std::fill_n(outputPointers_[ch],block.frames,0.f);
        }
        for(uint32_t ch=0;ch<profile_.outputs;++ch)if(block.frames)std::copy_n(outputPointers_[ch],block.frames,block.outputs[ch].samples);
        report=staged;
        return success?RAVET_VST3_OK:staged.failure==RAVET_VST3_PROCESSING_FAILURE_PUBLICATION?RAVET_VST3_CAPACITY:RAVET_VST3_NATIVE;
    }
    template<class Sample,class Block,class Boundary> int32_t processHostedBlock(const Block& block,
            const parameters_draft::Point* points,uint32_t pointCount,
            const RavetVst3EventRecord* events,uint32_t eventCount,
            const uint8_t* payload,uint32_t payloadBytes,const RavetVst3ContextRecord* context,
            RavetVst3ProcessingOutputs& outputs,RavetVst3ProcessingReport& report,
            const parameters_draft::Range* outerRanges=nullptr,size_t outerCount=0) noexcept {
        using namespace processing_draft;
        if(!hostedPrepared()||!processingCompatible()||!attached_||!running_||!parameterPool_||!processingPool_||
           (audio_!=std::thread::id{}&&audio_!=std::this_thread::get_id()))return RAVET_VST3_LIFECYCLE;
        if(hostedProfile_.sample_bits!=sizeof(Sample)*8||block.frames>hostedProfile_.maximum_frames||
           block.input_channels!=hostedLayout_.inputChannels||block.output_channels!=hostedLayout_.outputChannels||
           (!block.input_channels&&block.inputs)||(!block.output_channels&&block.outputs))return RAVET_VST3_INVALID;
        Boundary boundary;
        if(!boundary.admit(block,points,pointCount,events,eventCount,payload,payloadBytes,context,outputs,report,
            [&](const void* p,size_t bytes,size_t alignment){return stateExternal(p,bytes,alignment)&&
                parameterPool_->externalRange(p,bytes,alignment)&&processingPool_->externalRange(p,bytes,alignment);})||
           outerCount>HostedOuterRanges||(!outerRanges&&outerCount))return RAVET_VST3_INVALID;
        for(uint32_t p=0;p<pointCount;++p)if(!parameterCatalogue_.writable(points[p].id))return RAVET_VST3_INVALID;
        Steinberg::Vst::ProcessContext nativeContext{};
        if(context&&!sdkContext(*context,profile_.rate,nativeContext))return RAVET_VST3_INVALID;
        // Numeric snapshots protect immutable owner/scratch/catalogue and both
        // pools from SDK callback destination aliases. No owner pointer is
        // dereferenced by an escaped list after this Instance retires.
        const auto callerRangeCount=boundary.count();
        auto protect=[&](const void* p,size_t bytes,size_t alignment){parameters_draft::Range r{};
            return parameters_draft::range(p,bytes,1,alignment,r)&&boundary.append(r);};
        if(!protect(this,sizeof(*this),alignof(Instance))||
           !protect(parameterPool_,sizeof(*parameterPool_),alignof(parameters_draft::Pool))||
           !protect(processingPool_,sizeof(*processingPool_),alignof(EventPool)))return RAVET_VST3_CAPACITY;
        if(!currentScratch_.appendRanges(protect)||!pendingScratch_.appendRanges(protect))return RAVET_VST3_CAPACITY;
        std::array<parameters_draft::Range,2> parameterMetadata{};
        std::array<parameters_draft::Range,33> eventMetadata{};
        if(!parameterCatalogue_.protectionRanges(parameterMetadata))return RAVET_VST3_INVALID;
        for(auto r:parameterMetadata)if(!boundary.append(r))return RAVET_VST3_CAPACITY;
        const auto metadataCount=processingCatalogue_.protectionRanges(eventMetadata);
        if(!metadataCount)return RAVET_VST3_INVALID;
        for(size_t i=0;i<metadataCount;++i)if(!boundary.append(eventMetadata[i]))return RAVET_VST3_CAPACITY;
        for(size_t i=0;i<outerCount;++i){
            // Whole original caller storage must exclude every outer owner.
            if(!boundary.appendOwner(outerRanges[i],callerRangeCount))return RAVET_VST3_INVALID;
        }
        parameters_draft::Pool::Plan parameterPlan;EventPool::Plan eventPlan;
        const auto parameterAdmission=parameterPool_->preflight(block.frames,points,pointCount,parameterPlan,boundary.ranges(),boundary.count(),256);
        if(parameterAdmission!=parameters_draft::Admission::Ok)return parameterAdmission==parameters_draft::Admission::Capacity?RAVET_VST3_CAPACITY:RAVET_VST3_INVALID;
        const auto inputBuses=processingCatalogue_.selectedBuses(false),outputBuses=processingCatalogue_.selectedBuses(true);
        const auto eventAdmission=processingPool_->preflight(block.frames,events,eventCount,payload,payloadBytes,inputBuses,outputBuses,eventPlan,boundary.ranges(),boundary.count(),256);
        if(eventAdmission!=EventAdmission::Ok)return eventAdmission==EventAdmission::Capacity?RAVET_VST3_CAPACITY:RAVET_VST3_INVALID;
        parameters_draft::Pool::Reservation parameterReservation;EventPool::Reservation eventReservation;
        const auto reserveParameter=parameterPool_->reserve(parameterReservation);
        if(reserveParameter!=parameters_draft::Admission::Ok)return reserveParameter==parameters_draft::Admission::Capacity?RAVET_VST3_CAPACITY:RAVET_VST3_LIFECYCLE;
        const auto reserveEvent=processingPool_->reserve(eventReservation);
        if(reserveEvent!=EventAdmission::Ok)return reserveEvent==EventAdmission::Capacity?RAVET_VST3_CAPACITY:RAVET_VST3_LIFECYCLE;
        // Both reservations succeeded: activation is nonthrowing and performs
        // the first queue/input mutation. A failed second reservation above
        // cancels the first without copying input or clearing caller outputs.
        auto& parameterBatch=parameterPool_->activate(parameterPlan,parameterReservation);
        auto& eventBatch=processingPool_->activate(eventPlan,eventReservation);
        for(uint32_t ch=0;ch<hostedLayout_.inputChannels;++ch)if(block.frames)std::copy_n(block.inputs[ch].samples,block.frames,currentScratch_.input<Sample>(ch));
        bool threw=false,attempted=false;
        const auto native=processHosted<Sample>(block.frames,parameterBatch,eventBatch,context?&nativeContext:nullptr,attempted,threw);
        const auto parameterFailure=parameterBatch.freeze(),eventFailure=eventBatch.freeze();
        RavetVst3ProcessingReport staged{};staged.struct_size=sizeof(staged);
        staged.stage=RAVET_VST3_PROCESSING_STAGE_PROCESS;staged.index=UINT32_MAX;
        staged.input_parameter_failure=parameterFailure;staged.output_event_failure=eventFailure;
        if(attempted){staged.flags=RAVET_VST3_PROCESSING_ATTEMPTED|RAVET_VST3_PROCESSING_HISTORY_MAY_HAVE_ADVANCED|
            (threw?RAVET_VST3_PROCESSING_THROWN:RAVET_VST3_PROCESSING_RETURNED);if(!threw)staged.native_result=native;}
        bool success=attempted&&!threw&&native==Steinberg::kResultOk&&!parameterFailure&&!eventFailure;
        if(!success)staged.failure=threw?RAVET_VST3_PROCESSING_FAILURE_THROWN:
            parameterFailure||eventFailure?RAVET_VST3_PROCESSING_FAILURE_PROTOCOL:RAVET_VST3_PROCESSING_FAILURE_NATIVE;
        if(success&&!eventBatch.copyOutputCombined(outputs)){
            success=false;staged.stage=RAVET_VST3_PROCESSING_STAGE_PUBLICATION;
            staged.failure=RAVET_VST3_PROCESSING_FAILURE_PUBLICATION;
        }
        if(success){staged.flags|=RAVET_VST3_PROCESSING_PUBLISHED;
            staged.event_count=outputs.event_count;staged.payload_used=outputs.payload_used;
            staged.parameter_queue_count=outputs.parameter_queue_count;staged.parameter_point_count=outputs.parameter_point_count;
        }else{
            fault_=true;ready_=false;processingReady_=false;processingCompatible_=false;parametersCompatible_=false;
            staged.flags|=RAVET_VST3_PROCESSING_INVALIDATED;
            for(uint32_t ch=0;ch<hostedLayout_.outputChannels;++ch)std::fill_n(currentScratch_.output<Sample>(ch),block.frames,Sample(0));
        }
        for(uint32_t ch=0;ch<hostedLayout_.outputChannels;++ch)if(block.frames)std::copy_n(currentScratch_.output<Sample>(ch),block.frames,block.outputs[ch].samples);
        report=staged;
        return success?RAVET_VST3_OK:staged.failure==RAVET_VST3_PROCESSING_FAILURE_PUBLICATION?RAVET_VST3_CAPACITY:RAVET_VST3_NATIVE;
    }
    bool attached()const noexcept{return attached_;}
    // Borrowed MAIN-only controller for the editor companion. The provider
    // refuses child destruction while an editor exists, so the view is always
    // released before handler clearing, terminate and interface release.
    Steinberg::Vst::IEditController* editController()const noexcept{return controllerInitialized_||componentInitialized_?controller_.get():nullptr;}
    float* input(uint32_t index) {main();if(hostedMode_||attached_||index>=profile_.inputs)throw InstanceFailure{Stage::Profile,Steinberg::kInvalidArgument};return inputPointers_[index];}
    const float* output(uint32_t index) {main();if(hostedMode_||attached_||index>=profile_.outputs)throw InstanceFailure{Stage::Profile,Steinberg::kInvalidArgument};return outputPointers_[index];}
    uint32_t restartRequests()const{main();return handler_->requests();}
    // Edit-gesture companion (decision 0276). CONTROL on MAIN: needs a live
    // controller, a refreshed catalogue and no AUDIO lease of this child.
    // Snapshots the writable ids exactly as processHostedBlock admits points.
    void enableGestures() {
        main();
        if(!controller_||!parametersCompatible_||attached_||fault_||!(controllerInitialized_||componentInitialized_))
            throw InstanceFailure{Stage::Lifecycle,Steinberg::kInvalidArgument};
        std::array<uint32_t,parameters_draft::ParameterCatalogue::MaximumParameters> ids{};size_t used=0;
        for(const auto& parameter:parameterCatalogue_.parameters())if(parameter.writable())ids[used++]=parameter.id;
        struct View{const uint32_t* first;size_t count;const uint32_t* begin()const{return first;}const uint32_t* end()const{return first+count;}size_t size()const{return count;}};
        if(!handler_->enableGestures(View{ids.data(),used}))throw InstanceFailure{Stage::Lifecycle,Steinberg::kInvalidArgument};
    }
    bool gesturesEnabled()const noexcept{return handler_->gestures();}
    uint32_t closeOpenGestures()noexcept{return handler_->closeOpenGestures();}
    // Caller gesture outputs exclude the handler ring and every owned extent.
    bool gestureExternal(const void* pointer,size_t bytes,size_t alignment)const noexcept {
        Range requested{},handler{};
        if(!range(pointer,bytes,1,alignment,requested)||!range(handler_,1,sizeof(ControllerHandler),alignof(ControllerHandler),handler)||
           overlaps(requested,handler))return false;
        return stateExternal(pointer,bytes,alignment);
    }
    uint32_t drainGestures(RavetVst3EditGesture* output,uint32_t capacity){main();return handler_->drainGestures(output,capacity);}
    RavetVst3EditGestureState gestureState()const{main();return handler_->gestureState();}
    InstanceFailure stop() {
        main();if(attached_)throw InstanceFailure{Stage::Lifecycle,Steinberg::kInvalidArgument};
        InstanceFailure first{Stage::Stop,Steinberg::kResultOk};
        if(running_){running_=false;cleanup([&]{return processor_->setProcessing(false);},Stage::Stop,first);}
        if(active_){active_=false;cleanup([&]{return component_->setActive(false);},Stage::Deactivate,first);}
        if(first.origin!=FailureOrigin::Local||first.native!=Steinberg::kResultOk)fault_=true;
        return first;
    }
    InstanceFailure retire() {
        main();if(attached_)throw InstanceFailure{Stage::Lifecycle,Steinberg::kInvalidArgument};
        handler_->retire();
        auto first=stop();
        using namespace Steinberg::Vst;
        deactivateEventBuses(first);deactivateHostedBuses(first);
        if(outputActive_){outputActive_=false;cleanup([&]{return component_->activateBus(kAudio,kOutput,0,false);},Stage::Deactivate,first);}
        if(inputActive_){inputActive_=false;cleanup([&]{return component_->activateBus(kAudio,kInput,0,false);},Stage::Deactivate,first);}
        if(toComponent_){toComponent_=false;cleanup([&]{return controllerPoint_->disconnect(componentPoint_);},Stage::Disconnect,first);}
        if(toController_){toController_=false;cleanup([&]{return componentPoint_->disconnect(controllerPoint_);},Stage::Disconnect,first);}
        if(handlerAttempted_){handlerAttempted_=false;cleanup([&]{return controller_->setComponentHandler(nullptr);},Stage::Handler,first);}
        handlers_->observeRetired(*handler_);
        if(controllerInitialized_){controllerInitialized_=false;cleanup([&]{return controller_->terminate();},Stage::Terminate,first);}
        if(componentInitialized_){componentInitialized_=false;cleanup([&]{return component_->terminate();},Stage::Terminate,first);}
        controllerPoint_=nullptr;componentPoint_=nullptr;controller_=nullptr;processor_=nullptr;component_=nullptr;
        ready_=false;fault_=true;
        return first;
    }
    ~Instance() {
        // This internal owner is CONTROL-only, unlike a process capability.
        // Enforce rather than silently invoking UI cleanup on an audio thread.
        if(std::this_thread::get_id()!=main_||!pthread_main_np()||attached_)std::terminate();
        (void)retire();
        handlers_->detach(*handler_);
        // Fields release in reverse declaration order, with core_ last.
    }
};
}
#endif
