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
void requireAt(bool value,int line){if(!value)throw std::runtime_error("state fixture assertion line "+std::to_string(line));}
#define require(value) requireAt(bool(value),__LINE__)
struct Observation {
    std::string fail;
    std::vector<std::string> calls;
    int destroyed=0,initialized=0,terminated=0,processes=0;
    bool throwQuery=false,throwCreate=false;
    int controllerMode=0; // 0combined,1separate,2absent
    bool unsupported=false,empty=false,retaining=false,throwState=false,swallow=false;
    uint32_t failState=0,captured=0,restored=0;
    std::array<uint8_t,4> component{{11,12,13,14}},controller{{21,22,23,24}},mirrored{};
    std::vector<uint32_t> stateStages;
    std::vector<IBStream*> retained;
    std::function<void()> reentry;
    std::function<void(const char*)> controlCall;
    tresult stateCall(uint32_t stage,IBStream* stream) {
        require(stream&&pthread_main_np());stateStages.push_back(stage);
        if(retaining){stream->addRef();retained.push_back(stream);}
        if(reentry)reentry();
        if(stage<=2) {
            if(stage==2&&unsupported)return kNotImplemented;
            const auto& source=stage==1?component:controller;
            if(swallow&&stage==failState) {std::array<uint8_t,8> tooLarge{};(void)stream->write(tooLarge.data(),8);}
            else if(!empty)require(stream->write(const_cast<uint8_t*>(source.data()),4)==kResultOk);
        } else {
            int64 position=-1;require(stream->tell(&position)==kResultOk&&position==0);
            std::array<uint8_t,4> data{};int32 count=-1;
            require(stream->read(data.data(),empty?0:4,&count)==kResultOk&&count==(empty?0:4));
            uint8_t sentinel=77;require(stream->read(&sentinel,1,&count)==kResultOk&&count==0&&sentinel==77);
            if(stage==3)component=data;else if(stage==4)mirrored=data;else controller=data;
            if(swallow&&stage==failState) {uint8_t byte=0;(void)stream->write(&byte,1);}
        }
        if(stage==failState&&throwState)throw std::runtime_error("state after stream effects");
        return stage==failState&&!swallow?rejected:kResultOk;
    }
    tresult call(const char* name){calls.emplace_back(name);if(controlCall)controlCall(name);return fail==name?rejected:kResultOk;}
    int count(const char* name)const{return int(std::count(calls.begin(),calls.end(),name));}
};
// Valid SDK objects, including a real same-object edit controller. No invalid
// pointers or foreign isolation are simulated. All owner callbacks are checked.
class Component final:public IComponent,public IAudioProcessor,public IEditController {
    Observation& state_;
    uint32 refs_=1;
    uint32 channels_=2;
    IPtr<IComponentHandler> handler_;
public:
    explicit Component(Observation& state):state_(state){}
    ~Component(){++state_.destroyed;}
    tresult PLUGIN_API queryInterface(const TUID id,void** out)override{
        *out=nullptr;
        if(FUnknownPrivate::iidEqual(id,FUnknown::iid)||FUnknownPrivate::iidEqual(id,IComponent::iid))*out=static_cast<IComponent*>(this);
        else if(FUnknownPrivate::iidEqual(id,IAudioProcessor::iid))*out=static_cast<IAudioProcessor*>(this);
        else if(FUnknownPrivate::iidEqual(id,IEditController::iid)&&state_.controllerMode==0)*out=static_cast<IEditController*>(this);
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
    tresult PLUGIN_API getControllerClassId(TUID id)override{
        if(state_.controllerMode==1){std::memset(id,0,16);id[0]=2;return kResultOk;}return kResultFalse;
    }
    tresult PLUGIN_API setIoMode(IoMode)override{return kNotImplemented;}
    int32 PLUGIN_API getBusCount(MediaType type,BusDirection)override{
        require(state_.initialized>=1);return type==kAudio?(state_.fail=="bus-count"?17:1):0;
    }
    tresult PLUGIN_API getBusInfo(MediaType type,BusDirection direction,int32 index,BusInfo& info)override{
        require(index==0);info={};info.mediaType=type;info.direction=direction;info.channelCount=int32(channels_);info.busType=kMain;info.name[0]='x';return state_.call("bus-info");
    }
    tresult PLUGIN_API getRoutingInfo(RoutingInfo&,RoutingInfo&)override{return kNotImplemented;}
    tresult PLUGIN_API activateBus(MediaType,BusDirection direction,int32,TBool value)override{
        require(pthread_main_np());return state_.call(value?(direction==kInput?"input-bus":"output-bus"):(direction==kInput?"input-bus-off":"output-bus-off"));
    }
    tresult PLUGIN_API setActive(TBool value)override{require(pthread_main_np());return state_.call(value?"active":"deactivate");}
    tresult PLUGIN_API setState(IBStream* stream)override{
        const auto stage=state_.controllerMode==0&&state_.restored++?5u:3u;return state_.stateCall(stage,stream);
    }
    tresult PLUGIN_API getState(IBStream* stream)override{
        const auto stage=state_.controllerMode==0&&state_.captured++?2u:1u;return state_.stateCall(stage,stream);
    }
    tresult PLUGIN_API setBusArrangements(SpeakerArrangement* inputs,int32 ins,SpeakerArrangement* outputs,int32 outs)override{
        require(ins==1&&outs==1&&inputs&&outputs&&inputs[0]==outputs[0]);channels_=uint32(SpeakerArr::getChannelCount(outputs[0]));return state_.call("arrangement");
    }
    tresult PLUGIN_API getBusArrangement(BusDirection,int32,SpeakerArrangement& out)override{out=channels_==1?SpeakerArr::kMono:SpeakerArr::kStereo;return kResultOk;}
    tresult PLUGIN_API canProcessSampleSize(int32 size)override{require(size==kSample32);return state_.call("precision");}
    uint32 PLUGIN_API getLatencySamples()override{return 0;}
    tresult PLUGIN_API setupProcessing(ProcessSetup& setup)override{require(pthread_main_np()&&setup.sampleRate==48000);return state_.call("setup");}
    tresult PLUGIN_API setProcessing(TBool value)override{return state_.call(value?"start":"stop");}
    tresult PLUGIN_API process(ProcessData& data)override{
        require(!pthread_main_np()&&data.processContext==nullptr&&data.numInputs==1&&data.numOutputs==1);++state_.processes;
        for(int32 ch=0;ch<data.outputs[0].numChannels;++ch)std::fill_n(data.outputs[0].channelBuffers32[ch],data.numSamples,.75f);
        return state_.fail=="process"?rejected:kResultOk;
    }
    uint32 PLUGIN_API getTailSamples()override{return 0;}
    tresult PLUGIN_API setComponentState(IBStream* stream)override{return state_.stateCall(4,stream);}
    int32 PLUGIN_API getParameterCount()override{return 0;}
    tresult PLUGIN_API getParameterInfo(int32,ParameterInfo&)override{return kResultFalse;}
    tresult PLUGIN_API getParamStringByValue(ParamID,ParamValue,String128)override{return kResultFalse;}
    tresult PLUGIN_API getParamValueByString(ParamID,TChar*,ParamValue&)override{return kResultFalse;}
    ParamValue PLUGIN_API normalizedParamToPlain(ParamID,ParamValue value)override{return value;}
    ParamValue PLUGIN_API plainParamToNormalized(ParamID,ParamValue value)override{return value;}
    ParamValue PLUGIN_API getParamNormalized(ParamID)override{return 0;}
    tresult PLUGIN_API setParamNormalized(ParamID,ParamValue)override{return kNotImplemented;}
    tresult PLUGIN_API setComponentHandler(IComponentHandler* handler)override{
        handler_=handler;
        if(handler){require(handler->restartComponent(kLatencyChanged|kIoChanged)==kResultOk);require(handler->beginEdit(1)==kNotImplemented);}
        return state_.call(handler?"handler":"handler-clear");
    }
    IPlugView* PLUGIN_API createView(FIDString)override{return nullptr;}
};
class StateController final:public IEditController {
    Observation& state_;
    uint32_t references_=1;
    IPtr<IComponentHandler> handler_;
public:
    explicit StateController(Observation& state):state_(state){}
    ~StateController(){++state_.destroyed;}
    tresult PLUGIN_API queryInterface(const TUID id,void** output)override {
        *output=nullptr;
        if(FUnknownPrivate::iidEqual(id,FUnknown::iid)||FUnknownPrivate::iidEqual(id,IEditController::iid)) {
            *output=static_cast<IEditController*>(this);addRef();return kResultOk;
        }return kNoInterface;
    }
    uint32 PLUGIN_API addRef()override{return ++references_;}
    uint32 PLUGIN_API release()override{const auto n=--references_;if(!n)delete this;return n;}
    tresult PLUGIN_API initialize(FUnknown* host)override{require(host&&pthread_main_np());++state_.initialized;return kResultOk;}
    tresult PLUGIN_API terminate()override{require(!handler_&&pthread_main_np());++state_.terminated;return kResultOk;}
    tresult PLUGIN_API getState(IBStream* stream)override{return state_.stateCall(2,stream);}
    tresult PLUGIN_API setState(IBStream* stream)override{return state_.stateCall(5,stream);}
    tresult PLUGIN_API setComponentState(IBStream* stream)override{return state_.stateCall(4,stream);}
    int32 PLUGIN_API getParameterCount()override{return 0;}
    tresult PLUGIN_API getParameterInfo(int32,ParameterInfo&)override{return kResultFalse;}
    tresult PLUGIN_API getParamStringByValue(ParamID,ParamValue,String128)override{return kResultFalse;}
    tresult PLUGIN_API getParamValueByString(ParamID,TChar*,ParamValue&)override{return kResultFalse;}
    ParamValue PLUGIN_API normalizedParamToPlain(ParamID,ParamValue value)override{return value;}
    ParamValue PLUGIN_API plainParamToNormalized(ParamID,ParamValue value)override{return value;}
    ParamValue PLUGIN_API getParamNormalized(ParamID)override{return 0;}
    tresult PLUGIN_API setParamNormalized(ParamID,ParamValue)override{return kNotImplemented;}
    tresult PLUGIN_API setComponentHandler(IComponentHandler* handler)override{handler_=handler;return kResultOk;}
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
    tresult PLUGIN_API createInstance(FIDString id,FIDString,void** out)override{
        if(id[0]==2)*out=static_cast<IEditController*>(new StateController(state_));
        else *out=static_cast<IComponent*>(new Component(state_));
        if(state_.throwCreate)throw std::runtime_error("create after retained output");
        return state_.call("create");
    }
};
inline Catalogue metadata(){Catalogue c;Class item;item.category="Audio Module Class";item.id[0]=1;c.classes.push_back(item);item.category="Component Controller Class";item.id[0]=2;c.classes.push_back(item);return c;}
}
