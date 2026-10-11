#include "RavetVst3Instance.hpp"
#include "public.sdk/source/vst/hosting/hostclasses.h"
#include <iostream>
#include <stdexcept>
#include <string>
#include <functional>
using namespace Steinberg;
using namespace Steinberg::Vst;
using namespace ravet::vst3;
using namespace ravet::vst3::draft;
namespace {
constexpr tresult rejected=4242;
void requireAt(bool value,int line){if(!value){std::cerr<<"parameter transaction requirement line "<<line<<std::endl;throw std::runtime_error("parameter transaction requirement");}}
#define require(...) requireAt((__VA_ARGS__),__LINE__)
struct Observation {
    std::string fail,throwCall;
    std::function<void(const char*)> controlCall;
    std::function<void(IComponentHandler*)> handlerCall;
    std::vector<std::string> calls;
    int destroyed=0,initialized=0,terminated=0,processes=0;
    bool throwQuery=false,throwCreate=false,throwSetup=false;
    bool badInfo=false,refuseEdit=false,throwEdit=false,retainQueues=false,protocolFault=false,throwProcess=false;
    double controllerValue=.25,dspValue=.25;
    std::array<IParamValueQueue*,16> retained{};size_t retainedCount=0;
    std::function<void()> duringProcess;
    std::function<void(ProcessData&)> processing;
    int32 eventInputCount=2,eventOutputCount=2;
    bool hasMapping=true,hasRequirements=true,hasController=true,longEventNames=false;
    uint32 requirements=0;
    int32 audioInputCount=3,audioOutputCount=2;
    std::array<SpeakerArrangement,16> audioInputs{SpeakerArr::kStereo,SpeakerArr::kStereo,SpeakerArr::kStereo};
    std::array<SpeakerArrangement,16> audioOutputs{SpeakerArr::kStereo,SpeakerArr::kMono};
    std::array<bool,16> activeInputs{},activeOutputs{};
    int32 preparedBits=-1;uint32 latency=19,tail=37;
    std::vector<std::array<int32,3>> audioActivations;
    int mappingCalls=0;std::vector<std::array<int32,3>> eventActivations;
    int32 failedActivation=-1;bool throwingActivation=false;

    tresult call(const char* name){calls.emplace_back(name);if(controlCall)controlCall(name);if(throwCall==name)throw std::runtime_error("CONTROL exception");if(throwSetup&&std::strcmp(name,"setup")==0)throw std::runtime_error("setup exception");return fail==name?rejected:kResultOk;}
    int count(const char* name)const{return int(std::count(calls.begin(),calls.end(),name));}
};
// Valid SDK objects, including a real same-object edit controller. No invalid
// pointers or foreign isolation are simulated. All owner callbacks are checked.
class Component final:public IComponent,public IAudioProcessor,public IEditController,public IMidiMapping,public IProcessContextRequirements {
    Observation& state_;
    uint32 refs_=1;

