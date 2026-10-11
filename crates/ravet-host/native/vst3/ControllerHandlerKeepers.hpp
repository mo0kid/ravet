#ifndef RAVET_VST3_CONTROLLER_HANDLER_KEEPERS_HPP
#define RAVET_VST3_CONTROLLER_HANDLER_KEEPERS_HPP
#include "ImmutableCoreProtection.hpp"
#include "LiveCoreProtection.hpp"
#include "pluginterfaces/vst/ivsteditcontroller.h"
#include <atomic>
#include <array>
#include <memory>
#include <pthread.h>
#include <stdexcept>
#include <thread>
namespace ravet::vst3::draft {
class ControllerHandler final:public Steinberg::Vst::IComponentHandler {
    std::atomic<uint32_t> refs_{1},requests_{0};
    std::atomic<bool> ready_{true};
    const std::thread::id main_=std::this_thread::get_id();
    const parameters_draft::ImmutableCoreProtection* const immutable_;
    const parameters_draft::LiveCoreProtection* const live_;
    const void* const keeper_;const size_t keeperBytes_,keeperAlignment_;
    void* const origin_;void (*const refresh_)(void*);
    static_assert(std::atomic<uint32_t>::is_always_lock_free);
    bool external(const void* p,size_t bytes,size_t alignment)const noexcept {
        using namespace parameters_draft;Range requested{},self{},keeper{};
        return range(p,bytes,1,alignment,requested)&&range(this,1,sizeof(*this),alignof(ControllerHandler),self)&&
            range(keeper_,keeperBytes_,1,keeperAlignment_,keeper)&&!overlaps(requested,self)&&!overlaps(requested,keeper)&&
            (!immutable_||immutable_->allows(requested))&&(!live_||live_->allows(requested));
    }
public:
    ControllerHandler(const parameters_draft::ImmutableCoreProtection* immutable,
            const parameters_draft::LiveCoreProtection* live,const void* keeper,size_t bytes,size_t alignment,
            void* origin,void (*refresh)(void*)):
        immutable_(immutable),live_(live),keeper_(keeper),keeperBytes_(bytes),keeperAlignment_(alignment),origin_(origin),refresh_(refresh){}
    Steinberg::tresult PLUGIN_API queryInterface(const Steinberg::TUID id,void** output)override {
        const bool samePhase=ready_.load(std::memory_order_acquire)&&pthread_main_np()&&main_==std::this_thread::get_id()&&(!live_||live_->currentControl());
        parameters_draft::LiveCoreProtection::Read guard(samePhase?nullptr:live_);
        if(!samePhase&&!guard.admitted(live_))return Steinberg::kInvalidArgument;
        if(samePhase&&refresh_)refresh_(origin_);
        parameters_draft::Range selector{},destination{};
        if(!id||!output||!external(id,sizeof(Steinberg::TUID),1)||!external(output,sizeof(*output),alignof(void*))||
            !parameters_draft::range(id,sizeof(Steinberg::TUID),1,1,selector)||
            !parameters_draft::range(output,1,sizeof(*output),alignof(void*),destination)||parameters_draft::overlaps(selector,destination))return Steinberg::kInvalidArgument;
        *output=nullptr;
        if(Steinberg::FUnknownPrivate::iidEqual(id,Steinberg::FUnknown::iid)||Steinberg::FUnknownPrivate::iidEqual(id,Steinberg::Vst::IComponentHandler::iid)){
            addRef();*output=static_cast<Steinberg::Vst::IComponentHandler*>(this);return Steinberg::kResultOk;
        }return Steinberg::kNoInterface;
    }
    Steinberg::uint32 PLUGIN_API addRef()override{return refs_.fetch_add(1,std::memory_order_relaxed)+1;}
    Steinberg::uint32 PLUGIN_API release()override {
        // The permanent keeper reference prevents foreign last-delete on AUDIO.
        const auto previous=refs_.fetch_sub(1,std::memory_order_acq_rel);if(previous<=1)std::terminate();return previous-1;
    }
    Steinberg::tresult PLUGIN_API beginEdit(Steinberg::Vst::ParamID)override{return Steinberg::kNotImplemented;}
    Steinberg::tresult PLUGIN_API performEdit(Steinberg::Vst::ParamID,Steinberg::Vst::ParamValue)override{return Steinberg::kNotImplemented;}
    Steinberg::tresult PLUGIN_API endEdit(Steinberg::Vst::ParamID)override{return Steinberg::kNotImplemented;}
    Steinberg::tresult PLUGIN_API restartComponent(Steinberg::int32 flags)override {
        if(!ready_.load(std::memory_order_acquire)||!flags||(uint32_t(flags)&~uint32_t(0xfff)))return Steinberg::kInvalidArgument;
        requests_.fetch_or(uint32_t(flags),std::memory_order_release);return Steinberg::kResultOk;
    }
    uint32_t requests()const noexcept{return requests_.load(std::memory_order_acquire);}
    uint32_t references()const noexcept{return refs_.load(std::memory_order_acquire);}
    bool retired()const noexcept{return !ready_.load(std::memory_order_acquire);}
    void retire()noexcept{ready_.store(false,std::memory_order_release);}
    ~ControllerHandler(){if(!pthread_main_np()||main_!=std::this_thread::get_id()||references()!=1)std::terminate();}
};
class ControllerHandlerKeepers {
    struct Slot{std::unique_ptr<ControllerHandler> handler;bool attached=false;};
    std::array<Slot,256> slots_{};
    const std::thread::id main_=std::this_thread::get_id();
    const parameters_draft::ImmutableCoreProtection* const immutable_;
    const parameters_draft::LiveCoreProtection* const live_;
    void* const origin_;void (*const refresh_)(void*);void (*const pin_)(void*);
    bool everRetained_=false;
    void control()const{if(!pthread_main_np()||main_!=std::this_thread::get_id()||(live_&&!live_->currentControl()))throw std::logic_error("handler keeper CONTROL origin");}
    Slot& slot(ControllerHandler& handler){for(auto& s:slots_)if(s.handler.get()==&handler)return s;throw std::logic_error("handler keeper identity");}
public:
    static constexpr size_t MaximumHandlers=256;
    ControllerHandlerKeepers(const parameters_draft::ImmutableCoreProtection* immutable=nullptr,
            const parameters_draft::LiveCoreProtection* live=nullptr,void* origin=nullptr,
            void (*refresh)(void*)=nullptr,void (*pin)(void*)=nullptr):immutable_(immutable),live_(live),origin_(origin),refresh_(refresh),pin_(pin){}
    ControllerHandlerKeepers(const ControllerHandlerKeepers&)=delete;
    ControllerHandlerKeepers& operator=(const ControllerHandlerKeepers&)=delete;
    size_t count()const noexcept{size_t n=0;for(const auto& s:slots_)n+=bool(s.handler);return n;}
    bool available()const noexcept{return count()<MaximumHandlers;}
    ControllerHandler& attach(){control();for(auto& s:slots_)if(!s.handler){
        s.handler=std::make_unique<ControllerHandler>(immutable_,live_,this,sizeof(*this),alignof(ControllerHandlerKeepers),origin_,refresh_);s.attached=true;return *s.handler;
    }
        throw std::length_error("handler keeper capacity");
    }
    // Called after attempted clear and BEFORE subsequent foreign teardown.
    void observeRetired(ControllerHandler& handler){control();(void)slot(handler);
        if(!handler.retired())throw std::logic_error("handler retirement phase");
        if(handler.references()>1&&!everRetained_){if(pin_)pin_(origin_);everRetained_=true;}
    }
    void detach(ControllerHandler& handler){control();auto& s=slot(handler);if(!s.attached)throw std::logic_error("handler detach phase");handler.retire();observeRetired(handler);s.attached=false;}
    void reclaim(){control();for(auto& s:slots_)if(s.handler&&!s.attached){observeRetired(*s.handler);if(s.handler->references()==1)s.handler.reset();}}
    bool observedRetention()const noexcept{return everRetained_;}
    template<class Append>bool appendProtectionRanges(Append append)const {
        // Containing core already protects the value keeper; count duplicates conservatively in Instance extents.
        for(const auto& s:slots_)if(s.handler&&!append(s.handler.get(),sizeof(ControllerHandler),alignof(ControllerHandler)))return false;return true;
    }
    bool externalRange(const void* p,size_t bytes,size_t alignment)const noexcept {
        using namespace parameters_draft;Range requested{},owned{};if(!range(p,bytes,1,alignment,requested)||!range(this,1,sizeof(*this),alignof(ControllerHandlerKeepers),owned)||overlaps(requested,owned))return false;
        for(const auto& s:slots_)if(s.handler&&(!range(s.handler.get(),1,sizeof(ControllerHandler),alignof(ControllerHandler),owned)||overlaps(requested,owned)))return false;return true;
    }
    ~ControllerHandlerKeepers(){if(!pthread_main_np()||main_!=std::this_thread::get_id())std::terminate();for(const auto& s:slots_)if(s.attached||(s.handler&&s.handler->references()!=1))std::terminate();}
};
}
#endif
