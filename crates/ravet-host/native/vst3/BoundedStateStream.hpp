#pragma once
#include "pluginterfaces/base/ibstream.h"
#include "ImmutableCoreProtection.hpp"
#include "LiveCoreProtection.hpp"
#include <algorithm>
#include <array>
#include <atomic>
#include <cstring>
#include <limits>
#include <stdexcept>
#include <thread>
#include <vector>
#include <pthread.h>

namespace ravet::vst3::state_draft {
// Actual SDK IBStream: host keeps a reference, foreign AddRef is supported.
// All payload/cursor effects are CONTROL-only during one synchronous phase.
// Freeze rejects late effects; a host keeper prevents final foreign release
// from deallocating the payload on AUDIO. No stream points at borrowed Rust bytes.
class BoundedStateStream final : public Steinberg::IBStream {
    std::atomic<Steinberg::uint32> references_{1};
    const std::thread::id main_ = std::this_thread::get_id();
    static constexpr uint32_t Admitted=uint32_t{1}<<31;
    static_assert(std::atomic<uint32_t>::is_always_lock_free);
    // Admission and sticky fault publish together: freeze cannot race with a
    // late wrong-thread fault changing a completed diagnostic.
    std::atomic<uint32_t> state_{Admitted};
    const bool writable_;
    const parameters_draft::ImmutableCoreProtection* const immutable_;
    const parameters_draft::LiveCoreProtection* const live_;
    void* const refreshOwner_;
    void (*const refresh_)(void*);
    std::vector<uint8_t> storage_;
    size_t length_=0, cursor_=0;
    struct Range { uintptr_t first=0,last=0; };
    static bool extent(const void* data,size_t bytes,size_t alignment,Range& range) {
        const auto start=reinterpret_cast<uintptr_t>(data);
        if(bytes&&(!data||start%alignment||bytes>UINTPTR_MAX-start))return false;
        range={start,start+bytes};return true;
    }
    static bool overlap(Range a,Range b){return a.first<a.last&&b.first<b.last&&a.first<b.last&&b.first<a.last;}
    bool external(const void* data,size_t bytes,size_t alignment)const {
        Range requested{},object{},payload{};
        if(!extent(data,bytes,alignment,requested))return false;
        (void)extent(this,sizeof(*this),alignof(BoundedStateStream),object);
        (void)extent(storage_.data(),storage_.size(),1,payload);
        parameters_draft::Range numeric{requested.first,requested.last};
        return !overlap(requested,object)&&!overlap(requested,payload)&&
            (!immutable_||immutable_->allows(numeric))&&(!live_||live_->allows(numeric));
    }
    static size_t checkedCapacity(size_t capacity) {
        if(!pthread_main_np()||!capacity||capacity>8u*1024u*1024u)throw std::invalid_argument("bounded state stream capacity/thread");
        return capacity;
    }
    Steinberg::tresult bad(uint32_t kind=1) {
        auto previous=state_.load(std::memory_order_acquire);
        while((previous&Admitted)&&!(previous&~Admitted)) {
            if(state_.compare_exchange_weak(previous,previous|kind,std::memory_order_acq_rel,std::memory_order_acquire))break;
        }
        return Steinberg::kInvalidArgument;
    }
    bool control()const{return (state_.load(std::memory_order_acquire)&Admitted)&&pthread_main_np()&&main_==std::this_thread::get_id()&&(!live_||live_->currentControl());}
    template<class T> bool resultPointer(T* pointer)const{return !pointer||external(pointer,sizeof(T),alignof(T));}
public:
    static constexpr size_t MaximumBytes=8u*1024u*1024u;
    BoundedStateStream(size_t capacity,bool writable,const uint8_t* source=nullptr,size_t length=0,
            const parameters_draft::ImmutableCoreProtection* immutable=nullptr,
            const parameters_draft::LiveCoreProtection* live=nullptr,void* refreshOwner=nullptr,void (*refresh)(void*)=nullptr)
        :writable_(writable),immutable_(immutable),live_(live),refreshOwner_(refreshOwner),refresh_(refresh),storage_(checkedCapacity(capacity)),length_(length) {
        if(!pthread_main_np()||!capacity||capacity>MaximumBytes||length>capacity||(!writable&&!source&&length)|| (writable&&length))
            throw std::invalid_argument("bounded state stream profile");
        if(length)std::memcpy(storage_.data(),source,length);
    }
    Steinberg::tresult PLUGIN_API queryInterface(const Steinberg::TUID id,void** output)override {
        // QI/refcount remain available across threads and after freeze. Reject
        // known aliases before touching either range; QI does not change the
        // completed stream-phase diagnostic or cursor/payload.
        // Active original MAIN state callbacks already own exact CONTROL.
        // Frozen or off-thread QI takes a Read through its final write instead.
        const bool samePhase=control();
        parameters_draft::LiveCoreProtection::Read admission(samePhase?nullptr:live_);
        if(!samePhase&&!admission.admitted(live_))return Steinberg::kInvalidArgument;
        if(!id||!output||!external(id,sizeof(Steinberg::TUID),1)||
           !external(output,sizeof(*output),alignof(void*)))return Steinberg::kInvalidArgument;
        Range selector{},destination{};
        (void)extent(id,sizeof(Steinberg::TUID),1,selector);
        (void)extent(output,sizeof(*output),alignof(void*),destination);
        if(overlap(selector,destination))return Steinberg::kInvalidArgument;
        *output=nullptr;
        if(Steinberg::FUnknownPrivate::iidEqual(id,Steinberg::FUnknown::iid)||Steinberg::FUnknownPrivate::iidEqual(id,Steinberg::IBStream::iid)) {
            *output=static_cast<Steinberg::IBStream*>(this);addRef();return Steinberg::kResultOk;
        }
        return Steinberg::kNoInterface;
    }
    Steinberg::uint32 PLUGIN_API addRef()override{return references_.fetch_add(1,std::memory_order_relaxed)+1;}
    Steinberg::uint32 PLUGIN_API release()override {
        const auto value=references_.fetch_sub(1,std::memory_order_acq_rel)-1;
        if(!value)delete this;
        return value;
    }
    Steinberg::tresult PLUGIN_API read(void* output,Steinberg::int32 requested,Steinberg::int32* readBytes=nullptr)override {
        if(!control())return bad(2);
        if(requested<0||!external(output,size_t(requested),1)||!resultPointer(readBytes))return bad();
        Range data{},count{};
        (void)extent(output,size_t(requested),1,data);
        if(readBytes){(void)extent(readBytes,sizeof(*readBytes),alignof(Steinberg::int32),count);if(overlap(data,count))return bad();}
        const auto available=cursor_<length_?length_-cursor_:0;
        const auto n=std::min(size_t(requested),available);
        if(n)std::memcpy(output,storage_.data()+cursor_,n);
        cursor_+=n;if(readBytes)*readBytes=Steinberg::int32(n);
        return Steinberg::kResultOk;
    }
    Steinberg::tresult PLUGIN_API write(void* input,Steinberg::int32 requested,Steinberg::int32* written=nullptr)override {
        if(!control())return bad(2);
        if(!writable_)return bad(3);
        if(requested<0)return bad();
        if(size_t(requested)>storage_.size()-cursor_)return bad(4);
        if(!external(input,size_t(requested),1)||!resultPointer(written))return bad();
        Range data{},count{};
        (void)extent(input,size_t(requested),1,data);
        if(written){(void)extent(written,sizeof(*written),alignof(Steinberg::int32),count);if(overlap(data,count))return bad();}
        if(requested)std::memcpy(storage_.data()+cursor_,input,size_t(requested));
        cursor_+=size_t(requested);length_=std::max(length_,cursor_);
        if(written)*written=requested;return Steinberg::kResultOk;
    }
    Steinberg::tresult PLUGIN_API seek(Steinberg::int64 offset,Steinberg::int32 mode,Steinberg::int64* result=nullptr)override {
        if(!control())return bad(2);
        if(!resultPointer(result))return bad();
        Steinberg::int64 base=0;
        if(mode==kIBSeekCur)base=Steinberg::int64(cursor_);
        else if(mode==kIBSeekEnd)base=Steinberg::int64(length_);
        else if(mode!=kIBSeekSet)return bad();
        const auto limit=Steinberg::int64(writable_?storage_.size():length_);
        if(offset < -base||offset>limit-base)return bad();
        cursor_=size_t(base+offset);if(result)*result=Steinberg::int64(cursor_);return Steinberg::kResultOk;
    }
    Steinberg::tresult PLUGIN_API tell(Steinberg::int64* result)override {
        if(!control())return bad(2);
        if(!result||!resultPointer(result))return bad();
        *result=Steinberg::int64(cursor_);return Steinberg::kResultOk;
    }
    void refreshProtection() {
        if(live_&&(!control()||!refresh_||!refreshOwner_))throw std::logic_error("state original CONTROL view");
        if(refresh_)refresh_(refreshOwner_);
    }
    void freeze(){if(!pthread_main_np()||main_!=std::this_thread::get_id())throw std::logic_error("stream freeze thread");state_.fetch_and(~Admitted,std::memory_order_acq_rel);}
    bool frozen()const{return !(state_.load(std::memory_order_acquire)&Admitted);}
    Steinberg::uint32 references()const{return references_.load(std::memory_order_acquire);}
    Steinberg::tresult failure()const{return failureKind()?Steinberg::kInvalidArgument:Steinberg::kResultOk;}
    // 0none,1protocol/range,2thread/admission,3source-write,4capture-capacity.
    uint32_t failureKind()const{return state_.load(std::memory_order_acquire)&~Admitted;}
    template<class Append> bool appendProtectionRanges(Append append)const {
        return append(this,sizeof(*this),alignof(BoundedStateStream))&&append(storage_.data(),storage_.capacity(),1);
    }
    size_t capacity()const{return storage_.size();}
    size_t size()const{return length_;} // CONTROL, only after freeze
    const uint8_t* bytes()const{return storage_.data();} // CONTROL frozen view
    bool externalRange(const void* data,size_t bytes,size_t alignment)const{return external(data,bytes,alignment);}
};

// Internal native-core member, never copied or automatically destroyed at exit
// for a process-pinned core. All reserve/reclaim/destruction happens on CONTROL.
class StateStreamKeepers {
    std::array<Steinberg::IPtr<BoundedStateStream>,16> streams_{};
    size_t bytes_=0;
    const parameters_draft::ImmutableCoreProtection* immutable_=nullptr;
    const parameters_draft::LiveCoreProtection* live_=nullptr;
    void* refreshOwner_=nullptr;
    void (*refresh_)(void*)=nullptr;
    std::thread::id main_=std::this_thread::get_id();
    void control()const{if(!pthread_main_np()||main_!=std::this_thread::get_id())throw std::logic_error("stream keeper thread");}
public:
    struct Spec {size_t capacity=0;bool writable=false;const uint8_t* source=nullptr;size_t length=0;};
    using Batch=std::array<BoundedStateStream*,3>;
    static constexpr size_t MaximumBytes=64u*1024u*1024u;
    StateStreamKeepers(const parameters_draft::ImmutableCoreProtection* immutable=nullptr,
            const parameters_draft::LiveCoreProtection* live=nullptr,void* refreshOwner=nullptr,void (*refresh)(void*)=nullptr)
        :immutable_(immutable),live_(live),refreshOwner_(refreshOwner),refresh_(refresh){control();}
    StateStreamKeepers(const StateStreamKeepers&)=delete;
    StateStreamKeepers& operator=(const StateStreamKeepers&)=delete;
    ~StateStreamKeepers(){if(!pthread_main_np()||main_!=std::this_thread::get_id()||hasForeignReferences())std::terminate();}
    void reclaim(){control();for(auto& stream:streams_)if(stream&&stream->frozen()&&stream->references()==1){bytes_-=stream->capacity();stream=nullptr;}}
    Batch reserve(const Spec* specs,size_t count) {
        control();if(!specs||!count||count>3)throw std::invalid_argument("state stream batch");
        size_t wanted=0;
        for(size_t i=0;i<count;++i){const auto& s=specs[i];if(!s.capacity||s.capacity>BoundedStateStream::MaximumBytes||s.length>s.capacity||(!s.writable&&!s.source&&s.length)||(s.writable&&s.length))throw std::invalid_argument("state stream spec");wanted+=s.capacity;}
        reclaim();size_t free=0;for(const auto& stream:streams_)if(!stream)++free;
        if(free<count||wanted>MaximumBytes-bytes_)throw std::length_error("retained state stream budget");
        std::array<Steinberg::IPtr<BoundedStateStream>,3> prepared{};
        for(size_t i=0;i<count;++i)prepared[i]=Steinberg::owned(new BoundedStateStream(specs[i].capacity,specs[i].writable,specs[i].source,specs[i].length,immutable_,live_,refreshOwner_,refresh_));
        Batch result{};size_t next=0;
        for(auto& slot:streams_)if(!slot&&next<count){result[next]=prepared[next].get();bytes_+=prepared[next]->capacity();slot=std::move(prepared[next++]);}
        return result;
    }
    bool hasForeignReferences()const{for(const auto& stream:streams_)if(stream&&stream->references()>1)return true;return false;}
    bool externalRange(const void* data,size_t bytes,size_t alignment)const {
        control();return borrowedExternalRange(data,bytes,alignment);
    }
    // Read-only fixed storage inspection while an AUDIO capability exclusively
    // borrows the native core. Caller joins before CONTROL reserve/reclaim.
    bool borrowedExternalRange(const void* data,size_t bytes,size_t alignment)const {
        const auto start=reinterpret_cast<uintptr_t>(data);
        if(bytes&&(!data||start%alignment||bytes>UINTPTR_MAX-start))return false;
        const auto owner=reinterpret_cast<uintptr_t>(this);
        if(bytes&&start<owner+sizeof(*this)&&owner<start+bytes)return false;
        for(const auto& stream:streams_)if(stream&&!stream->externalRange(data,bytes,alignment))return false;
        return true;
    }
    template<class Append> bool appendProtectionRanges(Append append)const {
        if(!append(this,sizeof(*this),alignof(StateStreamKeepers)))return false;
        for(const auto& stream:streams_)if(stream&&!stream->appendProtectionRanges(append))return false;
        return true;
    }
    size_t retainedBytes()const{return bytes_;}
};
}
