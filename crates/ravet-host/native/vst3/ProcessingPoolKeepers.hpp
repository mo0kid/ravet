#ifndef RAVET_VST3_PROCESSING_POOL_KEEPERS_HPP
#define RAVET_VST3_PROCESSING_POOL_KEEPERS_HPP
#include "BoundedEventLists.hpp"
#include <memory>
#include <stdexcept>
#include <type_traits>

namespace ravet::vst3::processing_draft {
// CONTROL owns every complete process pool for one exact containing native core.
// AUDIO only borrows prepared storage; foreign final release destroys nothing.
// Supply the core's reservation pin before any foreign teardown or reclamation.
template<class Core> class ProcessingPoolKeepers {
    struct Slot {std::unique_ptr<EventPool> pool;bool attached=false;};
    std::array<Slot,16> slots_{};
    Core* const origin_;
    const parameters_draft::ImmutableCoreProtection* const coreProtection_;
    const parameters_draft::LiveCoreProtection* const liveProtection_;
    const std::thread::id control_=std::this_thread::get_id();
    bool everRetained_=false;
    void control(const Core& core)const {
        if(control_!=std::this_thread::get_id()||&core!=origin_)
            throw std::logic_error("processing pool CONTROL origin");
    }
    size_t countUnchecked()const {size_t n=0;for(const auto& slot:slots_)n+=bool(slot.pool);return n;}
public:
    static constexpr size_t MaximumPools=16,MaximumBytes=64*1024*1024;
    explicit ProcessingPoolKeepers(Core& core,const parameters_draft::ImmutableCoreProtection* protection=nullptr,const parameters_draft::LiveCoreProtection* live=nullptr):origin_(&core),coreProtection_(protection),liveProtection_(live){
        if(protection&&!protection->sealed())throw std::logic_error("unsealed immutable core protection");
    }
    ProcessingPoolKeepers(const ProcessingPoolKeepers&)=delete;
    ProcessingPoolKeepers& operator=(const ProcessingPoolKeepers&)=delete;
    size_t count(const Core& core)const {control(core);return countUnchecked();}
    // Actual complete object storage, including this keeper's fixed slots.
    size_t bytes(const Core& core)const {control(core);return sizeof(*this)+countUnchecked()*sizeof(EventPool);}
    EventPool& attach(Core& core) {
        control(core);
        if(countUnchecked()==MaximumPools||sizeof(EventPool)>MaximumBytes-bytes(core))
            throw std::length_error("processing pool object byte/owner capacity");
        for(auto& slot:slots_)if(!slot.pool){
            auto prepared=std::make_unique<EventPool>(coreProtection_,liveProtection_);
            slot.pool=std::move(prepared);slot.attached=true;return *slot.pool;
        }
        std::terminate();
    }
    void detach(Core& core,EventPool& pool) {
        control(core);
        for(auto& slot:slots_)if(slot.pool.get()==&pool){
            if(!slot.attached||!pool.idle())throw std::logic_error("processing pool detach admission");
            slot.attached=false;return;
        }
        throw std::logic_error("processing pool identity");
    }
    bool observedRetention(const Core& core)const {
        control(core);if(everRetained_)return true;
        for(const auto& slot:slots_)if(slot.pool&&
            (slot.pool->observedRetention()||slot.pool->hasForeignReferences()))return true;
        return false;
    }
    template<class Pin> void pinBeforeForeignTeardown(Core& core,Pin&& pin) {
        control(core);
        if(observedRetention(core)){
            everRetained_=true;
            // A refusal/throw prevents the caller from proceeding to teardown.
            std::forward<Pin>(pin)(core);
        }
    }
    template<class Pin> void reclaim(Core& core,Pin&& pin) {
        pinBeforeForeignTeardown(core,std::forward<Pin>(pin));
        for(auto& slot:slots_)if(slot.pool&&!slot.attached&&
            slot.pool->idle()&&!slot.pool->hasForeignReferences())slot.pool.reset();
    }
    // Pure borrowed numeric snapshot. Module CONTROL cannot attach/reclaim
    // until all AUDIO owners join; no thread-affine CONTROL method is called.
    template<class Append> bool appendProtectionRanges(Append append)const {
        if(!append(this,sizeof(*this),alignof(std::remove_reference_t<decltype(*this)>)))return false;
        for(const auto& slot:slots_)if(slot.pool&&!append(slot.pool.get(),sizeof(*slot.pool),alignof(std::remove_reference_t<decltype(*slot.pool)>)))return false;
        return true;
    }
    bool externalRange(const void* pointer,size_t size,size_t alignment)const {
        Range requested{},storage{};
        if(!range(pointer,size,1,alignment,requested)||
           !range(this,1,sizeof(*this),alignof(ProcessingPoolKeepers),storage)||overlaps(requested,storage)||
           (coreProtection_&&!coreProtection_->allows(requested)))return false;
        for(const auto& slot:slots_)if(slot.pool&&!slot.pool->externalRange(pointer,size,alignment))return false;
        return true;
    }
    ~ProcessingPoolKeepers() {
        if(control_!=std::this_thread::get_id())std::terminate();
        // The actual native core is process-pinned before foreign retirement
        // when retention was observed; it will not reach this destructor there.
        for(const auto& slot:slots_)if(slot.attached||(slot.pool&&
            (!slot.pool->idle()||slot.pool->hasForeignReferences())))std::terminate();
    }
};
}
#endif
