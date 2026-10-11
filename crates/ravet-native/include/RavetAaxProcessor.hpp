#pragma once
#include "RavetAaxParameters.hpp"
#include "RavetNativeClient.hpp"
#include "RavetNativeState.h"
#include "RavetAaxShortMidiInput.hpp"
#include "RavetAaxTransport.hpp"
#include "RavetAaxAudio.hpp"
#include "RavetAaxMidiOutput.hpp"
#include <atomic>
#include <cstdint>
#include <limits>
#include <type_traits>
#include <optional>
#include "RavetAaxTiming.hpp"
#include "RavetAaxPrecision.hpp"
#include "RavetAaxMeters.hpp"

namespace ravet::aax {
// Optional explicitly prepared host main/aux-planar-f32 owner. CONTROL serializes SDK intent,
// codec/staging and stopped lifecycle; one AUDIO consumer owns prepared DSP.
// Every external table/library/range remains alive through borrowers. No inferred
// host rate/private SDK layout. Instrument selects short notes by default;
// RawShortMidi explicitly selects every defined raw 1..3-byte input message;
// CaptureTransport explicitly selects initialized public-SDK transport capture;
// explicit prepared layout selects main/aux; nonzero MaximumOutputEvents selects
// fixed generated short-MIDI staging. NativeSampleBits explicitly selects native
// 32 or 64 tables; host buffers and parameter/state scalars remain f32. Native64
// owners must be heap-resident: their fixed 1 MiB scratch is constructed on CONTROL.
// Whole input capture precedes history; output narrowing can fail after history
// advanced and poisons readiness without claiming rollback. Aux-generated and
// deadline guarantees remain outside.
// Native setup/process/state statuses are authoritative; backend faults have no
// arbitrary history rollback. Lock-free atomics carry ring/latest process status;
// readiness is atomic; complete diagnostics require exclusive ownership and stop/join.
// EnableDynamicTiming opts into two CONTROL-prepared host-main-f32 effect slots. A
// matching ACK/getter reconciled on CONTROL precedes AUDIO adoption; only complete
// global state migrates, with CONTROL destruction after AUDIO retirement.
template <size_t MaximumParameters = 1024, size_t QueueSlots = 32, bool Instrument = false, size_t MaximumInputEvents = 1024, bool CaptureTransport = false, size_t MaximumOutputEvents = 0, bool EnableDynamicTiming = false, uint32_t NativeSampleBits = 32, bool RawShortMidi = false, size_t MaximumMeters = 0>
class NativeAaxProcessor final {
    static_assert(NativeSampleBits==32||NativeSampleBits==64);
    static_assert(MaximumMeters<=64);
    static_assert(MaximumInputEvents > 0 && MaximumInputEvents <= 1024 && MaximumInputEvents >= QueueSlots);
    static_assert(MaximumParameters > 0 && MaximumParameters <= 1024 && QueueSlots > 1 && QueueSlots <= 1024 && (QueueSlots & (QueueSlots-1)) == 0);
    static_assert(std::atomic<uint32_t>::is_always_lock_free && std::atomic<int32_t>::is_always_lock_free);
    static_assert(MaximumOutputEvents<=1024&&(!MaximumOutputEvents||Instrument));
    static_assert(!RawShortMidi||!MaximumOutputEvents,"Raw short MIDI input selects the input-only Context");
    static_assert(std::atomic<bool>::is_always_lock_free);
    static_assert(!EnableDynamicTiming||(!Instrument&&!CaptureTransport&&!MaximumOutputEvents));
public:
    static constexpr bool HasMidiInput=Instrument||RawShortMidi;
    static constexpr bool GuardedAudio=MaximumOutputEvents||EnableDynamicTiming||RawShortMidi||MaximumMeters;
    enum class PrecisionError:uint32_t{None,Preflight,NarrowOverflow,Exception};
    // Stop/join before reading; local conversion error is separate from the
    // original native process/state code, meaningful only when its Returned flag
    // is true. NarrowOverflow occurs after successful DSP/queue consumption,
    // before any host sample copy or generated MIDI delivery; no history rollback.
    struct PrecisionDiagnostics{PrecisionError error=PrecisionError::None;bool stateAttempted=false,stateReturned=false,nativeAttempted=false,nativeReturned=false,commandsConsumed=false,samplesTransferred=false;int32_t stateStatus=0,nativeStatus=1;AaxNativeDoubleAudio::Failure overflow{};};
    // Stop/join AUDIO before copying. Raw diagnostics describe the last admitted
    // block; native/state status is meaningful only when the matching Returned
    // flag is true. Preflight and Busy never invent a returned native status.
    using RawProcessDiagnostics=PrecisionDiagnostics;
    struct MeterProcessDiagnostics { AaxMeterDiagnostics meter{};bool nativeAttempted=false,nativeReturned=false,stateAttempted=false,stateReturned=false,exception=false;int32_t nativeStatus=1,stateStatus=1; };

    using OutputStage=AaxMidiOutput<MaximumOutputEvents?MaximumOutputEvents:1>;
    // Stop/join AUDIO before copying. nativeStatus is meaningful for this block
    // only when nativeReturned; latest atomic native code may belong to an older
    // block after known preflight rejection. Stage and returned SDK status are
    // independent: native0 never means that later host delivery succeeded.
    struct OutputProcessDiagnostics {
        bool nativeAttempted=false,nativeReturned=false,stateAttempted=false;
        int32_t nativeStatus=1,stateStatus=0;
        uint32_t reportStatus=0,reportCount=0;
        typename OutputStage::StageResult stage{};
        typename OutputStage::DeliveryResult delivery{};
    };
private:
    struct EmptyPrecision{};
    struct PreparedPrecision{AaxNativeDoubleAudio audio{};PrecisionDiagnostics diagnostic{};std::atomic<int32_t> nativeStatus{1};};
    std::conditional_t<NativeSampleBits==64,PreparedPrecision,EmptyPrecision> precision_{};
    struct EmptyOutput {};
    struct PreparedOutput {
        OutputStage stage{};
        std::array<RavetNativeEventV1,MaximumOutputEvents> events{};
        OutputProcessDiagnostics diagnostic{};
        std::atomic<int32_t> nativeStatus{1};
    };
    std::conditional_t<(MaximumOutputEvents>0),PreparedOutput,EmptyOutput> output_{};
    using Values = std::array<RavetNativeParameterValueV1, MaximumParameters>;
    struct Command { uint32_t kind = 0; RavetNativeEventV1 event{}; Values values{}; };
    struct EmptyAudioAdmission{};
    struct PreparedAudioAdmission{std::atomic<bool> processing{false};};
    std::conditional_t<GuardedAudio,PreparedAudioAdmission,EmptyAudioAdmission> audio_{};
    struct EmptyRawDiagnostics{};
    struct PreparedRawDiagnostics{RawProcessDiagnostics diagnostic{};std::atomic<int32_t> nativeStatus{1};};
    std::conditional_t<RawShortMidi,PreparedRawDiagnostics,EmptyRawDiagnostics> raw_{};
    std::array<std::optional<native::NativeProcessorClient>,EnableDynamicTiming?2:1> slots_{};
    std::atomic<uint32_t> activeSlot_{0};
    native::NativeProcessorClient& dsp(){return *slots_[activeSlot_.load(std::memory_order_acquire)];}
    struct EmptyTiming {};
    struct PreparedTiming {
        AaxTimingMailbox mailbox{};
        AaxTimingDiagnostic diagnostic{};
        RavetNativeProcessorConfigV1 config{};
        std::atomic<uint32_t> published{0},retired{0},phase{uint32_t(AaxTimingDiagnostic::Phase::Initial)};
        std::atomic<bool> faulted{false};
        std::atomic<int32_t> nativeProcess{1};
        uint32_t generation=0,candidateSlot=0,oldSlot=0,baseline=0;
        bool enabled=false,busy=false;
    };
    std::conditional_t<EnableDynamicTiming,PreparedTiming,EmptyTiming> timing_{};
    AaxTimingDiagnostic::Phase timingPhase()const{if constexpr(EnableDynamicTiming)return AaxTimingDiagnostic::Phase(timing_.phase.load(std::memory_order_acquire));else return AaxTimingDiagnostic::Phase::Initial;}
    void timingPhase(AaxTimingDiagnostic::Phase value){if constexpr(EnableDynamicTiming){timing_.phase.store(uint32_t(value),std::memory_order_release);}}
    void faultTiming(){if constexpr(EnableDynamicTiming){ready_=false;timing_.faulted.store(true,std::memory_order_release);timingPhase(AaxTimingDiagnostic::Phase::Faulted);}}

