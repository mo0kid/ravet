#ifndef RAVET_BOUNDED_PARAMETER_CHANGES_HPP
#define RAVET_BOUNDED_PARAMETER_CHANGES_HPP
#include "RavetVst3Parameters.h"
#include "ImmutableCoreProtection.hpp"
#include "LiveCoreProtection.hpp"
#include "pluginterfaces/vst/ivstparameterchanges.h"
#include <array>
#include <atomic>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <limits>
#include <thread>
#include <stdexcept>

namespace ravet::vst3::parameters_draft {
static_assert(sizeof(double)==8&&std::numeric_limits<double>::is_iec559,
    "Parameter normalization requires IEEE binary64");
using Point=RavetVst3ParameterPoint;
inline bool finite(const double& value) {
    uint64_t bits=0;std::memcpy(&bits,&value,sizeof(bits));
    // Foreign scalar returns can carry the caller compiler's finite-math ABI
    // assumptions. An integer volatile observation keeps this a bit check.
    volatile uint64_t observed=bits;
    return (observed&UINT64_C(0x7ff0000000000000))!=UINT64_C(0x7ff0000000000000);
}
class Pool;
class Batch final: public Steinberg::Vst::IParameterChanges {
    using Result=Steinberg::tresult;
    static constexpr uint32_t Admitted=uint32_t{1}<<31;
    struct Queue final: public Steinberg::Vst::IParamValueQueue {
        Batch* parent=nullptr;
        std::atomic<uint32_t> refs{1};
        uint32_t id=Steinberg::Vst::kNoParamId,start=0,count=0;
        Result PLUGIN_API queryInterface(const Steinberg::TUID selector,void** output) override {
            return parent->query(selector,output,Steinberg::Vst::IParamValueQueue::iid,
                static_cast<Steinberg::Vst::IParamValueQueue*>(this),refs);
        }
        Steinberg::uint32 PLUGIN_API addRef() override {return parent->retain(refs);}
        Steinberg::uint32 PLUGIN_API release() override {return parent->releaseOwned(refs);}
        Steinberg::Vst::ParamID PLUGIN_API getParameterId() override {
            return parent->admitted()?id:Steinberg::Vst::kNoParamId;
        }
        Steinberg::int32 PLUGIN_API getPointCount() override {
            return parent->admitted()?Steinberg::int32(count):0;
        }
        Result PLUGIN_API getPoint(Steinberg::int32 index,Steinberg::int32& offset,
                Steinberg::Vst::ParamValue& value) override {
            if(!parent->admitted())return Steinberg::kInvalidArgument;
            Range a{},b{};
            if(index<0||uint32_t(index)>=count||
               !parent->external(&offset,sizeof(offset),alignof(Steinberg::int32),a)||
               !parent->external(&value,sizeof(value),alignof(Steinberg::Vst::ParamValue),b)||overlaps(a,b)) {
                parent->bad(1);return Steinberg::kInvalidArgument;
            }
            const auto& point=parent->points_[start+uint32_t(index)];
            offset=Steinberg::int32(point.offset);value=point.value;return Steinberg::kResultOk;
        }
        Result PLUGIN_API addPoint(Steinberg::int32,Steinberg::Vst::ParamValue,Steinberg::int32&) override {
            parent->bad(3);return Steinberg::kInvalidArgument;
        }
    };
    std::atomic<uint32_t> refs_{1},phase_{0};
    std::thread::id owner_{};
    Range protected_{};
    const ImmutableCoreProtection* coreProtection_=nullptr;
    const LiveCoreProtection* liveProtection_=nullptr;
    std::array<Range,256> protectedExtras_{};
    uint32_t protectedExtraCount_=0;
    Pool* pool_=nullptr;
    uint32_t count_=0;
    std::array<Queue,64> queues_{};
    std::array<Point,2048> points_{};
    friend class Pool;
#ifdef RAVET_VST3_PARAMETER_CHANGES_TESTS
    friend struct ParameterChangesTestAccess;
#endif
    bool external(const void* pointer,size_t bytes,size_t alignment,Range& result)const {
        if(!range(pointer,bytes,1,alignment,result)||overlaps(result,protected_)||
           (coreProtection_&&!coreProtection_->allows(result))||(liveProtection_&&!liveProtection_->allows(result)))return false;
        for(uint32_t i=0;i<protectedExtraCount_;++i)if(overlaps(result,protectedExtras_[i]))return false;
        return true;
    }
    void bad(uint32_t kind) {
        auto phase=phase_.load(std::memory_order_acquire);
        while((phase&Admitted)&&!(phase&~Admitted)&&
            !phase_.compare_exchange_weak(phase,Admitted|kind,std::memory_order_acq_rel)){}
    }
    bool admitted() {
        if(!(phase_.load(std::memory_order_acquire)&Admitted))return false;
        if(owner_!=std::this_thread::get_id()){bad(2);return false;}
        return true;
    }
    bool tryRetain(std::atomic<uint32_t>& refs,uint32_t& result) {
        auto value=refs.load(std::memory_order_relaxed);
        for(;;){
            if(value==UINT32_MAX){bad(1);result=value;return false;}
            if(refs.compare_exchange_weak(value,value+1,std::memory_order_relaxed)){result=value+1;return true;}
        }
    }
    uint32_t retain(std::atomic<uint32_t>& refs) {uint32_t result=0;(void)tryRetain(refs,result);return result;}
    uint32_t releaseOwned(std::atomic<uint32_t>& refs) {
        auto value=refs.load(std::memory_order_relaxed);
        for(;;){
            // The permanent host reference is never released through SDK code.
            if(value<=1){bad(1);return 1;}
            if(refs.compare_exchange_weak(value,value-1,std::memory_order_acq_rel))return value-1;
        }
    }
    Result query(const Steinberg::TUID selector,void** output,const Steinberg::FUID& iid,
            void* interface,std::atomic<uint32_t>& refs) {
        LiveCoreProtection::Read admission(liveProtection_);
        if(!admission.admitted(liveProtection_))return Steinberg::kInvalidArgument;
        Range a{},b{};
        if(!selector||!output||!external(selector,sizeof(Steinberg::TUID),1,a)||
           !external(output,sizeof(*output),alignof(void*),b)||overlaps(a,b)){bad(1);return Steinberg::kInvalidArgument;}
        const bool supported=Steinberg::FUnknownPrivate::iidEqual(selector,Steinberg::FUnknown::iid)||
            Steinberg::FUnknownPrivate::iidEqual(selector,iid);
        *output=nullptr;if(!supported)return Steinberg::kNoInterface;
        uint32_t count=0;if(!tryRetain(refs,count))return Steinberg::kInternalError;
        *output=interface;return Steinberg::kResultOk;
    }
public:
    static constexpr size_t MaximumQueues=64,MaximumPoints=2048;
    Batch() {for(auto& queue:queues_)queue.parent=this;}
    Batch(const Batch&)=delete;
    Batch& operator=(const Batch&)=delete;
    Result PLUGIN_API queryInterface(const Steinberg::TUID selector,void** output) override {
        return query(selector,output,Steinberg::Vst::IParameterChanges::iid,
            static_cast<Steinberg::Vst::IParameterChanges*>(this),refs_);
    }
    Steinberg::uint32 PLUGIN_API addRef() override {return retain(refs_);}
    Steinberg::uint32 PLUGIN_API release() override {return releaseOwned(refs_);}
    Steinberg::int32 PLUGIN_API getParameterCount() override {return admitted()?Steinberg::int32(count_):0;}
    Steinberg::Vst::IParamValueQueue* PLUGIN_API getParameterData(Steinberg::int32 index) override {
        if(!admitted())return nullptr;
        if(index<0||uint32_t(index)>=count_){bad(1);return nullptr;}
        return &queues_[uint32_t(index)];
    }
    Steinberg::Vst::IParamValueQueue* PLUGIN_API addParameterData(const Steinberg::Vst::ParamID&,
            Steinberg::int32&) override {bad(3);return nullptr;}
    bool hostOnly()const {
        if(refs_.load(std::memory_order_acquire)!=1)return false;
        for(const auto& queue:queues_)if(queue.refs.load(std::memory_order_acquire)!=1)return false;
        return true;
    }
    bool frozen()const {return !(phase_.load(std::memory_order_acquire)&Admitted);}
    uint32_t failureKind()const {return phase_.load(std::memory_order_acquire)&~Admitted;}
    uint32_t freeze();
};
enum class Admission { Ok, Invalid, Capacity, Busy };
class Pool {
    std::array<Batch,16> batches_{};
    const ImmutableCoreProtection* const coreProtection_;
    const LiveCoreProtection* const liveProtection_;
    std::atomic<bool> busy_{false},retained_{false};
    static_assert(std::atomic<uint32_t>::is_always_lock_free);
    static_assert(std::atomic<bool>::is_always_lock_free);
    friend class Batch;
#ifdef RAVET_VST3_PAIRED_RESERVATION_TESTS
    friend struct PairedReservationAccess;
#endif
public:
    static constexpr size_t MaximumSlots=16;
    explicit Pool(const ImmutableCoreProtection* protection=nullptr,const LiveCoreProtection* live=nullptr):coreProtection_(protection),liveProtection_(live) {
        if(protection&&!protection->sealed())throw std::logic_error("unsealed immutable core protection");
        Range protectedRange{};
        if(!range(this,1,sizeof(*this),alignof(Pool),protectedRange))std::terminate();
        for(auto& batch:batches_){batch.pool_=this;batch.protected_=protectedRange;batch.coreProtection_=protection;batch.liveProtection_=live;}
    }
    Pool(const Pool&)=delete;
    Pool& operator=(const Pool&)=delete;
    // Immutable-source admission builds only a caller-local plan. Reserving a
    // slot copies no input or batch metadata. Two pools can therefore reserve
    // together, cancel either reservation, then activate only after both admit.
    class Plan {
        friend class Pool;
        const Point* source_=nullptr;size_t count_=0,groups_=0;
        std::array<uint32_t,64> ids_{},counts_{};
        std::array<Range,256> extras_{};uint32_t extraCount_=0;
        bool valid_=false;
    public: Plan()=default;
    };
    class Reservation {
        friend class Pool;
        Pool* pool_=nullptr;Batch* batch_=nullptr;std::thread::id owner_{};
    public:
        Reservation()=default;
        Reservation(const Reservation&)=delete;
        Reservation& operator=(const Reservation&)=delete;
        ~Reservation(){
            if(pool_){if(owner_!=std::this_thread::get_id())std::terminate();pool_->busy_.store(false,std::memory_order_release);}
        }
    };
    Admission preflight(uint32_t frames,const Point* source,size_t count,Plan& output,
            const Range* extras=nullptr,size_t extraCount=0,size_t rangeLimit=128)const {
        if(frames>65536||count>Batch::MaximumPoints||rangeLimit>256||extraCount>rangeLimit)return Admission::Capacity;
        Range input{},storage{},destination{},protection{};
        if(!range(source,count,sizeof(Point),alignof(Point),input)||
           !range(this,1,sizeof(*this),alignof(Pool),storage)||
           !range(&output,1,sizeof(output),alignof(Plan),destination)||
           !range(extras,extraCount,sizeof(Range),alignof(Range),protection)||
           overlaps(input,storage)||overlaps(destination,storage)||overlaps(destination,input)||
           overlaps(protection,storage)||overlaps(destination,protection)||
           (coreProtection_&&(!coreProtection_->allows(input)||!coreProtection_->allows(destination)||!coreProtection_->allows(protection))))return Admission::Invalid;
        Plan candidate;candidate.source_=source;candidate.count_=count;
        std::array<uint32_t,64> last{};
        for(size_t p=0;p<count;++p){
            const auto& point=source[p];
            if(point.id==Steinberg::Vst::kNoParamId||!finite(point.value)||point.value<0||point.value>1||
               (frames?point.offset>=frames:point.offset!=0)||(p&&point.offset<source[p-1].offset))return Admission::Invalid;
            size_t group=0;while(group<candidate.groups_&&candidate.ids_[group]!=point.id)++group;
            if(group==candidate.groups_){if(group==64)return Admission::Capacity;candidate.ids_[candidate.groups_++]=point.id;}
            else if(point.offset<=last[group])return Admission::Invalid;
            ++candidate.counts_[group];last[group]=point.offset;
        }
        for(size_t i=0;i<extraCount;++i){
            if(extras[i].start>extras[i].end)return Admission::Invalid;
            candidate.extras_[i]=extras[i];
        }
        candidate.extraCount_=uint32_t(extraCount);candidate.valid_=true;output=candidate;return Admission::Ok;
    }
    Admission reserve(Reservation& output) {
        Range destination{},storage{};
        if(output.pool_||!range(&output,1,sizeof(output),alignof(Reservation),destination)||
           !range(this,1,sizeof(*this),alignof(Pool),storage)||overlaps(destination,storage)||
           (coreProtection_&&!coreProtection_->allows(destination)))return Admission::Invalid;
        bool expected=false;
        if(!busy_.compare_exchange_strong(expected,true,std::memory_order_acq_rel))return Admission::Busy;
        Batch* selected=nullptr;
        for(auto& batch:batches_)if(batch.frozen()&&batch.hostOnly()){selected=&batch;break;}
        if(!selected){busy_.store(false,std::memory_order_release);return Admission::Capacity;}
        output.pool_=this;output.batch_=selected;output.owner_=std::this_thread::get_id();return Admission::Ok;
    }
    Batch& activate(const Plan& plan,Reservation& reservation)noexcept {
        if(!plan.valid_||reservation.pool_!=this||reservation.owner_!=std::this_thread::get_id())std::terminate();
        auto* selected=reservation.batch_;
        std::array<uint32_t,64> starts{},positions{};uint32_t start=0;
        for(size_t q=0;q<plan.groups_;++q){
            starts[q]=start;positions[q]=start;start+=plan.counts_[q];
            selected->queues_[q].id=plan.ids_[q];selected->queues_[q].start=starts[q];selected->queues_[q].count=plan.counts_[q];
        }
        for(size_t p=0;p<plan.count_;++p){size_t q=0;while(plan.ids_[q]!=plan.source_[p].id)++q;selected->points_[positions[q]++]=plan.source_[p];}
        selected->count_=uint32_t(plan.groups_);selected->owner_=std::this_thread::get_id();
        selected->protectedExtras_=plan.extras_;selected->protectedExtraCount_=plan.extraCount_;
        reservation.pool_=nullptr;reservation.batch_=nullptr;
        selected->phase_.store(Batch::Admitted,std::memory_order_release);return *selected;
    }
    Admission prepare(uint32_t frames,const Point* source,size_t count,Batch*& output) {
        if(frames>65536||count>Batch::MaximumPoints)return Admission::Capacity;
        Range destination{},input{};
        if(!externalRange(&output,sizeof(output),alignof(Batch*))||
           !range(&output,1,sizeof(output),alignof(Batch*),destination)||
           !range(source,count,sizeof(Point),alignof(Point),input)||overlaps(destination,input))return Admission::Invalid;
        Plan plan;const auto valid=preflight(frames,source,count,plan);
        if(valid!=Admission::Ok)return valid;
        Reservation reserved;const auto status=reserve(reserved);if(status!=Admission::Ok)return status;
        output=&activate(plan,reserved);return Admission::Ok;
    }
    bool hasForeignReferences()const {
        for(const auto& batch:batches_)if(!batch.hostOnly())return true;
        return false;
    }
    bool observedRetention()const {return retained_.load(std::memory_order_acquire);}
    bool idle()const {return !busy_.load(std::memory_order_acquire);}
    bool externalRange(const void* pointer,size_t bytes,size_t alignment)const {
        Range requested{},storage{};
        return range(pointer,bytes,1,alignment,requested)&&
            range(this,1,sizeof(*this),alignof(Pool),storage)&&!overlaps(requested,storage)&&
            (!coreProtection_||coreProtection_->allows(requested))&&(!liveProtection_||liveProtection_->allows(requested));
    }
    ~Pool() {
        // Core CONTROL reclamation proves detachment + no foreign references.
        if(busy_.load(std::memory_order_acquire)||hasForeignReferences())std::terminate();
    }
};
static_assert(sizeof(Pool)<=2*1024*1024,"Fixed parameter pool byte budget");
inline uint32_t Batch::freeze() {
    // Host calls only on the admitted owner. Late SDK calls cannot close phases.
    if(!(phase_.load(std::memory_order_acquire)&Admitted)||owner_!=std::this_thread::get_id())std::terminate();
    const auto failure=phase_.fetch_and(~Admitted,std::memory_order_acq_rel)&~Admitted;
    if(!hostOnly())pool_->retained_.store(true,std::memory_order_release);
    pool_->busy_.store(false,std::memory_order_release);return failure;
}
}
#endif
