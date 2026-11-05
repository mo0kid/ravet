// Generic AUv2 adapter. Product code and Apple's SDK remain separate libraries.
#include "Table.hpp"
#include "RavetAuAudioFormat.hpp"
#include "RavetAuMidiOutput.hpp"
#include "RavetAuTransport.hpp"
#include <AudioUnitSDK/AUEffectBase.h>
#include <AudioUnitSDK/AUMIDIEffectBase.h>
#include <AudioUnitSDK/MusicDeviceBase.h>
#import <AudioUnit/AUCocoaUIView.h>
#import <Cocoa/Cocoa.h>
#include <atomic>
#include <exception>
#include <functional>
#include <memory>
#include <vector>

namespace ravet::auv2 {
constexpr AudioUnitPropertyID lifetimeProperty = 0x52764155;
constexpr uint32_t eventCapacity = 1024;
constexpr uint32_t maximumStateBytes = 17 * 1024 * 1024;

// AU control methods may arrive on a worker. Only the table calls marked MAIN
// cross to the AppKit thread; scheduled events and render never dispatch.
template <class F> void mainCall(F function) {
    if (pthread_main_np()) { function(); return; }
    __block std::exception_ptr failure;
    dispatch_sync(dispatch_get_main_queue(), ^{
        try { function(); } catch (...) { failure = std::current_exception(); }
    });
    if (failure) std::rethrow_exception(failure);
}
inline CFStringRef string(const char *text) {
    return CFStringCreateWithCString(nullptr, text, kCFStringEncodingUTF8);
}
struct Life : std::enable_shared_from_this<Life> {
    const RavetPluginEntryV1 *entry;
    void *instance = nullptr;
    AudioUnit unit;
    bool editorOpen = false;
    std::function<void()> poll;
    void edits() {
        std::array<RavetPluginEditV1, 256> edits{};
        const auto count = std::min(entry->take_edits(instance, edits.data(), edits.size()), uint32_t(edits.size()));
        for (uint32_t i = 0; i < count; ++i) {
            const auto &edit = edits[i];
            RavetPluginParameterV1 p{};
            if (edit.index >= entry->product->parameter_count || entry->parameter(edit.index, &p)) continue;
            AudioUnitEvent event{};
            event.mArgument.mParameter = {unit, p.id, kAudioUnitScope_Global, 0};
            if (edit.kind == RAVET_PLUGIN_EDIT_BEGIN) event.mEventType = kAudioUnitEvent_BeginParameterChangeGesture;
            else if (edit.kind == RAVET_PLUGIN_EDIT_END) event.mEventType = kAudioUnitEvent_EndParameterChangeGesture;
            else if (edit.kind == RAVET_PLUGIN_EDIT_VALUE) event.mEventType = kAudioUnitEvent_ParameterValueChange;
            else continue;
            AUEventListenerNotify(nullptr, nullptr, &event);
        }
    }
    void closeEditor() {
        if (editorOpen) { editorOpen = false; entry->editor_close(instance); edits(); }
    }
};
} // namespace ravet::auv2

@interface RAVET_AU_VIEW_CLASS : NSView {
    std::weak_ptr<ravet::auv2::Life> _life;
    BOOL _ownsEditor;
}
- (instancetype)initWithLife:(std::weak_ptr<ravet::auv2::Life>)life size:(NSSize)size;
@end
@implementation RAVET_AU_VIEW_CLASS
- (instancetype)initWithLife:(std::weak_ptr<ravet::auv2::Life>)life size:(NSSize)size {
    self = [super initWithFrame:NSMakeRect(0, 0, size.width, size.height)];
    if (self) {
        _life = life;
        const auto token = _life.lock();
        if (!token || !token->instance || token->editorOpen ||
            token->entry->editor_open(token->instance, RAVET_PLUGIN_VIEW_COCOA,
                (__bridge void *)self, uint32_t(size.width), uint32_t(size.height))) return nil;
        token->editorOpen = true;
        _ownsEditor = YES;
    }
    return self;
}
- (void)setFrameSize:(NSSize)size {
    if (!std::isfinite(size.width) || !std::isfinite(size.height)) return;
    const auto token = _life.lock();
    if (_ownsEditor && token && token->instance) {
        const auto &p = *token->entry->product;
        if (!p.editor_resizable) size = NSMakeSize(p.editor_width, p.editor_height);
        else {
            size.width = std::clamp(size.width, CGFloat(p.editor_width), CGFloat(p.editor_width) * 4);
            size.height = std::clamp(size.height, CGFloat(p.editor_height), CGFloat(p.editor_height) * 4);
        }
        if (token->entry->editor_resize(token->instance, uint32_t(size.width), uint32_t(size.height))) return;
    }
    [super setFrameSize:size];
}
- (void)dealloc {
    const auto token = _life.lock();
    if (_ownsEditor && token && token->instance) token->closeEditor();
}
@end

