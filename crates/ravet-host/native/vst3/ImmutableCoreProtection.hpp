#ifndef RAVET_VST3_IMMUTABLE_CORE_PROTECTION_HPP
#define RAVET_VST3_IMMUTABLE_CORE_PROTECTION_HPP
#include "OwnershipRanges.hpp"
#include "RavetVst3Catalogue.hpp"
namespace ravet::vst3::parameters_draft {
// CONTROL seals this fixed numeric snapshot once, at its final NativeCore
// address, after the immutable factory catalogue has been committed. It covers
// only observable RAVET-owned objects/buffers, never foreign SDK allocations.
// Pools borrow it for their entire lifetimes. The core owns/destroys those pools
// first; escaped SDK references require the exact core's process pin before
// foreign teardown. No retired facade/Instance/caller object is dereferenced.
class ImmutableCoreProtection {
    std::array<Range,1600> ranges_{};
    size_t count_=0;
    bool sealed_=false;
public:
    static constexpr size_t MaximumRanges=1600;
    ImmutableCoreProtection()=default;
    ImmutableCoreProtection(const ImmutableCoreProtection&)=delete;
    ImmutableCoreProtection& operator=(const ImmutableCoreProtection&)=delete;
    bool sealed()const noexcept{return sealed_;}
    size_t count()const noexcept{return count_;}
    bool seal(const void* core,size_t bytes,size_t alignment,const std::string& key,
            const ravet::vst3::Catalogue& catalogue) noexcept {
        if(sealed_||catalogue.classes.size()>256)return false;
        std::array<Range,MaximumRanges> candidate{};size_t used=0;
        const auto add=[&](const void* p,size_t n,size_t width,size_t align){
            Range value{};
            if(used==candidate.size()||!range(p,n,width,align,value))return false;
            candidate[used++]=value;return true;
        };
        const auto text=[&](const std::string& value){
            if(value.capacity()==std::numeric_limits<size_t>::max())return false;
            return add(value.data(),value.capacity()+1,1,1);
        };
        if(!core||!bytes||!add(core,bytes,1,alignment)||
           !add(this,1,sizeof(*this),alignof(ImmutableCoreProtection))||!text(key)||
           !add(&catalogue,1,sizeof(catalogue),alignof(ravet::vst3::Catalogue))||
           !add(catalogue.classes.data(),catalogue.classes.capacity(),sizeof(ravet::vst3::Class),alignof(ravet::vst3::Class))||
           !text(catalogue.vendor)||!text(catalogue.url)||!text(catalogue.email))return false;
        for(const auto& item:catalogue.classes)
            if(!text(item.category)||!text(item.name)||!text(item.vendor)||
               !text(item.version)||!text(item.sdkVersion)||!text(item.subcategories))return false;
        ranges_=candidate;count_=used;sealed_=true;return true;
    }
    bool allows(Range requested)const noexcept {
        if(!sealed_||requested.start>requested.end)return false;
        for(size_t i=0;i<count_;++i)if(overlaps(requested,ranges_[i]))return false;
        return true;
    }
    bool externalRange(const void* pointer,size_t bytes,size_t alignment)const noexcept {
        Range requested{};
        return range(pointer,bytes,1,alignment,requested)&&allows(requested);
    }
};
static_assert(sizeof(ImmutableCoreProtection)<26*1024,"One bounded immutable metadata snapshot per core");
}
#endif
