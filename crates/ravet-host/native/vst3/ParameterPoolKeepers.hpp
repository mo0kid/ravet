#ifndef RAVET_VST3_PARAMETER_POOL_KEEPERS_HPP
#define RAVET_VST3_PARAMETER_POOL_KEEPERS_HPP
#include "BoundedParameterChanges.hpp"
#include <memory>
#include <stdexcept>
#include <type_traits>

namespace ravet::vst3::parameters_draft {
// Exact NativeCore CONTROL owner. AUDIO borrows a pool, never its keeper.
// Pin the containing native core before reclaim/foreign teardown whenever
// observedRetention() is true, even if workers have already released leases.
class ParameterPoolKeepers {
    struct Slot {std::unique_ptr<Pool> pool;bool attached=false;};
    std::array<Slot,16> slots_{};
    const ImmutableCoreProtection* const coreProtection_;
    const LiveCoreProtection* const liveProtection_;
    std::thread::id control_=std::this_thread::get_id();
    void control()const {if(control_!=std::this_thread::get_id())throw std::logic_error("parameter pool CONTROL owner");}
public:
    static constexpr size_t MaximumPools=16,MaximumBytes=64*1024*1024;
    explicit ParameterPoolKeepers(const ImmutableCoreProtection* protection=nullptr,const LiveCoreProtection* live=nullptr):coreProtection_(protection),liveProtection_(live){
        if(protection&&!protection->sealed())throw std::logic_error("unsealed immutable core protection");
    }
    ParameterPoolKeepers(const ParameterPoolKeepers&)=delete;
    ParameterPoolKeepers& operator=(const ParameterPoolKeepers&)=delete;
    size_t count()const {control();size_t n=0;for(const auto& slot:slots_)n+=bool(slot.pool);return n;}
    size_t bytes()const {return count()*sizeof(Pool);}
    Pool& attach() {
        control();
        if(count()==MaximumPools||sizeof(Pool)>MaximumBytes-bytes())throw std::length_error("parameter pool capacity");
        for(auto& slot:slots_)if(!slot.pool){
            auto prepared=std::make_unique<Pool>(coreProtection_,liveProtection_);
            slot.pool=std::move(prepared);slot.attached=true;return *slot.pool;
        }
        std::terminate();
    }
    void detach(Pool& pool) {
        control();
        for(auto& slot:slots_)if(slot.pool.get()==&pool){
            if(!slot.attached||!pool.idle())throw std::logic_error("parameter pool detach admission");
            slot.attached=false;return;
        }
        throw std::logic_error("parameter pool identity");
    }
    bool observedRetention()const {
        control();for(const auto& slot:slots_)if(slot.pool&&
            (slot.pool->observedRetention()||slot.pool->hasForeignReferences()))return true;
        return false;
    }
    void reclaim() {
        control();for(auto& slot:slots_)if(slot.pool&&!slot.attached&&
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
           !range(this,1,sizeof(*this),alignof(ParameterPoolKeepers),storage)||overlaps(requested,storage)||
           (coreProtection_&&!coreProtection_->allows(requested)))return false;
        for(const auto& slot:slots_)if(slot.pool&&!slot.pool->externalRange(pointer,size,alignment))return false;
        return true;
    }
    ~ParameterPoolKeepers() {
        if(control_!=std::this_thread::get_id())std::terminate();
        // A process-pinned core intentionally never reaches this destructor.
        for(const auto& slot:slots_)if(slot.attached||(slot.pool&&
            (!slot.pool->idle()||slot.pool->hasForeignReferences())))std::terminate();
    }
};
}
#endif
