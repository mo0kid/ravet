#pragma once
#include "RavetNativeState.h"
#include "RavetCachedParameters.h"
#include "RavetLv2NoteInput.hpp"
#include "RavetLv2MidiOutput.hpp"
#include <atomic>
#include <optional>
#include <type_traits>
#include "RavetLv2Transport.hpp"
#include "lv2/time/time.h"
#include "lv2/midi/midi.h"
#include "lv2/atom/atom.h"
#include "lv2/state/state.h"
#include "lv2/urid/urid.h"
#include <algorithm>
#include <array>
#include <cmath>
#include <cstring>
#include <limits>

namespace ravet::native {
// External-LV2-SDK, C++17, f32 main-bus/control-port profile. Every native table
// must expose the exact selected core and shared parameter-schema pointers and
// remain loaded. Callback-equivalent core/schema copies are refused before
// creators. Cache/state top-level addresses have no additional ABI identity
// oracle; their metadata accessors supply the proven provenance. Instantiate/retire
// and restore require stopped exclusive ownership. Save may overlap run through
// the native publication companion, never by reading the DSP. No hidden thread,
// audio allocation, retry loop, UI, or threadSafeRestore declaration.
// Effect ports: inputs, outputs, then writable schema controls in declared order.
// Explicit Instrument=true: zero inputs, outputs, controls, then one MIDI Atom
// Sequence port, with mandatory cached urid:map. Shared control/note budget1024.
// Explicit Transport=true additionally collects exact block-start time:Position
// before state transfer/DSP; later validated positions are omitted, no timeline
// inference or history carry. Default MIDI-only and Gain profiles are unchanged.
// Host ranges are live/aligned and cover frame counts; controls and instance
// metadata stay disjoint from writable samples. Inputs may alias all outputs;
// outputs are mutually disjoint. Compile with ordinary IEEE finite semantics.
// MaximumOutputEvents>0 appends one output Atom port and admits the complete
// configured Chunk body before state/DSP. Prepared output scratch is owned by
// the instance (allocate on CONTROL heap); MaximumFrames must match the native
// profile. One AUDIO CAS rejects nested/concurrent runs without mutation. CONTROL
// still requires stopped/joined exclusion before lifecycle/restore/diagnostics;
// busy checks are not a general lifecycle lock. Safe empty output requires an
// independently admitted16-byte write; late DSP failures do not roll back history.
// Native consumer hooks retain their own atomicity and deadline obligations.
template <size_t Parameters, size_t MaximumChannels = 2, bool Instrument = false, bool Transport = false, size_t MaximumOutputEvents = 0, uint32_t MaximumFrames = 65536>
class NativeLv2Processor final {
    static_assert(MaximumOutputEvents<=1024&&(!MaximumOutputEvents||Instrument), "Generated LV2 output requires the existing instrument input profile");
    static_assert(MaximumFrames>0&&MaximumFrames<=65536);
    static_assert(std::atomic<bool>::is_always_lock_free&&std::atomic<int32_t>::is_always_lock_free);
    static_assert(Parameters > 0 && Parameters <= 1024);
    static_assert(MaximumChannels > 0 && MaximumChannels <= 64);
    static_assert(!Transport || Instrument, "Transport profile requires the existing instrument Atom input");
public:
    using OutputStage=Lv2MidiOutput<MaximumOutputEvents?MaximumOutputEvents:1>;
    enum class OutputError:uint32_t{None,Inactive,Metadata,Capacity,Controls,Input,TransportContext,State,Native,Report,Stage,Reflect,Exception};
    // Caller stops/joins AUDIO before copying. Original status is meaningful
    // only with its matching Returned flag; initial1 is never-returned sentinel.
    struct OutputDiagnostic {OutputError error=OutputError::None;bool applyAttempted=false,applyReturned=false,restoreConsumed=false,nativeAttempted=false,nativeReturned=false,reflectAttempted=false,reflectReturned=false,exception=false,controlsConsumed=false;int32_t applyStatus=1,nativeStatus=1,reflectStatus=1;uint32_t nativeReport=0,nativeCount=0,published=0;typename OutputStage::Diagnostic stage{};};
    NativeLv2Processor(const RavetNativeProcessorEntryV1 *core,
                       const RavetNativeCachedParameterEntryV1 *cache,
                       const RavetNativeStateEntryV1 *state,
                       uint32_t channels, double rate, const char *stateKey,
                       const LV2_Feature *const *features)
        : core_(core), cache_(cache), state_(state), channels_(channels) {
        if (!core || !cache || !state || channels == 0 || channels > MaximumChannels ||
            !std::isfinite(rate) || rate <= 0. || !stateKey || !*stateKey ||
            core->version != 1 || core->byte_size != sizeof(*core) || core->sample_bits != 32 || core->reserved ||
            !core->create || !core->destroy || !core->prepare || !core->reset || !core->deactivate || !core->process || !core->timing ||
            cache->version != 1 || cache->byte_size != sizeof(*cache) || cache->sample_bits != 32 || cache->parameter_count != Parameters ||
            !cache->parameters_entry || !cache->create || !cache->destroy || !cache->info || !cache->normalize || !cache->denormalize ||
            state->version != 1 || state->byte_size != sizeof(*state) || state->sample_bits != 32 || state->parameter_count != Parameters ||
            !state->parameters_entry || !state->create || !state->destroy || !state->publish || !state->save_state || !state->apply || !state->reflect)
            return;
        // All foreign metadata queries precede owned resource acquisition. The
        // native ABI remains nonthrowing; a violating metadata exception refuses
        // this helper without invoking a cache, publication or DSP creator.
        const RavetNativeParameterEntryV1 *stateSchema = nullptr;
        const RavetNativeProcessorEntryV1 *origin = nullptr;
        try {
            schema_ = cache->parameters_entry();
            stateSchema = state->parameters_entry();
            if (!schema_ || stateSchema != schema_ || schema_->version != 1 ||
                schema_->byte_size != sizeof(*schema_) || schema_->sample_bits != 32 ||
                schema_->parameter_count != Parameters || !schema_->processor_entry ||
                schema_->state_bytes == 0 || schema_->state_bytes > 8208) return;
            origin = schema_->processor_entry();
        } catch (...) { return; }
        if (origin != core) return;
        cacheHandle_ = cache->create();
        if (!cacheHandle_) return;
        for (uint32_t i = 0; i < Parameters; ++i) {
            if (cache->info(cacheHandle_, i, &info_[i]) || (info_[i].flags & RAVET_PARAMETER_READ_ONLY)) return;
        }
        publication_ = state->create(nullptr, 0);
        processor_ = core->create();
        if (!publication_ || !processor_) return;
        RavetNativeProcessorConfigV1 config{};
        config.version = 1; config.maximum_frames = maximumFrames;
        config.maximum_events = eventCapacity; config.maximum_output_events=MaximumOutputEvents; config.bus_count = 1;
        config.sample_rate = rate; config.buses[0] = {inputChannels(), channels};
        if (core->prepare(processor_, &config)) return;
        if constexpr(MaximumOutputEvents){try{mapFeatures(features,stateKey);}catch(...){return;}}else mapFeatures(features,stateKey);
        if constexpr (Instrument) { if (!sequenceType_ || !midiType_ || !frameTimeType_) return; }
        if constexpr (Transport) { if (!timeTypes_.valid()) return; }
        if constexpr(MaximumOutputEvents){output_.stage.emplace(Lv2MidiOutputTypes{type_,sequenceType_,midiType_,frameTimeType_});if(!output_.stage->valid())return;}
        valid_ = true;
    }
    ~NativeLv2Processor() {
        if (processor_) { core_->deactivate(processor_); core_->destroy(processor_); }
        if (publication_) state_->destroy(publication_);
        if (cacheHandle_) cache_->destroy(cacheHandle_);
    }
    NativeLv2Processor(const NativeLv2Processor &) = delete;
    NativeLv2Processor &operator=(const NativeLv2Processor &) = delete;
    bool valid() const { return valid_; }
    void connect(uint32_t index, void *data) {
        if (!valid_) return;
        if constexpr(MaximumOutputEvents){if(output_.running.load(std::memory_order_acquire))return;if(index==audioPorts()+Parameters+1){output_.port=data;return;}}
        if constexpr (Instrument) { if (index == audioPorts() + Parameters) { notes_ = static_cast<const LV2_Atom_Sequence *>(data); return; } }
        if (index < audioPorts() + Parameters) ports_[index] = static_cast<float *>(data);
    }
    void activate() { if constexpr(MaximumOutputEvents){if(output_.running.load(std::memory_order_acquire))return;}active_ = valid_ && !failed_ && core_->reset(processor_) == 0; }
    void deactivate() { if constexpr(MaximumOutputEvents){if(output_.running.load(std::memory_order_acquire))return;}active_ = false; }
    void run(uint32_t frames){if constexpr(MaximumOutputEvents)runGenerated(frames);else runLegacy(frames);}
    bool outputDiagnosticsStopped(OutputDiagnostic& value)const{if constexpr(MaximumOutputEvents){if(output_.running.load(std::memory_order_acquire))return false;value=output_.diagnostic;return true;}else return false;}
    int32_t lastNativeOutputStatus()const{if constexpr(MaximumOutputEvents)return output_.nativeStatus.load(std::memory_order_acquire);else return 1;}
    LV2_State_Status save(LV2_State_Store_Function store, LV2_State_Handle handle) const {
        if (!valid_ || !store) return LV2_STATE_ERR_UNKNOWN;
        if (!key_ || !type_) return LV2_STATE_ERR_NO_FEATURE;
        std::array<uint8_t, 8208> bytes{};
        RavetNativeStateRevisionV1 revision{};
        if (state_->save_state(publication_, bytes.data(), schema_->state_bytes, &revision)) return LV2_STATE_ERR_UNKNOWN;
        return store(handle, key_, bytes.data(), schema_->state_bytes, type_, LV2_STATE_IS_POD | LV2_STATE_IS_PORTABLE);
    }
    LV2_State_Status restore(LV2_State_Retrieve_Function retrieve, LV2_State_Handle handle) {
        if constexpr(MaximumOutputEvents){if(output_.running.load(std::memory_order_acquire))return LV2_STATE_ERR_UNKNOWN;}
        if (!valid_ || !retrieve) return LV2_STATE_ERR_UNKNOWN;
        if (!key_ || !type_) return LV2_STATE_ERR_NO_FEATURE;
        size_t size = 0; uint32_t type = 0, flags = 0;
        const auto *bytes = static_cast<const uint8_t *>(retrieve(handle, key_, &size, &type, &flags));
        std::array<uint8_t, 8208> defaults{};
        if (!bytes) {
            // LV2 empty-map restore means schema defaults, never old state.
            void *temporary = state_->create(nullptr, 0);
            if (!temporary) return LV2_STATE_ERR_UNKNOWN;
            RavetNativeStateRevisionV1 revision{};
            const auto code = state_->save_state(temporary, defaults.data(), schema_->state_bytes, &revision);
            state_->destroy(temporary);
            if (code) return LV2_STATE_ERR_UNKNOWN;
            bytes = defaults.data(); size = schema_->state_bytes;
        } else {
            if (type != type_) return LV2_STATE_ERR_BAD_TYPE;
            if ((flags & (LV2_STATE_IS_POD | LV2_STATE_IS_PORTABLE)) != (LV2_STATE_IS_POD | LV2_STATE_IS_PORTABLE)) return LV2_STATE_ERR_BAD_FLAGS;
            if (size > 8208) return LV2_STATE_ERR_UNKNOWN;
        }
        RavetNativeStateRevisionV1 revision{};
        if (state_->publish(publication_, bytes, static_cast<uint32_t>(size), &revision)) return LV2_STATE_ERR_UNKNOWN;
        // Host ports are independently persisted by LV2. Hold the explicit
        // restored state until a connected canonical control changes; controls
        // the host changes during its own recall take precedence at next run.
        std::array<float, Parameters> baseline{};
        if (readControls(baseline)) { lastControls_ = baseline; seen_.fill(true); }
        else seen_.fill(false);
        return LV2_STATE_SUCCESS;
    }
    static constexpr uint32_t maximumFrames = MaximumFrames;
private:
    bool generatedRange(const void* pointer,size_t bytes,Lv2ProtectedRange& range)const{
        const auto first=reinterpret_cast<uintptr_t>(pointer);if(!pointer||bytes>UINTPTR_MAX-first)return false;range={first,first+bytes};return true;
    }
    bool generatedOverlap(Lv2ProtectedRange span,const Lv2ProtectedRange* ranges,uint32_t count)const{
        for(uint32_t i=0;i<count;++i)if(span.first<ranges[i].last&&ranges[i].first<span.last)return true;return false;
    }
    bool generatedAdd(const void* pointer,size_t bytes){
        if(!pointer||!bytes)return true;Lv2ProtectedRange span{};if(!generatedRange(pointer,bytes,span)||output_.metadataCount==output_.metadata.size())return false;output_.metadata[output_.metadataCount++]=span;return true;
    }
    bool generatedMetadata(uint32_t frames,bool& inputOkay){
        output_.metadataCount=0;output_.sampleCount=0;inputOkay=false;
        bool okay=generatedAdd(this,sizeof(*this))&&generatedAdd(core_,sizeof(*core_))&&generatedAdd(cache_,sizeof(*cache_))&&generatedAdd(state_,sizeof(*state_))&&generatedAdd(schema_,sizeof(*schema_));
        for(uint32_t i=0;i<Parameters;++i)okay=generatedAdd(ports_[audioPorts()+i],sizeof(float))&&okay;
        if(frames)for(uint32_t c=0;c<channels_;++c){Lv2ProtectedRange span{};const auto* sample=ports_[inputChannels()+c];if(sample){if(generatedRange(sample,size_t(frames)*sizeof(float),span))output_.samples[output_.sampleCount++]=span;else okay=false;}}
        Lv2ProtectedRange header{};
        if(notes_&&reinterpret_cast<uintptr_t>(notes_)%8==0&&generatedRange(notes_,sizeof(LV2_Atom_Sequence),header)){
            const bool separate=!generatedOverlap(header,output_.metadata.data(),output_.metadataCount)&&!generatedOverlap(header,output_.samples.data(),output_.sampleCount);
            const auto beforeInput=output_.metadataCount;okay=generatedAdd(notes_,sizeof(LV2_Atom_Sequence))&&okay;
            if(separate){LV2_Atom_Sequence candidate{};std::memcpy(&candidate,notes_,sizeof(candidate));Lv2ProtectedRange body{};
                if(generatedRange(notes_,sizeof(LV2_Atom)+size_t(candidate.atom.size),body)){okay=generatedAdd(notes_,sizeof(LV2_Atom)+size_t(candidate.atom.size))&&okay;inputOkay=candidate.atom.size>=sizeof(candidate.body)&&candidate.atom.size<=Lv2NoteInput::maximumBodyBytes&&!generatedOverlap(body,output_.metadata.data(),beforeInput)&&!generatedOverlap(body,output_.samples.data(),output_.sampleCount);}
                else okay=false;
            }
        }else if(notes_)okay=generatedAdd(notes_,sizeof(LV2_Atom_Sequence))&&okay;
        return okay;
    }
    void silenceGenerated(uint32_t frames){
        const auto supported=std::min(frames,maximumFrames);
        for(uint32_t c=0;c<channels_;++c){uintptr_t first=0,last=0;auto* samples=ports_[inputChannels()+c];if(supported&&range(samples,supported,first,last)&&!generatedOverlap({first,last},output_.metadata.data(),output_.metadataCount))std::fill_n(samples,supported,0.f);}
    }
    // Fixed scratch and one admitted token: nested run returns before foreign
    // header reads, empty publication, diagnostics, DSP or active samples mutate.
    void runGenerated(uint32_t frames){
        if(!valid_)return;bool expected=false;if(!output_.running.compare_exchange_strong(expected,true,std::memory_order_acq_rel,std::memory_order_acquire))return;
        struct Release{std::atomic<bool>& value;~Release(){value.store(false,std::memory_order_release);}}release{output_.running};
        output_.diagnostic={};output_.stage->beginBlock();auto& d=output_.diagnostic;typename OutputStage::Admission admission;bool geometry=false;
        auto reject=[&](OutputError error,bool poison=false){d.error=error;if(poison){failed_=true;active_=false;}if(geometry)silenceGenerated(frames);output_.stage->empty(admission);d.stage=output_.stage->diagnosticsStopped();return;};
        try{
            bool inputOkay=false;geometry=generatedMetadata(frames,inputOkay);if(!geometry){reject(OutputError::Metadata);return;}
            output_.protectedCount=output_.metadataCount;std::copy_n(output_.metadata.data(),output_.metadataCount,output_.protectedRanges.data());
            for(uint32_t i=0;i<output_.sampleCount;++i)output_.protectedRanges[output_.protectedCount++]=output_.samples[i];
            const bool capacity=output_.stage->admit(output_.port,output_.protectedRanges.data(),output_.protectedCount,admission);
            // Protect every established declared Chunk range, even wrong-type/
            // insufficient-capacity rejection. If no header could be consumed,
            // preserve its independently known eight bytes during audio silence.
            if(admission.first())geometry=generatedAdd(reinterpret_cast<const void*>(admission.first()),admission.last()-admission.first())&&geometry;
            else if(output_.port)geometry=generatedAdd(output_.port,sizeof(LV2_Atom))&&geometry;
            if(!capacity){reject(OutputError::Capacity);return;}
            if(!active_||frames>maximumFrames){reject(OutputError::Inactive);return;}
            if(!inputOkay||!validateAudio(frames)){reject(OutputError::Metadata);return;}
            for(uint32_t c=0;c<channels_;++c){if(frames){uintptr_t first=0,last=0;if(!range(ports_[inputChannels()+c],frames,first,last)||generatedOverlap({first,last},output_.metadata.data(),output_.metadataCount)){reject(OutputError::Metadata);return;}}}
            if(!readControls(output_.controls)){reject(OutputError::Controls);return;}
            uint32_t count=0;for(uint32_t i=0;i<Parameters;++i)if(!seen_[i]||output_.controls[i]!=lastControls_[i]){auto& event=output_.input[count++];event={};event.kind=RAVET_NATIVE_PARAMETER_VALUE;event.parameter_id=info_[i].id;event.value=output_.controls[i];}
            uint32_t notes=0;if(!Lv2NoteInput::convert(notes_,sequenceType_,midiType_,frameTimeType_,frames,output_.input.data()+count,eventCapacity-count,notes)){reject(OutputError::Input);return;}count+=notes;
            output_.transport={};const RavetNativeTransportV1* context=nullptr;if constexpr(Transport){if(!Lv2TimePosition::collect(notes_,timeTypes_,frames,output_.transport)){reject(OutputError::TransportContext);return;}if(output_.transport.present)context=&output_.transport;}
            output_.applied={};d.applyAttempted=true;d.applyStatus=state_->apply(processor_,publication_,&output_.applied);d.applyReturned=true;if(d.applyStatus&&d.applyStatus!=3){reject(OutputError::State,d.applyStatus==2);return;}
            if(!d.applyStatus){if(output_.applied.reserved||output_.applied.changed>1){reject(OutputError::State,true);return;}d.restoreConsumed=output_.applied.changed;}
            for(uint32_t c=0;c<channels_;++c)output_.outputs[c]=ports_[inputChannels()+c];
            const RavetNativeProcessorBusV1 bus{0,channels_,nullptr,output_.outputs.data()};output_.result={};
            d.nativeAttempted=true;d.nativeStatus=core_->process(processor_,&bus,1,frames,output_.input.data(),count,context,output_.generated.data(),MaximumOutputEvents,&output_.result);d.nativeReturned=true;output_.nativeStatus.store(d.nativeStatus,std::memory_order_release);
            d.nativeReport=output_.result.status;d.nativeCount=output_.result.event_count;
            if(d.nativeStatus){reject(OutputError::Native,d.nativeStatus==2);return;}
            d.controlsConsumed=true;lastControls_=output_.controls;seen_.fill(true);
            if(output_.result.status>1||output_.result.event_count>MaximumOutputEvents){reject(OutputError::Report,true);return;}
            if(!output_.stage->stage(output_.generated.data(),output_.result.event_count,frames)){reject(OutputError::Stage,true);return;}
            output_.revision={};d.reflectAttempted=true;d.reflectStatus=state_->reflect(processor_,publication_,&output_.revision);d.reflectReturned=true;if(d.reflectStatus&&d.reflectStatus!=3&&d.reflectStatus!=4){reject(OutputError::Reflect,d.reflectStatus==2);return;}
            if(!output_.stage->publish(admission)){reject(OutputError::Stage,true);return;}
            d.published=output_.result.event_count;d.stage=output_.stage->diagnosticsStopped();d.error=OutputError::None;
        }catch(...){d.exception=true;reject(OutputError::Exception,true);}
    }
    void mapFeatures(const LV2_Feature* const* features,const char* stateKey){
        if (features) {
            for (uint32_t i = 0; i < 256 && features[i]; ++i) {
                if (features[i]->URI && std::strcmp(features[i]->URI, LV2_URID__map) == 0) {
                    const auto *map = static_cast<const LV2_URID_Map *>(features[i]->data);
                    if (map && map->map) {
                        key_ = map->map(map->handle, stateKey); type_ = map->map(map->handle, LV2_ATOM__Chunk);
                        if constexpr (Instrument) {
                            sequenceType_ = map->map(map->handle, LV2_ATOM__Sequence);
                            midiType_ = map->map(map->handle, LV2_MIDI__MidiEvent);
                            frameTimeType_ = map->map(map->handle, LV2_ATOM__frameTime);
                            if constexpr (Transport) {
                                timeTypes_ = {sequenceType_, frameTimeType_, map->map(map->handle, LV2_ATOM__Object),
                                    map->map(map->handle, LV2_TIME__Position), map->map(map->handle, LV2_ATOM__Long),
                                    map->map(map->handle, LV2_ATOM__Double), map->map(map->handle, LV2_ATOM__Float),
                                    map->map(map->handle, LV2_ATOM__Int), map->map(map->handle, LV2_TIME__frame),
                                    map->map(map->handle, LV2_TIME__beat), map->map(map->handle, LV2_TIME__beatsPerMinute),
                                    map->map(map->handle, LV2_TIME__speed), map->map(map->handle, LV2_TIME__beatsPerBar),
                                    map->map(map->handle, LV2_TIME__beatUnit)};
                            }
                        }
                    }
                    break;
                }
            }
        }
    }
    void runLegacy(uint32_t frames) {
        if (!valid_) return;
        std::array<float, Parameters> controls{};
        if (!active_ || frames > maximumFrames || !validateAudio(frames) || !readControls(controls)) { silence(frames); return; }
        std::array<RavetNativeEventV1, eventCapacity> events{};
        uint32_t count = 0;
        for (uint32_t i = 0; i < Parameters; ++i) {
            if (!seen_[i] || controls[i] != lastControls_[i]) {
                events[count].kind = RAVET_NATIVE_PARAMETER_VALUE;
                events[count].parameter_id = info_[i].id;
                events[count++].value = controls[i];
            }
        }
        if constexpr (Instrument) {
            uint32_t noteCount = 0;
            if (!Lv2NoteInput::convert(notes_, sequenceType_, midiType_, frameTimeType_, frames,
                                      events.data() + count, eventCapacity - count, noteCount)) { silence(frames); return; }
            count += noteCount;
        }
        RavetNativeTransportV1 transport{};
        const RavetNativeTransportV1 *context = nullptr;
        if constexpr (Transport) {
            if (!Lv2TimePosition::collect(notes_, timeTypes_, frames, transport)) { silence(frames); return; }
            if (transport.present) context = &transport;
        }
        // Only complete preflight can consume a pending state restore. Consumer
        // backend failure after apply cannot generically roll back DSP history.
        RavetNativeStateApplyV1 applied{};
        const auto accepted = state_->apply(processor_, publication_, &applied);
        if (accepted && accepted != 3) { fail(accepted, frames); return; }
        std::array<const float *, MaximumChannels> inputs{};
        std::array<float *, MaximumChannels> outputs{};
        for (uint32_t c = 0; c < channels_; ++c) {
            if constexpr (!Instrument) inputs[c] = ports_[c];
            outputs[c] = ports_[inputChannels() + c];
        }
        const RavetNativeProcessorBusV1 bus{inputChannels(), channels_, Instrument ? nullptr : inputs.data(), outputs.data()};
        RavetNativeProcessorResultV1 result{};
        const auto processed = core_->process(processor_, &bus, 1, frames, events.data(), count, context, nullptr, 0, &result);
        if (processed || result.status > 1 || result.event_count) { fail(processed ? processed : 1, frames); return; }
        lastControls_ = controls; seen_.fill(true);
        RavetNativeStateRevisionV1 revision{};
        const auto reflected = state_->reflect(processor_, publication_, &revision);
        if (reflected && reflected != 3 && reflected != 4) fail(reflected, frames);
    }
    struct EmptyGenerated{};
    struct PreparedGenerated {
        std::optional<OutputStage> stage{};std::atomic<bool> running{false};std::atomic<int32_t> nativeStatus{1};void* port=nullptr;OutputDiagnostic diagnostic{};
        std::array<RavetNativeEventV1,1024> input{};std::array<RavetNativeEventV1,MaximumOutputEvents> generated{};
        std::array<float,Parameters> controls{};std::array<const float*,MaximumChannels> inputs{};std::array<float*,MaximumChannels> outputs{};
        std::array<Lv2ProtectedRange,Parameters+2*MaximumChannels+16> metadata{};uint32_t metadataCount=0;std::array<Lv2ProtectedRange,Parameters+2*MaximumChannels+16> protectedRanges{};uint32_t protectedCount=0;std::array<Lv2ProtectedRange,2*MaximumChannels> samples{};uint32_t sampleCount=0;
        RavetNativeProcessorResultV1 result{};RavetNativeStateApplyV1 applied{};RavetNativeStateRevisionV1 revision{};RavetNativeTransportV1 transport{};
    };
    std::conditional_t<(MaximumOutputEvents>0),PreparedGenerated,EmptyGenerated> output_{};
    const RavetNativeProcessorEntryV1 *core_;
    const RavetNativeCachedParameterEntryV1 *cache_;
    const RavetNativeStateEntryV1 *state_;
    const RavetNativeParameterEntryV1 *schema_ = nullptr;
    void *processor_ = nullptr, *cacheHandle_ = nullptr, *publication_ = nullptr;
    uint32_t channels_, key_ = 0, type_ = 0, sequenceType_ = 0, midiType_ = 0, frameTimeType_ = 0;
    const LV2_Atom_Sequence *notes_ = nullptr;
    Lv2TimePositionTypes timeTypes_{};
    static constexpr uint32_t eventCapacity = Instrument ? 1024 : Parameters;
    uint32_t inputChannels() const { return Instrument ? 0 : channels_; }
    uint32_t audioPorts() const { return inputChannels() + channels_; }
    bool valid_ = false, active_ = false, failed_ = false;
    std::array<float *, 2 * MaximumChannels + Parameters> ports_{};
    std::array<RavetNativeParameterInfoV1, Parameters> info_{};
    std::array<float, Parameters> lastControls_{};
    std::array<bool, Parameters> seen_{};
    static bool range(const float *pointer, uint32_t frames, uintptr_t &begin, uintptr_t &end) {
        begin = reinterpret_cast<uintptr_t>(pointer);
        const auto bytes = static_cast<uint64_t>(frames) * sizeof(float);
        if (!pointer || begin % alignof(float) || bytes > std::numeric_limits<uintptr_t>::max() - begin) return false;
        end = begin + static_cast<uintptr_t>(bytes); return true;
    }
    bool validateAudio(uint32_t frames) const {
        if (!frames) return true;
        for (uint32_t c = 0; c < channels_; ++c) {
            uintptr_t begin, end;
            if constexpr (!Instrument) { if (!range(ports_[c], frames, begin, end)) return false; }
            if (!range(ports_[inputChannels() + c], frames, begin, end)) return false;
            for (uint32_t earlier = 0; earlier < c; ++earlier) {
                uintptr_t otherBegin, otherEnd;
                if (!range(ports_[inputChannels() + earlier], frames, otherBegin, otherEnd) || (begin < otherEnd && otherBegin < end)) return false;
            }
        }
        return true;
    }
    bool readControls(std::array<float, Parameters> &values) const {
        for (uint32_t i = 0; i < Parameters; ++i) {
            const auto *port = ports_[audioPorts() + i];
            uintptr_t begin, end;
            float normalized;
            if (!range(port, 1, begin, end) || cache_->normalize(cacheHandle_, info_[i].id, *port, &normalized) ||
                cache_->denormalize(cacheHandle_, info_[i].id, normalized, &values[i])) return false;
        }
        return true;
    }
    void silence(uint32_t frames) {
        const auto count = std::min(frames, maximumFrames);
        for (uint32_t c = 0; c < channels_; ++c) {
            uintptr_t begin, end;
            if (count && range(ports_[inputChannels() + c], count, begin, end)) std::fill_n(ports_[inputChannels() + c], count, 0.f);
        }
    }
    void fail(int32_t code, uint32_t frames) { if (code == 2) { failed_ = true; active_ = false; } silence(frames); }
};
} // namespace ravet::native