@interface RAVET_AU_FACTORY_CLASS : NSObject <AUCocoaUIBase>
@end
@implementation RAVET_AU_FACTORY_CLASS
- (unsigned)interfaceVersion { return 0; }
- (NSString *)description { return @"RAVET product editor"; }
- (NSView *)uiViewForAudioUnit:(AudioUnit)unit withSize:(NSSize)preferred {
    (void)preferred;
    if (!pthread_main_np()) return nil;
    ravet::auv2::Life *life = nullptr;
    UInt32 size = sizeof(life);
    if (AudioUnitGetProperty(unit, ravet::auv2::lifetimeProperty, kAudioUnitScope_Global, 0, &life, &size) || !life) return nil;
    const auto weak = life->weak_from_this();
    const auto token = weak.lock();
    if (!token || !token->instance) return nil;
    const auto &p = *token->entry->product;
    return [[RAVET_AU_VIEW_CLASS alloc] initWithLife:weak size:NSMakeSize(p.editor_width, p.editor_height)];
}
@end

namespace ravet::auv2 {
class EffectBase : public ausdk::AUEffectBase {
public: explicit EffectBase(AudioUnit unit) : AUEffectBase(unit, false) {}
};
class MidiBase : public ausdk::AUMIDIEffectBase {
public: explicit MidiBase(AudioUnit unit) : AUMIDIEffectBase(unit, false) {}
};
class InstrumentBase : public ausdk::MusicDeviceBase {
public: explicit InstrumentBase(AudioUnit unit) : MusicDeviceBase(unit, 0, 1) {}
};

template <class Base> class PluginAU final : public Base {
public:
    explicit PluginAU(AudioUnit unit) : Base(unit), entry_(checked(ravet_plugin_v1())), product_(*entry_.product) {
        this->CreateElements();
        // A single main input element is present only for audio-input products.
        if (!product_.layouts[0].inputs) this->Inputs().SetNumberOfElements(0);
        else this->Inputs().SetNumberOfElements(1);
        for (uint32_t i = 0; i < product_.layout_count; ++i)
            channels_.push_back({SInt16(product_.layouts[i].inputs), SInt16(product_.layouts[i].outputs)});
        parameters_.resize(product_.parameter_count);
        for (uint32_t i = 0; i < product_.parameter_count; ++i) {
            auto &p = parameters_[i];
            if (entry_.parameter(i, &p) || entry_.parameter_index(p.id) != int32_t(i) ||
                !p.name || !p.unit || !std::isfinite(p.minimum) || !std::isfinite(p.maximum) ||
                p.minimum >= p.maximum || !std::isfinite(p.default_value))
                throw std::runtime_error("invalid AU parameter metadata");
            this->Globals()->SetParameter(p.id, p.default_value);
        }
        auto format = defaultFormat(product_.layouts[0].outputs);
        this->Output(0).SetStreamFormat(format);
        if (product_.layouts[0].inputs) this->Input(0).SetStreamFormat(defaultFormat(product_.layouts[0].inputs));
        life_ = std::make_shared<Life>();
        life_->entry = &entry_; life_->unit = unit;
        mainCall([&] {
            life_->instance = entry_.create();
            if (!life_->instance) throw std::runtime_error("RAVET instance creation failed");
            life_->poll = [this] { poll(); };
            const std::weak_ptr<Life> weak = life_;
            timer_ = [NSTimer timerWithTimeInterval:1.0 / 30 repeats:YES block:^(NSTimer *) {
                if (const auto token = weak.lock(); token && token->instance) token->poll();
            }];
            [[NSRunLoop mainRunLoop] addTimer:timer_ forMode:NSRunLoopCommonModes];
        });
    }
    ~PluginAU() override {
        mainCall([&] {
            [timer_ invalidate]; timer_ = nil;
            const auto token = life_;
            token->closeEditor();
            void *instance = token->instance;
            // Invalidate before destruction, including any reentrant native
            // view callback triggered while the product tears down its UI.
            token->instance = nullptr;
            token->poll = {};
            if (prepared_) entry_.deactivate(instance);
            entry_.destroy(instance);
            life_.reset();
        });
    }
    OSStatus Initialize() override {
        const auto &out = this->Output(0).GetStreamFormat();
        const auto inputs = this->Inputs().GetNumberOfElements() ? this->Input(0).GetStreamFormat().mChannelsPerFrame : 0;
        bool match = false;
        for (const auto &c : channels_) if (inputs == uint32_t(c.inChannels) && out.mChannelsPerFrame == uint32_t(c.outChannels)) match = true;
        if (!match || !ValidFormat(kAudioUnitScope_Output, 0, out) ||
            (inputs && (!ValidFormat(kAudioUnitScope_Input, 0, this->Input(0).GetStreamFormat()) ||
                this->Input(0).GetStreamFormat().mSampleRate != out.mSampleRate))) return kAudioUnitErr_FormatNotSupported;
        RavetNativeProcessorConfigV1 config{};
        config.version = 1; config.maximum_frames = this->GetMaxFramesPerSlice();
        config.maximum_events = eventCapacity; config.maximum_output_events = product_.note_output ? eventCapacity : 0;
        config.bus_count = 1; config.sample_rate = out.mSampleRate; config.buses[0] = {inputs, out.mChannelsPerFrame};
        if (!map_.configure(config)) return kAudioUnitErr_FormatNotSupported;
        const auto base = Base::Initialize();
        if (base) return base;
        int32_t result = 1;
        mainCall([&] { result = entry_.prepare(life_->instance, &config); });
        if (result) { Base::Cleanup(); return kAudioUnitErr_FailedInitialization; }
        prepared_ = true; inputChannels_ = inputs; sampleRate_ = out.mSampleRate;
        mainCall([&] { readTiming(); }); pending_ = 0;
        return noErr;
    }
    void Cleanup() override {
        if (prepared_) mainCall([&] { entry_.deactivate(life_->instance); });
        prepared_ = false; pending_ = 0;
        Base::Cleanup();
    }
    OSStatus Reset(AudioUnitScope scope, AudioUnitElement element) override {
        if (scope != kAudioUnitScope_Global || element) return kAudioUnitErr_InvalidScope;
        pending_ = 0;
        return entry_.reset(life_->instance) ? OSStatus(kAudioUnitErr_CannotDoInCurrentContext) : OSStatus(noErr);
    }
    bool CanScheduleParameters() const noexcept override { return true; }
    bool StreamFormatWritable(AudioUnitScope scope, AudioUnitElement element) override {
        return !this->IsInitialized() && !element &&
            (scope == kAudioUnitScope_Output || (scope == kAudioUnitScope_Input && this->Inputs().GetNumberOfElements()));
    }
    bool ValidFormat(AudioUnitScope scope, AudioUnitElement element, const AudioStreamBasicDescription &format) override {
        if (element || (scope != kAudioUnitScope_Output && scope != kAudioUnitScope_Input) ||
            !ravet::au::NativeAuAudioFormat::format(format, format.mChannelsPerFrame, format.mSampleRate)) return false;
        for (const auto &c : channels_)
            if (format.mChannelsPerFrame == uint32_t(scope == kAudioUnitScope_Input ? c.inChannels : c.outChannels)) return true;
        return false;
    }
    UInt32 SupportedNumChannels(const AUChannelInfo **info) override {
        if (info) *info = channels_.data();
        return channels_.size();
    }
    OSStatus GetParameterList(AudioUnitScope scope, AudioUnitParameterID *ids, UInt32 &count) override {
        if (scope != kAudioUnitScope_Global) return kAudioUnitErr_InvalidScope;
        count = parameters_.size();
        if (ids) for (const auto &p : parameters_) *ids++ = p.id;
        return noErr;
    }
    OSStatus GetParameterInfo(AudioUnitScope scope, AudioUnitParameterID id, AudioUnitParameterInfo &info) override {
        const auto index = entry_.parameter_index(id);
        if (scope != kAudioUnitScope_Global) return kAudioUnitErr_InvalidScope;
        if (index < 0 || uint32_t(index) >= parameters_.size()) return kAudioUnitErr_InvalidParameter;
        const auto &p = parameters_[index];
        info = {};
        info.cfNameString = string(p.name);
        info.flags = kAudioUnitParameterFlag_HasCFNameString | kAudioUnitParameterFlag_CFNameRelease | kAudioUnitParameterFlag_IsReadable;
        info.flags |= kAudioUnitParameterFlag_ValuesHaveStrings;
        if (!(p.flags & RAVET_PLUGIN_PARAMETER_READ_ONLY)) info.flags |= kAudioUnitParameterFlag_IsWritable;
        if (!(p.flags & RAVET_PLUGIN_PARAMETER_AUTOMATABLE)) info.flags |= kAudioUnitParameterFlag_NonRealTime;
        if (p.flags & RAVET_PLUGIN_PARAMETER_HIDDEN) info.flags |= kAudioUnitParameterFlag_ExpertMode;
        if (p.scale == 1) info.flags |= kAudioUnitParameterFlag_DisplayLogarithmic;
        info.minValue = p.minimum; info.maxValue = p.maximum; info.defaultValue = p.default_value;
        info.unit = kAudioUnitParameterUnit_Generic;
        if (p.flags & RAVET_PLUGIN_PARAMETER_BYPASS) info.unit = kAudioUnitParameterUnit_Boolean;
        else if (indexed(p)) info.unit = kAudioUnitParameterUnit_Indexed;
        else if (std::strcmp(p.unit, "dB") == 0) info.unit = kAudioUnitParameterUnit_Decibels;
        else if (std::strcmp(p.unit, "Hz") == 0) info.unit = kAudioUnitParameterUnit_Hertz;
        else if (std::strcmp(p.unit, "%") == 0) info.unit = kAudioUnitParameterUnit_Percent;
        else if (std::strcmp(p.unit, "ms") == 0) info.unit = kAudioUnitParameterUnit_Milliseconds;
        else if (*p.unit) { info.unit = kAudioUnitParameterUnit_CustomUnit; info.unitName = string(p.unit); }
        return noErr;
    }
    OSStatus GetParameterValueStrings(AudioUnitScope scope, AudioUnitParameterID id, CFArrayRef *strings) override {
        const auto index = entry_.parameter_index(id);
        if (scope != kAudioUnitScope_Global) return kAudioUnitErr_InvalidScope;
        if (index < 0 || uint32_t(index) >= parameters_.size() || !indexed(parameters_[index]) || parameters_[index].steps > 65535)
            return kAudioUnitErr_InvalidParameter;
        if (!strings) return noErr;
        const auto &p = parameters_[index];
        auto array = CFArrayCreateMutable(nullptr, p.steps + 1, &kCFTypeArrayCallBacks);
        if (!array) return memFullErr;
        for (uint32_t i = 0; i <= p.steps; ++i) {
            char text[1024]{};
            if (entry_.value_to_text(index, p.minimum + i, text, sizeof(text))) { CFRelease(array); return kAudioUnitErr_InvalidParameterValue; }
            auto value = string(text);
            if (!value) { CFRelease(array); return memFullErr; }
            CFArrayAppendValue(array, value); CFRelease(value);
        }
        *strings = array; return noErr;
    }
    OSStatus GetParameter(AudioUnitParameterID id, AudioUnitScope scope, AudioUnitElement element, AudioUnitParameterValue &value) noexcept override {
        const auto index = entry_.parameter_index(id);
        if (scope != kAudioUnitScope_Global || element) return kAudioUnitErr_InvalidScope;
        if (index < 0 || uint32_t(index) >= parameters_.size()) return kAudioUnitErr_InvalidParameter;
        value = entry_.get_value(life_->instance, index); return noErr;
    }
    OSStatus SetParameter(AudioUnitParameterID id, AudioUnitScope scope, AudioUnitElement element, AudioUnitParameterValue value, UInt32 offset) noexcept override {
        double physical = 0;
        const auto result = parameterValue(id, scope, element, value, physical);
        if (result) return result;
        if (!offset && pthread_main_np())
            return entry_.set_value(life_->instance, entry_.parameter_index(id), physical) ? OSStatus(kAudioUnitErr_InvalidParameterValue) : OSStatus(noErr);
        if (pending_ == eventCapacity || offset >= this->GetMaxFramesPerSlice()) return kAudioUnitErr_InvalidParameterValue;
        events_[pending_++] = parameterEvent(id, offset, physical); return noErr;
    }
    OSStatus ScheduleParameter(const AudioUnitParameterEvent *events, UInt32 count) noexcept override {
        if (count && !events) return kAudio_ParamError;
        std::array<RavetNativeEventV1, eventCapacity> candidate{};
        uint32_t used = 0;
        for (UInt32 i = 0; i < count; ++i) {
            const auto &e = events[i];
            if (e.eventType == kParameterEvent_Immediate) {
                double physical = 0;
                const auto status = parameterValue(e.parameter, e.scope, e.element, e.eventValues.immediate.value, physical);
                if (status) return status;
                if (used >= eventCapacity - pending_ || e.eventValues.immediate.bufferOffset >= this->GetMaxFramesPerSlice()) return kAudioUnitErr_InvalidParameterValue;
                candidate[used++] = parameterEvent(e.parameter, e.eventValues.immediate.bufferOffset, physical);
            } else if (e.eventType == kParameterEvent_Ramped) {
                const auto &r = e.eventValues.ramp;
                const int64_t start = r.startBufferOffset, end = start + r.durationInFrames;
                if (!r.durationInFrames || end <= 0 || end > this->GetMaxFramesPerSlice() ||
                    !std::isfinite(r.startValue) || !std::isfinite(r.endValue)) return kAudioUnitErr_InvalidParameterValue;
                // Bounded per-sample expansion preserves the product's own
                // automation policy without slicing its DSP/transport calls.
                for (int64_t frame = std::max<int64_t>(0, start); frame < end; ++frame) {
                    double physical = 0;
                    const auto value = r.startValue + (r.endValue - r.startValue) * double(frame - start) / r.durationInFrames;
                    const auto status = parameterValue(e.parameter, e.scope, e.element, value, physical);
                    if (status) return status;
                    if (used >= eventCapacity - pending_) return kAudioUnitErr_InvalidParameterValue;
                    candidate[used++] = parameterEvent(e.parameter, frame, physical);
                }
                double physical = 0;
                const auto status = parameterValue(e.parameter, e.scope, e.element, r.endValue, physical);
                if (status) return status;
                if (used >= eventCapacity - pending_) return kAudioUnitErr_InvalidParameterValue;
                candidate[used++] = parameterEvent(e.parameter, end, physical);
            } else return kAudioUnitErr_InvalidParameterValue;
        }
        std::copy_n(candidate.begin(), used, events_.begin() + pending_); pending_ += used; return noErr;
    }
    OSStatus MIDIEvent(UInt32 status, UInt32 data1, UInt32 data2, UInt32 offset) noexcept override {
        if (!product_.note_input) return kAudioUnitErr_InvalidProperty;
        if (status < 0x80 || status > 0xff || status == 0xf0 || status == 0xf7 || data1 > 127 || data2 > 127 ||
            offset >= this->GetMaxFramesPerSlice() || pending_ == eventCapacity) return kAudioUnitErr_InvalidParameterValue;
        const uint32_t kind = status & 0xf0;
        const uint32_t size = status < 0xf0 ? ((kind == 0xc0 || kind == 0xd0) ? 2 : 3) :
            (status == 0xf1 || status == 0xf3 ? 2 : status == 0xf2 ? 3 :
            (status == 0xf6 || status == 0xf8 || status == 0xfa || status == 0xfb || status == 0xfc || status == 0xfe || status == 0xff ? 1 : 0));
        if (!size) return kAudioUnitErr_InvalidParameterValue;
        RavetNativeEventV1 event{}; event.kind = RAVET_NATIVE_MIDI; event.offset = offset; event.detail = size;
        event.words[0] = status | (size >= 2 ? data1 << 8 : 0) | (size == 3 ? data2 << 16 : 0);
        events_[pending_++] = event; return noErr;
    }
    // Do not advertise MIDI 2/SysEx until the table can carry variable data.
    OSStatus SysEx(const UInt8 *, UInt32) noexcept override { return kAudio_UnimplementedError; }

    OSStatus Render(AudioUnitRenderActionFlags &flags, const AudioTimeStamp &time, UInt32 frames) noexcept override {
        auto &output = this->Output(0).GetBufferList();
        const auto count = pending_; pending_ = 0;
        auto fault = [&](OSStatus error) { silence(output, frames); flags |= kAudioUnitRenderAction_OutputIsSilence; return error; };
        if (!prepared_ || frames > map_.maximumFrames()) return fault(kAudioUnitErr_TooManyFramesToProcess);
        const AudioBufferList *input = nullptr;
        if (inputChannels_) {
            const auto status = this->Input(0).PullInput(flags, time, 0, frames);
            if (status) return fault(status);
            input = &this->Input(0).GetBufferList();
        }
        ravet::au::NativeAuAudioBlock block;
        AudioBufferList *out = &output;
        if (!map_.map(input ? &input : nullptr, input ? 1 : 0, &out, 1, frames, block)) return fault(kAudioUnitErr_FormatNotSupported);
        for (uint32_t i = 0; i < count; ++i) {
            if (events_[i].offset > frames) return fault(kAudioUnitErr_InvalidParameterValue);
            // Stable bounded insertion sort: preserves simultaneous host events.
            const auto event = events_[i]; uint32_t j = i;
            while (j && events_[j - 1].offset > event.offset) { events_[j] = events_[j - 1]; --j; }
            events_[j] = event;
        }
        ravet::au::NativeAuTransportSnapshot transport;
        if (!ravet::au::nativeTransport(this->GetHostCallbackInfo(), transport)) return fault(kAudioUnitErr_InvalidPropertyValue);
        uint32_t processCount = count;
        if (frames) while (processCount && events_[processCount - 1].offset == frames) --processCount;
        RavetNativeProcessorResultV1 result{};
        const auto code = entry_.process(life_->instance, block.buses(), 1, frames, events_.data(), processCount,
            &transport.transport(), outputEvents_.data(), product_.note_output ? eventCapacity : 0, &result);
        if (code || result.status > 1 || result.event_count > (product_.note_output ? eventCapacity : 0)) return fault(kAudioUnitErr_CannotDoInCurrentContext);
        const bool latencyChanged = entry_.latency_changed(life_->instance) != 0;
        // Tail can change without a latency change. Query between process calls
        // on the same audio owner; property getters only read atomic snapshots.
        if (readTiming() || latencyChanged) timingChanged_.store(true, std::memory_order_release);
        if (product_.note_output) {
            if (!midi_.stage(outputEvents_.data(), result.event_count, frames)) return fault(kAudioUnitErr_InvalidParameterValue);
            // A missing/rejecting callback cannot undo successful DSP. No replay.
            midi_.deliver(midiCallback_, &time, 0);
        }
        flags &= ~kAudioUnitRenderAction_OutputIsSilence;
        if (result.status == 1) { silence(output, frames); flags |= kAudioUnitRenderAction_OutputIsSilence; }
        // A ramp endpoint at the buffer boundary becomes offset zero of the
        // next render, so its final value is held after the ramp completes.
        for (uint32_t i = processCount; i < count; ++i) {
            events_[pending_] = events_[i]; events_[pending_++].offset = 0;
        }
        return noErr;
    }
    Float64 GetLatency() noexcept override { return latency_.load(std::memory_order_relaxed); }
    Float64 GetTailTime() noexcept override { return tail_.load(std::memory_order_relaxed); }
    bool SupportsTail() noexcept override { return true; }

    OSStatus SaveState(CFPropertyListRef *output) override {
        if (!output) return kAudio_ParamError;
        OSStatus status = kAudioUnitErr_InvalidPropertyValue;
        mainCall([&] {
            uint32_t length = 0;
            if (entry_.save_state(life_->instance, nullptr, 0, &length) || length > maximumStateBytes) return;
            std::vector<uint8_t> bytes(length);
            if (entry_.save_state(life_->instance, bytes.data(), bytes.size(), &length) || length > bytes.size()) return;
            auto dictionary = CFDictionaryCreateMutable(nullptr, 0, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
            for (const auto &field : {std::pair{CFSTR("version"), uint32_t(0)}, {CFSTR("type"), product_.au_type},
                {CFSTR("subtype"), product_.au_subtype}, {CFSTR("manufacturer"), product_.au_manufacturer}}) {
                const auto value = int32_t(field.second);
                auto number = CFNumberCreate(nullptr, kCFNumberSInt32Type, &value);
                CFDictionarySetValue(dictionary, field.first, number); CFRelease(number);
            }
            auto data = CFDataCreate(nullptr, bytes.data(), length);
            AUPreset preset{};
            Base::GetProperty(kAudioUnitProperty_PresentPreset, kAudioUnitScope_Global, 0, &preset);
            auto name = preset.presetName ? preset.presetName : string(product_.name);
            CFDictionarySetValue(dictionary, CFSTR("RAVET"), data); CFRelease(data);
            CFDictionarySetValue(dictionary, CFSTR("name"), name); CFRelease(name);
            *output = dictionary; status = noErr;
        });
        return status;
    }
    OSStatus RestoreState(CFPropertyListRef state) override {
        if (!state || CFGetTypeID(state) != CFDictionaryGetTypeID()) return kAudioUnitErr_InvalidPropertyValue;
        const auto dictionary = static_cast<CFDictionaryRef>(state);
        for (const auto &field : {std::pair{CFSTR("version"), uint32_t(0)}, {CFSTR("type"), product_.au_type},
            {CFSTR("subtype"), product_.au_subtype}, {CFSTR("manufacturer"), product_.au_manufacturer}}) {
            const auto value = CFDictionaryGetValue(dictionary, field.first); int32_t decoded = 0;
            if (!value || CFGetTypeID(value) != CFNumberGetTypeID() ||
                !CFNumberGetValue(static_cast<CFNumberRef>(value), kCFNumberSInt32Type, &decoded) || uint32_t(decoded) != field.second)
                return kAudioUnitErr_InvalidPropertyValue;
        }
        const auto value = CFDictionaryGetValue(dictionary, CFSTR("RAVET"));
        if (!value || CFGetTypeID(value) != CFDataGetTypeID()) return kAudioUnitErr_InvalidPropertyValue;
        auto data = static_cast<CFDataRef>(value);
        if (CFDataGetLength(data) > maximumStateBytes) return kAudioUnitErr_InvalidPropertyValue;
        int32_t result = 1;
        mainCall([&] {
            result = entry_.load_state(life_->instance, CFDataGetBytePtr(data), CFDataGetLength(data));
            if (!result) {
                auto name = CFDictionaryGetValue(dictionary, CFSTR("name"));
                if (name && CFGetTypeID(name) == CFStringGetTypeID())
                    Base::NewCustomPresetSet({-1, static_cast<CFStringRef>(name)});
                AudioUnitParameter parameter{this->GetComponentInstance(), kAUParameterListener_AnyParameter, kAudioUnitScope_Global, 0};
                AUParameterListenerNotify(nullptr, nullptr, &parameter);
                this->PropertyChanged(kAudioUnitProperty_BypassEffect, kAudioUnitScope_Global, 0);
            }
        });
        return result ? OSStatus(kAudioUnitErr_InvalidPropertyValue) : OSStatus(noErr);
    }
    OSStatus GetPropertyInfo(AudioUnitPropertyID id, AudioUnitScope scope, AudioUnitElement element, UInt32 &size, bool &writable) override {
        if (customProperty(id)) {
            if (scope != kAudioUnitScope_Global || element) return kAudioUnitErr_InvalidScope;
            if ((id == kAudioUnitProperty_BypassEffect && product_.bypass_index < 0) ||
                ((id == kAudioUnitProperty_MIDIOutputCallback || id == kAudioUnitProperty_MIDIOutputCallbackInfo) && !product_.note_output) ||
                ((id == kAudioUnitProperty_CocoaUI || id == lifetimeProperty) && !product_.editor_width)) return kAudioUnitErr_InvalidProperty;
            size = id == kAudioUnitProperty_BypassEffect ? sizeof(UInt32) :
                id == kAudioUnitProperty_CocoaUI ? sizeof(AudioUnitCocoaViewInfo) :
                id == lifetimeProperty ? sizeof(Life *) :
                id == kAudioUnitProperty_ParameterStringFromValue ? sizeof(AudioUnitParameterStringFromValue) :
                id == kAudioUnitProperty_ParameterValueFromString ? sizeof(AudioUnitParameterValueFromString) :
                id == kAudioUnitProperty_MIDIOutputCallback ? sizeof(AUMIDIOutputCallbackStruct) :
                id == kAudioUnitProperty_ClassInfoFromDocument ? sizeof(CFPropertyListRef) : sizeof(CFArrayRef);
            writable = id == kAudioUnitProperty_BypassEffect || id == kAudioUnitProperty_MIDIOutputCallback || id == kAudioUnitProperty_ClassInfoFromDocument;
            return noErr;
        }
        return Base::GetPropertyInfo(id, scope, element, size, writable);
    }
    OSStatus GetProperty(AudioUnitPropertyID id, AudioUnitScope scope, AudioUnitElement element, void *data) override {
        if (!customProperty(id)) return Base::GetProperty(id, scope, element, data);
        UInt32 size; bool writable;
        const auto status = GetPropertyInfo(id, scope, element, size, writable);
        if (status || !data) return status ? status : kAudio_ParamError;
        switch (id) {
        case lifetimeProperty: *static_cast<Life **>(data) = life_.get(); return noErr;
        case kAudioUnitProperty_CocoaUI: {
            auto &info = *static_cast<AudioUnitCocoaViewInfo *>(data);
            NSBundle *bundle = [NSBundle bundleWithIdentifier:@RAVET_AU_BUNDLE_ID];
            if (!bundle) return kAudioUnitErr_InvalidPropertyValue;
            info.mCocoaAUViewBundleLocation = (__bridge_retained CFURLRef)bundle.bundleURL;
            info.mCocoaAUViewClass[0] = (__bridge_retained CFStringRef)NSStringFromClass([RAVET_AU_FACTORY_CLASS class]); return noErr;
        }
        case kAudioUnitProperty_BypassEffect:
            *static_cast<UInt32 *>(data) = entry_.get_value(life_->instance, product_.bypass_index) >= .5; return noErr;
        case kAudioUnitProperty_ParameterStringFromValue: {
            auto &request = *static_cast<AudioUnitParameterStringFromValue *>(data);
            const auto index = entry_.parameter_index(request.inParamID);
            if (index < 0) return kAudioUnitErr_InvalidParameter;
            char text[1024]{};
            if (entry_.value_to_text(index, request.inValue ? *request.inValue : entry_.get_value(life_->instance, index), text, sizeof(text)))
                return kAudioUnitErr_InvalidParameterValue;
            request.outString = string(text); return request.outString ? OSStatus(noErr) : OSStatus(memFullErr);
        }
        case kAudioUnitProperty_ParameterValueFromString: {
            auto &request = *static_cast<AudioUnitParameterValueFromString *>(data);
            const auto index = entry_.parameter_index(request.inParamID);
            if (index < 0 || !request.inString || CFGetTypeID(request.inString) != CFStringGetTypeID()) return kAudioUnitErr_InvalidParameter;
            char text[4096]{}; double value = 0;
            for (CFIndex i = 0; i < CFStringGetLength(request.inString); ++i)
                if (CFStringGetCharacterAtIndex(request.inString, i) == 0) return kAudioUnitErr_InvalidParameterValue;
            if (!CFStringGetCString(request.inString, text, sizeof(text), kCFStringEncodingUTF8) ||
                entry_.text_to_value(index, text, &value) || !std::isfinite(value)) return kAudioUnitErr_InvalidParameterValue;
            request.outValue = value; return noErr;
        }
        case kAudioUnitProperty_MIDIOutputCallback: *static_cast<AUMIDIOutputCallbackStruct *>(data) = midiCallback_; return noErr;
        case kAudioUnitProperty_MIDIOutputCallbackInfo: {
            const void *name = CFSTR("RAVET MIDI Output");
            *static_cast<CFArrayRef *>(data) = CFArrayCreate(nullptr, &name, 1, &kCFTypeArrayCallBacks); return noErr;
        }
        case kAudioUnitProperty_ClassInfoFromDocument: return SaveState(static_cast<CFPropertyListRef *>(data));
        default: return kAudioUnitErr_InvalidProperty;
        }
    }
    OSStatus SetProperty(AudioUnitPropertyID id, AudioUnitScope scope, AudioUnitElement element, const void *data, UInt32 size) override {
        if (!customProperty(id)) return Base::SetProperty(id, scope, element, data, size);
        UInt32 expected; bool writable;
        const auto status = GetPropertyInfo(id, scope, element, expected, writable);
        if (status) return status;
        if (!writable) return kAudioUnitErr_PropertyNotWritable;
        if (!data || size != expected) return kAudioUnitErr_InvalidPropertyValue;
        if (id == kAudioUnitProperty_ClassInfoFromDocument) return RestoreState(*static_cast<const CFPropertyListRef *>(data));
        if (id == kAudioUnitProperty_MIDIOutputCallback) {
            if (this->IsInitialized()) return kAudioUnitErr_CannotDoInCurrentContext;
            midiCallback_ = *static_cast<const AUMIDIOutputCallbackStruct *>(data); return noErr;
        }
        const auto value = *static_cast<const UInt32 *>(data);
        if (value > 1) return kAudioUnitErr_InvalidPropertyValue;
        int32_t result = 1;
        mainCall([&] { result = entry_.set_value(life_->instance, product_.bypass_index, value); });
        if (!result) this->PropertyChanged(id, scope, element);
        return result ? OSStatus(kAudioUnitErr_InvalidPropertyValue) : OSStatus(noErr);
    }
private:
    const RavetPluginEntryV1 &entry_;
    const RavetPluginProductV1 &product_;
    std::vector<AUChannelInfo> channels_;
    std::vector<RavetPluginParameterV1> parameters_;
    std::shared_ptr<Life> life_;
    NSTimer *__strong timer_ = nil;
    ravet::au::NativeAuAudioBusMap map_;
    ravet::au::NativeAuMidiOutput<> midi_;
    AUMIDIOutputCallbackStruct midiCallback_{};
    std::array<RavetNativeEventV1, eventCapacity> events_{}, outputEvents_{};
    uint32_t pending_ = 0, inputChannels_ = 0;
    bool prepared_ = false;
    double sampleRate_ = 44100;
    std::atomic<double> latency_{0}, tail_{0};
    std::atomic<bool> timingChanged_{false};
    static bool indexed(const RavetPluginParameterV1 &p) {
        return p.steps && p.maximum - p.minimum == p.steps && std::trunc(p.minimum) == p.minimum;
    }
    static AudioStreamBasicDescription defaultFormat(uint32_t channels) {
        AudioStreamBasicDescription format{};
        format.mSampleRate = 44100; format.mFormatID = kAudioFormatLinearPCM;
        format.mFormatFlags = UInt32(kAudioFormatFlagsNativeFloatPacked) | UInt32(kAudioFormatFlagIsNonInterleaved);
        format.mChannelsPerFrame = channels; format.mBytesPerPacket = format.mBytesPerFrame = 4;
        format.mFramesPerPacket = 1; format.mBitsPerChannel = 32; return format;
    }
    OSStatus parameterValue(uint32_t id, uint32_t scope, uint32_t element, double value, double &physical) const noexcept {
        const auto index = entry_.parameter_index(id);
        if (scope != kAudioUnitScope_Global || element) return kAudioUnitErr_InvalidScope;
        if (index < 0 || uint32_t(index) >= parameters_.size()) return kAudioUnitErr_InvalidParameter;
        if ((parameters_[index].flags & RAVET_PLUGIN_PARAMETER_READ_ONLY) || !std::isfinite(value)) return kAudioUnitErr_InvalidParameterValue;
        double normalized = 0;
        if (entry_.normalize(index, value, &normalized) || entry_.denormalize(index, normalized, &physical)) return kAudioUnitErr_InvalidParameterValue;
        return noErr;
    }
    static RavetNativeEventV1 parameterEvent(uint32_t id, uint32_t offset, double value) noexcept {
        RavetNativeEventV1 event{}; event.kind = RAVET_NATIVE_PARAMETER_VALUE;
        event.parameter_id = id; event.offset = offset;
        // RavetPlugin.h accepts physical doubles and canonicalizes them before
        // forwarding the narrower native processor scalar contract.
        event.value = value; return event;
    }
    bool readTiming() noexcept {
        RavetNativeProcessorTimingV1 timing{};
        if (!entry_.timing(life_->instance, &timing)) {
            const double latency = timing.latency_samples / sampleRate_;
            const double tail = timing.tail_kind == 2 ? std::numeric_limits<double>::infinity() :
                timing.tail_kind == 1 ? timing.tail_samples / sampleRate_ : 0;
            const bool changed = latency_.load(std::memory_order_relaxed) != latency || tail_.load(std::memory_order_relaxed) != tail;
            latency_.store(latency, std::memory_order_relaxed);
            tail_.store(tail, std::memory_order_relaxed);
            return changed;
        }
        return false;
    }
    void poll() {
        if (life_->editorOpen) entry_.editor_idle(life_->instance);
        life_->edits();
        if (timingChanged_.exchange(false, std::memory_order_acquire)) {
            this->PropertyChanged(kAudioUnitProperty_Latency, kAudioUnitScope_Global, 0);
            this->PropertyChanged(kAudioUnitProperty_TailTime, kAudioUnitScope_Global, 0);
        }
    }
    static void silence(AudioBufferList &buffers, uint32_t frames) noexcept {
        for (UInt32 i = 0; i < buffers.mNumberBuffers; ++i) {
            auto &b = buffers.mBuffers[i];
            if (b.mData && b.mNumberChannels == 1 && uintptr_t(b.mData) % alignof(float) == 0)
                std::fill_n(static_cast<float *>(b.mData), std::min(frames, b.mDataByteSize / UInt32(sizeof(float))), 0.f);
        }
    }
    static bool customProperty(AudioUnitPropertyID id) {
        return id == lifetimeProperty || id == kAudioUnitProperty_CocoaUI || id == kAudioUnitProperty_BypassEffect ||
            id == kAudioUnitProperty_ParameterStringFromValue || id == kAudioUnitProperty_ParameterValueFromString ||
            id == kAudioUnitProperty_MIDIOutputCallback || id == kAudioUnitProperty_MIDIOutputCallbackInfo ||
            id == kAudioUnitProperty_ClassInfoFromDocument;
    }
};
} // namespace ravet::auv2

extern "C" __attribute__((visibility("default"))) void *RavetAUFactory(const AudioComponentDescription *description) {
    try {
        using namespace ravet::auv2;
        const auto &product = *checked(ravet_plugin_v1()).product;
        if (!description || description->componentType != product.au_type || description->componentSubType != product.au_subtype ||
            description->componentManufacturer != product.au_manufacturer) return nullptr;
        if (product.au_type == kAudioUnitType_MusicDevice)
            return ausdk::AUMusicDeviceFactory<PluginAU<InstrumentBase>>::Factory(description);
        if (product.au_type == kAudioUnitType_MusicEffect || product.note_input)
            return ausdk::AUMIDIEffectFactory<PluginAU<MidiBase>>::Factory(description);
        if (product.au_type == kAudioUnitType_Effect)
            return ausdk::AUBaseFactory<PluginAU<EffectBase>>::Factory(description);
        return nullptr;
    } catch (...) { return nullptr; }
}
