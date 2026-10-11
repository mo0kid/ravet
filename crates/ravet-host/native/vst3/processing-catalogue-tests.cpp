#include "ProcessingCatalogue.hpp"
#include "ParameterCatalogue.hpp"
#include <iostream>
#include <thread>
#include <stdexcept>
using namespace Steinberg;
using namespace Steinberg::Vst;
using namespace ravet::vst3::processing_draft;
namespace {
void require(bool ok,const char* message){if(!ok)throw std::runtime_error(message);}
struct Fixture final:IComponent,IMidiMapping,IProcessContextRequirements {
    int32 ins=2,outs=1;int counts=0,infos=0,mappings=0,contexts=0,activations=0;
    int badInfo=-1,badKind=0,badMap=-1;bool throwCount=false,throwInfo=false,throwMap=false,throwContext=false;
    uint32 requested=0;bool allUnassigned=false;std::array<int32,16> channels{};
    Fixture(){channels.fill(16);channels[1]=2;}
    tresult PLUGIN_API queryInterface(const TUID,void** out)override{*out=nullptr;return kNoInterface;}
    uint32 PLUGIN_API addRef()override{return 1;}
    uint32 PLUGIN_API release()override{return 1;}
    tresult PLUGIN_API initialize(FUnknown*)override{return kResultOk;}
    tresult PLUGIN_API terminate()override{return kResultOk;}
    tresult PLUGIN_API getControllerClassId(TUID)override{return kResultFalse;}
    tresult PLUGIN_API setIoMode(IoMode)override{return kResultOk;}
    int32 PLUGIN_API getBusCount(MediaType type,BusDirection direction)override{
        ++counts;require(type==kEvent,"event discovery only");if(throwCount)throw std::runtime_error("count");return direction==kInput?ins:outs;
    }
    tresult PLUGIN_API getBusInfo(MediaType type,BusDirection direction,int32 index,BusInfo& info)override{
        ++infos;if(throwInfo)throw std::runtime_error("info");info={};info.mediaType=type;info.direction=direction;
        info.channelCount=channels[uint32(index)];info.busType=index?kAux:kMain;info.flags=BusInfo::kDefaultActive;
        info.name[0]='B';info.name[1]=TChar('0'+index);info.name[2]=TChar(0xd83d);info.name[3]=TChar(0xde00);
        if(index==badInfo){switch(badKind){
            case 0:return 4242;case 1:info.mediaType=kAudio;break;case 2:info.direction=direction==kInput?kOutput:kInput;break;
            case 3:info.channelCount=17;break;case 4:info.busType=99;break;case 5:info.flags|=BusInfo::kIsControlVoltage;break;
            case 6:info.name[3]=0;break;case 7:std::fill_n(info.name,128,TChar('X'));break;default:info.name[0]=0;break;
        }}
        return kResultOk;
    }
    tresult PLUGIN_API getRoutingInfo(RoutingInfo&,RoutingInfo&)override{return kNotImplemented;}
    tresult PLUGIN_API activateBus(MediaType,BusDirection,int32,TBool)override{++activations;return kResultOk;}
    tresult PLUGIN_API setActive(TBool)override{return kResultOk;}
    tresult PLUGIN_API setState(IBStream*)override{return kNotImplemented;}
    tresult PLUGIN_API getState(IBStream*)override{return kNotImplemented;}
    tresult PLUGIN_API getMidiControllerAssignment(int32 bus,int16 channel,CtrlNumber controller,ParamID& id)override{
        const auto call=mappings++;require(bus>=0&&bus<ins&&channel>=0&&channel<channels[uint32(bus)]&&controller>=0&&controller<130,"bounded actual mapping call");
        if(call==badMap){if(throwMap)throw std::runtime_error("mapping");id=12345;return badKind==0?4242:kResultOk;}
        id=12345;if(allUnassigned)return kResultFalse;
        if(controller==0){id=0;return kResultOk;}
        if(controller==128){id=7;return kResultOk;}
        if(controller==129){id=UINT32_MAX-1;return kResultOk;}
        return kResultFalse;
    }
    uint32 PLUGIN_API getProcessContextRequirements()override{++contexts;if(throwContext)throw std::runtime_error("requirements");return requested;}
};
// Actual controller metadata is filtered using the same writable policy as
// provider refresh, before the independent processing-mapping admission.
struct PolicyController final:IEditController {
    uint32 policy=0;
    tresult PLUGIN_API queryInterface(const TUID,void** out)override{*out=nullptr;return kNoInterface;}
    uint32 PLUGIN_API addRef()override{return 1;}uint32 PLUGIN_API release()override{return 1;}
    tresult PLUGIN_API initialize(FUnknown*)override{return kResultOk;}tresult PLUGIN_API terminate()override{return kResultOk;}
    tresult PLUGIN_API setComponentState(IBStream*)override{return kNotImplemented;}
    tresult PLUGIN_API setState(IBStream*)override{return kNotImplemented;}tresult PLUGIN_API getState(IBStream*)override{return kNotImplemented;}
    int32 PLUGIN_API getParameterCount()override{return 3;}
    tresult PLUGIN_API getParameterInfo(int32 index,ParameterInfo& info)override{
        if(index<0||index>=3)return kInvalidArgument;info={};
        constexpr uint32_t values[]{0,7,UINT32_MAX-1};info.id=values[index];info.title[0]=TChar('A'+index);
        info.defaultNormalizedValue=.5;info.flags=index==1?policy:ParameterInfo::kCanAutomate;return kResultOk;
    }
    tresult PLUGIN_API getParamStringByValue(ParamID,ParamValue,String128)override{return kNotImplemented;}
    tresult PLUGIN_API getParamValueByString(ParamID,TChar*,ParamValue&)override{return kNotImplemented;}
    ParamValue PLUGIN_API normalizedParamToPlain(ParamID,ParamValue v)override{return v;}
    ParamValue PLUGIN_API plainParamToNormalized(ParamID,ParamValue v)override{return v;}
    ParamValue PLUGIN_API getParamNormalized(ParamID)override{return .5;}
    tresult PLUGIN_API setParamNormalized(ParamID,ParamValue)override{return kNotImplemented;}
    tresult PLUGIN_API setComponentHandler(IComponentHandler*)override{return kNotImplemented;}
    IPlugView* PLUGIN_API createView(FIDString)override{return nullptr;}
};
constexpr uint32_t ids[]{0,7,UINT32_MAX-1};
void copiedBusesMappingAndOptionalIdentity(){
    Fixture f;f.requested=2047;
    auto c=ProcessingCatalogue::inspect(f,&f,&f,ids,3,2,1);
    require(c.inputCount()==2&&c.outputCount()==1&&c.bus(false,1).name=="B1\xf0\x9f\x98\x80"&&c.bus(false,1).type==uint32(kAux),"copied indexed Unicode bus identity");
    require(c.selectedBuses(false)[0]==0&&c.selectedBuses(false)[1]==2&&c.selectedBuses(true)[0]==16,"explicit asymmetric selected channels");
    require(f.mappings==260&&f.activations==0&&c.hasMidiMapping()&&c.hasContextRequirements()&&c.contextRequirements()==2047,"CONTROL mapping only/no activation");
    for(uint32_t ch=0;ch<2;++ch)for(uint32_t ctrl=0;ctrl<130;++ctrl)
        require(c.assignment(1,ch,ctrl)==(ctrl==0?0:ctrl==128?7:ctrl==129?UINT32_MAX-1:kNoParamId),"independent all-controller identity oracle");
    f.channels[1]=16;f.requested=0;require(c.bus(false,1).channels==2&&c.contextRequirements()==2047,"native subsequent metadata cannot alter owned snapshot");
    Fixture absent;auto a=ProcessingCatalogue::inspect(absent,nullptr,nullptr,nullptr,0,0,0);
    require(!a.hasMidiMapping()&&!a.hasContextRequirements()&&a.contextRequirements()==0&&absent.mappings==0,"absence explicit");
    Fixture empty;empty.allUnassigned=true;auto e=ProcessingCatalogue::inspect(empty,&empty,&empty,nullptr,0,2,0);
    require(e.hasMidiMapping()&&e.hasContextRequirements()&&e.assignment(1,0,0)==kNoParamId&&e.contextRequirements()==0,"present-empty interface distinct/no false-output ID read");
}
void hiddenReadonlyMappingsRefuseWholeSelectedRefresh(){
    Fixture initial;auto committed=ProcessingCatalogue::inspect(initial,&initial,nullptr,ids,3,2,0);
    const auto originalName=committed.bus(false,1).name;
    for(uint32_t policy:{uint32_t(ParameterInfo::kIsReadOnly),uint32_t(ParameterInfo::kIsHidden|ParameterInfo::kIsReadOnly)}){
        PolicyController controller;controller.policy=policy;
        auto parameters=ravet::vst3::parameters_draft::ParameterCatalogue::inspect(controller);
        std::array<uint32_t,3> writable{};size_t count=0;
        for(const auto& parameter:parameters.parameters())if(parameter.writable())writable[count++]=parameter.id;
        require(count==2&&!parameters.writable(7)&&writable[0]==0&&writable[1]==UINT32_MAX-1,"actual hidden/readonly metadata excluded from writable mapping IDs");
        Fixture mapped;bool refused=false;
        try{committed=ProcessingCatalogue::inspect(mapped,&mapped,nullptr,writable.data(),uint32_t(count),2,0);}
        catch(const CatalogueFailure& error){refused=error.stage==CatalogueStage::Mapping&&error.kind==CatalogueError::Invalid&&!error.returned;}
        require(refused&&mapped.mappings==129&&mapped.activations==0&&committed.bus(false,1).name==originalName&&committed.assignment(1,1,128)==7,"selected hidden/readonly assignment rejects whole refresh preserving old catalogue");
        Fixture unselected;auto empty=ProcessingCatalogue::inspect(unselected,&unselected,nullptr,writable.data(),uint32_t(count),0,0);
        bool selectionRefused=false;try{(void)empty.assignment(1,0,128);}catch(const CatalogueFailure& error){selectionRefused=error.stage==CatalogueStage::Selection;}
        require(unselected.mappings==0&&unselected.activations==0&&empty.selectedBuses(false)[1]==0&&selectionRefused,"unselected mapping not queried or implicitly activated; lookup refuses by selection");
    }
}
void wholeFailuresAndOriginalProvenance(){
    Fixture base;auto committed=ProcessingCatalogue::inspect(base,&base,nullptr,ids,3,2,0);
    const auto old=committed.bus(false,1).name;
    const auto fail=[&](Fixture& f,CatalogueStage stage,CatalogueError kind,bool returned){
        bool refused=false;try{committed=ProcessingCatalogue::inspect(f,&f,&f,ids,3,2,0);}
        catch(const CatalogueFailure& e){refused=true;require(e.stage==stage&&e.kind==kind&&e.returned==returned,"failure stage/kind/returned provenance");if(returned)require(e.native==4242,"original status preserved");}
        require(refused&&committed.bus(false,1).name==old&&committed.assignment(1,1,129)==UINT32_MAX-1&&f.activations==0,"whole previous snapshot preserved");
    };
    for(int bad=0;bad<9;++bad){Fixture f;f.badInfo=1;f.badKind=bad;fail(f,CatalogueStage::BusInfo,bad==0?CatalogueError::Native:CatalogueError::Invalid,bad==0);}
    for(int kind=0;kind<3;++kind){Fixture f;f.badMap=259;f.badKind=kind;f.throwMap=kind==2;fail(f,CatalogueStage::Mapping,kind==0?CatalogueError::Native:kind==1?CatalogueError::Invalid:CatalogueError::Thrown,kind==0);}
    Fixture count;count.ins=17;fail(count,CatalogueStage::BusCount,CatalogueError::Invalid,false);
    Fixture negative;negative.outs=-1;fail(negative,CatalogueStage::BusCount,CatalogueError::Invalid,false);
    Fixture selection;selection.ins=1;fail(selection,CatalogueStage::Selection,CatalogueError::Invalid,false);
    Fixture requirements;requirements.requested=1u<<31;fail(requirements,CatalogueStage::ContextRequirements,CatalogueError::Invalid,false);
    Fixture ct;ct.throwCount=true;fail(ct,CatalogueStage::BusCount,CatalogueError::Thrown,false);
    Fixture bi;bi.throwInfo=true;fail(bi,CatalogueStage::BusInfo,CatalogueError::Thrown,false);
    Fixture cr;cr.throwContext=true;fail(cr,CatalogueStage::ContextRequirements,CatalogueError::Thrown,false);
}
void boundsOriginThreadAndBeforeForeignCalls(){
    for(const auto bad:std::array<std::array<uint32_t,3>,2>{{{{0,0,7}},{{0,7,kNoParamId}}}}){
        Fixture f;bool refused=false;try{(void)ProcessingCatalogue::inspect(f,&f,&f,bad.data(),3,2,0);}
        catch(const CatalogueFailure& e){refused=e.stage==CatalogueStage::Parameters;}
        require(refused&&f.counts==0&&f.mappings==0,"whole ID admission before all foreign calls");
    }
    Fixture f;bool refused=false;std::thread worker([&]{try{(void)ProcessingCatalogue::inspect(f,&f,&f,ids,3,2,0);}
        catch(const CatalogueFailure& e){refused=e.stage==CatalogueStage::Thread;}});worker.join();require(refused&&f.counts==0,"actual wrong-thread refused before callbacks");
    Fixture maximum;maximum.ins=maximum.outs=16;maximum.channels.fill(16);
    auto all=ProcessingCatalogue::inspect(maximum,&maximum,nullptr,ids,3,0xffff,0xffff);
    require(maximum.mappings==33280&&maximum.infos==32&&all.selectedBuses(false)[15]==16&&all.assignment(15,15,129)==UINT32_MAX-1,"actual maximum bus/channel/controller bounds");
    for(auto query:std::array<std::array<uint32_t,3>,4>{{{{16,0,0}},{{15,16,0}},{{15,0,130}},{{0,0,UINT32_MAX}}}}){
        bool bad=false;try{(void)all.assignment(query[0],query[1],query[2]);}catch(const CatalogueFailure&){bad=true;}require(bad,"whole lookup bounds");
    }
    std::array<uint32_t,1024> many{};for(uint32_t i=0;i<1024;++i)many[i]=i;
    Fixture empty;empty.allUnassigned=true;(void)ProcessingCatalogue::inspect(empty,&empty,nullptr,many.data(),many.size(),2,0);
    Fixture over;bool bad=false;try{(void)ProcessingCatalogue::inspect(over,nullptr,nullptr,ids,1025,0,0);}
    catch(const CatalogueFailure& e){bad=e.stage==CatalogueStage::Parameters;}require(bad&&over.counts==0,"capacity before source read");
}
}
int main(){try{copiedBusesMappingAndOptionalIdentity();wholeFailuresAndOriginalProvenance();boundsOriginThreadAndBeforeForeignCalls();hiddenReadonlyMappingsRefuseWholeSelectedRefresh();
    std::cout<<"PASS actual SDK copied processing catalogue:4 groups,32bus/33280mapping maxima,all130controllers,optional requirements,whole failure/provenance,CONTROL thread/no activation,hidden-readonly selected mapping refusal\n";
    return 0;}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
