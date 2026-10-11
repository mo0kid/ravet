#ifndef RAVET_VST3_BOUNDED_EVENT_LISTS_HPP
#define RAVET_VST3_BOUNDED_EVENT_LISTS_HPP
#include "EventCodec.hpp"
#include "LiveCoreProtection.hpp"
#include <atomic>
#include <thread>
#include <utility>

namespace ravet::vst3::processing_draft {
using parameters_draft::Range;
using parameters_draft::range;
using parameters_draft::overlaps;
class EventPool;
enum class EventAdmission { Ok, Invalid, Capacity, Busy };
class EventBatch {
    using Result=Steinberg::tresult;
    static constexpr uint32_t Active=uint32_t{1}<<31;
    struct List final:Steinberg::Vst::IEventList {
        EventBatch* batch=nullptr;
        bool output=false;
        std::atomic<uint32_t> refs{1};
        Result PLUGIN_API queryInterface(const Steinberg::TUID id,void** result) override {
            return batch->query(*this,id,result);
        }
        Steinberg::uint32 PLUGIN_API addRef() override {return batch->retain(refs);}
        Steinberg::uint32 PLUGIN_API release() override {return batch->release(refs);}
        Steinberg::int32 PLUGIN_API getEventCount() override {
            return batch->admitted()?Steinberg::int32(output?batch->outputCount_:batch->inputCount_):0;
        }
        Result PLUGIN_API getEvent(Steinberg::int32 index,Steinberg::Vst::Event& event) override {
            if(!batch->admitted())return Steinberg::kInvalidArgument;
            Range destination{};
            const uint32_t count=output?batch->outputCount_:batch->inputCount_;
            if(index<0||uint32_t(index)>=count||!batch->sdkExternal(&event,sizeof(event),alignof(Steinberg::Vst::Event),destination)) {
                batch->bad(1);return Steinberg::kInvalidArgument;
            }
            const auto& record=(output?batch->outputRecords_:batch->inputRecords_)[uint32_t(index)];
            event=sdkEvent(record,(output?batch->outputPayload_:batch->inputPayload_).data());
            return Steinberg::kResultOk;
        }
        Result PLUGIN_API addEvent(Steinberg::Vst::Event& event) override;
    };
    struct ParameterQueue final:Steinberg::Vst::IParamValueQueue {
        EventBatch* batch=nullptr;
        std::atomic<uint32_t> refs{1};
        uint32_t id=Steinberg::Vst::kNoParamId,count=0;
        uint16_t head=UINT16_MAX,tail=UINT16_MAX;
        Result PLUGIN_API queryInterface(const Steinberg::TUID selector,void** result) override {
            return batch->queryInterface(selector,result,Steinberg::Vst::IParamValueQueue::iid,
                static_cast<Steinberg::Vst::IParamValueQueue*>(this),refs);
        }
        Steinberg::uint32 PLUGIN_API addRef() override {return batch->retain(refs);}
        Steinberg::uint32 PLUGIN_API release() override {return batch->release(refs);}
        Steinberg::Vst::ParamID PLUGIN_API getParameterId() override {
            return batch->admitted()?id:Steinberg::Vst::kNoParamId;
        }
        Steinberg::int32 PLUGIN_API getPointCount() override {
            return batch->admitted()?Steinberg::int32(count):0;
        }
        Result PLUGIN_API getPoint(Steinberg::int32 index,Steinberg::int32& offset,
                Steinberg::Vst::ParamValue& value) override {
            if(!batch->admitted())return Steinberg::kInvalidArgument;
            Range a{},b{};
            if(index<0||uint32_t(index)>=count||
               !batch->sdkExternal(&offset,sizeof(offset),alignof(Steinberg::int32),a)||
               !batch->sdkExternal(&value,sizeof(value),alignof(Steinberg::Vst::ParamValue),b)||overlaps(a,b)) {
                batch->bad(1);return Steinberg::kInvalidArgument;
            }
            uint16_t at=head;
            for(int32_t i=0;i<index;++i)at=batch->parameterNext_[at];
            const auto& point=batch->parameterPoints_[at];
            offset=Steinberg::int32(point.offset);value=point.value;return Steinberg::kResultOk;
        }
        Result PLUGIN_API addPoint(Steinberg::int32 offset,Steinberg::Vst::ParamValue value,
                Steinberg::int32& index) override {
            if(!batch->admitted())return Steinberg::kInvalidArgument;
            Range destination{};
            if(id==Steinberg::Vst::kNoParamId||offset<0||
               (batch->frames_?uint32_t(offset)>=batch->frames_:offset!=0)||
               !finite(value)||value<0||value>1||
               (count&&uint32_t(offset)<=batch->parameterPoints_[tail].offset)||
               !batch->sdkExternal(&index,sizeof(index),alignof(Steinberg::int32),destination)) {
                batch->bad(1);return Steinberg::kInvalidArgument;
            }
            if(batch->parameterPointCount_==MaximumParameterPoints){batch->bad(4);return Steinberg::kInternalError;}
            const auto at=uint16_t(batch->parameterPointCount_);
            batch->parameterPoints_[at]={id,uint32_t(offset),value};
            batch->parameterNext_[at]=UINT16_MAX;
            if(count)batch->parameterNext_[tail]=at;else head=at;
            tail=at;index=Steinberg::int32(count);++count;++batch->parameterPointCount_;
            return Steinberg::kResultOk;
        }
    };
    struct ParameterList final:Steinberg::Vst::IParameterChanges {
        EventBatch* batch=nullptr;
        std::atomic<uint32_t> refs{1};
        Result PLUGIN_API queryInterface(const Steinberg::TUID selector,void** result) override {
            return batch->queryInterface(selector,result,Steinberg::Vst::IParameterChanges::iid,
                static_cast<Steinberg::Vst::IParameterChanges*>(this),refs);
        }
        Steinberg::uint32 PLUGIN_API addRef() override {return batch->retain(refs);}
        Steinberg::uint32 PLUGIN_API release() override {return batch->release(refs);}
        Steinberg::int32 PLUGIN_API getParameterCount() override {
            return batch->admitted()?Steinberg::int32(batch->parameterQueueCount_):0;
        }
        Steinberg::Vst::IParamValueQueue* PLUGIN_API getParameterData(Steinberg::int32 index) override {
            if(!batch->admitted())return nullptr;
            if(index<0||uint32_t(index)>=batch->parameterQueueCount_){batch->bad(1);return nullptr;}
            return &batch->parameterQueues_[uint32_t(index)];
        }
        Steinberg::Vst::IParamValueQueue* PLUGIN_API addParameterData(const Steinberg::Vst::ParamID& id,
                Steinberg::int32& index) override;
    };
    std::atomic<uint32_t> phase_{0};
    // Wrong-thread SDK calls must not race a plain thread-id write on reuse.
    std::atomic<std::thread::id> owner_{};
    Range protected_{};
    const parameters_draft::ImmutableCoreProtection* coreProtection_=nullptr;
    const parameters_draft::LiveCoreProtection* liveProtection_=nullptr;
    std::array<Range,256> protectedExtras_{};
    uint32_t protectedExtraCount_=0;
    EventPool* pool_=nullptr;
    uint32_t frames_=0,inputCount_=0,outputCount_=0,inputUsed_=0,outputUsed_=0;
    EventBuses inputBuses_{},outputBuses_{};
    List input_,output_;
    ParameterList parameters_;
    uint32_t parameterQueueCount_=0,parameterPointCount_=0;
    std::array<ParameterQueue,64> parameterQueues_{};
    std::array<parameters_draft::Point,2048> parameterPoints_{};
    std::array<uint16_t,2048> parameterNext_{};
    std::array<Record,2048> inputRecords_{},outputRecords_{};
    alignas(Steinberg::Vst::TChar) std::array<uint8_t,65536> inputPayload_{},outputPayload_{};
    friend class EventPool;
#ifdef RAVET_VST3_EVENT_LIST_TESTS
    friend struct EventListTestAccess;
#endif
    bool external(const void* pointer,size_t size,size_t alignment,Range& result)const {
        return range(pointer,size,1,alignment,result)&&!overlaps(result,protected_)&&
            (!coreProtection_||coreProtection_->allows(result))&&(!liveProtection_||liveProtection_->allows(result));
    }
    bool sdkExternal(const void* pointer,size_t size,size_t alignment,Range& result)const {
        if(!external(pointer,size,alignment,result))return false;
        for(uint32_t i=0;i<protectedExtraCount_;++i)if(overlaps(result,protectedExtras_[i]))return false;
        return true;
    }
    bool sdkPayload(Range requested)const;
    void bad(uint32_t failure) {
        auto phase=phase_.load(std::memory_order_acquire);
        while((phase&Active)&&!(phase&~Active)&&
            !phase_.compare_exchange_weak(phase,Active|failure,std::memory_order_acq_rel)){}
    }
    bool admitted() {
        auto phase=phase_.load(std::memory_order_acquire);
        if(!(phase&Active))return false;
        if(owner_.load(std::memory_order_acquire)!=std::this_thread::get_id()){bad(2);return false;}
        return !(phase&~Active);
    }
    bool tryRetain(std::atomic<uint32_t>& refs,uint32_t& result) {
        auto value=refs.load(std::memory_order_relaxed);
        for(;;){
            if(value==UINT32_MAX){bad(1);result=value;return false;}
            if(refs.compare_exchange_weak(value,value+1,std::memory_order_relaxed)){result=value+1;return true;}
        }
    }
    uint32_t retain(std::atomic<uint32_t>& refs){uint32_t result=0;(void)tryRetain(refs,result);return result;}
    uint32_t release(std::atomic<uint32_t>& refs) {
        auto value=refs.load(std::memory_order_relaxed);
        for(;;){
            if(value<=1){bad(1);return 1;}
            if(refs.compare_exchange_weak(value,value-1,std::memory_order_acq_rel))return value-1;
        }
    }
    Result queryInterface(const Steinberg::TUID id,void** output,const Steinberg::FUID& iid,
            void* interface,std::atomic<uint32_t>& refs) {
        parameters_draft::LiveCoreProtection::Read admission(liveProtection_);
        if(!admission.admitted(liveProtection_))return Steinberg::kInvalidArgument;
        Range selector{},destination{};
        if(!id||!output||!sdkExternal(id,sizeof(Steinberg::TUID),1,selector)||
           !sdkExternal(output,sizeof(*output),alignof(void*),destination)||overlaps(selector,destination)) {
            bad(1);return Steinberg::kInvalidArgument;
        }
        const bool supported=Steinberg::FUnknownPrivate::iidEqual(id,Steinberg::FUnknown::iid)||
            Steinberg::FUnknownPrivate::iidEqual(id,iid);
        *output=nullptr;if(!supported)return Steinberg::kNoInterface;
        uint32_t count=0;if(!tryRetain(refs,count))return Steinberg::kInternalError;
        *output=interface;return Steinberg::kResultOk;
    }
    Result query(List& list,const Steinberg::TUID id,void** output) {
        return queryInterface(id,output,Steinberg::Vst::IEventList::iid,
            static_cast<Steinberg::Vst::IEventList*>(&list),list.refs);
    }
public:
    static constexpr size_t MaximumEvents=2048,MaximumPayload=65536;
    static constexpr size_t MaximumParameterQueues=64,MaximumParameterPoints=2048;
    EventBatch(){input_.batch=this;output_.batch=this;output_.output=true;parameters_.batch=this;
        for(auto& queue:parameterQueues_)queue.batch=this;}
    EventBatch(const EventBatch&)=delete;
    EventBatch& operator=(const EventBatch&)=delete;
    Steinberg::Vst::IEventList& input(){return input_;}
    Steinberg::Vst::IEventList& output(){return output_;}
    Steinberg::Vst::IParameterChanges& outputParameters(){return parameters_;}
    bool hostOnly()const {
        if(input_.refs.load(std::memory_order_acquire)!=1||output_.refs.load(std::memory_order_acquire)!=1||
           parameters_.refs.load(std::memory_order_acquire)!=1)return false;
        for(const auto& queue:parameterQueues_)if(queue.refs.load(std::memory_order_acquire)!=1)return false;
        return true;
    }
    bool frozen()const {return !(phase_.load(std::memory_order_acquire)&Active);}
    uint32_t failureKind()const {return phase_.load(std::memory_order_acquire)&~Active;}
    uint32_t freeze();
    // Native wrapper additionally checks all these ranges against the keeper,
    // core, original borrowed input/context/report and caller audio spans before
    // DSP. This primitive protects its complete pool and pairwise destinations.
    bool admitOutputStorage(const RavetVst3ProcessingOutputs& output)const {
        Range destinations[5]{};
        if(!external(&output,sizeof(output),alignof(RavetVst3ProcessingOutputs),destinations[0])||
           output.struct_size!=sizeof(output)||output.reserved||
           output.event_capacity>MaximumEvents||output.payload_capacity>MaximumPayload||
           output.parameter_queue_capacity>MaximumParameterQueues||
           output.parameter_point_capacity>MaximumParameterPoints||
           !external(output.events,size_t(output.event_capacity)*sizeof(Record),alignof(Record),destinations[1])||
           !external(output.payload,output.payload_capacity,1,destinations[2])||
           !external(output.parameter_ids,size_t(output.parameter_queue_capacity)*sizeof(uint32_t),alignof(uint32_t),destinations[3])||
           !external(output.parameter_points,size_t(output.parameter_point_capacity)*sizeof(parameters_draft::Point),alignof(parameters_draft::Point),destinations[4]))return false;
        for(size_t i=0;i<5;++i)for(size_t j=i+1;j<5;++j)if(overlaps(destinations[i],destinations[j]))return false;
        return true;
    }
    bool copyOutputCombined(RavetVst3ProcessingOutputs& output)const {
        if(!frozen()||failureKind()||owner_.load(std::memory_order_acquire)!=std::this_thread::get_id()||
           !admitOutputStorage(output)||output.event_capacity<outputCount_||output.payload_capacity<outputUsed_||
           output.parameter_queue_capacity<parameterQueueCount_||output.parameter_point_capacity<parameterPointCount_)return false;
        if(outputCount_)std::memcpy(output.events,outputRecords_.data(),outputCount_*sizeof(Record));
        if(outputUsed_)std::memcpy(output.payload,outputPayload_.data(),outputUsed_);
        for(uint32_t i=0;i<parameterQueueCount_;++i)output.parameter_ids[i]=parameterQueues_[i].id;
        if(parameterPointCount_)std::memcpy(output.parameter_points,parameterPoints_.data(),parameterPointCount_*sizeof(parameters_draft::Point));
        output.event_count=outputCount_;output.payload_used=outputUsed_;
        output.parameter_queue_count=parameterQueueCount_;output.parameter_point_count=parameterPointCount_;
        return true;
    }
    // Queues retain first-creation order, including empty queues. Points retain
    // global insertion order, which need not be sample-ordered across different
    // IDs. Each ID is strictly sample-ordered; no sorting or hidden merging.
    bool copyOutputParameters(uint32_t* ids,size_t queueCapacity,parameters_draft::Point* points,
            size_t pointCapacity,uint32_t& queues,uint32_t& count)const {
        if(!frozen()||failureKind()||owner_.load(std::memory_order_acquire)!=std::this_thread::get_id()||
           queueCapacity<parameterQueueCount_||queueCapacity>MaximumParameterQueues||
           pointCapacity<parameterPointCount_||pointCapacity>MaximumParameterPoints)return false;
        Range destinations[4]{};
        if(!external(ids,queueCapacity*sizeof(uint32_t),alignof(uint32_t),destinations[0])||
           !external(points,pointCapacity*sizeof(parameters_draft::Point),alignof(parameters_draft::Point),destinations[1])||
           !external(&queues,sizeof(queues),alignof(uint32_t),destinations[2])||
           !external(&count,sizeof(count),alignof(uint32_t),destinations[3]))return false;
        for(size_t i=0;i<4;++i)for(size_t j=i+1;j<4;++j)if(overlaps(destinations[i],destinations[j]))return false;
        for(uint32_t i=0;i<parameterQueueCount_;++i)ids[i]=parameterQueues_[i].id;
        if(parameterPointCount_)std::memcpy(points,parameterPoints_.data(),parameterPointCount_*sizeof(parameters_draft::Point));
        queues=parameterQueueCount_;count=parameterPointCount_;return true;
    }
    // Whole caller destination admission; no partial event/payload/count write.
    // Native process wrapper invokes this only after successful SDK return/freeze.
    bool copyOutput(Record* records,size_t capacity,uint8_t* bytes,size_t byteCapacity,
            uint32_t& count,uint32_t& used)const {
        if(!frozen()||failureKind()||owner_.load(std::memory_order_acquire)!=std::this_thread::get_id()||
           capacity<outputCount_||capacity>MaximumEvents||byteCapacity<outputUsed_||byteCapacity>MaximumPayload)return false;
        Range ranges[4]{};
        if(!external(records,capacity*sizeof(Record),alignof(Record),ranges[0])||
           !external(bytes,byteCapacity,1,ranges[1])||
           !external(&count,sizeof(count),alignof(uint32_t),ranges[2])||
           !external(&used,sizeof(used),alignof(uint32_t),ranges[3]))return false;
        for(size_t i=0;i<4;++i)for(size_t j=i+1;j<4;++j)if(overlaps(ranges[i],ranges[j]))return false;
        if(outputCount_)std::memcpy(records,outputRecords_.data(),outputCount_*sizeof(Record));
        if(outputUsed_)std::memcpy(bytes,outputPayload_.data(),outputUsed_);
        count=outputCount_;used=outputUsed_;return true;
    }
};
class EventPool {
    std::array<EventBatch,16> batches_{};
    const parameters_draft::ImmutableCoreProtection* const coreProtection_;
    const parameters_draft::LiveCoreProtection* const liveProtection_;
    std::atomic<bool> busy_{false},retained_{false};
    const std::thread::id controlOwner_=std::this_thread::get_id();
    std::array<uint32_t,1024> parameterIds_{};
    uint32_t parameterIdCount_=0;
    friend class EventBatch;
#ifdef RAVET_VST3_PAIRED_RESERVATION_TESTS
    friend struct PairedReservationAccess;
#endif
    static_assert(std::atomic<uint32_t>::is_always_lock_free);
    static_assert(std::atomic<bool>::is_always_lock_free);
    static_assert(std::atomic<std::thread::id>::is_always_lock_free,
        "Local event-list admission requires lock-free thread identities");
    bool readablePayload(Range requested)const {
        Range storage{};if(!range(this,1,sizeof(*this),alignof(EventPool),storage))return false;
        if(!overlaps(requested,storage))return true;
        // Echo/duplicate payloads obtained from our SDK lists are valid sources.
        // Permit only already published payload prefixes, never object metadata,
        // unused capacity, or a range crossing into the append destination.
        for(const auto& batch:batches_){
            for(const auto data:{std::make_pair(batch.inputPayload_.data(),batch.inputUsed_),
                    std::make_pair(batch.outputPayload_.data(),batch.outputUsed_)}){
                Range prefix{};if(!range(data.first,data.second,1,1,prefix))return false;
                if(requested.start>=prefix.start&&requested.end<=prefix.end)return true;
            }
        }
        return false;
    }
public:
    static constexpr size_t MaximumSlots=16;
    explicit EventPool(const parameters_draft::ImmutableCoreProtection* protection=nullptr,const parameters_draft::LiveCoreProtection* live=nullptr):coreProtection_(protection),liveProtection_(live) {
        if(protection&&!protection->sealed())throw std::logic_error("unsealed immutable core protection");
        Range storage{};if(!range(this,1,sizeof(*this),alignof(EventPool),storage))std::terminate();
        for(auto& batch:batches_){batch.pool_=this;batch.protected_=storage;batch.coreProtection_=protection;batch.liveProtection_=live;}
    }
    EventPool(const EventPool&)=delete;
    EventPool& operator=(const EventPool&)=delete;
    // CONTROL-only copied catalogue admission. Old retained queues forbid
    // replacement; wrong-thread and whole invalid-ID input preserve catalogue.
    bool configureParameters(const uint32_t* ids,size_t count) {
        if(std::this_thread::get_id()!=controlOwner_||count>parameterIds_.size())return false;
        Range source{},storage{};
        if(!range(ids,count,sizeof(uint32_t),alignof(uint32_t),source)||
           !range(this,1,sizeof(*this),alignof(EventPool),storage)||overlaps(source,storage)||
           (coreProtection_&&!coreProtection_->allows(source)))return false;
        for(size_t i=0;i<count;++i){
            if(ids[i]==Steinberg::Vst::kNoParamId)return false;
            for(size_t j=0;j<i;++j)if(ids[i]==ids[j])return false;
        }
        bool expected=false;if(!busy_.compare_exchange_strong(expected,true,std::memory_order_acq_rel))return false;
        if(hasForeignReferences()){busy_.store(false,std::memory_order_release);return false;}
        if(count)std::memcpy(parameterIds_.data(),ids,count*sizeof(uint32_t));
        parameterIdCount_=uint32_t(count);busy_.store(false,std::memory_order_release);return true;
    }
    bool knownParameter(uint32_t id)const {
        for(uint32_t i=0;i<parameterIdCount_;++i)if(parameterIds_[i]==id)return true;
        return false;
    }
    class Plan {
        friend class EventPool;
        uint32_t frames_=0,payloadBytes_=0;size_t count_=0;
        const Record* records_=nullptr;const uint8_t* payload_=nullptr;
        EventBuses inputs_{},outputs_{};
        std::array<Range,256> extras_{};uint32_t extraCount_=0;
        bool valid_=false;
    public:Plan()=default;
    };
    class Reservation {
        friend class EventPool;
        EventPool* pool_=nullptr;EventBatch* batch_=nullptr;std::thread::id owner_{};
    public:
        Reservation()=default;
        Reservation(const Reservation&)=delete;
        Reservation& operator=(const Reservation&)=delete;
        ~Reservation(){if(pool_){if(owner_!=std::this_thread::get_id())std::terminate();pool_->busy_.store(false,std::memory_order_release);}}
    };
    EventAdmission preflight(uint32_t frames,const Record* records,size_t count,
            const uint8_t* payload,uint32_t payloadBytes,const EventBuses& inputs,
            const EventBuses& outputs,Plan& result,const Range* extras=nullptr,size_t extraCount=0,size_t rangeLimit=128)const {
        if(frames>65536||count>EventBatch::MaximumEvents||payloadBytes>EventBatch::MaximumPayload||rangeLimit>256||extraCount>rangeLimit)return EventAdmission::Capacity;
        Range sources[5]{},destination{},storage{};
        if(!range(this,1,sizeof(*this),alignof(EventPool),storage)||
           !range(records,count,sizeof(Record),alignof(Record),sources[0])||
           !range(payload,payloadBytes,1,1,sources[1])||
           !range(&inputs,1,sizeof(inputs),alignof(EventBuses),sources[2])||
           !range(&outputs,1,sizeof(outputs),alignof(EventBuses),sources[3])||
           !range(extras,extraCount,sizeof(Range),alignof(Range),sources[4])||
           !range(&result,1,sizeof(result),alignof(Plan),destination)||overlaps(destination,storage)||
           (coreProtection_&&!coreProtection_->allows(destination)))return EventAdmission::Invalid;
        for(const auto source:sources)if(overlaps(source,storage)||overlaps(source,destination)||
           (coreProtection_&&!coreProtection_->allows(source)))return EventAdmission::Invalid;
        for(const auto channels:inputs)if(channels>16)return EventAdmission::Invalid;
        for(const auto channels:outputs)if(channels>16)return EventAdmission::Invalid;
        uint32_t used=0;
        for(size_t i=0;i<count;++i){
            if(!validate(records[i],payload,payloadBytes,frames,inputs,false)||
               (i&&records[i].offset<records[i-1].offset))return EventAdmission::Invalid;
            uint32_t next=0;if(!storedPayloadEnd(records[i],used,next))return EventAdmission::Capacity;used=next;
        }
        Plan candidate;candidate.frames_=frames;candidate.records_=records;candidate.count_=count;
        candidate.payload_=payload;candidate.payloadBytes_=payloadBytes;candidate.inputs_=inputs;candidate.outputs_=outputs;
        for(size_t i=0;i<extraCount;++i){if(extras[i].start>extras[i].end)return EventAdmission::Invalid;candidate.extras_[i]=extras[i];}
        candidate.extraCount_=uint32_t(extraCount);candidate.valid_=true;result=candidate;return EventAdmission::Ok;
    }
    EventAdmission reserve(Reservation& result) {
        Range destination{},storage{};
        if(result.pool_||!range(&result,1,sizeof(result),alignof(Reservation),destination)||
           !range(this,1,sizeof(*this),alignof(EventPool),storage)||overlaps(destination,storage)||
           (coreProtection_&&!coreProtection_->allows(destination)))return EventAdmission::Invalid;
        bool expected=false;if(!busy_.compare_exchange_strong(expected,true,std::memory_order_acq_rel))return EventAdmission::Busy;
        EventBatch* selected=nullptr;
        for(auto& batch:batches_)if(batch.frozen()&&batch.hostOnly()){selected=&batch;break;}
        if(!selected){busy_.store(false,std::memory_order_release);return EventAdmission::Capacity;}
        result.pool_=this;result.batch_=selected;result.owner_=std::this_thread::get_id();return EventAdmission::Ok;
    }
    EventBatch& activate(const Plan& plan,Reservation& reservation)noexcept {
        if(!plan.valid_||reservation.pool_!=this||reservation.owner_!=std::this_thread::get_id())std::terminate();
        auto* selected=reservation.batch_;
        selected->frames_=plan.frames_;selected->inputBuses_=plan.inputs_;selected->outputBuses_=plan.outputs_;
        selected->inputCount_=uint32_t(plan.count_);selected->outputCount_=0;
        selected->inputUsed_=0;selected->outputUsed_=0;
        selected->parameterQueueCount_=0;selected->parameterPointCount_=0;
        for(auto& queue:selected->parameterQueues_){queue.id=Steinberg::Vst::kNoParamId;queue.count=0;queue.head=UINT16_MAX;queue.tail=UINT16_MAX;}
        for(size_t i=0;i<plan.count_;++i)storePayload(plan.records_[i],plan.payload_,selected->inputPayload_.data(),selected->inputUsed_,selected->inputRecords_[i]);
        selected->protectedExtras_=plan.extras_;selected->protectedExtraCount_=plan.extraCount_;
        selected->owner_.store(std::this_thread::get_id(),std::memory_order_release);
        reservation.pool_=nullptr;reservation.batch_=nullptr;
        selected->phase_.store(EventBatch::Active,std::memory_order_release);return *selected;
    }
    EventAdmission prepare(uint32_t frames,const Record* records,size_t count,
            const uint8_t* payload,uint32_t payloadBytes,const EventBuses& inputs,
            const EventBuses& outputs,EventBatch*& result) {
        if(frames>65536||count>EventBatch::MaximumEvents||payloadBytes>EventBatch::MaximumPayload)return EventAdmission::Capacity;
        Range destination{},sources[4]{};
        if(!externalRange(&result,sizeof(result),alignof(EventBatch*))||
           !range(&result,1,sizeof(result),alignof(EventBatch*),destination)||
           !range(records,count,sizeof(Record),alignof(Record),sources[0])||
           !range(payload,payloadBytes,1,1,sources[1])||
           !range(&inputs,1,sizeof(inputs),alignof(EventBuses),sources[2])||
           !range(&outputs,1,sizeof(outputs),alignof(EventBuses),sources[3]))return EventAdmission::Invalid;
        for(const auto source:sources)if(overlaps(source,destination))return EventAdmission::Invalid;
        Plan plan;const auto valid=preflight(frames,records,count,payload,payloadBytes,inputs,outputs,plan);
        if(valid!=EventAdmission::Ok)return valid;
        Reservation reserved;const auto status=reserve(reserved);if(status!=EventAdmission::Ok)return status;
        result=&activate(plan,reserved);return EventAdmission::Ok;
    }
    bool hasForeignReferences()const {for(const auto& batch:batches_)if(!batch.hostOnly())return true;return false;}
    bool observedRetention()const {return retained_.load(std::memory_order_acquire);}
    bool idle()const {return !busy_.load(std::memory_order_acquire);}
    bool externalRange(const void* pointer,size_t size,size_t alignment)const {
        Range requested{},storage{};return range(pointer,size,1,alignment,requested)&&
            range(this,1,sizeof(*this),alignof(EventPool),storage)&&!overlaps(requested,storage)&&
            (!coreProtection_||coreProtection_->allows(requested))&&(!liveProtection_||liveProtection_->allows(requested));
    }
    ~EventPool(){if(!idle()||hasForeignReferences())std::terminate();}
};
static_assert(sizeof(EventPool)<10*1024*1024,"Event pool declared object byte bound");
inline bool EventBatch::sdkPayload(Range requested)const {
    if(coreProtection_&&!coreProtection_->allows(requested))return false;
    Range exactPool{};
    if(!range(pool_,1,sizeof(*pool_),alignof(EventPool),exactPool)||
       (liveProtection_&&!liveProtection_->allowsExcept(requested,exactPool)))return false;
    for(uint32_t i=0;i<protectedExtraCount_;++i){
        const auto protectedRange=protectedExtras_[i];
        if(protectedRange.start==protected_.start&&protectedRange.end==protected_.end)continue;
        if(overlaps(requested,protectedRange))return false;
    }
    return pool_->readablePayload(requested);
}
inline Steinberg::Vst::IParamValueQueue* EventBatch::ParameterList::addParameterData(
        const Steinberg::Vst::ParamID& id,Steinberg::int32& index) {
    if(!batch->admitted())return nullptr;
    Range source{},destination{};
    if(!batch->sdkExternal(&id,sizeof(id),alignof(Steinberg::Vst::ParamID),source)||
       !batch->sdkExternal(&index,sizeof(index),alignof(Steinberg::int32),destination)||
       overlaps(source,destination)||!batch->pool_->knownParameter(id)){batch->bad(1);return nullptr;}
    for(uint32_t i=0;i<batch->parameterQueueCount_;++i)if(batch->parameterQueues_[i].id==id){
        index=Steinberg::int32(i);return &batch->parameterQueues_[i];
    }
    if(batch->parameterQueueCount_==MaximumParameterQueues){batch->bad(4);return nullptr;}
    const auto at=batch->parameterQueueCount_;
    batch->parameterQueues_[at].id=id;index=Steinberg::int32(at);++batch->parameterQueueCount_;
    return &batch->parameterQueues_[at];
}
inline EventBatch::Result EventBatch::List::addEvent(Steinberg::Vst::Event& event) {
    if(!batch->admitted())return Steinberg::kInvalidArgument;
    if(!output){batch->bad(3);return Steinberg::kInvalidArgument;}
    Range source{};
    if(!batch->sdkExternal(&event,sizeof(event),alignof(Steinberg::Vst::Event),source)){
        batch->bad(1);return Steinberg::kInvalidArgument;
    }
    Record record{};NativePayload payload{};
    if(!extract(event,record,payload)) {batch->bad(1);return Steinberg::kInvalidArgument;}
    Range bytes{};
    const uint32_t readable=payload.size+(payload.text?2u:0u);
    if(!range(payload.bytes,readable,1,payload.text?alignof(Steinberg::Vst::TChar):1,bytes)||
       !batch->sdkPayload(bytes)) {batch->bad(1);return Steinberg::kInvalidArgument;}
    if(payload.text){uint16_t terminator=1;std::memcpy(&terminator,payload.bytes+payload.size,2);
        if(terminator){batch->bad(1);return Steinberg::kInvalidArgument;}}
    if(!validate(record,payload.bytes,payload.size,batch->frames_,batch->outputBuses_,true)||
       (batch->outputCount_&&record.offset<batch->outputRecords_[batch->outputCount_-1].offset)){
        batch->bad(1);return Steinberg::kInvalidArgument;
    }
    uint32_t next=0;
    if(batch->outputCount_==MaximumEvents||!storedPayloadEnd(record,batch->outputUsed_,next)){
        batch->bad(4);return Steinberg::kInternalError;
    }
    storePayload(record,payload.bytes,batch->outputPayload_.data(),batch->outputUsed_,batch->outputRecords_[batch->outputCount_]);
    ++batch->outputCount_;return Steinberg::kResultOk;
}
inline uint32_t EventBatch::freeze() {
    if(frozen()||owner_.load(std::memory_order_acquire)!=std::this_thread::get_id())std::terminate();
    const uint32_t failure=phase_.fetch_and(~Active,std::memory_order_acq_rel)&~Active;
    if(!hostOnly())pool_->retained_.store(true,std::memory_order_release);
    pool_->busy_.store(false,std::memory_order_release);return failure;
}
}
#endif
