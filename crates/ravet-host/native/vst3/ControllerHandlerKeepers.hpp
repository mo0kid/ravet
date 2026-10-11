#ifndef RAVET_VST3_CONTROLLER_HANDLER_KEEPERS_HPP
#define RAVET_VST3_CONTROLLER_HANDLER_KEEPERS_HPP
#include "ImmutableCoreProtection.hpp"
#include "LiveCoreProtection.hpp"
#include "RavetVst3EditGestures.h"
#include "pluginterfaces/vst/ivsteditcontroller.h"
#include <algorithm>
#include <atomic>
#include <array>
#include <cstring>
#include <limits>
#include <memory>
#include <pthread.h>
#include <stdexcept>
#include <thread>
namespace ravet::vst3::draft {
static_assert(Steinberg::Vst::kParamValuesChanged==4&&Steinberg::Vst::kLatencyChanged==8&&Steinberg::Vst::kParamTitlesChanged==16,
    "edit-gesture restart policy follows the SDK flag values");
static_assert(sizeof(RavetVst3EditGesture)==32&&sizeof(RavetVst3EditGestureState)==104,"edit-gesture records");
class ControllerHandler final:public Steinberg::Vst::IComponentHandler {
    std::atomic<uint32_t> refs_{1},requests_{0};
    std::atomic<bool> ready_{true};
    // Opt-in edit-gesture capture (decision 0276). The flag is read on every
    // thread; the ring, id snapshot and sequence are touched only on MAIN.
    // Counters are atomics because off-main refusals are counted too.
    std::atomic<bool> gestures_{false};
    std::array<RavetVst3EditGesture,RAVET_VST3_EDIT_GESTURE_CAPACITY> ring_{};
    uint32_t head_=0,pending_=0;
    // Open gestures (accepted beginEdit without its endEdit). Each one keeps
    // one ring slot reserved, so its endEdit is never refused for capacity.
    std::array<uint32_t,RAVET_VST3_EDIT_OPEN_GESTURES> open_{};
    uint32_t openCount_=0;
    std::array<uint32_t,1024> writable_{};
    uint32_t writableCount_=0;
    uint64_t sequence_=0;
    std::atomic<uint64_t> accepted_{0},drained_{0},overflowed_{0},invalid_{0},unknown_{0},
        offThread_{0},retiredRefused_{0},restartAccepted_{0},restartRefused_{0},unpaired_{0};
    static void count(std::atomic<uint64_t>& counter)noexcept {
        auto current=counter.load(std::memory_order_relaxed);
        while(current!=std::numeric_limits<uint64_t>::max()&&
              !counter.compare_exchange_weak(current,current+1,std::memory_order_relaxed)){}
    }
    bool onMain()const noexcept{return pthread_main_np()&&main_==std::this_thread::get_id();}
    static bool unit(double value)noexcept {
        uint64_t bits=0;std::memcpy(&bits,&value,sizeof(bits));
        // Bit checks stay meaningful under -ffast-math.
        volatile uint64_t observed=bits;
        if((observed&UINT64_C(0x7ff0000000000000))==UINT64_C(0x7ff0000000000000))return false;
        return !(value<0.)&&!(value>1.);
    }
    bool known(uint32_t id)const noexcept {
        return std::binary_search(writable_.begin(),writable_.begin()+writableCount_,id);
    }
    // Admission shared by the three edit entries. Returns kResultOk to record.
    Steinberg::tresult admitEdit(uint32_t id)noexcept {
        if(!onMain()){count(offThread_);return Steinberg::kResultFalse;}
        if(!ready_.load(std::memory_order_acquire)){count(retiredRefused_);return Steinberg::kResultFalse;}
        // kNoParamId (0xFFFFFFFF) is never in the snapshot (the catalogue
        // refuses it), so it is refused here as unknown and never queued.
        if(id==Steinberg::Vst::kNoParamId||!known(id)){count(unknown_);return Steinberg::kInvalidArgument;}
        return Steinberg::kResultOk;
    }
    // Unreserved free slots: capacity minus pending records minus one reserved
    // end slot per open gesture.
    uint32_t unreserved()const noexcept{return uint32_t(ring_.size())-pending_-openCount_;}
    uint32_t* openSlot(uint32_t id)noexcept{
        for(uint32_t i=0;i<openCount_;++i)if(open_[i]==id)return &open_[i];
        return nullptr;
    }
    // `reserved` consumes the end slot of an open gesture; every other record
    // needs `need` unreserved slots (2 for a begin: itself and its end).
    bool push(uint32_t kind,uint32_t id,uint32_t flags,double value,uint32_t need=1,bool reserved=false)noexcept {
        if(sequence_==std::numeric_limits<uint64_t>::max())return false;
        if(reserved?pending_==ring_.size():unreserved()<need)return false;
        auto& slot=ring_[(head_+pending_)%ring_.size()];
        slot.struct_size=sizeof(slot);slot.kind=kind;slot.id=id;slot.flags=flags;slot.value=value;slot.sequence=sequence_++;
        ++pending_;return true;
    }
    Steinberg::tresult record(uint32_t kind,uint32_t id,double value)noexcept {
        if(!push(kind,id,0,value)){count(overflowed_);return Steinberg::kResultFalse;}
        count(accepted_);return Steinberg::kResultOk;
    }
    // A begin needs an open-gesture entry plus two unreserved slots; a second
    // begin for an already open id is unpaired and refused.
    Steinberg::tresult begin(uint32_t id)noexcept {
        if(openSlot(id)){count(unpaired_);return Steinberg::kResultFalse;}
        if(openCount_==open_.size()||!push(RAVET_VST3_EDIT_BEGIN,id,0,0.,2)){count(overflowed_);return Steinberg::kResultFalse;}
        open_[openCount_++]=id;count(accepted_);return Steinberg::kResultOk;
    }
    // The end of an open gesture uses its reserved slot and always records;
    // an end without an accepted begin is unpaired and refused, so the host
    // never sees an orphan End.
    Steinberg::tresult end(uint32_t id)noexcept {
        auto* slot=openSlot(id);
        if(!slot){count(unpaired_);return Steinberg::kResultFalse;}
        // The reserved slot always exists; only an exhausted 64-bit sequence
        // (2^64 records, unreachable in practice) can refuse, in which case the
        // gesture stays open and the call is counted as overflow.
        if(!push(RAVET_VST3_EDIT_END,id,0,0.,0,true)){count(overflowed_);return Steinberg::kResultFalse;}
        *slot=open_[--openCount_];
        count(accepted_);return Steinberg::kResultOk;
    }
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
    // Without enableGestures these keep the original refusal (kNotImplemented).
    Steinberg::tresult PLUGIN_API beginEdit(Steinberg::Vst::ParamID id)override{
        if(!gestures_.load(std::memory_order_acquire))return Steinberg::kNotImplemented;
        const auto admitted=admitEdit(id);return admitted==Steinberg::kResultOk?begin(id):admitted;
    }
    Steinberg::tresult PLUGIN_API performEdit(Steinberg::Vst::ParamID id,Steinberg::Vst::ParamValue value)override{
        if(!gestures_.load(std::memory_order_acquire))return Steinberg::kNotImplemented;
        const auto admitted=admitEdit(id);if(admitted!=Steinberg::kResultOk)return admitted;
        if(!unit(value)){count(invalid_);return Steinberg::kInvalidArgument;}
        return record(RAVET_VST3_EDIT_PERFORM,id,value);
    }
    Steinberg::tresult PLUGIN_API endEdit(Steinberg::Vst::ParamID id)override{
        if(!gestures_.load(std::memory_order_acquire))return Steinberg::kNotImplemented;
        const auto admitted=admitEdit(id);return admitted==Steinberg::kResultOk?end(id):admitted;
    }
    Steinberg::tresult PLUGIN_API restartComponent(Steinberg::int32 flags)override {
        if(!ready_.load(std::memory_order_acquire)||!flags||(uint32_t(flags)&~uint32_t(0xfff))){
            if(gestures_.load(std::memory_order_acquire)){count(ready_.load(std::memory_order_acquire)?invalid_:retiredRefused_);}
            return Steinberg::kInvalidArgument;
        }
        if(!gestures_.load(std::memory_order_acquire)){
            requests_.fetch_or(uint32_t(flags),std::memory_order_release);return Steinberg::kResultOk;
        }
        if(!onMain()){count(offThread_);return Steinberg::kResultFalse;}
        const bool supported=!(uint32_t(flags)&~uint32_t(RAVET_VST3_EDIT_RESTART_SUPPORTED));
        if(!supported){
            // Refused whole; recorded for the host when the ring has room.
            count(restartRefused_);if(!push(RAVET_VST3_EDIT_RESTART_REFUSED,0,uint32_t(flags),0.))count(overflowed_);
            return Steinberg::kResultFalse;
        }
        if(!push(RAVET_VST3_EDIT_RESTART,0,uint32_t(flags),0.)){count(overflowed_);return Steinberg::kResultFalse;}
        count(restartAccepted_);
        requests_.fetch_or(uint32_t(flags),std::memory_order_release);return Steinberg::kResultOk;
    }
    // MAIN: snapshot the sorted writable ids and switch capture on.
    template<class Ids> bool enableGestures(const Ids& ids)noexcept {
        if(!onMain()||ids.size()>writable_.size())return false;
        uint32_t used=0;for(auto id:ids)writable_[used++]=id;
        std::sort(writable_.begin(),writable_.begin()+used);
        writableCount_=uint32_t(std::unique(writable_.begin(),writable_.begin()+used)-writable_.begin());
        gestures_.store(true,std::memory_order_release);return true;
    }
    bool gestures()const noexcept{return gestures_.load(std::memory_order_acquire);}
    // MAIN, at editor close: record a synthesised End (flags 1) for every open
    // gesture in its reserved slot, oldest first. Returns how many were closed.
    uint32_t closeOpenGestures()noexcept {
        if(!onMain()||!gestures())return 0;
        uint32_t closed=0;
        while(openCount_){
            const auto id=open_[0];
            if(!push(RAVET_VST3_EDIT_END,id,RAVET_VST3_EDIT_SYNTHESIZED,0.,0,true))break;
            open_[0]=open_[--openCount_];++closed;
        }
        return closed;
    }
    // MAIN: move up to capacity oldest records out; never reads foreign memory.
    uint32_t drainGestures(RavetVst3EditGesture* output,uint32_t capacity)noexcept {
        const auto copied=std::min(capacity,pending_);
        for(uint32_t i=0;i<copied;++i){output[i]=ring_[head_];head_=(head_+1)%uint32_t(ring_.size());}
        pending_-=copied;for(uint32_t i=0;i<copied;++i)count(drained_);
        return copied;
    }
    RavetVst3EditGestureState gestureState()const noexcept {
        RavetVst3EditGestureState state{};state.struct_size=sizeof(state);
        state.flags=(gestures()?1u:0u)|(retired()?2u:0u);state.capacity=uint32_t(ring_.size());state.pending=pending_;
        state.open=openCount_;state.unpaired=unpaired_.load(std::memory_order_relaxed);
        state.accepted=accepted_.load(std::memory_order_relaxed);state.drained=drained_.load(std::memory_order_relaxed);
        state.overflowed=overflowed_.load(std::memory_order_relaxed);state.invalid=invalid_.load(std::memory_order_relaxed);
        state.unknown=unknown_.load(std::memory_order_relaxed);state.off_thread=offThread_.load(std::memory_order_relaxed);
        state.retired=retiredRefused_.load(std::memory_order_relaxed);
        state.restart_accepted=restartAccepted_.load(std::memory_order_relaxed);state.restart_refused=restartRefused_.load(std::memory_order_relaxed);
        return state;
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
