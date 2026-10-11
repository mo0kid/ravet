#include "ParameterCatalogue.hpp"
#include <algorithm>
#include <iostream>
#include <stdexcept>
using namespace Steinberg;
using namespace Steinberg::Vst;
using namespace ravet::vst3::parameters_draft;
namespace {
void require(bool condition,const char* message){if(!condition)throw std::runtime_error(message);}
double fromBits(uint64_t bits){volatile uint64_t runtime=bits;bits=runtime;double value=0;std::memcpy(&value,&bits,sizeof(value));return value;}
class Controller final:public IEditController {
public:
    std::array<ParameterInfo,3> infos{};
    int32 count=3,calls=0,failIndex=-1,throwIndex=-1;
    tresult refusal=kResultFalse;
    double current=.5,converted=.25;
    bool throwRead=false,throwEdit=false,throwCount=false,refuseEdit=false,malformedText=false,mutateInput=false;
    Controller(){
        for(int32 i=0;i<3;++i){infos[size_t(i)].id=uint32(i+10);infos[size_t(i)].title[0]=TChar('A'+i);infos[size_t(i)].defaultNormalizedValue=.5;}
        infos[0].flags=ParameterInfo::kCanAutomate;infos[0].units[0]='d';infos[0].units[1]='B';
        infos[1].flags=ParameterInfo::kIsBypass;infos[1].stepCount=1;
        infos[2].flags=ParameterInfo::kIsHidden|ParameterInfo::kIsReadOnly;
        infos[2].title[1]=TChar(0xd83c);infos[2].title[2]=TChar(0xdfb5);
    }
    tresult PLUGIN_API queryInterface(const TUID,void** output)override{if(output)*output=nullptr;return kNoInterface;}
    uint32 PLUGIN_API addRef()override{return 1;}
    uint32 PLUGIN_API release()override{return 1;}
    tresult PLUGIN_API initialize(FUnknown*)override{return kResultOk;}
    tresult PLUGIN_API terminate()override{return kResultOk;}
    tresult PLUGIN_API setComponentState(IBStream*)override{return kNotImplemented;}
    tresult PLUGIN_API setState(IBStream*)override{return kNotImplemented;}
    tresult PLUGIN_API getState(IBStream*)override{return kNotImplemented;}
    int32 PLUGIN_API getParameterCount()override{++calls;if(throwCount)throw 1;return count;}
    tresult PLUGIN_API getParameterInfo(int32 i,ParameterInfo& info)override {
        ++calls;if(i==throwIndex)throw 1;if(i==failIndex)return refusal;
        if(i<0||i>=count)return kInvalidArgument;
        if(count>3){info={};info.id=uint32(i+10);info.defaultNormalizedValue=.5;
            for(auto* text:{info.title,info.shortTitle,info.units})std::fill_n(text,127,TChar(0xe000));
        }else info=infos[size_t(i)];
        return kResultOk;
    }
    tresult PLUGIN_API getParamStringByValue(ParamID,ParamValue,String128 text)override {
        ++calls;if(malformedText){std::fill_n(text,128,TChar('x'));return kResultOk;}
        text[0]='Q';text[1]=TChar(0xd83c);text[2]=TChar(0xdfb5);text[3]=0;return kResultOk;
    }
    tresult PLUGIN_API getParamValueByString(ParamID,TChar* text,ParamValue& value)override {
        ++calls;value=converted;if(mutateInput)text[0]='X';return kResultOk;
    }
    ParamValue PLUGIN_API normalizedParamToPlain(ParamID,ParamValue value)override{++calls;return throwRead?fromBits(UINT64_C(0x7ff0000000000000)):-60+60*value;}
    ParamValue PLUGIN_API plainParamToNormalized(ParamID,ParamValue)override{++calls;return converted;}
    ParamValue PLUGIN_API getParamNormalized(ParamID)override{++calls;if(throwRead)throw 1;return current;}
    tresult PLUGIN_API setParamNormalized(ParamID,ParamValue value)override{++calls;current=value;if(throwEdit)throw 1;return refuseEdit?refusal:kResultOk;}
    tresult PLUGIN_API setComponentHandler(IComponentHandler*)override{return kResultOk;}
    IPlugView* PLUGIN_API createView(FIDString)override{return nullptr;}
};
uint32_t failureAttempts=0;
template<class Operation> ParameterFailure failure(Operation operation){
    ++failureAttempts;
    try{operation();}catch(const ParameterFailure& error){return error;}
    throw std::runtime_error("expected parameter failure at attempt "+std::to_string(failureAttempts));
}
void ownedMetadataAndControls() {
    Controller controller;auto catalogue=ParameterCatalogue::inspect(controller);
    require(catalogue.parameters().size()==3&&catalogue.parameters()[0].title=="A"&&catalogue.parameters()[0].units=="dB", "owned metadata");
    require(catalogue.parameters()[2].title=="C\xf0\x9f\x8e\xb5"&&!catalogue.writable(12)&&catalogue.writable(10)&&catalogue.writable(11),"UTF16 supplementary scalar and writable policy");
    controller.infos[0].title[0]='X';require(catalogue.parameters()[0].title=="A","snapshot retains owned text");
    require(catalogue.read(controller,10)==.5&&catalogue.plain(controller,10,.5)==-30&&catalogue.normalize(controller,10,-45)==.25,"explicit read and native conversions");
    require(catalogue.format(controller,10,.25)=="Q\xf0\x9f\x8e\xb5","formatted UTF16 text");
    String128 text{};text[0]='Q';controller.mutateInput=true;
    require(catalogue.parse(controller,10,text)==.25&&text[0]=='Q',"parse passes private mutable text copy");
    catalogue.edit(controller,10,.75);require(controller.current==.75,"controller-only edit");
    auto calls=controller.calls;
    require(failure([&]{catalogue.edit(controller,12,.8);}).kind==ParameterErrorKind::ReadOnly&&controller.calls==calls,"readonly before foreign call");
    require(failure([&]{catalogue.read(controller,99);}).kind==ParameterErrorKind::UnknownId&&controller.calls==calls,"unknown id before foreign call");
}
void completeInlineOwnedTextProtection() {
    Controller controller;controller.count=1024;
    auto catalogue=ParameterCatalogue::inspect(controller);
    require(catalogue.parameters().size()==1024,"full accepted parameter catalogue");
    std::array<Range,2> protectedRanges{};
    require(catalogue.protectionRanges(protectedRanges),"complete bounded numeric protection snapshot");
    for(const auto& parameter:catalogue.parameters())for(const auto* text:{&parameter.title,&parameter.shortTitle,&parameter.units}) {
        Range bytes{};require(text->size()==381&&range(text->data(),text->size()+1,1,1,bytes),"maximum valid String128 UTF8 retained with NUL");
        require(bytes.start>=protectedRanges[1].start&&bytes.end<=protectedRanges[1].end,"every text lives in the one full owned vector range");
        require(text->data()[381]==0&&!catalogue.externalRange(text->data(),text->size()+1,1),"caller SDK writes cannot overlap any selected text");
    }
    auto moved=std::move(catalogue);std::array<Range,2> movedRanges{};
    require(moved.protectionRanges(movedRanges)&&movedRanges[1].start==protectedRanges[1].start&&movedRanges[1].end==protectedRanges[1].end,"CONTROL owner move preserves owned heap identity");
    auto copied=moved;std::array<Range,2> copiedRanges{};
    require(copied.protectionRanges(copiedRanges)&&!overlaps(copiedRanges[1],movedRanges[1])&&copied.parameters()[1023].title==std::string_view(moved.parameters()[1023].title.data(),moved.parameters()[1023].title.size()),"copied catalogue owns independent inline UTF8 storage");
}
void completeCatalogueRejection() {
    Controller original;auto accepted=ParameterCatalogue::inspect(original);
    for(int mode=0;mode<11;++mode){
        Controller controller;
        switch(mode){
            case 0:controller.infos[2].id=10;break;
            case 1:controller.infos[2].id=kNoParamId;break;
            case 2:controller.infos[2].stepCount=-1;break;
            case 3:controller.infos[2].flags=1<<14;break;
            case 4:controller.infos[2].flags=ParameterInfo::kIsReadOnly|ParameterInfo::kCanAutomate;break;
            case 5:controller.infos[2].flags=ParameterInfo::kIsHidden;break;
            case 6:controller.infos[2].flags=ParameterInfo::kIsBypass;controller.infos[2].stepCount=1;break;
            case 7:controller.infos[2].title[1]=TChar(0xdc00);break;
            case 8:std::fill_n(controller.infos[2].title,128,TChar('x'));break;
            case 9:controller.infos[2].defaultNormalizedValue=fromBits(UINT64_C(0x7ff8000000000001));break;
            case 10:controller.infos[2].title[0]=0;break;
        }
        const auto result=failure([&]{auto candidate=ParameterCatalogue::inspect(controller);accepted=std::move(candidate);});
        require(result.kind==ParameterErrorKind::Metadata&&result.stage==ParameterStage::Info&&result.index==2,"late metadata failure exact scope");
        require(accepted.parameters()[0].title=="A"&&accepted.parameters()[2].id==12,"failed staged refresh preserves previous complete data");
    }
    Controller controller;controller.count=1025;
    require(failure([&]{ParameterCatalogue::inspect(controller);}).kind==ParameterErrorKind::Capacity&&controller.calls==1,"oversize before metadata calls");
    controller.count=-1;require(failure([&]{ParameterCatalogue::inspect(controller);}).kind==ParameterErrorKind::Capacity,"negative count");
    controller.count=0;require(ParameterCatalogue::inspect(controller).parameters().empty(),"valid empty parameter catalogue");
}
void statusAndFiniteFailures() {
    Controller controller;auto catalogue=ParameterCatalogue::inspect(controller);
    controller.failIndex=2;controller.refusal=kNotImplemented;
    auto result=failure([&]{ParameterCatalogue::inspect(controller);});
    require(result.kind==ParameterErrorKind::Native&&result.native==kNotImplemented&&result.index==2,"original returned metadata failure");
    controller.failIndex=-1;controller.throwIndex=1;result=failure([&]{ParameterCatalogue::inspect(controller);});
    require(result.kind==ParameterErrorKind::Thrown&&result.index==1,"metadata throw distinct from returned result");
    controller.throwRead=true;result=failure([&]{catalogue.read(controller,10);});
    require(result.kind==ParameterErrorKind::Thrown&&result.stage==ParameterStage::Read,"read throw");
    result=failure([&]{catalogue.plain(controller,10,.5);});require(result.kind==ParameterErrorKind::Metadata,"infinite plain return");
    controller.throwRead=false;
    for(auto value:{fromBits(UINT64_C(0x7ff8000000000001)),fromBits(UINT64_C(0x7ff0000000000000)),-.1,1.1}){
        controller.current=value;controller.converted=value;
        require(failure([&]{catalogue.read(controller,10);}).kind==ParameterErrorKind::Metadata,"invalid returned normalized read");
        require(failure([&]{catalogue.normalize(controller,10,0);}).kind==ParameterErrorKind::Metadata,"invalid returned conversion");
        String128 text{};require(failure([&]{catalogue.parse(controller,10,text);}).kind==ParameterErrorKind::Metadata,"invalid returned parse");
        auto calls=controller.calls;require(failure([&]{catalogue.edit(controller,10,value);}).kind==ParameterErrorKind::Metadata&&controller.calls==calls,"invalid edit before native mutation");
    }
    controller.refuseEdit=true;controller.refusal=kInvalidArgument;
    result=failure([&]{catalogue.edit(controller,10,.3);});
    require(result.kind==ParameterErrorKind::Native&&result.native==kInvalidArgument&&controller.current==.3,"mutating refusal is not rollback");
    controller.throwEdit=true;result=failure([&]{catalogue.edit(controller,10,.4);});
    require(result.kind==ParameterErrorKind::Thrown&&controller.current==.4,"mutating throw distinct and history retained");
    controller.malformedText=true;require(failure([&]{catalogue.format(controller,10,.5);}).kind==ParameterErrorKind::Metadata,"unterminated output text");
}
}
int main(){try{
    completeInlineOwnedTextProtection();std::cout<<"PASS all1024 inline titles/short-titles/units protected by one heap range and owned move/copy\n";
        ownedMetadataAndControls();completeCatalogueRejection();statusAndFiniteFailures();
    std::cout<<"PASS actual SDK controller parameter catalogue:3 groups,whole owned UTF16 metadata,original status/throw and control conversions\n";
    return 0;
}catch(const std::exception& error){std::cerr<<error.what()<<'\n';return 1;}}
