#ifndef RAVET_VST3_LIVE_CORE_PROTECTION_HPP
#define RAVET_VST3_LIVE_CORE_PROTECTION_HPP
#include "OwnershipRanges.hpp"
#include <atomic>
#include <thread>
namespace ravet::vst3::parameters_draft {
// One exact containing-core owner. CONTROL publishes only while exclusive;
// escaped QI retains Read through its final destination write. No waits or
// retries: interference is a bounded refusal, never an unlocked fallback.
class LiveCoreProtection {
    static constexpr uint32_t Control=uint32_t{1}<<31;
    mutable std::atomic<uint32_t> admission_{0};
    const std::thread::id control_=std::this_thread::get_id();
    std::array<Range,12288> ranges_{};
    size_t count_=0;
public:
    static constexpr size_t MaximumRanges=12288;
    static_assert(std::atomic<uint32_t>::is_always_lock_free);
    LiveCoreProtection()=default;
    LiveCoreProtection(const LiveCoreProtection&)=delete;
    LiveCoreProtection& operator=(const LiveCoreProtection&)=delete;
    class Read {
        const LiveCoreProtection* owner_=nullptr;
    public:
        explicit Read(const LiveCoreProtection* owner):owner_(owner) {
            if(!owner)return;
            auto current=owner->admission_.load(std::memory_order_acquire);
            if(current>=Control-1||!owner->admission_.compare_exchange_strong(current,current+1,std::memory_order_acq_rel))owner_=nullptr;
        }
        Read(const Read&)=delete;Read& operator=(const Read&)=delete;
        ~Read(){if(owner_)owner_->admission_.fetch_sub(1,std::memory_order_release);}
        bool admitted(const LiveCoreProtection* requested)const noexcept{return !requested||owner_==requested;}
    };
    bool currentControl()const noexcept {
        return control_==std::this_thread::get_id()&&admission_.load(std::memory_order_acquire)==Control;
    }
    bool beginControl()noexcept {
        uint32_t expected=0;
        return admission_.compare_exchange_strong(expected,Control,std::memory_order_acq_rel);
    }
    void clear()noexcept {if(admission_.load(std::memory_order_relaxed)!=Control)std::terminate();count_=0;}
    bool append(const void* p,size_t n,size_t width,size_t alignment)noexcept {
        Range value{};
        if(admission_.load(std::memory_order_relaxed)!=Control||count_==MaximumRanges||!range(p,n,width,alignment,value))return false;
        if(value.start!=value.end)ranges_[count_++]=value;
        return true;
    }
    void endControl()noexcept {
        if(admission_.load(std::memory_order_relaxed)!=Control)std::terminate();
        admission_.store(0,std::memory_order_release);
    }
    // Caller owns a Read or the whole original AUDIO/CONTROL exclusion.
    bool allows(Range requested)const noexcept {
        if(requested.start>requested.end)return false;
        for(size_t i=0;i<count_;++i)if(overlaps(requested,ranges_[i]))return false;
        return true;
    }
    bool allowsExcept(Range requested,Range exactOwner)const noexcept {
        if(requested.start>requested.end)return false;
        for(size_t i=0;i<count_;++i){
            if(ranges_[i].start==exactOwner.start&&ranges_[i].end==exactOwner.end)continue;
            if(overlaps(requested,ranges_[i]))return false;
        }
        return true;
    }
    size_t count()const noexcept{return count_;}
    enum class Alias { None, LiveOwned, CopiedNumeric };
    Alias classify(Range requested,const Range* copied,size_t count)const noexcept {
        if(!allows(requested))return Alias::LiveOwned;
        for(size_t i=0;i<count;++i)if(overlaps(requested,copied[i]))return Alias::CopiedNumeric;
        return Alias::None;
    }
};
}
#endif
