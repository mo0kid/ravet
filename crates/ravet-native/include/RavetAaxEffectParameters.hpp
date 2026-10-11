#pragma once
#include "RavetAaxProcessor.hpp"
#include "AAX_CEffectParameters.h"
#include "AAX_IController.h"
#include "AAX_IMIDINode.h"
#include <type_traits>
#include <cstddef>

namespace ravet::aax {
// Optional public-SDK mono/stereo host-f32 model. CONTROL serializes all SDK callbacks,
// codec/staging and lifecycle, while AUDIO exclusively owns DSP through Context.
// Hosts stop and join AUDIO before initialize/reset/reprepare/uninitialize.
// Tables/libraries/foreign ranges outlive every borrower. No private SDK layout.
// Profiles require writable global parameters and native complete chunks;
// DSP/read-only parameter reflection, factory program catalogue and AudioSuite
// remain outside; the optional native editor facade composes this model.
// Explicit Instrument=true selects no-input short MIDI notes and optional bypass.
// RawShortMidi=true selects every defined short input: the existing instrument
// Context, or explicit RawEffect/RawMultibus contexts with one added node field.
// CaptureTransport=true borrows the initialized public versioned SDK proxy.
// Multibus=true selects explicit stopped profile/effect Context with sidechain
// field4; main stems currently None/mono/stereo, additional planes explicitly declared.
// NativeSampleBits explicitly selects matching native32/64 tables and codecs;
// SDK audio Context fields and f32 parameter/state scalars remain unchanged.
// Native64 models must be heap-resident for fixed prepared scratch.
// EnableDynamicTiming selects two prepared host-main-f32 effect cores; CONTROL must
// reconcile a delivered latency ACK before next AUDIO, then retire the old slot
// only after AUDIO acknowledgement. It supplies no generic host tail declaration.
template<size_t MaximumParameters=1024,size_t QueueSlots=32,bool Instrument=false,size_t MaximumInputEvents=1024,bool CaptureTransport=false,bool Multibus=false,size_t MaximumOutputEvents=0,bool EnableDynamicTiming=false,uint32_t NativeSampleBits=32,bool RawShortMidi=false,size_t MaximumMeters=0>
class NativeAaxEffectParameters : public AAX_CEffectParameters {
    static_assert(!EnableDynamicTiming||!Multibus,"Dynamic timing currently selects matching mono/stereo main-only effect Context");
    static_assert(!(Instrument&&Multibus),"The SDK multibus Context currently selects effect routing; short-note instrument aux needs an explicit separate Context");
public:
    static_assert(MaximumOutputEvents<=1024&&(!MaximumOutputEvents||(Instrument&&!Multibus)),"Generated MIDI currently selects the explicit instrument main-only Context");
    using Owner=NativeAaxProcessor<MaximumParameters,QueueSlots,Instrument,MaximumInputEvents,CaptureTransport,MaximumOutputEvents,EnableDynamicTiming,NativeSampleBits,RawShortMidi,MaximumMeters>;
    struct EffectContext { Owner **owner;int32_t *frames;float **inputs;float **outputs; };
    struct RawEffectContext { Owner **owner;int32_t *frames;float **inputs;float **outputs;AAX_IMIDINode *notes; };
    struct RawMultibusContext { Owner **owner;int32_t *frames;float **inputs;float **outputs;int32_t *sidechainFirst;AAX_IMIDINode *notes; };
    struct InstrumentContext { Owner **owner;int32_t *frames;float **outputs;AAX_IMIDINode *notes; };
    struct MultibusContext { Owner **owner;int32_t *frames;float **inputs;float **outputs;int32_t *sidechainFirst; };
    struct GeneratedInstrumentContext { Owner **owner;int32_t *frames;float **outputs;AAX_IMIDINode *notes;AAX_IMIDINode *generated; };
    using DefaultContext=std::conditional_t<(MaximumOutputEvents>0),GeneratedInstrumentContext,std::conditional_t<Instrument,InstrumentContext,std::conditional_t<Multibus,std::conditional_t<RawShortMidi,RawMultibusContext,MultibusContext>,std::conditional_t<RawShortMidi,RawEffectContext,EffectContext>>>>;
    struct MeterEffectContext { Owner **owner;int32_t *frames;float **inputs;float **outputs;float **meterTaps; };
    struct MeterRawEffectContext { Owner **owner;int32_t *frames;float **inputs;float **outputs;AAX_IMIDINode *notes;float **meterTaps; };
    struct MeterRawMultibusContext { Owner **owner;int32_t *frames;float **inputs;float **outputs;int32_t *sidechainFirst;AAX_IMIDINode *notes;float **meterTaps; };
    struct MeterInstrumentContext { Owner **owner;int32_t *frames;float **outputs;AAX_IMIDINode *notes;float **meterTaps; };
    struct MeterMultibusContext { Owner **owner;int32_t *frames;float **inputs;float **outputs;int32_t *sidechainFirst;float **meterTaps; };
    struct MeterGeneratedInstrumentContext { Owner **owner;int32_t *frames;float **outputs;AAX_IMIDINode *notes;AAX_IMIDINode *generated;float **meterTaps; };
    using MeterContext=std::conditional_t<(MaximumOutputEvents>0),MeterGeneratedInstrumentContext,std::conditional_t<Instrument,MeterInstrumentContext,std::conditional_t<Multibus,std::conditional_t<RawShortMidi,MeterRawMultibusContext,MeterMultibusContext>,std::conditional_t<RawShortMidi,MeterRawEffectContext,MeterEffectContext>>>>;
    using Context=std::conditional_t<(MaximumMeters>0),MeterContext,DefaultContext>;