    IPtr<IComponentHandler> handler_;
public:
    explicit Component(Observation& state):state_(state){}
    ~Component(){++state_.destroyed;}
    tresult PLUGIN_API queryInterface(const TUID id,void** out)override{
        *out=nullptr;
        if(FUnknownPrivate::iidEqual(id,FUnknown::iid)||FUnknownPrivate::iidEqual(id,IComponent::iid))*out=static_cast<IComponent*>(this);
        else if(FUnknownPrivate::iidEqual(id,IAudioProcessor::iid))*out=static_cast<IAudioProcessor*>(this);
        else if(FUnknownPrivate::iidEqual(id,IEditController::iid)&&state_.hasController)*out=static_cast<IEditController*>(this);
        else if(FUnknownPrivate::iidEqual(id,IMidiMapping::iid)&&state_.hasMapping)*out=static_cast<IMidiMapping*>(this);
        else if(FUnknownPrivate::iidEqual(id,IProcessContextRequirements::iid)&&state_.hasRequirements)*out=static_cast<IProcessContextRequirements*>(this);
        else return kNoInterface;
        addRef();
        if(FUnknownPrivate::iidEqual(id,IAudioProcessor::iid)){
            if(state_.throwQuery)throw std::runtime_error("query after retained output");
            return state_.call("query");
        }return kResultOk;
    }
    uint32 PLUGIN_API addRef()override{return ++refs_;}
    uint32 PLUGIN_API release()override{const auto value=--refs_;if(!value)delete this;return value;}
    tresult PLUGIN_API initialize(FUnknown* host)override{
        require(host&&pthread_main_np());const auto result=state_.call("initialize");if(result==kResultOk)++state_.initialized;return result;
    }
    tresult PLUGIN_API terminate()override{
        require(pthread_main_np()&&!handler_);++state_.terminated;return state_.call("terminate");
    }
    tresult PLUGIN_API getControllerClassId(TUID)override{return state_.call("controller-id")==kResultOk?kResultFalse:rejected;}
    tresult PLUGIN_API setIoMode(IoMode)override{return kNotImplemented;}
    int32 PLUGIN_API getBusCount(MediaType type,BusDirection direction)override{
        require(state_.initialized==1);if(state_.throwCall=="bus-count")throw std::runtime_error("bus count exception");return type==kAudio?(state_.fail=="bus-count"?17:(direction==kInput?state_.audioInputCount:state_.audioOutputCount)):(direction==kInput?state_.eventInputCount:state_.eventOutputCount);
    }
    tresult PLUGIN_API getBusInfo(MediaType type,BusDirection direction,int32 index,BusInfo& info)override{
        require(index>=0&&index<(type==kEvent?(direction==kInput?state_.eventInputCount:state_.eventOutputCount):(direction==kInput?state_.audioInputCount:state_.audioOutputCount)));info={};info.mediaType=type;info.direction=direction;
        info.channelCount=type==kEvent?(index?2:16):int32(std::bitset<64>((direction==kInput?state_.audioInputs:state_.audioOutputs)[size_t(index)]).count());info.busType=index?kAux:kMain;info.name[0]=TChar('x'+index);
        if(type==kEvent&&state_.longEventNames)for(size_t i=0;i<127;++i)info.name[i]=TChar(0x800+index);
        return state_.call("bus-info");
    }
    tresult PLUGIN_API getRoutingInfo(RoutingInfo&,RoutingInfo&)override{return kNotImplemented;}
    tresult PLUGIN_API activateBus(MediaType type,BusDirection direction,int32 index,TBool value)override{
        if(type==kEvent){
            state_.eventActivations.push_back({int32(direction),index,int32(value)});
            if(value&&index==state_.failedActivation){if(state_.throwingActivation)throw std::runtime_error("event activation");return rejected;}
            return kResultOk;
        }
        require(pthread_main_np());state_.audioActivations.push_back({int32(direction),index,int32(value)});
        (direction==kInput?state_.activeInputs:state_.activeOutputs)[size_t(index)]=bool(value);
        return state_.call(value?(direction==kInput?"input-bus":"output-bus"):(direction==kInput?"input-bus-off":"output-bus-off"));
    }
    tresult PLUGIN_API setActive(TBool value)override{require(pthread_main_np());return state_.call(value?"active":"deactivate");}
    tresult PLUGIN_API setState(IBStream*)override{return kNotImplemented;}
    tresult PLUGIN_API getState(IBStream*)override{return kNotImplemented;}
    tresult PLUGIN_API setBusArrangements(SpeakerArrangement* inputs,int32 ins,SpeakerArrangement* outputs,int32 outs)override {
        require(ins==state_.audioInputCount&&outs==state_.audioOutputCount);
        for(int32 i=0;i<ins;++i)state_.audioInputs[size_t(i)]=inputs[i];
        for(int32 i=0;i<outs;++i)state_.audioOutputs[size_t(i)]=outputs[i];
        return state_.call("arrangement");
    }
    tresult PLUGIN_API getBusArrangement(BusDirection direction,int32 index,SpeakerArrangement& out)override {
        out=(direction==kInput?state_.audioInputs:state_.audioOutputs)[size_t(index)];return state_.call("get-arrangement");
    }
    tresult PLUGIN_API canProcessSampleSize(int32 size)override{require(size==kSample32||size==kSample64);return state_.call(size==kSample32?"precision":"precision64");}
    uint32 PLUGIN_API getLatencySamples()override{state_.call("latency");return state_.latency;}
    tresult PLUGIN_API setupProcessing(ProcessSetup& setup)override{require(pthread_main_np()&&setup.sampleRate==48000);state_.preparedBits=setup.symbolicSampleSize;return state_.call("setup");}
    uint32 PLUGIN_API getTailSamples()override{state_.call("tail");return state_.tail;}
    tresult PLUGIN_API setProcessing(TBool value)override{return state_.call(value?"start":"stop");}
    tresult PLUGIN_API process(ProcessData& data)override{
        require(!pthread_main_np()&&data.numInputs==state_.audioInputCount&&data.numOutputs==state_.audioOutputCount&&data.symbolicSampleSize==state_.preparedBits);++state_.processes;
        for(bool output:{false,true})for(int32 i=0;i<(output?data.numOutputs:data.numInputs);++i) {
            auto& bus=(output?data.outputs:data.inputs)[i];const auto index=size_t(i);
            require(bus.numChannels==int32(std::bitset<64>((output?state_.audioOutputs:state_.audioInputs)[index]).count()));
            const bool active=(output?state_.activeOutputs:state_.activeInputs)[index];
            if(data.symbolicSampleSize==kSample32){require(bus.channelBuffers32);for(int32 ch=0;ch<bus.numChannels;++ch)require(bool(bus.channelBuffers32[ch])==active);}
            else {require(bus.channelBuffers64);for(int32 ch=0;ch<bus.numChannels;++ch)require(bool(bus.channelBuffers64[ch])==active);}
        }
        if(state_.duringProcess)state_.duringProcess();
        if(state_.processing)state_.processing(data);
        if(auto* changes=data.inputParameterChanges){
            const auto count=changes->getParameterCount();
            for(int32 q=0;q<count;++q){
                auto* queue=changes->getParameterData(q);require(queue);
                if(state_.retainQueues){require(state_.retainedCount<16);queue->addRef();state_.retained[state_.retainedCount++]=queue;}
                for(int32 p=0;p<queue->getPointCount();++p){int32 offset=-1;double value=-1;require(queue->getPoint(p,offset,value)==kResultOk);
                    if(queue->getParameterId()==1)state_.dspValue=value;
                }
                if(state_.protocolFault){int32 offset=77;double value=88;require(queue->getPoint(100,offset,value)==kInvalidArgument&&offset==77&&value==88);}
            }
        }
        for(int32 bus=0;bus<data.numOutputs;++bus)if(state_.activeOutputs[size_t(bus)])
            for(int32 ch=0;ch<data.outputs[bus].numChannels;++ch)for(int32 frame=0;frame<data.numSamples;++frame) {
                if(data.symbolicSampleSize==kSample32) {
                    const float main=data.numInputs&&state_.activeInputs[0]?data.inputs[0].channelBuffers32[ch%data.inputs[0].numChannels][frame]:0.f;
                    const float aux=data.numInputs>2&&state_.activeInputs[2]?data.inputs[2].channelBuffers32[ch%data.inputs[2].numChannels][frame]:0.f;
                    data.outputs[bus].channelBuffers32[ch][frame]=float(double(main)*state_.dspValue+double(aux)*.25);
                }else {
                    const double main=data.numInputs&&state_.activeInputs[0]?data.inputs[0].channelBuffers64[ch%data.inputs[0].numChannels][frame]:0;
                    const double aux=data.numInputs>2&&state_.activeInputs[2]?data.inputs[2].channelBuffers64[ch%data.inputs[2].numChannels][frame]:0;
                    data.outputs[bus].channelBuffers64[ch][frame]=main*state_.dspValue+aux*.25;
                }
            }
        if(state_.throwProcess)throw std::runtime_error("component process exception");
        return state_.fail=="process"?rejected:kResultOk;
    }
    tresult PLUGIN_API setComponentState(IBStream*)override{return kNotImplemented;}
    int32 PLUGIN_API getParameterCount()override{return 2;}
    tresult PLUGIN_API getParameterInfo(int32 i,ParameterInfo& p)override{
        require(pthread_main_np());if(i<0||i>1)return kResultFalse;
        p={};p.id=state_.badInfo?1:uint32(i+1);p.title[0]=i?'R':'L';p.defaultNormalizedValue=.25;
        p.flags=i?ParameterInfo::kIsReadOnly:ParameterInfo::kCanAutomate;return kResultOk;
    }
    tresult PLUGIN_API getParamStringByValue(ParamID,ParamValue,String128)override{return kResultFalse;}
    tresult PLUGIN_API getParamValueByString(ParamID,TChar*,ParamValue&)override{return kResultFalse;}
    ParamValue PLUGIN_API normalizedParamToPlain(ParamID,ParamValue value)override{return value;}
    ParamValue PLUGIN_API plainParamToNormalized(ParamID,ParamValue value)override{return value;}
    ParamValue PLUGIN_API getParamNormalized(ParamID)override{require(pthread_main_np());return state_.controllerValue;}
    tresult PLUGIN_API setParamNormalized(ParamID,ParamValue v)override{
        require(pthread_main_np());state_.controllerValue=v;
        if(state_.throwEdit)throw std::runtime_error("controller mutation exception");
        return state_.refuseEdit?rejected:kResultOk;
    }
    tresult PLUGIN_API setComponentHandler(IComponentHandler* handler)override{
        handler_=handler;
        if(state_.handlerCall)state_.handlerCall(handler);
        if(handler){require(handler->restartComponent(kLatencyChanged|kIoChanged)==kResultOk);require(handler->beginEdit(1)==kNotImplemented);}
        return state_.call(handler?"handler":"handler-clear");
    }
    tresult PLUGIN_API getMidiControllerAssignment(int32 bus,int16 channel,CtrlNumber control,ParamID& id)override{
        require(pthread_main_np()&&bus==1&&channel>=0&&channel<2&&control>=0&&control<130);
        ++state_.mappingCalls;if(control==7){id=1;return kResultOk;}id=kNoParamId;return kResultFalse;
    }
    uint32 PLUGIN_API getProcessContextRequirements()override{require(pthread_main_np());return state_.requirements;}
    IPlugView* PLUGIN_API createView(FIDString)override{return nullptr;}
};
class Factory final:public IPluginFactory {
    Observation& state_;
public:
    explicit Factory(Observation& state):state_(state){}
    tresult PLUGIN_API queryInterface(const TUID,void** out)override{*out=nullptr;return kNoInterface;}
    uint32 PLUGIN_API addRef()override{return 1;}
    uint32 PLUGIN_API release()override{return 1;}
    int32 PLUGIN_API countClasses()override{return 1;}
    tresult PLUGIN_API getFactoryInfo(PFactoryInfo*)override{return kNotImplemented;}
    tresult PLUGIN_API getClassInfo(int32,PClassInfo*)override{return kNotImplemented;}
    tresult PLUGIN_API createInstance(FIDString,FIDString,void** out)override{
        *out=static_cast<IComponent*>(new Component(state_));
        if(state_.throwCreate)throw std::runtime_error("create after retained output");
        return state_.call("create");
    }
};
Catalogue metadata(){Catalogue c;Class item;item.category="Audio Module Class";item.id[0]=1;c.classes.push_back(item);return c;}
}
