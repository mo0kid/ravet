#ifndef RAVET_VST3_OWNERSHIP_RANGES_HPP
#define RAVET_VST3_OWNERSHIP_RANGES_HPP
#include <cstddef>
#include <cstdint>
#include <limits>
namespace ravet::vst3::parameters_draft {
struct Range { uintptr_t start=0,end=0; };
inline bool range(const void* pointer,size_t count,size_t width,size_t alignment,Range& result) {
    if(!width||!alignment||count>std::numeric_limits<uintptr_t>::max()/width)return false;
    const auto start=reinterpret_cast<uintptr_t>(pointer),bytes=count*width;
    if(bytes&&(!pointer||start%alignment||bytes>std::numeric_limits<uintptr_t>::max()-start))return false;
    result={start,start+bytes};return true;
}
inline bool overlaps(Range a,Range b) {return a.start<a.end&&b.start<b.end&&a.start<b.end&&b.start<a.end;}
}
#endif