    const RavetNativeCachedParameterEntryV1* admitted_=nullptr;
    AaxPreparedMeters<MaximumMeters> meters_;
    struct EmptyMeterProcess{};
    struct PreparedMeterProcess{float* taps=nullptr;bool nativeAttempted=false,nativeReturned=false,stateAttempted=false,stateReturned=false,exception=false;int32_t stateStatus=1;std::atomic<int32_t> nativeStatus{1};};
    std::conditional_t<(MaximumMeters>0),PreparedMeterProcess,EmptyMeterProcess> meterProcess_{};
    NativeAaxParameters<MaximumParameters> schema_;
    const RavetNativeStateEntryV1 *codec_ = nullptr;
    const RavetNativeParameterEntryV1 *parameters_ = nullptr;
    void *validator_ = nullptr;
    Values intent_{};
    std::array<Command, QueueSlots> queue_{};
    std::atomic<uint32_t> head_{0}, tail_{0};
    uint64_t reservation_ = 0;
    bool staged_ = false, valid_ = false;
    std::atomic<bool> ready_{false};
    std::atomic<int32_t> lastProcess_{1};
    uint32_t channels_ = 0, maximumFrames_ = 0;
    const AAX_ITransport *transportProxy_ = nullptr;
    AaxTransportSnapshot transportSnapshot_{};
    AaxTransportResult transportResult_{};
    AaxNativeAudioMap mapper_{};
    AaxNativeAudioBlock mapped_{};
    Command pending_{};
    Values pendingIntent_{};
    static void put(uint8_t *output, uint32_t value) { for (uint32_t i=0;i<4;++i) output[i]=uint8_t(value>>(i*8)); }
    bool encode(const Values &values, std::array<uint8_t, 8208> &bytes) const {
        if (!valid_ || parameters_->state_bytes != 16u + 8u * schema_.count()) return false;
        std::memcpy(bytes.data(),parameters_->state_product,8);put(bytes.data()+8,parameters_->state_version);put(bytes.data()+12,schema_.count());
        for(uint32_t i=0;i<schema_.count();++i){put(bytes.data()+16+i*8,values[i].id);uint32_t bits=0;std::memcpy(&bits,&values[i].value,4);put(bytes.data()+20+i*8,bits);}
        return true;
    }
    bool validateIntent(const Values &values) {
        std::array<uint8_t,8208> bytes{};RavetNativeStateRevisionV1 revision{};
        return encode(values,bytes)&&codec_->publish(validator_,bytes.data(),parameters_->state_bytes,&revision)==0;
    }
    // The supported raw CONTROL producer may publish scalar/state intent while
    // AUDIO renders. It reads CONTROL-owned staging and the SPSC queue indices,
    // never AUDIO diagnostics. Stopped lifecycle/profile operations still require
    // the caller to stop/join AUDIO; observing this bit is not atomic exclusion.
    bool available() const { return valid_ && ready_ && !staged_ && (RawShortMidi || (MaximumMeters&&!MaximumOutputEvents&&!EnableDynamicTiming) || !processingAudio()) && reservation_ != UINT64_MAX && uint32_t(tail_.load(std::memory_order_relaxed)-head_.load(std::memory_order_acquire)) < QueueSlots-1; }
    static bool range(const void *value, size_t bytes, size_t alignment, uintptr_t &first, uintptr_t &last) {
        first=reinterpret_cast<uintptr_t>(value);
        if(!first || first%alignment || bytes>UINTPTR_MAX-first)return false;
        last=first+bytes;return true;
    }
    static bool overlaps(uintptr_t a,uintptr_t b,uintptr_t c,uintptr_t d){return a<d&&c<b;}

public:
    struct Reservation { const void *owner=nullptr;uint64_t generation=0;double normalized=0.; };
private:
    static const RavetNativeCachedParameterEntryV1 *preflightTelemetry(const RavetNativeProcessorEntryV1 *core,const RavetNativeCachedParameterEntryV1 *cache,const RavetNativeTelemetryEntryV1 *table){
        if(!table)return cache;
        if(!core||core->version!=1||core->byte_size!=sizeof(*core)||core->sample_bits!=NativeSampleBits||core->reserved!=0||!core->create||!core->destroy||!core->prepare||!core->reset||!core->deactivate||!core->process||!core->timing||table->version!=1||table->byte_size!=sizeof(*table)||table->sample_bits!=NativeSampleBits||table->telemetry_count>1024||!table->processor_entry||!table->create_cache||!table->destroy_cache||!table->info||!table->snapshot)return nullptr;
        try{return table->processor_entry()==core?cache:nullptr;}catch(...){return nullptr;}
    }
    // Query complete immutable provenance before any cache, DSP or codec owner
    // is acquired. Callback-equivalent table copies are never exact origins.
    static const RavetNativeCachedParameterEntryV1* preflightTables(const RavetNativeProcessorEntryV1* core,const RavetNativeAudioStateEntryV1* audio,const RavetNativeCachedParameterEntryV1* cache,const RavetNativeStateEntryV1* codec,const RavetNativeTelemetryEntryV1* telemetry){
        try{
            if(!core||core->version!=1||core->byte_size!=sizeof(*core)||core->sample_bits!=NativeSampleBits||core->reserved||!core->create||!core->destroy||!core->prepare||!core->reset||!core->deactivate||!core->process||!core->timing)return nullptr;
            if(!cache||cache->version!=1||cache->byte_size!=sizeof(*cache)||cache->sample_bits!=NativeSampleBits||!cache->parameter_count||cache->parameter_count>MaximumParameters||!cache->parameters_entry||!cache->create||!cache->destroy||!cache->info||!cache->normalize||!cache->denormalize||!cache->convert_events)return nullptr;
            const auto* parameters=cache->parameters_entry();
            if(!parameters||parameters->version!=1||parameters->byte_size!=sizeof(*parameters)||parameters->sample_bits!=NativeSampleBits||parameters->parameter_count!=cache->parameter_count||!parameters->processor_entry||parameters->processor_entry()!=core||!parameters->state_version||parameters->state_bytes!=16u+8u*cache->parameter_count)return nullptr;
            if(!audio||audio->version!=1||audio->byte_size!=sizeof(*audio)||audio->sample_bits!=NativeSampleBits||audio->parameter_count!=cache->parameter_count||!audio->cached_parameters_entry||audio->cached_parameters_entry()!=cache||!audio->apply||!audio->snapshot)return nullptr;
            if(!codec||codec->version!=1||codec->byte_size!=sizeof(*codec)||codec->sample_bits!=NativeSampleBits||codec->parameter_count!=cache->parameter_count||!codec->parameters_entry||codec->parameters_entry()!=parameters||!codec->create||!codec->destroy||!codec->publish||!codec->snapshot||!codec->save_state)return nullptr;
            return preflightTelemetry(core,cache,telemetry);
        }catch(...){return nullptr;}
    }
public:
    NativeAaxProcessor(const RavetNativeProcessorEntryV1 *core,const RavetNativeAudioStateEntryV1 *audio,
                       const RavetNativeCachedParameterEntryV1 *cache,const RavetNativeStateEntryV1 *codec,const RavetNativeTelemetryEntryV1 *telemetry=nullptr,const AaxMeterBinding* meterBindings=nullptr)
        : admitted_(preflightTables(core,audio,cache,codec,telemetry)),meters_(admitted_?core:nullptr,telemetry,meterBindings),schema_(meters_.valid()?admitted_:nullptr,NativeSampleBits),codec_(codec) {
        slots_[0].emplace(!schema_.valid()?nullptr:core,audio,telemetry);
        if(!dsp().valid()||dsp().sampleBits()!=NativeSampleBits||!dsp().hasAudioState()||!schema_.valid()||dsp().parameterCount()!=schema_.count()||!audio||!audio->cached_parameters_entry)return;
        const auto *origin=audio->cached_parameters_entry();
        if(!cache||origin!=cache)return;
        parameters_=schema_.parametersEntry();
        if(!codec||codec->version!=1||codec->byte_size!=sizeof(*codec)||codec->sample_bits!=NativeSampleBits||codec->parameter_count!=schema_.count()||!codec->parameters_entry||codec->parameters_entry()!=parameters_||!codec->create||!codec->destroy||!codec->publish||!codec->snapshot||!codec->save_state||!parameters_->state_version||parameters_->state_bytes!=16u+8u*schema_.count())return;
        validator_=codec->create(nullptr,0);if(!validator_)return;
        RavetNativeStateRevisionV1 revision{};
        if(codec->snapshot(validator_,intent_.data(),uint32_t(intent_.size()),&revision))return;
        valid_=true;
        // Initial codec snapshot must retain the declared complete native shape.
        for(uint32_t i=0;i<schema_.count();++i)if(intent_[i].id!=schema_.info(schema_.id(i))->id){valid_=false;return;}
        if(!validateIntent(intent_))valid_=false;
    }
    ~NativeAaxProcessor(){if(validator_)codec_->destroy(validator_);}
    NativeAaxProcessor(const NativeAaxProcessor&)=delete;
    NativeAaxProcessor& operator=(const NativeAaxProcessor&)=delete;
    class AudioAdmission {
        friend class NativeAaxProcessor;
        NativeAaxProcessor *owner_=nullptr;
        bool consumed_=false;
        explicit AudioAdmission(NativeAaxProcessor *owner):owner_(owner){}
    public:
        AudioAdmission(const AudioAdmission&)=delete;
        AudioAdmission& operator=(const AudioAdmission&)=delete;
        ~AudioAdmission(){if constexpr(GuardedAudio){if(owner_)owner_->audio_.processing.store(false,std::memory_order_release);}}
        explicit operator bool()const{return owner_!=nullptr;}
    };
    // Whole registered callback admission begins BEFORE foreign input getters.
    // Exclusive AUDIO ownership required; one lock-free CAS rejects nested/concurrent
    // admission without spinning or mutation. CONTROL availability reads only this
    // atomic admission bit, never AUDIO-owned output storage or diagnostics.
    // This is AUDIO admission, not a lock for stopped CONTROL check-then-act calls.
    AudioAdmission admitAudio(){
        if constexpr(!GuardedAudio)return AudioAdmission(nullptr);
        else{bool expected=false;if(!audio_.processing.compare_exchange_strong(expected,true,std::memory_order_acq_rel,std::memory_order_acquire))return AudioAdmission(nullptr);if constexpr(MaximumMeters){meters_.beginAudio();meterProcess_.taps=nullptr;meterProcess_.nativeAttempted=false;meterProcess_.nativeReturned=false;meterProcess_.stateAttempted=false;meterProcess_.stateReturned=false;meterProcess_.exception=false;meterProcess_.stateStatus=1;}return AudioAdmission(this);}
    }
    AudioAdmission admitGeneratedAudio(){if constexpr(MaximumOutputEvents)return admitAudio();else return AudioAdmission(nullptr);}
    AudioAdmission admitTimedAudio(){if constexpr(EnableDynamicTiming)return admitAudio();else return AudioAdmission(nullptr);}
    AudioAdmission admitRawAudio(){if constexpr(RawShortMidi&&!MaximumOutputEvents)return admitAudio();else return AudioAdmission(nullptr);}
    bool processingAudio()const{if constexpr(GuardedAudio)return audio_.processing.load(std::memory_order_acquire);else return false;}
    OutputProcessDiagnostics outputDiagnosticsStopped()const{if constexpr(MaximumOutputEvents)return output_.diagnostic;else return {};}
    RawProcessDiagnostics rawDiagnosticsStopped()const{if constexpr(RawShortMidi)return raw_.diagnostic;else return {};}
    PrecisionDiagnostics precisionDiagnosticsStopped()const{if constexpr(NativeSampleBits==64)return precision_.diagnostic;else return {};}
    int32_t lastNativeProcessStatus()const{if constexpr(NativeSampleBits==64)return precision_.nativeStatus.load(std::memory_order_acquire);else if constexpr(RawShortMidi)return raw_.nativeStatus.load(std::memory_order_acquire);else if constexpr(MaximumOutputEvents)return output_.nativeStatus.load(std::memory_order_acquire);else if constexpr(MaximumMeters)return meterProcess_.nativeStatus.load(std::memory_order_acquire);else if constexpr(EnableDynamicTiming)return timing_.nativeProcess.load(std::memory_order_acquire);else return lastProcessStatus();}
    bool valid()const{return valid_;}bool ready()const{return ready_;}
    int32_t lastProcessStatus()const{return lastProcess_.load(std::memory_order_acquire);}
    uint32_t channels()const{return channels_;}
    // Stopped/serialized caller only: proxy is borrowed through the model lifetime.
    // Observation copies require AUDIO stopped/joined; these are not atomic snapshots.
    void attachTransportStopped(const AAX_ITransport *proxy){transportProxy_=CaptureTransport?proxy:nullptr;}
    AaxTransportSnapshot transportSnapshotStopped()const{return transportSnapshot_;}
    AaxTransportResult transportResultStopped()const{return transportResult_;}
    // Diagnostics describe latest transport collection attempt; earlier map/MIDI/
    // capacity rejection leaves them unchanged. Native AUDIO status stays separate.
    void invalidate(){ready_=false;} // readiness-only CONTROL failure; stop/join before lifecycle
    uint32_t stateBytes()const{return valid_?parameters_->state_bytes:0;}
    const NativeAaxParameters<MaximumParameters>& schema()const{return schema_;}
    // Whole known profile faults preserve an existing owner; backend invocation
    // invalidates readiness until complete authoritative setup/state success.
    int32_t prepare(uint32_t channels,double rate,uint32_t frames=1024){
        if(channels!=1&&channels!=2)return 1;
        return prepareLayoutStopped(AaxNativeAudioLayout::main(Instrument?0u:channels,channels,frames),rate);
    }
    // Stop/join AUDIO first. Whole known layout/rate/profile validation precedes
    // backend calls. Backend setup failure is fail-closed; no history rollback.
    int32_t prepareLayoutStopped(const AaxNativeAudioLayout &layout,double rate){
        AaxNativeAudioMap next{};
        if constexpr(EnableDynamicTiming){if(timing_.faulted.load(std::memory_order_acquire)||(timing_.enabled&&timingPhase()!=AaxTimingDiagnostic::Phase::Initial))return 1;if(layout.busCount!=1)return 1;}
        if(!valid_||staged_||processingAudio()||!native::scalar::finite(rate)||rate<=0.||!next.prepare(layout))return 1;
        if constexpr(Instrument){if(layout.inputCapacity)return 1;}
        RavetNativeProcessorConfigV1 config{};config.version=1;config.maximum_frames=layout.maximumFrames;config.maximum_events=uint32_t(HasMidiInput?MaximumInputEvents:QueueSlots);config.maximum_output_events=uint32_t(MaximumOutputEvents);config.bus_count=layout.busCount;config.sample_rate=rate;std::copy(layout.buses.begin(),layout.buses.end(),std::begin(config.buses));
        if constexpr(MaximumOutputEvents){if(!output_.stage.invalidate())return 1;}
        if constexpr(NativeSampleBits==64){if(!precision_.audio.prepare(layout))return 1;}
        ready_=false;mapper_=next;mapped_=AaxNativeAudioBlock{};channels_=layout.buses[0].outputs;maximumFrames_=layout.maximumFrames;
        auto status=dsp().prepare(config);if(status)return status;status=dsp().applyState(intent_.data(),schema_.count());if(status)return status;
        head_.store(0,std::memory_order_relaxed);tail_.store(0,std::memory_order_relaxed);
        if constexpr(EnableDynamicTiming){timing_.config=config;if(timing_.enabled){RavetNativeProcessorTimingV1 raw{};AaxNativeTiming canonical{};status=dsp().timing(raw);if(status)return status;if(!AaxNativeTiming::canonical(raw,canonical)||canonical.latency!=timing_.diagnostic.currentLatency)return 1;timing_.diagnostic.current=raw;}}
        ready_=true;return 0;
    }
    AaxNativeAudioLayout layoutStopped()const{return mapper_.layout();}
    // Immutable prepared geometry read under admitted raw AUDIO (or stopped/joined
    // ownership). Never read pointer-array contents before the public node getter.
    bool rawNodeArrayMetadataDisjoint(const AAX_IMIDINode* node,const float* const* inputs,float* const* outputs)const{
        if constexpr(!RawShortMidi&&!MaximumMeters)return false;
        else{uintptr_t nf=0,nl=0,first=0,last=0;if(!range(node,sizeof(*node),alignof(AAX_IMIDINode),nf,nl))return false;
            const auto& layout=mapper_.layout();
            if(layout.inputCapacity&&(!range(inputs,size_t(layout.inputCapacity)*sizeof(float*),alignof(float*),first,last)||overlaps(nf,nl,first,last)))return false;
            if(layout.outputCapacity&&(!range(outputs,size_t(layout.outputCapacity)*sizeof(float*),alignof(float*),first,last)||overlaps(nf,nl,first,last)))return false;
            return true;}
    }