    struct PreparedProfile { AaxNativeAudioLayout layout{};AAX_EStemFormat inputStem=AAX_eStemFormat_None,outputStem=AAX_eStemFormat_None; };
    struct TimingProfile {uint32_t initialLatency=0;};
    struct Identity { const char *effect;uint32_t channels;AAX_CTypeID manufacturer,product,plugin,chunk; };
private:
    mutable Owner owner_;
    Identity identity_;
    PreparedProfile profile_{};
    TimingProfile timingProfile_{};
    bool initialized_=false,ready_=false,offline_=false,busy_=false,effectInitPermit_=false;
    struct ControlScope {
        bool &busy;bool acquired;
        explicit ControlScope(bool &value):busy(value),acquired(!value){if(acquired)busy=true;}
        ~ControlScope(){if(acquired)busy=false;}
    };
    int32_t nativeStatus_=1;
    bool declared(AAX_CParamID id)const{return ready_&&owner_.ready()&&owner_.schema().info(id);}
    bool writable(AAX_CParamID id)const{const auto *p=declared(id)?owner_.schema().info(id):nullptr;return p&&!(p->flags&RAVET_PARAMETER_READ_ONLY);}
    static bool stemChannels(AAX_EStemFormat stem,uint32_t &channels){switch(stem){case AAX_eStemFormat_None:channels=0;return true;case AAX_eStemFormat_Mono:channels=1;return true;case AAX_eStemFormat_Stereo:channels=2;return true;default:return false;}}
    static PreparedProfile defaultProfile(Identity identity){const auto stem=identity.channels==1?AAX_eStemFormat_Mono:AAX_eStemFormat_Stereo;return {AaxNativeAudioLayout::main(Instrument?0u:identity.channels,identity.channels,1024),Instrument?AAX_eStemFormat_None:stem,stem};}
    bool validProfile(const PreparedProfile &profile)const{
        AaxNativeAudioMap mapper;uint32_t inputs=0,outputs=0;
        if(!identity_.effect||(identity_.channels!=1&&identity_.channels!=2)||!mapper.prepare(profile.layout)||!stemChannels(profile.inputStem,inputs)||!stemChannels(profile.outputStem,outputs)||inputs!=profile.layout.buses[0].inputs||outputs!=profile.layout.buses[0].outputs||outputs!=identity_.channels)return false;
        if constexpr(!Multibus){if(profile.layout.busCount!=1||inputs!=(Instrument?0u:identity_.channels))return false;}
        if constexpr(Instrument){if(profile.layout.inputCapacity)return false;}
        return true;
    }
    AAX_Result queryPrepare(const PreparedProfile &profile){
        if(!Controller()||!validProfile(profile))return AAX_ERROR_NULL_OBJECT;
        AAX_CString effect;auto status=Controller()->GetEffectID(&effect);if(status)return status;
        AAX_CSampleRate rate=0.;AAX_EStemFormat input=AAX_eStemFormat_None,output=AAX_eStemFormat_None;
        status=Controller()->GetSampleRate(&rate);if(status)return status;
        status=Controller()->GetInputStemFormat(&input);if(status)return status;
        status=Controller()->GetOutputStemFormat(&output);if(status)return status;
        if(effect!=identity_.effect||input!=profile.inputStem||output!=profile.outputStem||!native::scalar::finite(rate)||rate<=0.f)return AAX_ERROR_INVALID_ARGUMENT;
        nativeStatus_=owner_.prepareLayoutStopped(profile.layout,double(rate));
        if constexpr(CaptureTransport){if(!nativeStatus_)owner_.attachTransportStopped(Transport());}
        return nativeStatus_?AAX_ERROR_NULL_OBJECT:AAX_SUCCESS;
    }
    AAX_Result queryPrepare(){return queryPrepare(profile_);}
    bool chunkHeader(AAX_CTypeID id,const AAX_SPlugInChunk *chunk,bool version=true)const{
        return ready_&&owner_.ready()&&id==identity_.chunk&&chunk&&reinterpret_cast<uintptr_t>(chunk)%alignof(AAX_SPlugInChunk)==0&&chunk->fSize==int32_t(owner_.stateBytes())&&(!version||chunk->fVersion==1)&&chunk->fChunkID==identity_.chunk&&chunk->fManufacturerID==identity_.manufacturer&&chunk->fProductID==identity_.product&&chunk->fPlugInID==identity_.plugin;
    }
public:
    NativeAaxEffectParameters(Identity identity,const RavetNativeProcessorEntryV1 *core,const RavetNativeAudioStateEntryV1 *audio,const RavetNativeCachedParameterEntryV1 *cache,const RavetNativeStateEntryV1 *state,const RavetNativeTelemetryEntryV1 *telemetry=nullptr,const AaxMeterBinding* meterBindings=nullptr)
        :NativeAaxEffectParameters(identity,core,audio,cache,state,defaultProfile(identity),telemetry,meterBindings){}
    NativeAaxEffectParameters(Identity identity,const RavetNativeProcessorEntryV1 *core,const RavetNativeAudioStateEntryV1 *audio,const RavetNativeCachedParameterEntryV1 *cache,const RavetNativeStateEntryV1 *state,PreparedProfile profile,const RavetNativeTelemetryEntryV1 *telemetry=nullptr,const AaxMeterBinding* meterBindings=nullptr)
        :owner_(core,audio,cache,state,telemetry,meterBindings),identity_(identity),profile_(profile){}
    NativeAaxEffectParameters(Identity identity,const RavetNativeProcessorEntryV1 *core,const RavetNativeAudioStateEntryV1 *audio,const RavetNativeCachedParameterEntryV1 *cache,const RavetNativeStateEntryV1 *state,TimingProfile timing,const RavetNativeTelemetryEntryV1 *telemetry=nullptr,const AaxMeterBinding* meterBindings=nullptr)
        :owner_(core,audio,cache,state,telemetry,meterBindings),identity_(identity),profile_(defaultProfile(identity)),timingProfile_(timing){}
    ~NativeAaxEffectParameters()override{(void)Uninitialize();}
    // Exact stopped native lifecycle code; AUDIO status has a separate atomic observation.
    int32_t nativeStatus()const{return nativeStatus_;}
    int32_t lastProcessStatus()const{return owner_.lastProcessStatus();}
    typename Owner::MeterProcessDiagnostics meterDiagnosticsStopped()const{return owner_.meterDiagnosticsStopped();}
    AaxMeterDiagnostics candidateMeterDiagnosticsStopped()const{return owner_.candidateMeterDiagnosticsStopped();}
    const AaxMeterBinding* meterBindingStopped(uint32_t index)const{return owner_.meterBindingStopped(index);}
    // CONTROL only after stop/join AUDIO; the optional exact native cache belongs
    // to the model. Host parameter reflection and publication are not implied.
    int32_t snapshotTelemetryStopped(RavetNativeTelemetryValueV1 *values,uint32_t capacity){
        const auto output=reinterpret_cast<uintptr_t>(values),self=reinterpret_cast<uintptr_t>(this);const auto bytes=size_t(capacity)*sizeof(*values);
        if(capacity>1024||!output||output%alignof(RavetNativeTelemetryValueV1)||bytes>UINTPTR_MAX-output||(output<self+sizeof(*this)&&self<output+bytes))return 1;
        ControlScope scope(busy_);return scope.acquired&&ready_?owner_.snapshotTelemetryStopped(values,capacity):1;
    }
    // Selected raw/native64/generated/timed profiles retain last actually returned
    // native AUDIO code. Legacy main/note native32 keeps its final-status alias.
    // Whole admitted-block details require AUDIO stopped/joined.
    int32_t lastNativeProcessStatus()const{return owner_.lastNativeProcessStatus();}
    // Caller stops/joins AUDIO; exact native code requires Returned, while Busy
    // preserves the previously admitted block's complete diagnostics unchanged.
    typename Owner::RawProcessDiagnostics rawDiagnosticsStopped()const{return owner_.rawDiagnosticsStopped();}
    typename Owner::PrecisionDiagnostics precisionDiagnosticsStopped()const{return owner_.precisionDiagnosticsStopped();}
    bool offlineMetadata()const{return offline_;}
    // Caller must stop/join AUDIO before copying transport diagnostics.
    AaxTransportSnapshot transportSnapshotStopped()const{return owner_.transportSnapshotStopped();}
    AaxTransportResult transportResultStopped()const{return owner_.transportResultStopped();}
    AAX_Result Initialize(IACFUnknown *controller)override{
        ControlScope scope(busy_);
        if(!scope.acquired||initialized_||owner_.timingFaulted()||!controller)return AAX_ERROR_INVALID_ARGUMENT;
        initialized_=true;effectInitPermit_=true;
        try{const auto status=AAX_CEffectParameters::Initialize(controller);effectInitPermit_=false;return status;}catch(...){effectInitPermit_=false;ready_=false;return AAX_ERROR_NULL_OBJECT;}
    }
    AAX_Result EffectInit()override{
        if(busy_){if(!effectInitPermit_)return AAX_ERROR_INVALID_ARGUMENT;effectInitPermit_=false;return effectInitOwned();}
        ControlScope scope(busy_);return effectInitOwned();
    }
private:
    AAX_Result effectInitOwned(){
        if(owner_.timingFaulted()||ready_||mParameterManager.NumParameters())return AAX_ERROR_INVALID_ARGUMENT;
        try{
            if(!owner_.valid())return AAX_ERROR_NULL_OBJECT;
            if constexpr(!Instrument){if(!owner_.schema().masterBypass())return AAX_ERROR_NULL_OBJECT;}
            for(uint32_t i=0;i<owner_.schema().count();++i)if(owner_.schema().info(owner_.schema().id(i))->flags&RAVET_PARAMETER_READ_ONLY)return AAX_ERROR_INVALID_ARGUMENT;
            typename NativeAaxParameters<MaximumParameters>::Prepared objects{};
            if(!owner_.schema().prepare(objects))return AAX_ERROR_NULL_OBJECT;
            // Restore accepted complete global intent on fresh initialization.
            for(uint32_t i=0;i<owner_.schema().count();++i){RavetNativeParameterValueV1 value{};double normalized=0.;if(!owner_.intentValue(i,value)||!owner_.schema().normalizedPhysical(i,value.value,normalized))return AAX_ERROR_INVALID_ARGUMENT;objects[i]->UpdateNormalizedValue(normalized);}
            const auto status=queryPrepare();if(status)return status;
            if constexpr(EnableDynamicTiming){const auto timing=owner_.checkInitialTimingStopped(timingProfile_.initialLatency,Controller());if(timing)return timing;}
            if(!owner_.schema().registerEmpty(mParameterManager,objects)){nativeStatus_=owner_.deactivate();return AAX_ERROR_NULL_OBJECT;}
            ready_=true;return AAX_SUCCESS;
        }catch(...){ready_=false;nativeStatus_=owner_.deactivate();mParameterManager.RemoveAllParameters();return AAX_ERROR_NULL_OBJECT;}
    }
public:
    AAX_Result Uninitialize()override{
        ControlScope scope(busy_);if(!scope.acquired)return AAX_ERROR_INVALID_ARGUMENT;
        if constexpr(EnableDynamicTiming||RawShortMidi||MaximumOutputEvents||MaximumMeters){if(owner_.processingAudio())return AAX_ERROR_INVALID_ARGUMENT;}
        ready_=false;nativeStatus_=owner_.deactivate();
        const auto status=AAX_CEffectParameters::Uninitialize();initialized_=false;offline_=false;
        return status?status:(nativeStatus_?AAX_ERROR_NULL_OBJECT:AAX_SUCCESS);
    }
    // Explicit stopped operations; SDK ResetFieldData only publishes context.
    AAX_Result resetStopped(){ControlScope scope(busy_);if(!scope.acquired||!ready_||(((EnableDynamicTiming||RawShortMidi||MaximumOutputEvents||MaximumMeters)&&owner_.processingAudio())||(EnableDynamicTiming&&owner_.timingFaulted())))return AAX_ERROR_INVALID_ARGUMENT;nativeStatus_=owner_.reset();if(nativeStatus_){if constexpr(EnableDynamicTiming){if(!owner_.ready())ready_=false;}else ready_=false;}return nativeStatus_?AAX_ERROR_NULL_OBJECT:AAX_SUCCESS;}
    AAX_Result reprepareStopped(){ControlScope scope(busy_);if(!scope.acquired||!ready_||(((EnableDynamicTiming||RawShortMidi||MaximumOutputEvents||MaximumMeters)&&owner_.processingAudio())||(EnableDynamicTiming&&owner_.timingFaulted())))return AAX_ERROR_INVALID_ARGUMENT;const auto status=queryPrepare();if(status&&!owner_.ready())ready_=false;return status;}
    // Stop/join AUDIO; registered SDK descriptor/context must match the selected
    // layout. Setter changes no host registration or metadata capacity contract.
    AAX_Result configureProfileStopped(PreparedProfile profile){
        ControlScope scope(busy_);if(!scope.acquired||!ready_||!owner_.ready()||((EnableDynamicTiming||RawShortMidi||MaximumOutputEvents||MaximumMeters)&&owner_.processingAudio())||!validProfile(profile))return AAX_ERROR_INVALID_ARGUMENT;
        const auto status=queryPrepare(profile);if(!status)profile_=profile;else if(!owner_.ready())ready_=false;return status;
    }
    PreparedProfile profileStopped()const{return profile_;}
    AAX_Result NotificationReceived(AAX_CTypeID type,const void *data,uint32_t size)override{
        if constexpr(EnableDynamicTiming){if(type==AAX_eNotificationEvent_SignalLatencyChanged||type==AAX_eNotificationEvent_DelayCompensationState)return owner_.timingNotification(type,data,size);}
        if(busy_)return AAX_ERROR_INVALID_ARGUMENT;
        if(type==AAX_eNotificationEvent_EnteringOfflineMode||type==AAX_eNotificationEvent_ExitingOfflineMode){if(!ready_||!owner_.ready()||data||size)return AAX_ERROR_INVALID_ARGUMENT;offline_=type==AAX_eNotificationEvent_EnteringOfflineMode;return AAX_SUCCESS;}
        // No AudioSuite profile or unsafe base preview-state dereference.
        if(type==AAX_eNotificationEvent_ASPreviewState||type==AAX_eNotificationEvent_ASProcessingState)return AAX_ERROR_UNIMPLEMENTED;
        return AAX_SUCCESS;
    }
    AaxTimingDiagnostic timingStopped()const{return owner_.timingStopped();}
    AAX_Result stageTimingCandidateStopped(typename Owner::TimingTables tables){ControlScope scope(busy_);if(!scope.acquired||!ready_||((EnableDynamicTiming||RawShortMidi||MaximumOutputEvents||MaximumMeters)&&owner_.processingAudio()))return AAX_ERROR_INVALID_ARGUMENT;nativeStatus_=owner_.stageTimingCandidateStopped(tables);return nativeStatus_?AAX_ERROR_INVALID_ARGUMENT:AAX_SUCCESS;}
    AAX_Result requestStagedTimingControl(){ControlScope scope(busy_);if(!scope.acquired||!ready_)return AAX_ERROR_INVALID_ARGUMENT;return owner_.requestStagedTimingControl(Controller());}
    AAX_Result reconcileTimingControl(){ControlScope scope(busy_);if(!scope.acquired||!ready_)return AAX_ERROR_INVALID_ARGUMENT;return owner_.reconcileTimingControl(Controller());}
    AAX_Result collectRetiredTimingControl(){ControlScope scope(busy_);if(!scope.acquired||!ready_)return AAX_ERROR_INVALID_ARGUMENT;return owner_.collectRetiredTimingControl();}
    AAX_Result ResetFieldData(AAX_CFieldIndex field,void *data,uint32_t size)const override{
        if(field!=0)return AAX_ERROR_INVALID_ARGUMENT;
        if(!data||reinterpret_cast<uintptr_t>(data)%alignof(Owner*)||size!=sizeof(Owner*))return AAX_ERROR_INVALID_ARGUMENT;
        if(!ready_||!owner_.ready())return AAX_ERROR_NULL_OBJECT;
        *static_cast<Owner**>(data)=&owner_;return AAX_SUCCESS;
    }
    static void AAX_CALLBACK process(Context *const *begin,const void *end){
        if(!begin||!end||reinterpret_cast<uintptr_t>(begin)%alignof(Context*)||reinterpret_cast<uintptr_t>(end)<reinterpret_cast<uintptr_t>(begin)||(reinterpret_cast<uintptr_t>(end)-reinterpret_cast<uintptr_t>(begin))%sizeof(Context*))return;
        const auto count=(reinterpret_cast<uintptr_t>(end)-reinterpret_cast<uintptr_t>(begin))/sizeof(Context*);
        if(count>1024)return;
        for(size_t i=0;i<count;++i){
            const auto *context=begin[i];
            if(!context||reinterpret_cast<uintptr_t>(context)%alignof(Context)||reinterpret_cast<uintptr_t>(context)>UINTPTR_MAX-sizeof(Context)||!context->owner||reinterpret_cast<uintptr_t>(context->owner)%alignof(Owner*)||reinterpret_cast<uintptr_t>(context->owner)>UINTPTR_MAX-sizeof(Owner*)||!context->frames||reinterpret_cast<uintptr_t>(context->frames)%alignof(int32_t)||reinterpret_cast<uintptr_t>(context->frames)>UINTPTR_MAX-sizeof(int32_t))continue;
            // Typed live exact-owner storage is the caller contract; once its
            // owner is known, reject self-aliasing metadata before reading frames.
            auto *owner=*context->owner;if(!owner||reinterpret_cast<uintptr_t>(owner)%alignof(Owner)||reinterpret_cast<uintptr_t>(owner)>UINTPTR_MAX-sizeof(*owner))continue;
            auto admission=owner->admitAudio();
            if constexpr(Owner::GuardedAudio){if(!admission)continue;} // one profile token, before every nested getter/mutation
            const auto own=reinterpret_cast<uintptr_t>(owner),ownEnd=own+sizeof(*owner);
            auto fieldRange=[](const void *pointer,size_t bytes){const auto first=reinterpret_cast<uintptr_t>(pointer);return AaxAudioProtectedRange{first,first+bytes};};
            const auto frameRange=fieldRange(context->frames,sizeof(int32_t)),ownerRange=fieldRange(context->owner,sizeof(Owner*));
            std::array<AaxAudioProtectedRange,9> metadata{};metadata[0]=fieldRange(begin,size_t(count)*sizeof(Context*));metadata[1]=fieldRange(context,sizeof(Context));metadata[2]=ownerRange;metadata[3]=frameRange;uint32_t metadataCount=4;
            if constexpr(Multibus){metadata[4]=context->sidechainFirst?fieldRange(context->sidechainFirst,sizeof(int32_t)):AaxAudioProtectedRange{};metadataCount=5;}
            auto intersectsOwner=[&](AaxAudioProtectedRange range){return range.first<ownEnd&&own<range.last;};
            const float *const *inputs=nullptr;if constexpr(!Instrument)inputs=context->inputs;
            if(intersectsOwner(frameRange)||intersectsOwner(ownerRange)){owner->rejectLayoutCallback(inputs,context->outputs,0,metadata.data(),metadataCount);continue;}
            const auto frames=*context->frames;
            int32_t first=-1;
            if constexpr(Multibus){
                const bool dynamic=owner->requiresSidechainInput()&&!(frames==0&&!context->inputs&&!context->outputs);
                if(dynamic){if(!context->sidechainFirst||reinterpret_cast<uintptr_t>(context->sidechainFirst)%alignof(int32_t)||metadata[4].first>metadata[4].last||intersectsOwner(metadata[4])){owner->rejectLayoutCallback(inputs,context->outputs,frames,metadata.data(),metadataCount);continue;}first=*context->sidechainFirst;}
            }
            auto prepareMeter=[&]{if constexpr(MaximumMeters)return owner->prepareMetersAdmitted(admission,inputs,context->outputs,frames,first,context->meterTaps,metadata.data(),metadataCount,uint32_t(metadata.size()));else return true;};
            if constexpr(Instrument||RawShortMidi){
                if constexpr(MaximumOutputEvents||RawShortMidi||MaximumMeters){
                    // Protect only the public known base, including bounded
                    // unaligned spans on refusal; no private host extent guess.
                    auto nodeRange=[&](AAX_IMIDINode *node){const auto address=reinterpret_cast<uintptr_t>(node);const auto priorCount=metadataCount;auto &span=metadata[metadataCount++];const bool bounded=node&&address<=UINTPTR_MAX-sizeof(AAX_IMIDINode);if(bounded)span=fieldRange(node,sizeof(AAX_IMIDINode));bool disjoint=true;
                        if constexpr(RawShortMidi||MaximumMeters){if(bounded)for(uint32_t i=0;i<priorCount;++i)if(span.first<metadata[i].last&&metadata[i].first<span.last)disjoint=false;if(!owner->rawNodeArrayMetadataDisjoint(node,inputs,context->outputs))disjoint=false;}
                        return bounded&&address%alignof(AAX_IMIDINode)==0&&!intersectsOwner(span)&&disjoint;};
                    const bool validInput=nodeRange(context->notes);
                    bool validOutput=true;if constexpr(MaximumOutputEvents)validOutput=nodeRange(context->generated);
                    if(!validInput||!validOutput){
                        if constexpr(MaximumMeters){prepareMeter();owner->rejectLayoutCallback(inputs,context->outputs,frames,metadata.data(),metadataCount);}
                        else if constexpr(MaximumOutputEvents)owner->processGeneratedNotesAdmitted(admission,context->outputs,frames,nullptr,nullptr,metadata.data(),metadataCount);
                        else if constexpr(Instrument)owner->processRawNotesAdmitted(admission,context->outputs,frames,nullptr,metadata.data(),metadataCount);
                        else owner->processRawLayoutAdmitted(admission,inputs,context->outputs,frames,nullptr,first,metadata.data(),metadataCount);
                        continue;
                    }
                }
                if(!prepareMeter()){owner->rejectLayoutCallback(inputs,context->outputs,frames,metadata.data(),metadataCount);continue;}
                const AAX_CMidiStream *stream=nullptr;try{if(context->notes)stream=context->notes->GetNodeBuffer();}catch(...){stream=nullptr;}
                if constexpr(MaximumOutputEvents)owner->processGeneratedNotesAdmitted(admission,context->outputs,frames,stream,context->generated,metadata.data(),metadataCount);
                else if constexpr(RawShortMidi){if constexpr(Instrument)owner->processRawNotesAdmitted(admission,context->outputs,frames,stream,metadata.data(),metadataCount);else owner->processRawLayoutAdmitted(admission,inputs,context->outputs,frames,stream,first,metadata.data(),metadataCount);}
                else if constexpr(MaximumMeters)owner->processMeteredNotesAdmitted(admission,context->outputs,frames,stream,metadata.data(),metadataCount);
                else owner->processNotes(context->outputs,frames,stream,metadata.data(),metadataCount);
            }else{
                if(!prepareMeter()){owner->rejectLayoutCallback(inputs,context->outputs,frames,metadata.data(),metadataCount);continue;}
                if constexpr(Multibus){if constexpr(MaximumMeters)owner->processLayoutAdmitted(admission,inputs,context->outputs,frames,first,metadata.data(),metadataCount);else owner->processLayout(inputs,context->outputs,frames,first,metadata.data(),metadataCount);}
                else{if constexpr(EnableDynamicTiming||MaximumMeters)owner->processLayoutAdmitted(admission,inputs,context->outputs,frames,-1,metadata.data(),metadataCount);else owner->processLayout(inputs,context->outputs,frames,-1,metadata.data(),metadataCount);}
            }
        }
    }
    AAX_Result GetNumberOfParameters(int32_t *output) const override {
        if (!ready_ || !output) return AAX_ERROR_NULL_ARGUMENT;
        *output = int32_t(owner_.schema().count()); return AAX_SUCCESS;
    }
    AAX_Result GetMasterBypassParameter(AAX_IString *output) const override {
        const auto *id = ready_ ? owner_.schema().masterBypass() : nullptr;
        if (!id || !output) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (output->MaxLength() < std::strlen(id)) return AAX_ERROR_INVALID_ARGUMENT;
        output->Set(id); return AAX_SUCCESS;
    }
    AAX_Result GetParameter(AAX_CParamID id, AAX_IParameter **output) override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        const auto next = mParameterManager.GetParameterByID(id);
        if (!next) return AAX_ERROR_INVALID_PARAMETER_ID;
        *output = next; return AAX_SUCCESS;
    }
    AAX_Result GetParameterIndex(AAX_CParamID id, int32_t *output) const override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        *output = int32_t(owner_.schema().index(id)); return AAX_SUCCESS;
    }
    AAX_Result GetParameterIDFromIndex(int32_t index, AAX_IString *output) const override {
        if (!ready_ || index < 0 || uint32_t(index) >= owner_.schema().count()) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        const char *id = owner_.schema().id(uint32_t(index));
        if (output->MaxLength() < std::strlen(id)) return AAX_ERROR_INVALID_ARGUMENT;
        output->Set(id); return AAX_SUCCESS;
    }
    AAX_Result GetParameterName(AAX_CParamID id, AAX_IString *output) const override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        const auto *info = owner_.schema().info(id);
        if (output->MaxLength() < info->name_length) return AAX_ERROR_INVALID_ARGUMENT;
        output->Set(reinterpret_cast<const char *>(info->name)); return AAX_SUCCESS;
    }
    AAX_Result GetParameterNameOfLength(AAX_CParamID id, AAX_IString *output, int32_t length) const override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (length <= 0 || uint32_t(length) < owner_.schema().info(id)->name_length) return AAX_ERROR_INVALID_ARGUMENT;
        return GetParameterName(id, output);
    }
    AAX_Result GetParameterIsAutomatable(AAX_CParamID id, AAX_CBoolean *output) const override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        *output = writable(id) && (owner_.schema().info(id)->flags & RAVET_PARAMETER_AUTOMATABLE); return AAX_SUCCESS;
    }
    AAX_Result GetParameterNumberOfSteps(AAX_CParamID id, int32_t *output) const override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        *output = int32_t(mParameterManager.GetParameterByID(id)->GetNumberOfSteps()); return AAX_SUCCESS;
    }
    AAX_Result GetParameterDefaultNormalizedValue(AAX_CParamID id, double *output) const override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        return owner_.schema().defaultValue(id, *output) ? AAX_SUCCESS : AAX_ERROR_INVALID_PARAMETER_ID;
    }
    AAX_Result SetParameterDefaultNormalizedValue(AAX_CParamID id, double) override {
        return declared(id) ? AAX_ERROR_UNIMPLEMENTED : AAX_ERROR_INVALID_PARAMETER_ID;
    }
    AAX_Result GetParameterType(AAX_CParamID id, AAX_EParameterType *output) const override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        *output = mParameterManager.GetParameterByID(id)->GetType(); return AAX_SUCCESS;
    }
    AAX_Result GetParameterOrientation(AAX_CParamID id, AAX_EParameterOrientation *output) const override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        *output = mParameterManager.GetParameterByID(id)->GetOrientation(); return AAX_SUCCESS;
    }
    AAX_Result GetParameterValueInfo(AAX_CParamID id, int32_t selector, int32_t *output) const override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        int32_t next = 0; const auto status = AAX_CEffectParameters::GetParameterValueInfo(id, selector, &next);
        if (status == AAX_SUCCESS) *output = next;
        return status;
    }
    AAX_Result GetParameterNormalizedValue(AAX_CParamID id, double *output) const override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        double next = 0.; const auto status = AAX_CEffectParameters::GetParameterNormalizedValue(id, &next);
        if (status != AAX_SUCCESS) return status;
        return owner_.schema().value(id, next, *output) ? AAX_SUCCESS : AAX_ERROR_INVALID_ARGUMENT;
    }
    AAX_Result SetParameterNormalizedValue(AAX_CParamID id, double input) override {
        ControlScope scope(busy_);if(!scope.acquired||((EnableDynamicTiming&&!RawShortMidi)&&owner_.processingAudio()))return AAX_ERROR_INVALID_ARGUMENT;
        double next = 0.;
        if (!writable(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!owner_.schema().control(id, input, next)) return AAX_ERROR_INVALID_ARGUMENT;
        try{return AAX_CEffectParameters::SetParameterNormalizedValue(id,next);}catch(...){owner_.invalidate();ready_=false;return AAX_ERROR_NULL_OBJECT;}
    }
    AAX_Result UpdateParameterNormalizedValue(AAX_CParamID id, double input, AAX_EUpdateSource source) override {
        ControlScope scope(busy_);if(!scope.acquired)return AAX_ERROR_INVALID_ARGUMENT;
        if (!writable(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        double next=0.; if(!owner_.schema().control(id,input,next)) return AAX_ERROR_INVALID_ARGUMENT;
        typename Owner::Reservation reservation{};
        if(!owner_.stageParameter(id,next,reservation)) return AAX_ERROR_INVALID_ARGUMENT;
        try{
            const auto status=AAX_CEffectParameters::UpdateParameterNormalizedValue(id,reservation.normalized,source);
            if(status!=AAX_SUCCESS){owner_.cancel(reservation);owner_.invalidate();ready_=false;return status;}
            return owner_.commit(reservation)?AAX_SUCCESS:AAX_ERROR_NULL_OBJECT;
        }catch(...){owner_.cancel(reservation);owner_.invalidate();ready_=false;return AAX_ERROR_NULL_OBJECT;}
    }
    AAX_Result SetParameterNormalizedRelative(AAX_CParamID id, double delta) override {
        if (!writable(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!ravet::native::scalar::finite(delta)) return AAX_ERROR_INVALID_ARGUMENT;
        double previous = 0.; const auto status = GetParameterNormalizedValue(id, &previous);
        if (status != AAX_SUCCESS) return status;
        return SetParameterNormalizedValue(id, previous + delta);
    }
    AAX_Result GetParameterValueFromString(AAX_CParamID id, double *output, const AAX_IString &text) const override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        return owner_.schema().parse(id, text, *output) ? AAX_SUCCESS : AAX_ERROR_INVALID_ARGUMENT;
    }
    AAX_Result GetParameterStringFromValue(AAX_CParamID id, double value, AAX_IString *output, int32_t maximum) const override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        return owner_.schema().format(id, value, maximum, *output) ? AAX_SUCCESS : AAX_ERROR_INVALID_ARGUMENT;
    }
    AAX_Result GetParameterValueString(AAX_CParamID id, AAX_IString *output, int32_t maximum) const override {
        double value = 0.; const auto status = GetParameterNormalizedValue(id, &value);
        return status == AAX_SUCCESS ? GetParameterStringFromValue(id, value, output, maximum) : status;
    }
    AAX_Result TouchParameter(AAX_CParamID id) override { ControlScope scope(busy_);if(!scope.acquired)return AAX_ERROR_INVALID_ARGUMENT;return writable(id) ? AAX_CEffectParameters::TouchParameter(id) : AAX_ERROR_INVALID_PARAMETER_ID; }
    AAX_Result ReleaseParameter(AAX_CParamID id) override { ControlScope scope(busy_);if(!scope.acquired)return AAX_ERROR_INVALID_ARGUMENT;return writable(id) ? AAX_CEffectParameters::ReleaseParameter(id) : AAX_ERROR_INVALID_PARAMETER_ID; }
    AAX_Result UpdateParameterTouch(AAX_CParamID id, AAX_CBoolean touch) override {
        ControlScope scope(busy_);if(!scope.acquired)return AAX_ERROR_INVALID_ARGUMENT;
        if (!writable(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        return touch == 0 || touch == 1 ? AAX_CEffectParameters::UpdateParameterTouch(id, touch) : AAX_ERROR_INVALID_ARGUMENT;
    }
    AAX_Result GetNumberOfChunks(int32_t *output)const override{if(!ready_||!output)return AAX_ERROR_NULL_ARGUMENT;*output=1;return AAX_SUCCESS;}
    AAX_Result GetChunkIDFromIndex(int32_t index,AAX_CTypeID *output)const override{if(!output)return AAX_ERROR_NULL_ARGUMENT;if(!ready_||index)return AAX_ERROR_INVALID_CHUNK_INDEX;*output=identity_.chunk;return AAX_SUCCESS;}
    AAX_Result GetChunkSize(AAX_CTypeID id,uint32_t *output)const override{if(!ready_||id!=identity_.chunk)return AAX_ERROR_INVALID_CHUNK_ID;if(!output)return AAX_ERROR_NULL_ARGUMENT;*output=owner_.stateBytes();return AAX_SUCCESS;}
    AAX_Result GetChunk(AAX_CTypeID id,AAX_SPlugInChunk *chunk)const override{
        if(!chunkHeader(id,chunk,false))return AAX_ERROR_INCORRECT_CHUNK_SIZE;
        std::array<uint8_t,8208> bytes{};if(!owner_.save(bytes.data(),owner_.stateBytes()))return AAX_ERROR_INVALID_ARGUMENT;
        chunk->fVersion=1;std::memcpy(chunk->fData,bytes.data(),owner_.stateBytes());std::memset(chunk->fName,0,sizeof(chunk->fName));std::memcpy(chunk->fName,"RAVET native state",18);return AAX_SUCCESS;
    }
    AAX_Result SetChunk(AAX_CTypeID id,const AAX_SPlugInChunk *chunk)override{
        ControlScope scope(busy_);if(!scope.acquired)return AAX_ERROR_INVALID_ARGUMENT;
        if(!chunkHeader(id,chunk))return AAX_ERROR_INCORRECT_CHUNK_SIZE;
        typename Owner::Reservation reservation{};if(!owner_.stageRestore(reinterpret_cast<const uint8_t*>(chunk->fData),uint32_t(chunk->fSize),reservation))return AAX_ERROR_INVALID_ARGUMENT;
        std::array<double,MaximumParameters> normalized{};
        for(uint32_t i=0;i<owner_.schema().count();++i){RavetNativeParameterValueV1 value{};if(!owner_.stagedValue(reservation,i,value)||!owner_.schema().normalizedPhysical(i,value.value,normalized[i])||!mParameterManager.GetParameterByID(owner_.schema().id(i))){owner_.cancel(reservation);return AAX_ERROR_INVALID_ARGUMENT;}}
        // Owned SDK objects' void updates may notify arbitrary foreign delegates;
        // complete local preflight is atomic, foreign sideeffects have no rollback.
        try{
            for(uint32_t i=0;i<owner_.schema().count();++i){const auto status=AAX_CEffectParameters::UpdateParameterNormalizedValue(owner_.schema().id(i),normalized[i],AAX_eUpdateSource_Unspecified);if(status){owner_.cancel(reservation);owner_.invalidate();ready_=false;return status;}}
            return owner_.commit(reservation)?AAX_SUCCESS:AAX_ERROR_NULL_OBJECT;
        }catch(...){owner_.cancel(reservation);owner_.invalidate();ready_=false;return AAX_ERROR_NULL_OBJECT;}
    }
    AAX_Result CompareActiveChunk(const AAX_SPlugInChunk *chunk,AAX_CBoolean *output)const override{
        if(!output)return AAX_ERROR_NULL_ARGUMENT;if(!chunkHeader(identity_.chunk,chunk))return AAX_ERROR_INCORRECT_CHUNK_SIZE;
        std::array<uint8_t,8208> bytes{};if(!owner_.save(bytes.data(),owner_.stateBytes()))return AAX_ERROR_INVALID_ARGUMENT;*output=!std::memcmp(bytes.data(),chunk->fData,owner_.stateBytes());return AAX_SUCCESS;
    }
};
} // namespace ravet::aax
