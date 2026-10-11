#ifndef RAVET_VST3_PARAMETER_CATALOGUE_HPP
#define RAVET_VST3_PARAMETER_CATALOGUE_HPP
#include "BoundedParameterChanges.hpp"
#include "RavetVst3Catalogue.hpp"
#include "pluginterfaces/vst/ivsteditcontroller.h"
#include <utility>
#include <string_view>

namespace ravet::vst3::parameters_draft {
enum class ParameterErrorKind { Capacity, Metadata, UnknownId, ReadOnly, Native, Thrown };
enum class ParameterStage { Count, Info, Read, Plain, Normalize, Format, Parse, Edit };
struct ParameterFailure {
    ParameterErrorKind kind;ParameterStage stage;uint32_t index;
    Steinberg::tresult native=Steinberg::kResultOk;
};
// String128 has at most127 non-NUL UTF16 code units. Its valid UTF8 text
// therefore occupies at most381 bytes. Retain text inline in the single bounded
// parameter vector, so complete SDK-write protection needs one owned heap span
// rather than up to3072 independently allocated string buffers.
class ParameterText {
    std::array<char,512> bytes_{};
    size_t length_=0;
public:
    ParameterText& operator=(const std::string& text) {
        if(text.size()>=bytes_.size())
            throw ParameterFailure{ParameterErrorKind::Metadata,ParameterStage::Info,0};
        bytes_.fill(0);std::memcpy(bytes_.data(),text.data(),text.size());length_=text.size();
        return *this;
    }
    const char* data()const{return bytes_.data();}
    size_t size()const{return length_;}
    friend bool operator==(const ParameterText& text,std::string_view other) {
        return std::string_view(text.data(),text.size())==other;
    }
};
struct Parameter {
    uint32_t id=0,flags=0;int32_t steps=0,unit=0;double defaultNormalized=0;
    ParameterText title,shortTitle,units;
    bool writable()const {
        return !(flags&uint32_t(Steinberg::Vst::ParameterInfo::kIsReadOnly|Steinberg::Vst::ParameterInfo::kIsHidden));
    }
};
// CONTROL-only helper. The owning instance supplies original-main lifecycle and
// foreign reentry exclusion. Returned data is owned, never an SDK interface.
class ParameterCatalogue {
    std::vector<Parameter> parameters_;
    template<class Call> static auto operation(ParameterStage stage,uint32_t index,Call call) {
        try{return call();}
        catch(...){throw ParameterFailure{ParameterErrorKind::Thrown,stage,index,Steinberg::kInternalError};}
    }
    template<class Call> static void status(ParameterStage stage,uint32_t index,Call call) {
        const auto native=operation(stage,index,call);
        if(native!=Steinberg::kResultOk)throw ParameterFailure{ParameterErrorKind::Native,stage,index,native};
    }
    static void normalized(const double& value,ParameterStage stage,uint32_t id) {
        if(!finite(value)||value<0||value>1)throw ParameterFailure{ParameterErrorKind::Metadata,stage,id};
    }
    static std::string text128(const Steinberg::Vst::String128& text,ParameterStage stage,uint32_t id,bool required=false) {
        try{return ravet::vst3::text(text,required);}
        catch(const ravet::vst3::Failure&){throw ParameterFailure{ParameterErrorKind::Metadata,stage,id};}
    }
    const Parameter& require(uint32_t id,ParameterStage stage)const {
        for(const auto& parameter:parameters_)if(parameter.id==id)return parameter;
        throw ParameterFailure{ParameterErrorKind::UnknownId,stage,id};
    }
public:
    static constexpr size_t MaximumParameters=1024;
    static ParameterCatalogue inspect(Steinberg::Vst::IEditController& controller) {
        using Steinberg::Vst::ParameterInfo;
        const auto count=operation(ParameterStage::Count,0,[&]{return controller.getParameterCount();});
        if(count<0||size_t(count)>MaximumParameters)throw ParameterFailure{ParameterErrorKind::Capacity,ParameterStage::Count,0};
        ParameterCatalogue candidate;candidate.parameters_.reserve(size_t(count));
        bool bypass=false;
        constexpr uint32_t knownFlags=ParameterInfo::kCanAutomate|ParameterInfo::kIsReadOnly|
            ParameterInfo::kIsWrapAround|ParameterInfo::kIsList|ParameterInfo::kIsHidden|
            ParameterInfo::kIsProgramChange|ParameterInfo::kIsBypass;
        for(Steinberg::int32 i=0;i<count;++i){
            ParameterInfo info{};
            status(ParameterStage::Info,uint32_t(i),[&]{return controller.getParameterInfo(i,info);});
            const auto flags=uint32_t(info.flags);
            const bool isBypass=bool(flags&ParameterInfo::kIsBypass);
            if(info.id==Steinberg::Vst::kNoParamId||info.stepCount<0||(flags&~knownFlags)||
               ((flags&ParameterInfo::kIsReadOnly)&&(flags&ParameterInfo::kCanAutomate))||
               ((flags&ParameterInfo::kIsHidden)&&!(flags&ParameterInfo::kIsReadOnly))||
               (isBypass&&(bypass||info.stepCount!=1)))
                throw ParameterFailure{ParameterErrorKind::Metadata,ParameterStage::Info,uint32_t(i)};
            for(const auto& previous:candidate.parameters_)if(previous.id==info.id)
                throw ParameterFailure{ParameterErrorKind::Metadata,ParameterStage::Info,uint32_t(i)};
            normalized(info.defaultNormalizedValue,ParameterStage::Info,uint32_t(i));
            Parameter parameter;parameter.id=info.id;parameter.flags=flags;
            parameter.steps=info.stepCount;parameter.unit=info.unitId;parameter.defaultNormalized=info.defaultNormalizedValue;
            parameter.title=text128(info.title,ParameterStage::Info,uint32_t(i),true);
            parameter.shortTitle=text128(info.shortTitle,ParameterStage::Info,uint32_t(i));
            parameter.units=text128(info.units,ParameterStage::Info,uint32_t(i));
            candidate.parameters_.push_back(std::move(parameter));bypass|=isBypass;
        }
        return candidate;
    }
    const auto& parameters()const{return parameters_;}
    bool externalRange(const void* pointer,size_t bytes,size_t alignment)const {
        Range requested{},storage{};
        if(!range(pointer,bytes,1,alignment,requested)||
           !range(this,1,sizeof(*this),alignof(ParameterCatalogue),storage)||overlaps(requested,storage)||
           !range(parameters_.data(),parameters_.capacity(),sizeof(Parameter),alignof(Parameter),storage)||overlaps(requested,storage))return false;
        return true;
    }
    // Internal immutable numeric snapshots can outlive this CONTROL catalogue:
    // SDK wrappers compare ranges only and never dereference the original owner.
    bool protectionRanges(std::array<Range,2>& output)const {
        std::array<Range,2> staged{};
        if(!range(this,1,sizeof(*this),alignof(ParameterCatalogue),staged[0])||
           !range(parameters_.data(),parameters_.capacity(),sizeof(Parameter),alignof(Parameter),staged[1]))return false;
        output=staged;return true;
    }
    bool writable(uint32_t id)const {
        for(const auto& parameter:parameters_)if(parameter.id==id)return parameter.writable();
        return false;
    }
    double read(Steinberg::Vst::IEditController& controller,uint32_t id)const {
        (void)require(id,ParameterStage::Read);
        const auto value=operation(ParameterStage::Read,id,[&]{return controller.getParamNormalized(id);});
        normalized(value,ParameterStage::Read,id);return value;
    }
    double plain(Steinberg::Vst::IEditController& controller,uint32_t id,double value)const {
        (void)require(id,ParameterStage::Plain);normalized(value,ParameterStage::Plain,id);
        const auto result=operation(ParameterStage::Plain,id,[&]{return controller.normalizedParamToPlain(id,value);});
        if(!finite(result))throw ParameterFailure{ParameterErrorKind::Metadata,ParameterStage::Plain,id};return result;
    }
    double normalize(Steinberg::Vst::IEditController& controller,uint32_t id,double value)const {
        (void)require(id,ParameterStage::Normalize);
        if(!finite(value))throw ParameterFailure{ParameterErrorKind::Metadata,ParameterStage::Normalize,id};
        const auto result=operation(ParameterStage::Normalize,id,[&]{return controller.plainParamToNormalized(id,value);});
        normalized(result,ParameterStage::Normalize,id);return result;
    }
    std::string format(Steinberg::Vst::IEditController& controller,uint32_t id,double value)const {
        (void)require(id,ParameterStage::Format);normalized(value,ParameterStage::Format,id);
        Steinberg::Vst::String128 text{};
        status(ParameterStage::Format,id,[&]{return controller.getParamStringByValue(id,value,text);});
        return text128(text,ParameterStage::Format,id);
    }
    double parse(Steinberg::Vst::IEditController& controller,uint32_t id,const Steinberg::Vst::String128& source)const {
        (void)require(id,ParameterStage::Parse);(void)text128(source,ParameterStage::Parse,id);
        Steinberg::Vst::String128 text{};std::memcpy(text,source,sizeof(text));double value=0;
        status(ParameterStage::Parse,id,[&]{return controller.getParamValueByString(id,text,value);});
        normalized(value,ParameterStage::Parse,id);return value;
    }
    void edit(Steinberg::Vst::IEditController& controller,uint32_t id,double value)const {
        const auto& parameter=require(id,ParameterStage::Edit);normalized(value,ParameterStage::Edit,id);
        if(!parameter.writable())throw ParameterFailure{ParameterErrorKind::ReadOnly,ParameterStage::Edit,id};
        status(ParameterStage::Edit,id,[&]{return controller.setParamNormalized(id,value);});
    }
};
}
#endif