    // Optional exact native telemetry cache is constructed/retired on CONTROL.
    // Caller must stop/join AUDIO; these observations are not synchronized host
    // parameters or an implicit cross-thread publication/scheduling mechanism.
    int32_t snapshotTelemetryStopped(RavetNativeTelemetryValueV1 *values,uint32_t capacity){
        uintptr_t first=0,last=0;const auto self=reinterpret_cast<uintptr_t>(this);
        if(!ready_||processingAudio()||capacity>1024||!range(values,sizeof(*values)*size_t(capacity),alignof(RavetNativeTelemetryValueV1),first,last)||overlaps(first,last,self,self+sizeof(*this)))return 1;
        return dsp().snapshotTelemetry(values,capacity);
    }
    // CONTROL after stop/join; SDK tap publication never makes these a live UI cache.
    MeterProcessDiagnostics meterDiagnosticsStopped()const{if constexpr(MaximumMeters)return {meters_.diagnosticsStopped(),meterProcess_.nativeAttempted,meterProcess_.nativeReturned,meterProcess_.stateAttempted,meterProcess_.stateReturned,meterProcess_.exception,meterProcess_.nativeStatus.load(std::memory_order_acquire),meterProcess_.stateStatus};else return {};}
    AaxMeterDiagnostics candidateMeterDiagnosticsStopped()const{if constexpr(MaximumMeters)return meters_.candidateDiagnosticsStopped();else return {};}
    const AaxMeterBinding* meterBindingStopped(uint32_t index)const{if constexpr(MaximumMeters)return meters_.binding(index);else return nullptr;}
    // Callback caller owns the known Context ranges. Add slot/tap spans before
    // rejecting them so supported silence can never overwrite admitted metadata.
    bool prepareMetersAdmitted(AudioAdmission& admission,const float* const* inputs,float* const* outputs,int32_t frames,int32_t sidechainFirst,float** tapSlot,AaxAudioProtectedRange* metadata,uint32_t& count,uint32_t capacity){
        if constexpr(!MaximumMeters)return false;
        else{
            if(admission.owner_!=this||admission.consumed_||!audio_.processing.load(std::memory_order_acquire)||capacity>12||count>capacity||capacity-count<2)return false;
            uintptr_t mf=0,ml=0,sf=0,sl=0,tf=0,tl=0;
            const auto self=reinterpret_cast<uintptr_t>(this);
            if(!range(metadata,size_t(capacity)*sizeof(*metadata),alignof(AaxAudioProtectedRange),mf,ml)||overlaps(mf,ml,self,self+sizeof(*this)))return false;
            const auto slot=reinterpret_cast<uintptr_t>(tapSlot);if(slot&&slot<=UINTPTR_MAX-sizeof(float*))metadata[count++]={slot,slot+sizeof(float*)};
            if(!range(tapSlot,sizeof(float*),alignof(float*),sf,sl)||!ready_||frames<0||uint32_t(frames)>maximumFrames_)return false;
            if(overlaps(sf,sl,self,self+sizeof(*this)))return false;
            for(uint32_t i=0;i+1<count;++i)if(metadata[i].first>metadata[i].last||overlaps(sf,sl,metadata[i].first,metadata[i].last))return false;
            std::array<AaxAudioProtectedRange,16> protectedRanges{};uint32_t protectedCount=0;AaxNativeAudioBlock temporary{};
            if(!protection(0,0,0,0,metadata,count,protectedRanges,protectedCount)||!mapper_.map(inputs,outputs,uint32_t(frames),sidechainFirst,temporary,protectedRanges.data(),protectedCount))return false;
            float* taps=nullptr;std::memcpy(&taps,tapSlot,sizeof(taps));const auto address=reinterpret_cast<uintptr_t>(taps);
            if(address&&sizeof(float)*MaximumMeters<=UINTPTR_MAX-address)metadata[count++]={address,address+sizeof(float)*MaximumMeters};
            if(!range(taps,sizeof(float)*MaximumMeters,alignof(float),tf,tl)||overlaps(tf,tl,self,self+sizeof(*this)))return false;
            for(uint32_t i=0;i+1<count;++i)if(metadata[i].first>metadata[i].last||overlaps(tf,tl,metadata[i].first,metadata[i].last))return false;
            if(!protection(0,0,0,0,metadata,count,protectedRanges,protectedCount)||!mapper_.map(inputs,outputs,uint32_t(frames),sidechainFirst,temporary,protectedRanges.data(),protectedCount))return false;
            meterProcess_.taps=taps;return true;
        }
    }
    bool requiresSidechainInput()const{for(uint32_t bus=0;bus<mapper_.layout().busCount;++bus)if(mapper_.layout().inputFirst[bus]==AaxNativeAudioLayout::Sidechain)return true;return false;}
    int32_t reset(){if constexpr(EnableDynamicTiming){if(timing_.faulted.load(std::memory_order_acquire)||(timing_.enabled&&timingPhase()!=AaxTimingDiagnostic::Phase::Initial))return 1;}if(!ready_||staged_||processingAudio())return 1;if constexpr(MaximumOutputEvents)output_.stage.invalidate();ready_=false;auto status=dsp().reset();if(!status)status=dsp().applyState(intent_.data(),schema_.count());if(!status){head_.store(0,std::memory_order_relaxed);tail_.store(0,std::memory_order_relaxed);ready_=true;}return status;}
    int32_t deactivate(){if(processingAudio())return 1;if constexpr(EnableDynamicTiming){timing_.published.store(0,std::memory_order_release);timing_.enabled=false;for(uint32_t i=0;i<2;++i)if(i!=activeSlot_.load(std::memory_order_relaxed))slots_[i].reset();}if constexpr(MaximumOutputEvents)output_.stage.invalidate();ready_=false;transportProxy_=nullptr;staged_=false;auto status=dsp().deactivate();head_.store(0,std::memory_order_relaxed);tail_.store(0,std::memory_order_relaxed);return status;}
    struct TimingTables {const RavetNativeProcessorEntryV1 *core=nullptr;const RavetNativeAudioStateEntryV1 *audio=nullptr;const RavetNativeCachedParameterEntryV1 *cache=nullptr;const RavetNativeStateEntryV1 *codec=nullptr;const RavetNativeTelemetryEntryV1* telemetry=nullptr;};
    bool timingFaulted()const{if constexpr(EnableDynamicTiming)return timing_.faulted.load(std::memory_order_acquire);else return false;}
    AaxTimingDiagnostic timingStopped()const{if constexpr(EnableDynamicTiming){auto out=timing_.diagnostic;out.phase=timingPhase();out.notices=timing_.mailbox.snapshot();return out;}else return {};}
    AAX_Result timingNotification(AAX_CTypeID type,const void *data,uint32_t size){if constexpr(EnableDynamicTiming)return timing_.mailbox.intake(type,data,size);else return AAX_ERROR_UNIMPLEMENTED;}
    AAX_Result checkInitialTimingStopped(uint32_t declared,AAX_IController *controller){
        if constexpr(!EnableDynamicTiming)return AAX_ERROR_UNIMPLEMENTED;
        else{
            if(timingFaulted()||!ready_||timing_.enabled||!controller||declared>uint32_t(INT32_MAX))return AAX_ERROR_INVALID_ARGUMENT;
            auto& d=timing_.diagnostic;d={};d.nativeAttempted=true;
            RavetNativeProcessorTimingV1 raw{};AaxNativeTiming value{};d.nativeStatus=dsp().timing(raw);d.nativeReturned=true;
            if(d.nativeStatus||!AaxNativeTiming::canonical(raw,value)||value.latency!=int32_t(declared)){ready_=false;return AAX_ERROR_INVALID_ARGUMENT;}
            d.getterAttempted=true;try{d.getterStatus=controller->GetSignalLatency(&d.observedLatency);d.getterReturned=true;}catch(...){ready_=false;return AAX_ERROR_NULL_OBJECT;}
            if(d.getterStatus||d.observedLatency!=value.latency){ready_=false;return d.getterStatus?d.getterStatus:AAX_ERROR_INVALID_ARGUMENT;}
            d.current=raw;d.currentLatency=value.latency;timing_.enabled=true;return AAX_SUCCESS;
        }
    }
    // Stop/join before stage; old DSP/history remains untouched on every rejection.
    int32_t stageTimingCandidateStopped(TimingTables tables){
        if constexpr(!EnableDynamicTiming)return 1;
        else{
            using Phase=AaxTimingDiagnostic::Phase;
            if(!ready_||!timing_.enabled||timing_.busy||staged_||processingAudio()||timingPhase()!=Phase::Initial||timing_.faulted.load(std::memory_order_acquire)||timing_.generation==uint32_t(INT32_MAX))return 1;
            auto& d=timing_.diagnostic;d.nativeAttempted=false;d.nativeReturned=false;
            if(!preflightTables(tables.core,tables.audio,tables.cache,tables.codec,tables.telemetry))return 1;
            if constexpr(MaximumMeters){if(!meters_.compatibleStopped(tables.core,tables.telemetry))return 1;}
            NativeAaxParameters<MaximumParameters> candidate(tables.cache,NativeSampleBits);
            if(!candidate.valid()||candidate.count()!=schema_.count())return 1;
            const auto *p=candidate.parametersEntry();
            if(!p||p->state_bytes!=parameters_->state_bytes||p->state_version!=parameters_->state_version||p->sample_bits!=parameters_->sample_bits||std::memcmp(p->state_product,parameters_->state_product,8))return 1;
            for(uint32_t i=0;i<schema_.count();++i)if(std::memcmp(candidate.info(candidate.id(i)),schema_.info(schema_.id(i)),sizeof(RavetNativeParameterInfoV1)))return 1;
            if(!tables.audio||!tables.audio->cached_parameters_entry||!tables.cache)return 1;
            const auto *origin=tables.audio->cached_parameters_entry();
            if(origin!=tables.cache)return 1;
            const auto *c=tables.codec;
            if(!c||c->version!=1||c->byte_size!=sizeof(*c)||c->sample_bits!=NativeSampleBits||c->parameter_count!=schema_.count()||!c->parameters_entry||c->parameters_entry()!=p||!c->create||!c->destroy||!c->publish||!c->snapshot||!c->save_state)return 1;
            std::array<uint8_t,8208> bytes{};if(!encode(intent_,bytes))return 1;
            void *validator=c->create(nullptr,0);if(!validator)return 1;
            struct Validator{const RavetNativeStateEntryV1* codec;void* handle;~Validator(){codec->destroy(handle);}} owner{c,validator};
            RavetNativeStateRevisionV1 revision{};d.nativeAttempted=true;const auto code=c->publish(validator,bytes.data(),p->state_bytes,&revision);d.nativeReturned=true;d.nativeStatus=code;if(code)return code;
            const auto index=1u-activeSlot_.load(std::memory_order_acquire);if(slots_[index])return 1;
            slots_[index].emplace(tables.core,tables.audio,tables.telemetry);auto& next=*slots_[index];
            auto rejectCandidate=[&](int32_t status){slots_[index].reset();return status;};
            if(!next.valid()||next.sampleBits()!=NativeSampleBits||next.parameterCount()!=schema_.count()||!next.hasAudioState())return rejectCandidate(1);
            d.nativeAttempted=true;d.nativeReturned=false;auto status=next.prepare(timing_.config);d.nativeReturned=true;d.nativeStatus=status;if(status)return rejectCandidate(status);
            status=next.applyState(intent_.data(),schema_.count());d.nativeStatus=status;if(status)return rejectCandidate(status);
            RavetNativeProcessorTimingV1 raw{};AaxNativeTiming value{};status=next.timing(raw);d.nativeStatus=status;if(status)return rejectCandidate(status);
            if(!AaxNativeTiming::canonical(raw,value)||value.latency==d.currentLatency)return rejectCandidate(1);
            d.nativeStatus=0;d.candidate=raw;d.requestedLatency=value.latency;timingPhase(Phase::Candidate);d.generation=++timing_.generation;
            timing_.candidateSlot=index;timing_.oldSlot=activeSlot_.load(std::memory_order_acquire);return 0;
        }
    }
    AAX_Result requestStagedTimingControl(AAX_IController *controller){
        if constexpr(!EnableDynamicTiming)return AAX_ERROR_UNIMPLEMENTED;
        else{
            using Phase=AaxTimingDiagnostic::Phase;auto& d=timing_.diagnostic;
            if(!controller||!ready_||timing_.busy||timingPhase()!=Phase::Candidate)return AAX_ERROR_INVALID_ARGUMENT;
            const auto notice=timing_.mailbox.snapshot();if(notice.exhausted||notice.compensation==AaxTimingMailbox::Compensation::Disabled)return AAX_ERROR_INVALID_ARGUMENT;
            timing_.baseline=notice.sequence;timing_.busy=true;struct Busy{bool& flag;~Busy(){flag=false;}} busy{timing_.busy};
            uint32_t expected=uint32_t(Phase::Candidate);if(!timing_.phase.compare_exchange_strong(expected,uint32_t(Phase::Request),std::memory_order_acq_rel))return AAX_ERROR_INVALID_ARGUMENT;d.requestAttempted=true;d.requestReturned=false;
            try{d.requestStatus=controller->SetSignalLatency(d.requestedLatency);d.requestReturned=true;}catch(...){faultTiming();return AAX_ERROR_NULL_OBJECT;}
            if(d.requestStatus){
                if(timing_.mailbox.snapshot().sequence!=timing_.baseline){faultTiming();}
                else{slots_[timing_.candidateSlot].reset();uint32_t expected=uint32_t(Phase::Request);(void)timing_.phase.compare_exchange_strong(expected,uint32_t(Phase::Initial),std::memory_order_acq_rel);}
                return d.requestStatus;
            }
            uint32_t pending=uint32_t(Phase::Request);if(!timing_.phase.compare_exchange_strong(pending,uint32_t(Phase::Pending),std::memory_order_acq_rel))return AAX_ERROR_NULL_OBJECT;return AAX_SUCCESS;
        }
    }
    // Reconcile before next AUDIO after ACK; query match alone is never an ACK.
    AAX_Result reconcileTimingControl(AAX_IController *controller){
        if constexpr(!EnableDynamicTiming)return AAX_ERROR_UNIMPLEMENTED;
        else{
            using Phase=AaxTimingDiagnostic::Phase;auto& d=timing_.diagnostic;
            if(!controller||!timing_.enabled||timing_.busy||timingPhase()==Phase::Faulted)return AAX_ERROR_INVALID_ARGUMENT;
            const auto notice=timing_.mailbox.snapshot();if(notice.exhausted){timing_.faulted.store(true,std::memory_order_release);timingPhase(Phase::Faulted);return AAX_ERROR_INVALID_ARGUMENT;}
            if(timingPhase()!=Phase::Pending&&timingPhase()!=Phase::Initial)return AAX_ERROR_INVALID_ARGUMENT;
            if(notice.sequence==timing_.baseline||!notice.eligible)return AAX_SUCCESS;
            timing_.busy=true;struct Busy{bool& flag;~Busy(){flag=false;}} busy{timing_.busy};d.getterAttempted=true;d.getterReturned=false;
            try{d.getterStatus=controller->GetSignalLatency(&d.observedLatency);d.getterReturned=true;}catch(...){faultTiming();return AAX_ERROR_NULL_OBJECT;}
            const auto expected=timingPhase()==Phase::Pending?d.requestedLatency:d.currentLatency;
            if(d.getterStatus||d.observedLatency!=expected){faultTiming();return d.getterStatus?d.getterStatus:AAX_ERROR_INVALID_ARGUMENT;}
            timing_.baseline=notice.sequence;d.ackSequence=notice.sequence;
            if(timingPhase()==Phase::Pending){uint32_t expected=uint32_t(Phase::Pending);if(!timing_.phase.compare_exchange_strong(expected,uint32_t(Phase::Published),std::memory_order_acq_rel))return AAX_ERROR_NULL_OBJECT;timing_.published.store((d.generation<<1)|timing_.candidateSlot,std::memory_order_release);}
            return AAX_SUCCESS;
        }
    }
    AAX_Result collectRetiredTimingControl(){
        if constexpr(!EnableDynamicTiming)return AAX_ERROR_UNIMPLEMENTED;
        else{const auto token=timing_.retired.load(std::memory_order_acquire);if(!token)return AAX_ERROR_INVALID_ARGUMENT;auto status=slots_[timing_.oldSlot]->deactivate();timing_.diagnostic.nativeAttempted=true;timing_.diagnostic.nativeReturned=true;timing_.diagnostic.nativeStatus=status;if(status)return AAX_ERROR_NULL_OBJECT;slots_[timing_.oldSlot].reset();timing_.retired.store(0,std::memory_order_release);timingPhase(timingFaulted()?AaxTimingDiagnostic::Phase::Faulted:AaxTimingDiagnostic::Phase::Initial);return AAX_SUCCESS;}
    }
    // Staging borrows no SDK/DSP. Codec work is isolated CONTROL validation.
    // Reserve before host/manager mutation. One producer must commit/cancel;
    // nested staging rejects and leaves an outer reservation intact.
    bool stageParameter(const char *id,double value,Reservation &output){
        if(!available())return false;
        Command next{};next.kind=1;double normalized=0.;
        if(!schema_.event(id,value,next.event)||!schema_.control(id,value,normalized))return false;
        auto values=intent_;const auto index=schema_.index(id);values[index].value=float(next.event.value);
        // Known scalar mapping preserves already validated complete intent;
        // codecs remain CONTROL save/restore/construction work, never setters.
        pending_=next;pendingIntent_=values;staged_=true;++reservation_;output={this,reservation_,normalized};return true;
    }
    bool stageRestore(const uint8_t *bytes,uint32_t length,Reservation &output){
        if(!available()||!bytes||!length||length>8208)return false;
        RavetNativeStateRevisionV1 revision{};Command next{};next.kind=2;
        if(codec_->publish(validator_,bytes,length,&revision)||codec_->snapshot(validator_,next.values.data(),uint32_t(next.values.size()),&revision))return false;
        pendingIntent_=next.values;pending_=next;staged_=true;++reservation_;output={this,reservation_,0.};return true;
    }
    bool commit(const Reservation &reservation){
        if(!staged_||reservation.owner!=this||reservation.generation!=reservation_)return false;
        const auto tail=tail_.load(std::memory_order_relaxed);queue_[tail%QueueSlots]=pending_;intent_=pendingIntent_;staged_=false;tail_.store(tail+1,std::memory_order_release);return true;
    }
    bool stagedValue(const Reservation &reservation,uint32_t index,RavetNativeParameterValueV1 &output)const{
        if(!staged_||reservation.owner!=this||reservation.generation!=reservation_||index>=schema_.count())return false;
        output=pendingIntent_[index];return true;
    }
    bool cancel(const Reservation &reservation){if(!staged_||reservation.owner!=this||reservation.generation!=reservation_)return false;staged_=false;return true;}
    // Save acknowledged complete CONTROL intent, including commands pending audio.
    // The native codec authoritatively validates and emits canonical bytes.
    bool save(uint8_t *bytes,uint32_t capacity){
        if(!valid_||!bytes||capacity<stateBytes()||capacity>8208||!validateIntent(intent_))return false;
        RavetNativeStateRevisionV1 revision{};return codec_->save_state(validator_,bytes,capacity,&revision)==0;
    }
    bool intentValue(uint32_t index,RavetNativeParameterValueV1 &output)const{if(!valid_||index>=schema_.count())return false;output=intent_[index];return true;}
    void silence(float *const *outputs,int32_t frames,const float *const *inputs=nullptr,uintptr_t headerFirst=0,uintptr_t headerLast=0,uintptr_t packetsFirst=0,uintptr_t packetsLast=0,const AaxAudioProtectedRange *callbackMetadata=nullptr,uint32_t callbackCount=0)const{
        std::array<AaxAudioProtectedRange,16> protectedRanges{};uint32_t count=0;
        if(!protection(headerFirst,headerLast,packetsFirst,packetsLast,callbackMetadata,callbackCount,protectedRanges,count))return;
        mapper_.silence(inputs,outputs,frames,protectedRanges.data(),count);
    }
    int32_t rejectLayoutCallback(const float *const *inputs,float *const *outputs,int32_t frames,const AaxAudioProtectedRange *callbackMetadata,uint32_t callbackCount){
        // This callback was admitted, but its known Context metadata failed before
        // processOwned. Publish a fresh local refusal without replacing the last
        // actually returned backend status or inspecting untrusted frame storage.
        if constexpr(RawShortMidi){raw_.diagnostic={};raw_.diagnostic.error=PrecisionError::Preflight;}
        if constexpr(NativeSampleBits==64){precision_.diagnostic={};precision_.diagnostic.error=PrecisionError::Preflight;}
        silence(outputs,frames,inputs,0,0,0,0,callbackMetadata,callbackCount);lastProcess_.store(1,std::memory_order_release);return 1;}
    int32_t process(const float *const *inputs,float *const *outputs,int32_t frames){return processLayout(inputs,outputs,frames,-1);}
    // Explicit accepted flat metadata capacities and dynamic sidechain index.
    int32_t processLayout(const float *const *inputs,float *const *outputs,int32_t frames,int32_t sidechainFirst,const AaxAudioProtectedRange *callbackMetadata=nullptr,uint32_t callbackCount=0){
        if constexpr(MaximumOutputEvents||RawShortMidi||MaximumMeters)return 1;
        if constexpr(EnableDynamicTiming){auto admission=admitTimedAudio();if(!admission)return 1;return processLayoutAdmitted(admission,inputs,outputs,frames,sidechainFirst,callbackMetadata,callbackCount);}
        else{const auto status=processOwned(inputs,outputs,frames,nullptr,sidechainFirst,callbackMetadata,callbackCount);lastProcess_.store(status,std::memory_order_release);return status;}
    }
    int32_t processLayoutAdmitted(AudioAdmission& admission,const float *const *inputs,float *const *outputs,int32_t frames,int32_t sidechainFirst,const AaxAudioProtectedRange *metadata,uint32_t count){
        if constexpr(!EnableDynamicTiming&&!MaximumMeters)return 1;
        else{if(admission.owner_!=this||admission.consumed_||!audio_.processing.load(std::memory_order_acquire))return 1;admission.consumed_=true;const auto status=processOwned(inputs,outputs,frames,nullptr,sidechainFirst,metadata,count);lastProcess_.store(status,std::memory_order_release);return status;}
    }
    // Instrument profile only. Stream/packets are immutable borrowed AUDIO ranges.
    int32_t processNotes(float *const *outputs,int32_t frames,const AAX_CMidiStream *stream,const AaxAudioProtectedRange *callbackMetadata=nullptr,uint32_t callbackCount=0){
        if constexpr(!Instrument||MaximumOutputEvents||MaximumMeters)return 1;
        if constexpr(RawShortMidi){auto admission=admitRawAudio();if(!admission)return 3;return processRawNotesAdmitted(admission,outputs,frames,stream,callbackMetadata,callbackCount);}
        else{const auto status=Instrument?processOwned(nullptr,outputs,frames,stream,-1,callbackMetadata,callbackCount):1;lastProcess_.store(status,std::memory_order_release);return status;}
    }
    int32_t processMeteredNotesAdmitted(AudioAdmission& admission,float* const* outputs,int32_t frames,const AAX_CMidiStream* stream,const AaxAudioProtectedRange* metadata,uint32_t count){
        if constexpr(!Instrument||!MaximumMeters||RawShortMidi||MaximumOutputEvents)return 1;
        else{if(admission.owner_!=this||admission.consumed_||!audio_.processing.load(std::memory_order_acquire))return 3;admission.consumed_=true;const auto status=processOwned(nullptr,outputs,frames,stream,-1,metadata,count);lastProcess_.store(status,std::memory_order_release);return status;}
    }
    int32_t processRawNotesAdmitted(AudioAdmission& admission,float *const *outputs,int32_t frames,const AAX_CMidiStream *stream,const AaxAudioProtectedRange *metadata=nullptr,uint32_t count=0){
        if constexpr(!Instrument||!RawShortMidi||MaximumOutputEvents)return 1;
        else{if(admission.owner_!=this||admission.consumed_||!audio_.processing.load(std::memory_order_acquire))return 3;admission.consumed_=true;const auto status=processOwned(nullptr,outputs,frames,stream,-1,metadata,count);lastProcess_.store(status,std::memory_order_release);return status;}
    }
    // Explicit raw effect/main-aux entry. Stream is never silently omitted.
    int32_t processRawLayout(const float *const *inputs,float *const *outputs,int32_t frames,const AAX_CMidiStream *stream,int32_t sidechainFirst=-1,const AaxAudioProtectedRange *metadata=nullptr,uint32_t count=0){
        if constexpr(!RawShortMidi||Instrument||MaximumOutputEvents)return 1;
        else{auto admission=admitRawAudio();if(!admission)return 3;return processRawLayoutAdmitted(admission,inputs,outputs,frames,stream,sidechainFirst,metadata,count);}
    }
    int32_t processRawLayoutAdmitted(AudioAdmission& admission,const float *const *inputs,float *const *outputs,int32_t frames,const AAX_CMidiStream *stream,int32_t sidechainFirst=-1,const AaxAudioProtectedRange *metadata=nullptr,uint32_t count=0){
        if constexpr(!RawShortMidi||Instrument||MaximumOutputEvents)return 1;
        else{if(admission.owner_!=this||admission.consumed_||!audio_.processing.load(std::memory_order_acquire))return 3;admission.consumed_=true;const auto status=processOwned(inputs,outputs,frames,stream,sidechainFirst,metadata,count);lastProcess_.store(status,std::memory_order_release);return status;}
    }
    // Reentry rejects before active state/sample/diagnostic/packet mutation.
    // Live typed node/base extent, callback-only borrow. Up to ten declared
    // callback metadata ranges are copied/protected along with original metadata
    // and public node base. Full derived node capacity/liveness is caller-owned.
    int32_t processGeneratedNotes(float *const *outputs,int32_t frames,const AAX_CMidiStream *stream,AAX_IMIDINode *node,const AaxAudioProtectedRange *callbackMetadata=nullptr,uint32_t callbackCount=0){
        auto admission=admitGeneratedAudio();if(!admission)return 1;
        return processGeneratedNotesAdmitted(admission,outputs,frames,stream,node,callbackMetadata,callbackCount);
    }
    int32_t processGeneratedNotesAdmitted(AudioAdmission &admission,float *const *outputs,int32_t frames,const AAX_CMidiStream *stream,AAX_IMIDINode *node,const AaxAudioProtectedRange *callbackMetadata=nullptr,uint32_t callbackCount=0){
        if constexpr(!MaximumOutputEvents)return 1;
        else {
            if(admission.owner_!=this||admission.consumed_||!audio_.processing.load(std::memory_order_acquire))return 1;
            admission.consumed_=true;
            output_.diagnostic={};output_.stage.invalidate();
            auto rejectNode=[&]{silence(outputs,frames,nullptr,0,0,0,0,callbackMetadata,callbackCount);lastProcess_.store(1,std::memory_order_release);return int32_t(1);};
            uintptr_t nf=0,nl=0,mf=0,ml=0;const auto own=reinterpret_cast<uintptr_t>(this);
            if(callbackCount>10)return rejectNode();
            const bool validNode=range(node,sizeof(AAX_IMIDINode),alignof(AAX_IMIDINode),nf,nl)&&!overlaps(nf,nl,own,own+sizeof(*this));
            std::array<AaxAudioProtectedRange,12> protectedMetadata{};
            if(callbackCount){if(!range(callbackMetadata,size_t(callbackCount)*sizeof(*callbackMetadata),alignof(AaxAudioProtectedRange),mf,ml)||overlaps(mf,ml,own,own+sizeof(*this)))return rejectNode();for(uint32_t i=0;i<callbackCount;++i){if(callbackMetadata[i].first>callbackMetadata[i].last)return rejectNode();protectedMetadata[i]=callbackMetadata[i];}}
            if(nf&&nl>=nf)protectedMetadata[callbackCount++]={nf,nl};if(mf)protectedMetadata[callbackCount++]={mf,ml};
            // Invalid node still preflights borrowed stream metadata before any
            // supported silence, so rejection cannot overwrite its packet/header.
            const auto status=processOwned(nullptr,outputs,frames,stream,-1,protectedMetadata.data(),callbackCount,validNode?node:nullptr);
            lastProcess_.store(status,std::memory_order_release);return status;
        }
    }
private:
    RavetNativeEventV1* outputEventData(){if constexpr(MaximumOutputEvents)return output_.events.data();else return nullptr;}
    bool protection(uintptr_t hf,uintptr_t hl,uintptr_t pf,uintptr_t pl,const AaxAudioProtectedRange *metadata,uint32_t metadataCount,std::array<AaxAudioProtectedRange,16> &output,uint32_t &count)const{
        if(metadataCount>12)return false;const auto own=reinterpret_cast<uintptr_t>(this);output[0]={own,own+sizeof(*this)};output[1]={hf,hl};output[2]={pf,pl};count=3;
        if(metadataCount){uintptr_t first=0,last=0;if(!range(metadata,size_t(metadataCount)*sizeof(*metadata),alignof(AaxAudioProtectedRange),first,last)||overlaps(first,last,own,own+sizeof(*this)))return false;output[count++]={first,last};for(uint32_t i=0;i<metadataCount;++i){if(metadata[i].first>metadata[i].last)return false;output[count++]=metadata[i];}}
        return true;
    }
    int32_t processOwned(const float *const *inputs,float *const *outputs,int32_t frames,const AAX_CMidiStream *stream,int32_t sidechainFirst=-1,const AaxAudioProtectedRange *callbackMetadata=nullptr,uint32_t callbackCount=0,AAX_IMIDINode *outputNode=nullptr){
        if constexpr(NativeSampleBits==64){precision_.diagnostic={};precision_.diagnostic.error=PrecisionError::Preflight;}
        if constexpr(RawShortMidi){raw_.diagnostic={};raw_.diagnostic.error=PrecisionError::Preflight;}
        uintptr_t hf=0,hl=0,pf=0,pl=0;
        auto reject=[&](int32_t status){if constexpr(RawShortMidi&&NativeSampleBits==64)raw_.diagnostic=precision_.diagnostic;silence(outputs,frames,inputs,hf,hl,pf,pl,callbackMetadata,callbackCount);return status;};
        if constexpr(HasMidiInput){
            const auto own=reinterpret_cast<uintptr_t>(this);
            if(!range(stream,sizeof(*stream),alignof(AAX_CMidiStream),hf,hl)||overlaps(hf,hl,own,own+sizeof(*this)))return reject(1);
            // Do not consume stream metadata aliased with the owner/output pointer array.
            uintptr_t of=0,ol=0;
            if(!(MaximumOutputEvents&&frames==0&&!outputs)&&(!range(outputs,sizeof(float*)*mapper_.layout().outputCapacity,alignof(float*),of,ol)||overlaps(hf,hl,of,ol)))return reject(1);
            uintptr_t inf=0,inl=0;
            if(mapper_.layout().inputCapacity&&(!range(inputs,sizeof(float*)*mapper_.layout().inputCapacity,alignof(float*),inf,inl)||overlaps(inf,inl,own,own+sizeof(*this))||overlaps(hf,hl,inf,inl)))return reject(1);
            AAX_CMidiStream header{};std::memcpy(&header,stream,sizeof(header));
            if(header.mBufferSize){
                if(!range(header.mBuffer,sizeof(AAX_CMidiPacket)*size_t(header.mBufferSize),alignof(AAX_CMidiPacket),pf,pl))return 1; // unbounded extent cannot define supported silence
                if(overlaps(pf,pl,own,own+sizeof(*this))||overlaps(pf,pl,of,ol)||overlaps(pf,pl,inf,inl)||overlaps(pf,pl,hf,hl))return reject(1);
            }
            if(header.mBufferSize>AaxNoteInput::maximumPackets)return reject(1);
        }
        if(!ready_||frames<0||uint32_t(frames)>maximumFrames_)return reject(1);
        if constexpr(MaximumMeters){if(!meterProcess_.taps)return reject(1);}
        std::array<AaxAudioProtectedRange,16> protectedRanges{};uint32_t protectedCount=0;
        if(!protection(hf,hl,pf,pl,callbackMetadata,callbackCount,protectedRanges,protectedCount)||!mapper_.map(inputs,outputs,uint32_t(frames),sidechainFirst,mapped_,protectedRanges.data(),protectedCount))return reject(1);
        if constexpr(MaximumOutputEvents){if(!outputNode)return reject(1);}
        std::array<RavetNativeEventV1,HasMidiInput?MaximumInputEvents:1> notes{};uint32_t noteCount=0;
        if constexpr(HasMidiInput){if(!AaxShortMidiPreflight::convert<RawShortMidi>(stream,uint32_t(frames),notes.data(),uint32_t(MaximumInputEvents),noteCount))return reject(1);}
        const auto head=head_.load(std::memory_order_relaxed),tail=tail_.load(std::memory_order_acquire);
        if constexpr(!RawShortMidi){if(!frames&&(tail!=head||noteCount))return reject(1);}
        if(uint32_t(tail-head)>=QueueSlots){ready_=false;return reject(2);}
        uint32_t lastRestore=tail;
        for(uint32_t i=head;i!=tail;++i){const auto &command=queue_[i%QueueSlots];if(command.kind==2)lastRestore=i;else if(command.kind!=1){ready_=false;return reject(2);}}
        std::array<RavetNativeEventV1,HasMidiInput?MaximumInputEvents:QueueSlots> events{};uint32_t used=0;
        const auto first=lastRestore==tail?head:lastRestore+1;
        if(uint32_t(tail-first)>events.size()-noteCount)return reject(1); // capacity before applyState/dequeue
        for(uint32_t i=first;i!=tail;++i)events[used++]=queue_[i%QueueSlots].event;
        for(uint32_t i=0;i<noteCount;++i)events[used++]=notes[i]; // controls0 precede equal-offset notes
        if constexpr(CaptureTransport){
            transportResult_=aaxTransport(transportProxy_,transportSnapshot_);
            if(!transportResult_)return reject(1); // before state apply/dequeue/native DSP
        }
        const RavetNativeTransportV1 *transport=nullptr;
        if constexpr(CaptureTransport){if(transportSnapshot_.transport().present)transport=&transportSnapshot_.transport();}
        if constexpr(NativeSampleBits==64){if(!precision_.audio.capture(mapped_))return reject(1);precision_.diagnostic.error=PrecisionError::None;}
        if constexpr(RawShortMidi)raw_.diagnostic.error=PrecisionError::None;
        try {
        int32_t status=0;
        uint32_t retirement=0;
        struct RetireAtExit{NativeAaxProcessor& owner;uint32_t& generation;~RetireAtExit(){if constexpr(EnableDynamicTiming){if(generation)owner.timing_.retired.store(generation,std::memory_order_release);}}} retire{*this,retirement};
        if constexpr(EnableDynamicTiming){
            if(timing_.faulted.load(std::memory_order_acquire))return reject(1);
            const auto publication=timing_.published.load(std::memory_order_acquire);
            if(publication){
                Values current{};timing_.diagnostic.nativeAttempted=true;timing_.diagnostic.nativeReturned=false;
                status=dsp().snapshotState(current.data(),schema_.count());timing_.diagnostic.nativeReturned=true;timing_.diagnostic.nativeStatus=status;
                if(!status){status=slots_[publication&1u]->applyState(current.data(),schema_.count());timing_.diagnostic.nativeStatus=status;}
                if(status){timing_.faulted.store(true,std::memory_order_release);ready_=false;timingPhase(AaxTimingDiagnostic::Phase::Faulted);return reject(status);}
                activeSlot_.store(publication&1u,std::memory_order_release);timing_.published.store(0,std::memory_order_release);
                timing_.diagnostic.current=timing_.diagnostic.candidate;timing_.diagnostic.currentLatency=timing_.diagnostic.requestedLatency;timingPhase(AaxTimingDiagnostic::Phase::Adopted);retirement=publication;
            }
        }
        if(lastRestore!=tail){if constexpr(MaximumMeters)meterProcess_.stateAttempted=true;if constexpr(NativeSampleBits==64)precision_.diagnostic.stateAttempted=true;if constexpr(RawShortMidi)raw_.diagnostic.stateAttempted=true;status=dsp().applyState(queue_[lastRestore%QueueSlots].values.data(),schema_.count());if constexpr(MaximumMeters){meterProcess_.stateReturned=true;meterProcess_.stateStatus=status;}if constexpr(NativeSampleBits==64){precision_.diagnostic.stateReturned=true;precision_.diagnostic.stateStatus=status;}if constexpr(RawShortMidi){raw_.diagnostic.stateReturned=true;raw_.diagnostic.stateStatus=status;}if constexpr(MaximumOutputEvents){output_.diagnostic.stateAttempted=true;output_.diagnostic.stateStatus=status;}}
        RavetNativeProcessorResultV1 report{};
        if(!status){
            if constexpr(MaximumMeters)meterProcess_.nativeAttempted=true;
            if constexpr(RawShortMidi)raw_.diagnostic.nativeAttempted=true;
            if constexpr(NativeSampleBits==64){
                precision_.diagnostic.nativeAttempted=true;
                if constexpr(MaximumOutputEvents)output_.diagnostic.nativeAttempted=true;
                status=dsp().process(precision_.audio.buses(),precision_.audio.busCount(),uint32_t(frames),events.data(),used,transport,MaximumOutputEvents?outputEventData():nullptr,uint32_t(MaximumOutputEvents),report);
                precision_.diagnostic.nativeReturned=true;precision_.diagnostic.nativeStatus=status;precision_.nativeStatus.store(status,std::memory_order_release);
                if constexpr(MaximumOutputEvents){output_.diagnostic.nativeReturned=true;output_.diagnostic.nativeStatus=status;output_.diagnostic.reportStatus=report.status;output_.diagnostic.reportCount=report.event_count;output_.nativeStatus.store(status,std::memory_order_release);}
                if constexpr(EnableDynamicTiming)timing_.nativeProcess.store(status,std::memory_order_release);
            }else if constexpr(MaximumOutputEvents){output_.diagnostic.nativeAttempted=true;status=dsp().process(mapped_.buses(),mapped_.busCount(),uint32_t(frames),events.data(),used,transport,output_.events.data(),uint32_t(MaximumOutputEvents),report);output_.diagnostic.nativeReturned=true;output_.diagnostic.nativeStatus=status;output_.diagnostic.reportStatus=report.status;output_.diagnostic.reportCount=report.event_count;output_.nativeStatus.store(status,std::memory_order_release);}
            else{status=dsp().process(mapped_.buses(),mapped_.busCount(),uint32_t(frames),events.data(),used,transport,nullptr,0,report);if constexpr(EnableDynamicTiming)timing_.nativeProcess.store(status,std::memory_order_release);}
            if constexpr(MaximumMeters){meterProcess_.nativeReturned=true;meterProcess_.nativeStatus.store(status,std::memory_order_release);}
            if constexpr(RawShortMidi){raw_.diagnostic.nativeReturned=true;raw_.diagnostic.nativeStatus=status;raw_.nativeStatus.store(status,std::memory_order_release);}
        }
        if(status){ready_=false;if constexpr(EnableDynamicTiming)faultTiming();return reject(status);}
        head_.store(tail,std::memory_order_release);
        if constexpr(RawShortMidi){raw_.diagnostic.commandsConsumed=true;if constexpr(NativeSampleBits==32)raw_.diagnostic.samplesTransferred=true;}
        if constexpr(NativeSampleBits==64){precision_.diagnostic.commandsConsumed=true;if(!precision_.audio.check(precision_.diagnostic.overflow)){precision_.diagnostic.error=PrecisionError::NarrowOverflow;ready_=false;if constexpr(EnableDynamicTiming)faultTiming();return reject(1);}}
        if constexpr(MaximumOutputEvents){
            if(report.status>1||report.event_count>MaximumOutputEvents)return reject(1);
            output_.diagnostic.stage=output_.stage.stage(output_.events.data(),report.event_count,uint32_t(frames));
            if(!output_.diagnostic.stage)return reject(1);
            if constexpr(NativeSampleBits==64){if(!precision_.audio.copy(mapped_)){ready_=false;return reject(1);}precision_.diagnostic.samplesTransferred=true;}
            output_.diagnostic.delivery=output_.stage.deliver(outputNode);
            if(!output_.diagnostic.delivery)return reject(1);
        }
        if constexpr(NativeSampleBits==64&&!MaximumOutputEvents){if(!precision_.audio.copy(mapped_)){ready_=false;if constexpr(EnableDynamicTiming)faultTiming();return reject(1);}precision_.diagnostic.samplesTransferred=true;}
        if constexpr(RawShortMidi){if constexpr(NativeSampleBits==64)raw_.diagnostic=precision_.diagnostic;else raw_.diagnostic.samplesTransferred=true;}
        if constexpr(MaximumMeters){
            // Post-DSP failure preserves successful audio and every prior tap.
            // Poison/violating throw may make future readiness unavailable; no
            // already consumed command/native voice history rollback is claimed.
            if(!meters_.publish(dsp(),meterProcess_.taps)){const auto& d=meters_.diagnosticsStopped();if((d.snapshotReturned&&d.snapshotStatus==2)||d.error==AaxMeterDiagnostics::Error::Exception){ready_=false;if constexpr(EnableDynamicTiming)faultTiming();}}
        }
        return 0;
        }catch(...){
            if constexpr(MaximumMeters)meterProcess_.exception=true;
            if constexpr(NativeSampleBits==64){precision_.diagnostic.error=PrecisionError::Exception;ready_=false;if constexpr(EnableDynamicTiming)faultTiming();return reject(1);}
            else if constexpr(RawShortMidi){raw_.diagnostic.error=PrecisionError::Exception;ready_=false;if constexpr(EnableDynamicTiming)faultTiming();return reject(1);}
            else if constexpr(MaximumMeters){ready_=false;return reject(1);}
            else throw;
        }
    }
};
} // namespace ravet::aax
