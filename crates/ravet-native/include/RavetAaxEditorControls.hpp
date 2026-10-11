#pragma once
#include "RavetAaxParameters.hpp"
#include "AAX_IEffectParameters.h"
#include "AAX_IAutomationDelegate.h"
#include <array>
#include <cstdint>
#include <cstring>

namespace ravet::aax {
// MAIN-only borrowed SDK/schema bridge. The native callback context owns balanced
// model/automation/library refs through each entered callback and retirement.
// Post success accepts a host request; it does not update model state or DSP.
// Callers serialize MAIN; reentry fails before SDK calls. Final retirement makes
// one end attempt for accepted active begins, never replays an uncertain end.
template<size_t MaximumParameters=1024>
class AaxEditorControls final {
    static_assert(MaximumParameters>0&&MaximumParameters<=1024);
public:
    enum class Error:uint32_t{Ok,Invalid,NotReady,WrongThread,Busy,Closed,Sdk,Exception,Unresolved};
    enum class Gesture:uint8_t{Idle,Active,Unresolved};
    struct Attempt{bool attempted=false,returned=false;AAX_Result status=0;};
    struct Diagnostics{Attempt initialization{},begin{},value{},end{},read{};uint32_t active=0,unresolved=0;};
    struct ParameterDiagnostics{Gesture gesture=Gesture::Idle;Attempt begin{},value{},end{};};
    struct Snapshot{uint32_t count=0;std::array<double,MaximumParameters> values{};};
    using IsMain=bool(*)();
private:
    const NativeAaxParameters<MaximumParameters>& schema_;
    AAX_IEffectParameters* model_=nullptr;
    AAX_IAutomationDelegate* automation_=nullptr;
    IsMain main_=nullptr;
    std::array<ParameterDiagnostics,MaximumParameters> parameters_{};
    Diagnostics diagnostic_{};
    bool valid_=false,busy_=false,closing_=false;
    // Integer observation also covers scalar values returned by foreign SDK code
    // under a caller's finite-math assumptions, not only referenced input bytes.
    static bool finite(const double& value){uint64_t bits=0;std::memcpy(&bits,&value,8);volatile uint64_t observed=bits;return (observed&UINT64_C(0x7ff0000000000000))!=UINT64_C(0x7ff0000000000000);}
    Error available(bool retirement=false)const{
        if(!main_||!main_())return Error::WrongThread;
        if(busy_)return Error::Busy;
        if(!valid_)return Error::NotReady;
        if(closing_&&!retirement)return Error::Closed;
        return Error::Ok;
    }
    struct Scope{bool& value;explicit Scope(bool& busy):value(busy){value=true;}~Scope(){value=false;}};
    template<class F>static Error call(Attempt& result,F&& operation){result.attempted=true;result.returned=false;try{result.status=operation();result.returned=true;return result.status==AAX_SUCCESS?Error::Ok:Error::Sdk;}catch(...){return Error::Exception;}}
    uint32_t writable(const char* id)const{const auto i=schema_.index(id);const auto* p=i<schema_.count()?schema_.info(id):nullptr;return p&&!(p->flags&RAVET_PARAMETER_READ_ONLY)?i:schema_.count();}
    bool value(const char* id,double raw,double& canonical)const{return writable(id)<schema_.count()&&finite(raw)&&raw>=0&&raw<=1&&schema_.control(id,raw,canonical)&&finite(canonical)&&canonical>=0&&canonical<=1;}
    void counts(){diagnostic_.active=diagnostic_.unresolved=0;for(uint32_t i=0;i<schema_.count();++i){diagnostic_.active+=parameters_[i].gesture==Gesture::Active;diagnostic_.unresolved+=parameters_[i].gesture==Gesture::Unresolved;}}
    Error beginOwned(uint32_t i){if(parameters_[i].gesture!=Gesture::Idle)return Error::Invalid;const auto error=call(parameters_[i].begin,[&]{return automation_->PostTouchRequest(schema_.id(i));});diagnostic_.begin=parameters_[i].begin;parameters_[i].gesture=error==Error::Ok?Gesture::Active:Gesture::Unresolved;counts();return error;}
    Error valueOwned(uint32_t i,double amount){if(parameters_[i].gesture!=Gesture::Active)return Error::Invalid;const auto error=call(parameters_[i].value,[&]{return automation_->PostSetValueRequest(schema_.id(i),amount);});diagnostic_.value=parameters_[i].value;return error;}
    Error endOwned(uint32_t i){if(parameters_[i].gesture!=Gesture::Active)return Error::Invalid;const auto error=call(parameters_[i].end,[&]{return automation_->PostReleaseRequest(schema_.id(i));});diagnostic_.end=parameters_[i].end;parameters_[i].gesture=error==Error::Ok?Gesture::Idle:Gesture::Unresolved;counts();return error;}
public:
    AaxEditorControls(const NativeAaxParameters<MaximumParameters>& schema,AAX_IEffectParameters* model,AAX_IAutomationDelegate* automation,IsMain main):schema_(schema),model_(model),automation_(automation),main_(main){
        if(!main_||!main_()||!schema_.valid()||!model_||!automation_)return;
        int32_t count=0;if(call(diagnostic_.initialization,[&]{return model_->GetNumberOfParameters(&count);})!=Error::Ok||count<0||uint32_t(count)!=schema_.count())return;
        for(uint32_t i=0;i<schema_.count();++i){AAX_CString id;if(call(diagnostic_.initialization,[&]{return model_->GetParameterIDFromIndex(int32_t(i),&id);})!=Error::Ok||std::strcmp(id.Get(),schema_.id(i)))return;}
        valid_=true;
    }
    AaxEditorControls(const AaxEditorControls&)=delete;
    AaxEditorControls& operator=(const AaxEditorControls&)=delete;
    bool valid()const{return valid_;}
    bool closing()const{return closing_;}
    // MAIN lifetime signal is allowed while an entered SDK callback is active.
    // Context stays alive until that callback returns; no SDK action here.
    Error closeMain(){if(!main_||!main_())return Error::WrongThread;closing_=true;return Error::Ok;}
    Diagnostics diagnosticsMain()const{return diagnostic_;}
    ParameterDiagnostics parameterDiagnosticsMain(const char* id)const{const auto i=schema_.index(id);return i<schema_.count()?parameters_[i]:ParameterDiagnostics{};}
    Error begin(const char* id){const auto check=available();if(check!=Error::Ok)return check;const auto i=writable(id);if(i==schema_.count())return Error::Invalid;Scope scope(busy_);return beginOwned(i);}
    Error set(const char* id,double raw){const auto check=available();if(check!=Error::Ok)return check;double amount=0;if(!value(id,raw,amount))return Error::Invalid;const auto i=writable(id);if(parameters_[i].gesture!=Gesture::Active)return Error::Invalid;Scope scope(busy_);return valueOwned(i,amount);}
    Error end(const char* id){const auto check=available();if(check!=Error::Ok)return check;const auto i=writable(id);if(i==schema_.count())return Error::Invalid;Scope scope(busy_);return endOwned(i);}
    Error edit(const char* id,double raw){
        const auto check=available();if(check!=Error::Ok)return check;double amount=0;if(!value(id,raw,amount))return Error::Invalid;const auto i=writable(id);if(parameters_[i].gesture!=Gesture::Idle)return Error::Invalid;Scope scope(busy_);
        const auto begin=beginOwned(i);if(begin!=Error::Ok)return begin;if(closing_)return Error::Closed;
        const auto change=valueOwned(i,amount);if(closing_)return Error::Closed;
        const auto release=endOwned(i);return release!=Error::Ok?release:change;
    }
    Error snapshot(Snapshot& output){
        const auto check=available();if(check!=Error::Ok)return check;
        const auto a=reinterpret_cast<uintptr_t>(this),b=reinterpret_cast<uintptr_t>(&output);if(b>UINTPTR_MAX-sizeof(output)||(b<a+sizeof(*this)&&a<b+sizeof(output)))return Error::Invalid;
        Scope scope(busy_);Snapshot next{};next.count=schema_.count();
        for(uint32_t i=0;i<schema_.count();++i){double raw=0;const auto error=call(diagnostic_.read,[&]{return model_->GetParameterNormalizedValue(schema_.id(i),&raw);});if(error!=Error::Ok)return error;if(closing_)return Error::Closed;if(!finite(raw)||raw<0||raw>1)return Error::Invalid;next.values[i]=raw;}
        output=next;return Error::Ok;
    }
    // One bounded best-effort end per still-active begin. Refused/thrown begin/end
    // is externally unresolved; retirement does not manufacture successful end.
    Error retireMain(){const auto check=available(true);if(check!=Error::Ok)return check;Scope scope(busy_);closing_=true;Error result=Error::Ok;for(uint32_t i=0;i<schema_.count();++i)if(parameters_[i].gesture==Gesture::Active){const auto next=endOwned(i);if(next!=Error::Ok)result=next;}return result==Error::Ok&&diagnostic_.unresolved?Error::Unresolved:result;}
};
} // namespace ravet::aax
