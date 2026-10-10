// Public reference adaptation; external SDK implementation stays outside RAVET.
#define AUWrapperFactory RavetPerformanceSdkAUWrapperFactory
#include "public.sdk/source/vst/auwrapper/auwrapper.mm"
#undef AUWrapperFactory
#include "AudioUnitSDK/AUPlugInDispatch.h"
#include "RavetPerformance.h"
#include "RavetAuParameters.hpp"
#include "RavetAuState.hpp"
#include "RavetAuAudio.hpp"
#include "RavetAuVst3Transport.hpp"
#include <cmath>
#include <limits>
using namespace Steinberg;
using namespace Steinberg::Vst;
namespace {
class PerformanceAU final : public AUWrapper {
public:
    explicit PerformanceAU(ComponentInstanceRecord *instance) : AUWrapper(instance),
        schema_(ravet_performance_cached_parameters_v1(32)), state_(ravet_performance_state_v1(32), schema_.parametersEntry()) {
        eventList.setMaxSize(1024);
    }
    ComponentResult Initialize() override {
        if (!schema_.valid() || !state_.valid()) return kAudioUnitErr_FailedInitialization;
        const auto status = AUWrapper::Initialize();
        if (status) return status;
        RavetNativeProcessorConfigV1 layout{};
        layout.version = 1; layout.maximum_frames = GetMaxFramesPerSlice(); layout.maximum_events = 1024; layout.bus_count = 1;
        layout.sample_rate = GetOutput(0)->GetStreamFormat().mSampleRate;
        layout.buses[0].outputs = GetOutput(0)->GetStreamFormat().mChannelsPerFrame;
        if (!audioMap_.configure(layout)) { AUWrapper::Cleanup(); return kAudioUnitErr_FailedInitialization; }
        return noErr;
    }
    ComponentResult GetParameterList(AudioUnitScope scope, AudioUnitParameterID *ids, UInt32 &count) override {
        if (scope != kAudioUnitScope_Global) return kAudioUnitErr_InvalidScope;
        return schema_.list(ids, ids ? schema_.count() : 0, count) ? ComponentResult(noErr) : kAudioUnitErr_InvalidParameter;
    }
    ComponentResult GetParameterInfo(AudioUnitScope scope, AudioUnitParameterID id, AudioUnitParameterInfo &output) override {
        if (scope != kAudioUnitScope_Global) return kAudioUnitErr_InvalidScope;
        return schema_.info(id, output) ? ComponentResult(noErr) : kAudioUnitErr_InvalidParameter;
    }
    ComponentResult GetParameter(AudioUnitParameterID id, AudioUnitScope scope, AudioUnitElement element, AudioUnitParameterValue &output) override {
        if (scope != kAudioUnitScope_Global || element) return kAudioUnitErr_InvalidScope;
        if (!schema_.contains(id)) return kAudioUnitErr_InvalidParameter;
        AudioUnitParameterValue next = 0.f;
        const auto status = AUWrapper::GetParameter(id, scope, element, next);
        if (status) return status;
        if (!std::isfinite(next) || next < 0.f || next > 1.f) return kAudioUnitErr_InvalidParameterValue;
        output = next; return noErr;
    }
    bool ValidFormat(AudioUnitScope scope, AudioUnitElement element, const AudioStreamBasicDescription &format) override {
        const auto flags = UInt32(kAudioFormatFlagsNativeFloatPacked) | UInt32(kAudioFormatFlagIsNonInterleaved);
        return scope == kAudioUnitScope_Output && element == 0 && std::isfinite(format.mSampleRate) && format.mSampleRate > 0. &&
            format.mFormatID == kAudioFormatLinearPCM && (format.mFormatFlags == flags ||
            (format.mChannelsPerFrame == 1 && format.mFormatFlags == kAudioFormatFlagsNativeFloatPacked)) &&
            format.mChannelsPerFrame >= 1 && format.mChannelsPerFrame <= 2 && format.mBitsPerChannel == 32 &&
            format.mBytesPerFrame == 4 && format.mBytesPerPacket == 4 && format.mFramesPerPacket == 1;
    }
    OSStatus MIDIEvent(UInt32 status, UInt32 data1, UInt32 data2, UInt32 offset) override {
        // The SDK narrows UInt32 MIDI words before its byte callbacks. Validate
        // the complete API values so malformed input cannot become valid notes.
        if (status < 0x80 || status > 0xff || data1 > 127 || data2 > 127 || offset >= 65536)
            return kAudioUnitErr_InvalidParameterValue;
        const auto kind = status & 0xf0;
        if (kind == 0x80) return HandleNoteOff(status & 15, data1, data2, offset);
        if (kind == 0x90) return HandleNoteOn(status & 15, data1, data2, offset);
        if (kind != 0xf0) {
            if ((kind == 0xc0 || kind == 0xd0) && data2) return kAudioUnitErr_InvalidParameterValue;
            return noErr; // Valid non-note channel messages are ignored.
        }
        if (status == 0xf0 || status == 0xf7) return kAudio_UnimplementedError;
        if (status == 0xf2) return noErr;
        if (status == 0xf1 || status == 0xf3) return data2 ? kAudioUnitErr_InvalidParameterValue : OSStatus(noErr);
        if (status == 0xf6 || status == 0xf8 || status == 0xfa || status == 0xfb || status == 0xfc || status == 0xfe || status == 0xff)
            return (data1 || data2) ? kAudioUnitErr_InvalidParameterValue : OSStatus(noErr);
        return kAudioUnitErr_InvalidParameterValue;
    }
    OSStatus HandleNoteOn(UInt8 channel, UInt8 key, UInt8 velocity, UInt32 offset) override {
        if (!velocity) return HandleNoteOff(channel, key, 0, offset);
        return note(true, channel, key, float(velocity) / 127.f, -1, 0.f, offset);
    }
    OSStatus HandleNoteOff(UInt8 channel, UInt8 key, UInt8 velocity, UInt32 offset) override {
        return note(false, channel, key, float(velocity) / 127.f, -1, 0.f, offset);
    }
    ComponentResult StartNote(MusicDeviceInstrumentID instrument, MusicDeviceGroupID group,
                              NoteInstanceID *output, UInt32 offset, const MusicDeviceNoteParams &parameters) override {
        if ((instrument != 0 && instrument != kMusicNoteEvent_UseGroupInstrument) || !output || group > 15 || parameters.argCount != 2 ||
            !std::isfinite(parameters.mPitch) || parameters.mPitch < 0.f || parameters.mPitch >= 128.f ||
            !std::isfinite(parameters.mVelocity) || parameters.mVelocity < 0.f || parameters.mVelocity > 127.f ||
            nextIdentity_ > 0xfffffu) return kAudioUnitErr_InvalidParameterValue;
        const auto key = static_cast<uint32_t>(parameters.mPitch);
        const auto identity = (nextIdentity_ << 11) | (group << 7) | key;
        const auto cents = (parameters.mPitch - float(key)) * 100.f;
        const auto status = note(true, group, key, parameters.mVelocity / 127.f, int32_t(identity), cents, offset);
        if (!status) { ++nextIdentity_; *output = identity; }
        return status;
    }
    ComponentResult StopNote(MusicDeviceGroupID group, NoteInstanceID identity, UInt32 offset) override {
        if (group > 15 || !(identity >> 11) || identity > 0x7fffffffu || ((identity >> 7) & 15) != group ||
            (identity >> 11) >= nextIdentity_) return kAudioUnitErr_InvalidParameterValue;
        return note(false, group, identity & 127, 0.f, int32_t(identity), 0.f, offset);
    }
    OSStatus HandleNonNoteEvent(UInt8, UInt8 channel, UInt8 data1, UInt8 data2, UInt32 offset) override {
        if (channel > 15 || data1 > 127 || data2 > 127 || offset >= 65536) return kAudioUnitErr_InvalidParameterValue;
        return noErr;
    }
    OSStatus HandleSysEx(const UInt8 *, UInt32) override { return kAudio_UnimplementedError; }
#if AUSDK_MIDI2_AVAILABLE
    OSStatus MIDIEventList(UInt32, const struct MIDIEventList *) override { return kAudio_UnimplementedError; }
#endif
    ComponentResult GetProperty(AudioUnitPropertyID id, AudioUnitScope scope, AudioUnitElement element, void *data) override {
        if (!data) return kAudioUnitErr_InvalidPropertyValue;
        if (id == kAudioUnitProperty_ParameterStringFromValue || id == kAudioUnitProperty_ParameterValueFromString) {
            if (scope != kAudioUnitScope_Global || element) return kAudioUnitErr_InvalidScope;
            if (id == kAudioUnitProperty_ParameterStringFromValue) {
                auto &request = *static_cast<AudioUnitParameterStringFromValue *>(data);
                AudioUnitParameterValue value = 0.f;
                if (request.inValue) value = *request.inValue;
                else { const auto status = GetParameter(request.inParamID, scope, element, value); if (status) return status; }
                return schema_.format(request.inParamID, value, request.outString) ? ComponentResult(noErr) : kAudioUnitErr_InvalidParameterValue;
            }
            auto &request = *static_cast<AudioUnitParameterValueFromString *>(data);
            return schema_.parse(request.inParamID, request.inString, request.outValue) ? ComponentResult(noErr) : kAudioUnitErr_InvalidParameterValue;
        }
#if AUSDK_MIDI2_AVAILABLE
        if (id == kAudioUnitProperty_AudioUnitMIDIProtocol && scope == kAudioUnitScope_Global && element == 0) {
            *static_cast<SInt32 *>(data) = kMIDIProtocol_1_0; return noErr;
        }
#endif
        return AUWrapper::GetProperty(id, scope, element, data);
    }
    ComponentResult SetParameter(AudioUnitParameterID id, AudioUnitScope scope, AudioUnitElement element,
                                  AudioUnitParameterValue value, UInt32 offset) override {
        AudioUnitParameterValue canonical = 0.f;
        if (scope != kAudioUnitScope_Global || element || !schema_.control(id, value, offset, canonical) ||
            expandedNotes_ + parameterOperations_ + 2 > 1024)
            return kAudioUnitErr_InvalidParameterValue;
        const auto status = AUWrapper::SetParameter(id, scope, element, canonical, offset);
        if (!status) ++parameterOperations_;
        return status;
    }
    ComponentResult SetProperty(AudioUnitPropertyID id, AudioUnitScope scope, AudioUnitElement element,
                                 const void *data, UInt32 size) override {
        if (id == kAudioUnitProperty_ClassInfoFromDocument) {
            if (scope != kAudioUnitScope_Global || element || !data || size != sizeof(CFPropertyListRef)) return kAudioUnitErr_InvalidPropertyValue;
            auto state = *static_cast<const CFPropertyListRef *>(data);
            return state ? restoreComplete(state, true) : kAudioUnitErr_InvalidPropertyValue;
        }
        return AUWrapper::SetProperty(id, scope, element, data, size);
    }
    ComponentResult SaveState(CFPropertyListRef *output) override {
        const auto status = AUWrapper::SaveState(output);
        if (status || !output || !*output || CFGetTypeID(*output) != CFDictionaryGetTypeID()) return status ? status : kAudioUnitErr_InvalidPropertyValue;
        auto dictionary = static_cast<CFMutableDictionaryRef>(const_cast<void *>(*output));
        // The processor's complete Level is authoritative; do not persist a
        // stale asynchronous controller cache alongside a newer DSP snapshot.
        return state_.cohere(dictionary) ? ComponentResult(noErr) : kAudioUnitErr_InvalidPropertyValue;
    }
    ComponentResult RestoreState(CFPropertyListRef state) override { return restoreComplete(state, false); }
    ComponentResult Render(AudioUnitRenderActionFlags &flags, const AudioTimeStamp &time, UInt32 frames) override {
        if (frames > 65536) { clearPending(); return kAudioUnitErr_TooManyFramesToProcess; }
        for (int32 i = 0; i < eventList.getEventCount(); ++i) {
            Event event{};
            if (eventList.getEvent(i, event) != kResultOk || event.sampleOffset < 0 ||
                (frames ? uint32_t(event.sampleOffset) >= frames : event.sampleOffset != 0)) {
                clearPending(); return kAudioUnitErr_InvalidParameterValue;
            }
        }
        // MusicDevice factory delegates normal AUBase render ownership here.
        // This explicit zero-input main-output profile checks native process
        // status instead of concealing processor rejection behind SDK noErr.
        if (!audioProcessor || processData.numInputs != 0 || processData.numOutputs != 1) { clearPending(); return kAudioUnitErr_FailedInitialization; }
        auto *output = GetOutput(0);
        AudioBufferList *lists[]{&output->GetBufferList()};
        if (!audioMap_.map(nullptr, 0, lists, 1, frames, audioBlock_)) { clearPending(); return kAudioUnitErr_InvalidParameterValue; }
        const auto &nativeBus = audioBlock_.buses()[0];
        const auto channels = nativeBus.output_channels;
        const auto pointers = static_cast<float *const *>(nativeBus.outputs);
        for (uint32_t c = 0; c < channels; ++c) processData.outputs[0].channelBuffers32[c] = frames ? pointers[c] : nullptr;
        processData.outputs[0].numChannels = channels;
        processData.outputs[0].silenceFlags = 0;
        ravet::au::NativeAuTransportSnapshot transport;
        if (!ravet::au::nativeTransport(GetHostCallbackInfo(), transport)) return kAudioUnitErr_InvalidParameterValue;
        processData.processContext = ravet::au::vst3Transport(transport, getSampleRate(), processContext);
        processParamChanges.clearQueue(); transferParamChanges.transferChangesTo(processParamChanges);
        processData.numSamples = frames;
        processData.processMode = isOfflineRender ? kOffline : kRealtime;
        const auto rendered = audioProcessor->process(processData);
        outputParamChanges.clearQueue(); processParamChanges.clearQueue(); clearPending();
        if (rendered != kResultOk) return kAudioUnitErr_InvalidParameterValue;
        if (processData.outputs[0].silenceFlags == ((uint64_t(1) << channels) - 1)) flags |= kAudioUnitRenderAction_OutputIsSilence;
        else flags &= ~kAudioUnitRenderAction_OutputIsSilence;
        return noErr;
    }
    void Cleanup() override { clearPending(); audioBlock_ = {}; callerBlock_ = {}; audioMap_ = {}; AUWrapper::Cleanup(); }
    OSStatus renderCaller(AudioUnitRenderActionFlags *flags, const AudioTimeStamp *time, UInt32 bus, UInt32 frames, AudioBufferList *buffers) noexcept {
        if (!time || !buffers) return kAudio_ParamError;
        AudioUnitRenderActionFlags temporary{};
        auto &actualFlags = flags ? *flags : temporary;
        // AUBase can substitute internal buffers or reject a caller copy only
        // after virtual DSP. Check the actual supported caller ranges first so
        // malformed output aliases cannot advance voices or consume notes.
        AudioBufferList *caller[]{buffers};
        const bool rejectCaller = audioMap_.valid() && bus == 0 && frames <= audioMap_.maximumFrames() &&
            !audioMap_.map(nullptr, 0, caller, 1, frames, callerBlock_);
        const auto status = rejectCaller ? OSStatus(kAudioUnitErr_InvalidParameterValue) : DoRender(actualFlags, *time, bus, frames, *buffers);
        if (status) {
            // AUBase may reject before our virtual Render or skip its final
            // buffer copy. Clear only exact supported caller shapes/prefixes.
            const auto channels = GetOutput(0)->GetStreamFormat().mChannelsPerFrame;
            if (bus == 0 && channels >= 1 && channels <= 2 && buffers->mNumberBuffers == channels) {
                const auto count = std::min({frames, GetMaxFramesPerSlice(), UInt32(65536)});
                bool complete = count == frames;
                for (uint32_t c = 0; c < channels; ++c) {
                    const auto &buffer = buffers->mBuffers[c];
                    const auto begin = reinterpret_cast<uintptr_t>(buffer.mData);
                    const auto available = std::min(count, buffer.mDataByteSize / UInt32(sizeof(float)));
                    if (buffer.mNumberChannels == 1 && begin && begin % alignof(float) == 0 &&
                        uint64_t(available) * sizeof(float) <= std::numeric_limits<uintptr_t>::max() - begin) {
                        std::fill_n(static_cast<float *>(buffer.mData), available, 0.f);
                        complete = complete && available == frames;
                    } else complete = false;
                }
                if (complete) actualFlags |= kAudioUnitRenderAction_OutputIsSilence;
            }
        }
        return status;
    }
    ComponentResult Reset(AudioUnitScope scope, AudioUnitElement element) override {
        clearPending();
        if (audioProcessor) { audioProcessor->setProcessing(false); audioProcessor->setProcessing(true); }
        return AUWrapper::Reset(scope, element);
    }
private:
    uint32_t nextIdentity_ = 1, expandedNotes_ = 0, parameterOperations_ = 0;
    ravet::au::NativeAuParameters<1> schema_;
    ravet::au::NativeAuState state_;
    ravet::au::NativeAuAudioBusMap audioMap_;
    ravet::au::NativeAuAudioBlock audioBlock_;
    ravet::au::NativeAuAudioBlock callerBlock_;
    ComponentResult restoreComplete(CFPropertyListRef value, bool document) {
        if (!state_.validate(value, GetComponentDescription())) return kAudioUnitErr_InvalidPropertyValue;
        return AUWrapper::restoreState(value, document);
    }
    void clearPending() { eventList.clear(); expandedNotes_ = 0; parameterOperations_ = 0; }
    OSStatus note(bool on, uint32_t channel, uint32_t key, float velocity, int32_t identity, float cents, uint32_t offset) {
        const auto expanded = on && cents != 0.f ? 2u : 1u;
        if (channel > 15 || key > 127 || !std::isfinite(velocity) || velocity < 0.f || velocity > 1.f || offset >= 65536 ||
            expandedNotes_ + parameterOperations_ + expanded + 1 > 1024) return kAudioUnitErr_InvalidParameterValue;
        if (eventList.getEventCount()) {
            Event previous{}; eventList.getEvent(eventList.getEventCount() - 1, previous);
            if (offset < uint32_t(previous.sampleOffset)) return kAudioUnitErr_InvalidParameterValue;
        }
        Event event{}; event.sampleOffset = offset;
        if (on) { event.type = Event::kNoteOnEvent; event.noteOn.channel = channel; event.noteOn.pitch = key; event.noteOn.velocity = velocity; event.noteOn.noteId = identity; event.noteOn.tuning = cents; }
        else { event.type = Event::kNoteOffEvent; event.noteOff.channel = channel; event.noteOff.pitch = key; event.noteOff.velocity = velocity; event.noteOff.noteId = identity; }
        if (eventList.addEvent(event) != kResultOk) return kAudioUnitErr_InvalidParameterValue;
        expandedNotes_ += expanded; return noErr;
    }
};
OSStatus renderEntry(void *self, AudioUnitRenderActionFlags *flags, const AudioTimeStamp *time, UInt32 bus, UInt32 frames, AudioBufferList *buffers) noexcept {
    const auto instance = static_cast<ausdk::AudioComponentPlugInInstance *>(self);
    return reinterpret_cast<PerformanceAU *>(&instance->mInstanceStorage)->renderCaller(flags, time, bus, frames, buffers);
}
OSStatus setPropertyEntry(void *self, AudioUnitPropertyID id, AudioUnitScope scope,
                          AudioUnitElement element, const void *data, UInt32 size) {
    // AUBase handles this property before the virtual SetProperty callback.
    // Preflight the bounded profile before SDK buffer allocation or mutation,
    // then retain its normal instance guard, exception boundary and dispatch.
    if (id == kAudioUnitProperty_MaximumFramesPerSlice) {
        if (scope != kAudioUnitScope_Global || element || !data || size != sizeof(UInt32)) return kAudioUnitErr_InvalidPropertyValue;
        UInt32 frames; std::memcpy(&frames, data, sizeof(frames));
        if (!frames || frames > 65536) return kAudioUnitErr_InvalidPropertyValue;
    }
    using Callback = OSStatus (*)(void *, AudioUnitPropertyID, AudioUnitScope, AudioUnitElement, const void *, UInt32);
    const auto sdk = reinterpret_cast<Callback>(ausdk::AUMusicLookup::Lookup(kAudioUnitSetPropertySelect));
    return sdk(self, id, scope, element, data, size);
}
struct PerformanceLookup {
    static AudioComponentMethod Lookup(SInt16 selector) {
        if (selector == kAudioUnitRenderSelect) return reinterpret_cast<AudioComponentMethod>(renderEntry);
        if (selector == kAudioUnitSetPropertySelect) return reinterpret_cast<AudioComponentMethod>(setPropertyEntry);
        return ausdk::AUMusicLookup::Lookup(selector);
    }
};
}
extern "C" __attribute__((visibility("default"))) void *AUWrapperFactory(const AudioComponentDescription *description) {
    return ausdk::APFactory<PerformanceLookup, PerformanceAU>::Factory(description);
}
