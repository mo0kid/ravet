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
    std::string fail;
    std::vector<std::string> calls;
    int destroyed=0,initialized=0,terminated=0,processes=0;
    bool throwQuery=false,throwCreate=false,throwSetup=false;
    bool badInfo=false,refuseEdit=false,throwEdit=false,retainQueues=false,protocolFault=false,throwProcess=false;
    double controllerValue=.25,dspValue=.25;
    std::array<IParamValueQueue*,16> retained{};size_t retainedCount=0;
    std::function<void()> duringProcess;
    tresult call(const char* name){calls.emplace_back(name);if(throwSetup&&std::strcmp(name,"setup")==0)throw std::runtime_error("setup exception");return fail==name?rejected:kResultOk;}
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
        else if(FUnknownPrivate::iidEqual(id,IEditController::iid))*out=static_cast<IEditController*>(this);
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
    int32 PLUGIN_API getBusCount(MediaType type,BusDirection)override{
        require(state_.initialized==1);return type==kAudio?(state_.fail=="bus-count"?17:1):0;
    }
    tresult PLUGIN_API getBusInfo(MediaType type,BusDirection direction,int32 index,BusInfo& info)override{
        require(index==0);info={};info.mediaType=type;info.direction=direction;info.channelCount=int32(channels_);info.busType=kMain;info.name[0]='x';return state_.call("bus-info");
    }
    tresult PLUGIN_API getRoutingInfo(RoutingInfo&,RoutingInfo&)override{return kNotImplemented;}
    tresult PLUGIN_API activateBus(MediaType,BusDirection direction,int32,TBool value)override{
        require(pthread_main_np());return state_.call(value?(direction==kInput?"input-bus":"output-bus"):(direction==kInput?"input-bus-off":"output-bus-off"));
    }
    tresult PLUGIN_API setActive(TBool value)override{require(pthread_main_np());return state_.call(value?"active":"deactivate");}
    tresult PLUGIN_API setState(IBStream*)override{return kNotImplemented;}
    tresult PLUGIN_API getState(IBStream*)override{return kNotImplemented;}
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
        if(state_.duringProcess)state_.duringProcess();
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
        for(int32 ch=0;ch<data.outputs[0].numChannels;++ch)std::fill_n(data.outputs[0].channelBuffers32[ch],data.numSamples,float(state_.dspValue));
        if(state_.throwProcess)throw std::runtime_error("component process exception");
        return state_.fail=="process"?rejected:kResultOk;
    }
    uint32 PLUGIN_API getTailSamples()override{return 0;}
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
        if(handler){require(handler->restartComponent(kLatencyChanged|kIoChanged)==kResultOk);require(handler->beginEdit(1)==kNotImplemented);}
        return state_.call(handler?"handler":"handler-clear");
    }
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
