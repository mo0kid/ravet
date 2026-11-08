// Generic VST3 export over RavetPlugin.h. The SDK and product stay external.
#ifdef _WIN32
#ifndef NOMINMAX
#define NOMINMAX
#endif
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#endif
#include "RavetPlugin.h"
#include "RavetVst3Audio.hpp"
#include "RavetVst3Transport.hpp"
#include "RavetVst3PerformanceInput.hpp"
#include "RavetVst3PerformanceOutput.hpp"
#include "base/source/fstring.h"
#include "base/source/timer.h"
#include "pluginterfaces/base/ibstream.h"
#include "pluginterfaces/vst/ivstmidicontrollers.h"
#include "pluginterfaces/vst/ivstparameterchanges.h"
#include "public.sdk/source/common/pluginview.h"
#include "public.sdk/source/main/pluginfactory.h"
#include "public.sdk/source/vst/vstaudioeffect.h"
#include "public.sdk/source/vst/vsteditcontroller.h"
#include <algorithm>
#include <array>
#include <atomic>
#include <cmath>
#include <cstring>
#include <functional>
#include <limits>
#include <memory>
#include <string>
#include <vector>
#ifdef RAVET_PRODUCT_LIBRARY_NAME
#ifdef _WIN32
#include <windows.h>
#else
#include <dlfcn.h>
#endif
#endif

using namespace Steinberg;
using namespace Steinberg::Vst;
namespace ravet::plugin::vst3 {
constexpr uint32_t capacity = 1024, stateLimit = 16 * 1024 * 1024;
// Auxiliary buses after the main bus (RavetPluginProductV1::auxiliary_count).
constexpr uint32_t maxAuxiliaries = 8;
// Hidden MIDI IDs use a deterministic free range without restricting product IDs.
constexpr uint32_t midiControllers = 130;
IPtr<FUnknown> factoryContext;

// Host-facing methods that allocate (state buffers, bus and parameter objects,
// messages, views, strings) run through this guard: a C++ exception must never
// unwind into the host, which the SDK does not catch for us. process() does not
// allocate and is not wrapped.
template <class R, class F> R guarded(R failure, F&& body) noexcept {
    try { return body(); } catch (...) { return failure; }
}

const RavetPluginEntryV1* entry() {
#ifdef RAVET_PRODUCT_LIBRARY_NAME
    // Resolve beside this adapter, never against a process-wide symbol or cwd.
    struct Library {
        const RavetPluginEntryV1* table = nullptr;
#ifdef _WIN32
        HMODULE handle = nullptr;
        Library() {
            HMODULE self = nullptr;
            if (!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                reinterpret_cast<LPCWSTR>(&entry), &self)) return;
            std::wstring path(32768, L'\0');
            auto size = GetModuleFileNameW(self, path.data(), DWORD(path.size()));
            if (!size || size >= path.size()) return;
            path.resize(size); path.resize(path.find_last_of(L"\\/") + 1);
            String name(RAVET_PRODUCT_LIBRARY_NAME, kCP_Utf8); name.toWideString();
            path.append(reinterpret_cast<const wchar_t*>(name.text16()));
            handle = LoadLibraryExW(path.c_str(), nullptr, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS);
            if (handle) {
                auto get = reinterpret_cast<const RavetPluginEntryV1*(*)()>(GetProcAddress(handle, "ravet_plugin_v1"));
                if (get) table = get();
            }
        }
        ~Library() { if (handle) FreeLibrary(handle); }
#else
        void* handle = nullptr;
        Library() {
            Dl_info info{};
            if (!dladdr(reinterpret_cast<const void*>(&entry), &info) || !info.dli_fname) return;
            std::string path(info.dli_fname);
            path.resize(path.find_last_of('/') + 1); path += RAVET_PRODUCT_LIBRARY_NAME;
            handle = dlopen(path.c_str(), RTLD_NOW | RTLD_LOCAL);
            if (handle) {
                auto get = reinterpret_cast<const RavetPluginEntryV1*(*)()>(dlsym(handle, "ravet_plugin_v1"));
                if (get) table = get();
            }
        }
        ~Library() { if (handle) dlclose(handle); }
#endif
    };
    static Library library;
    return library.table;
#else
    return ravet_plugin_v1();
#endif
}

bool validTable(const RavetPluginEntryV1* e) {
    if (!e || e->version != 1 || e->byte_size < sizeof(*e) || !e->product) return false;
    const auto& p = *e->product;
    if (!p.name || !p.vendor || !p.version || !p.url || !p.layouts || !p.layout_count ||
        p.parameter_count > capacity || p.bypass_index < -1 ||
        p.bypass_index >= int32_t(p.parameter_count) || p.category > RAVET_PLUGIN_MIDI_EFFECT ||
        p.note_input > 1 || p.note_output > 1 || p.editor_width > 16384 || p.editor_height > 16384) return false;
    for (uint32_t i = 0; i < p.layout_count; ++i) {
        auto l = p.layouts[i];
        if (l.inputs > 64 || l.outputs > 64 || (!l.inputs && !l.outputs)) return false;
    }
    if (p.auxiliary_count > maxAuxiliaries || (p.auxiliary_count && !p.auxiliaries)) return false;
    for (uint32_t i = 0; i < p.auxiliary_count; ++i) {
        auto a = p.auxiliaries[i];
        if (!a.name || !*a.name || a.inputs > 64 || a.outputs > 64 || (!a.inputs && !a.outputs)) return false;
    }
    if (!e->parameter || !e->parameter_index || !e->normalize || !e->denormalize ||
        !e->value_to_text || !e->text_to_value || !e->create || !e->destroy ||
        !e->get_value || !e->set_value || !e->save_state || !e->load_state ||
        !e->prepare || !e->process || !e->reset || !e->deactivate || !e->timing ||
        !e->latency_changed || !e->take_edits || !e->editor_open || !e->editor_resize ||
        !e->editor_close || !e->editor_idle) return false;
    for (uint32_t i = 0; i < p.parameter_count; ++i) {
        RavetPluginParameterV1 m{};
        if (e->parameter(i, &m) || m.id > kMaxParamId || e->parameter_index(m.id) != int32_t(i) ||
            !m.name || !m.unit || m.steps > INT32_MAX || !std::isfinite(m.minimum) ||
            !std::isfinite(m.maximum) || !std::isfinite(m.default_value) || m.minimum > m.maximum ||
            m.default_value < m.minimum || m.default_value > m.maximum) return false;
    }
    return true;
}
ParamID midiStart() {
    static const ParamID start = [] {
        ParamID base = 0x60000000u;
        bool conflict;
        do {
            conflict = false;
            for (uint32_t i = 0; i < entry()->product->parameter_count; ++i) {
                RavetPluginParameterV1 meta{}; entry()->parameter(i, &meta);
                if (meta.id >= base && meta.id - base < 16 * midiControllers) { base = meta.id + 1; conflict = true; }
            }
        } while (conflict);
        return base;
    }();
    return start;
}
FUID cid(const uint32_t* words) { return FUID(words[0], words[1], words[2], words[3]); }
SpeakerArrangement arrangement(uint32_t channels) {
    if (channels == 1) return SpeakerArr::kMono;
    if (channels == 2) return SpeakerArr::kStereo;
    // The table declares counts, not named speakers: discrete channels above 2.
    return channels == 64 ? ~SpeakerArrangement(0) : (SpeakerArrangement(1) << channels) - 1;
}
bool normalized(double value) { return std::isfinite(value) && value >= 0. && value <= 1.; }
bool offsetValid(int32 offset, int32 frames) { return offset >= 0 && (frames ? offset < frames : offset == 0); }
void utf16(const char* source, TChar* dest, int32 size) {
    String s(source, kCP_Utf8); s.toWideString(); s.copyTo16(dest, 0, size - 1); dest[size - 1] = 0;
}
std::string utf8(const TChar* source) {
    String s(source); s.toMultiByte(kCP_Utf8); return s.text8();
}
struct Instance {
    const RavetPluginEntryV1* api = entry();
    void* handle = api->create();
    ~Instance() { if (handle) api->destroy(handle); }
};
uint64 channelMask(uint32_t channels) { return channels == 64 ? ~uint64(0) : (uint64(1) << channels) - 1; }
template <class T> bool aligned(const T* pointer) { return pointer && uintptr_t(pointer) % alignof(T) == 0; }
// Silent inputs and scratch outputs for auxiliary buses the host leaves
// inactive or omits. Sized at prepare (MAIN) so process never allocates. All
// silent input channels share one zero buffer (inputs may alias); every scratch
// output channel is distinct (outputs must be disjoint). Scratch contents are
// discarded.
struct AuxiliaryBuffers {
    std::vector<Sample32> silence, scratch;
    std::array<Sample32*, 64> silent{};
    std::array<std::array<Sample32*, 64>, maxAuxiliaries> outputs{};
    uint32_t frames = 0;
    bool allocate(const RavetPluginProductV1& p, uint32_t maximumFrames) {
        return guarded(false, [&] {
            if (!p.auxiliary_count) return true;
            size_t channels = 0;
            for (uint32_t i = 0; i < p.auxiliary_count; ++i) channels += p.auxiliaries[i].outputs;
            silence.assign(maximumFrames, 0.f); scratch.assign(channels * maximumFrames, 0.f);
            silent.fill(silence.data());
            size_t next = 0;
            for (uint32_t i = 0; i < p.auxiliary_count; ++i)
                for (uint32_t c = 0; c < p.auxiliaries[i].outputs; ++c) outputs[i][c] = scratch.data() + maximumFrames * next++;
            frames = maximumFrames; return true;
        });
    }
};

// Runs on the host UI loop. No worker thread may call editor or host edit APIs.
class MainTimer final : public FObject, public ITimerCallback
#if SMTG_OS_LINUX
    , public Linux::ITimerHandler
#endif
{
public:
    explicit MainTimer(std::function<void()> fn) : callback(std::move(fn)) {}
    ~MainTimer() override { stop(); }
    bool start(FUnknown* context) {
        if (timer || loop) return true;
#if SMTG_OS_LINUX
        FUnknownPtr<Linux::IRunLoop> candidate(context);
        if (!candidate) candidate = FUnknownPtr<Linux::IRunLoop>(factoryContext);
        if (!candidate || candidate->registerTimer(this, 33) != kResultOk) return false;
        loop = candidate; return true;
#else
        (void)context;
        timer = owned(Timer::create(this, 33)); return bool(timer);
#endif
    }
    void stop() {
#if SMTG_OS_LINUX
        if (loop) { loop->unregisterTimer(this); loop = nullptr; }
#endif
        if (timer) { timer->stop(); timer = nullptr; }
    }
    void onTimer(Timer*) override { fire(); }
#if SMTG_OS_LINUX
    void PLUGIN_API onTimer() override { fire(); }
#endif
    OBJ_METHODS(MainTimer, FObject)
    DEFINE_INTERFACES
#if SMTG_OS_LINUX
        DEF_INTERFACE(Linux::ITimerHandler)
#endif
    END_DEFINE_INTERFACES(FObject)
    REFCOUNT_METHODS(FObject)
private:
    // The host's run loop calls this: nothing may unwind out of it.
    void fire() noexcept { IPtr<MainTimer> keepAlive(this); guarded(0, [&] { callback(); return 0; }); }
    std::function<void()> callback;
    IPtr<Timer> timer;
    IPtr<Linux::IRunLoop> loop;
};

bool readState(IBStream* stream, std::vector<uint8_t>& bytes) {
    if (!stream) return false;
    std::array<uint8_t, 4096> buffer{};
    while (true) {
        int32 count = 0;
        auto result = stream->read(buffer.data(), int32(buffer.size()), &count);
        if (count < 0 || count > int32(buffer.size()) || bytes.size() + size_t(count) > stateLimit) return false;
        bytes.insert(bytes.end(), buffer.data(), buffer.data() + count);
        if (!count) return !bytes.empty() && (result == kResultOk || result == kResultFalse);
        if (result != kResultOk && result != kResultFalse) return false;
    }
}
bool save(Instance& instance, IBStream* stream) {
    if (!stream || !instance.handle) return false;
    uint32_t length = 0;
    if (instance.api->save_state(instance.handle, nullptr, 0, &length) || length > stateLimit) return false;
    std::vector<uint8_t> bytes(length);
    if (instance.api->save_state(instance.handle, bytes.data(), length, &length) || length > bytes.size()) return false;
    uint32_t used = 0;
    while (used < length) {
        int32 count = 0;
        auto result = stream->write(bytes.data() + used, int32(length - used), &count);
        if (count <= 0 || uint32_t(count) > length - used || result != kResultOk) return false;
        used += uint32_t(count);
    }
    return true;
}
bool sendValue(ComponentBase& sender, uint32_t index, double value) {
    auto message = owned(sender.allocateMessage());
    if (!message) return false;
    message->setMessageID("RAVET.Value");
    message->getAttributes()->setInt("index", index);
    message->getAttributes()->setFloat("value", value);
    return sender.sendMessage(message) == kResultOk;
}

class Component final : public AudioEffect {
public:
    Component() { setControllerClass(cid(instance.api->product->vst3_controller_cid));
        processContextRequirements.flags = ravet::vst3::nativeTransportRequirements;
        reflected.fill(std::numeric_limits<double>::quiet_NaN()); }
    ~Component() override { if (timer) timer->stop(); }
    static FUnknown* create(void*) {
        return guarded<FUnknown*>(nullptr, [] { return static_cast<IAudioProcessor*>(new Component); });
    }
    tresult PLUGIN_API initialize(FUnknown* context) override {
        return guarded<tresult>(kOutOfMemory, [&] { return initializeComponent(context); });
    }
    tresult initializeComponent(FUnknown* context) {
        auto result = AudioEffect::initialize(context);
        if (result != kResultOk) return result;
        if (!instance.handle) return kOutOfMemory;
        const auto& p = *instance.api->product; layout = p.layouts[0];
        reflected.fill(std::numeric_limits<double>::quiet_NaN());
        if (layout.inputs) addAudioInput(STR16("Input"), arrangement(layout.inputs));
        if (layout.outputs) addAudioOutput(STR16("Output"), arrangement(layout.outputs));
        addAuxiliaryBuses();
        if (p.note_input) addEventInput(STR16("MIDI In"), 16);
        if (p.note_output) addEventOutput(STR16("MIDI Out"), 16);
        timer = owned(new MainTimer([this] { idle(); })); timer->start(context);
        return kResultOk;
    }
    tresult PLUGIN_API terminate() override {
        if (timer) { timer->stop(); timer = nullptr; }
        return AudioEffect::terminate();
    }
    tresult PLUGIN_API canProcessSampleSize(int32 size) override { return size == kSample32 ? kResultTrue : kResultFalse; }
    tresult PLUGIN_API setBusArrangements(SpeakerArrangement* inputs, int32 ni, SpeakerArrangement* outputs, int32 no) override {
        return guarded<tresult>(kResultFalse, [&] { return arrangeBuses(inputs, ni, outputs, no); });
    }
    // The main bus takes any product layout; auxiliary buses (after it, in
    // table order) accept only their declared arrangement. The host lists
    // every bus, or only the main bus(es): auxiliary buses then keep their
    // declared arrangement. Complete lists are matched first, so a count that
    // fits both readings for different layouts means the complete list.
    tresult arrangeBuses(SpeakerArrangement* inputs, int32 ni, SpeakerArrangement* outputs, int32 no) {
        if (active || ni < 0 || no < 0 || (ni && !inputs) || (no && !outputs)) return kResultFalse;
        auto& p = *instance.api->product;
        int32 auxInputs = 0, auxOutputs = 0;
        for (uint32_t i = 0; i < auxiliaryCount; ++i) {
            auxInputs += p.auxiliaries[i].inputs ? 1 : 0; auxOutputs += p.auxiliaries[i].outputs ? 1 : 0;
        }
        for (const bool complete : {true, false}) {
            if (!complete && !auxiliaryCount) break; // Without auxiliary buses both readings coincide.
            for (uint32_t i = 0; i < p.layout_count; ++i) {
                auto l = p.layouts[i];
                const int32 mi = l.inputs ? 1 : 0, mo = l.outputs ? 1 : 0;
                if (ni != mi + (complete ? auxInputs : 0) || no != mo + (complete ? auxOutputs : 0) ||
                    (mi && inputs[0] != arrangement(l.inputs)) || (mo && outputs[0] != arrangement(l.outputs))) continue;
                bool declared = true;
                for (uint32_t a = 0, in = uint32_t(mi), out = uint32_t(mo); complete && a < auxiliaryCount; ++a) {
                    const auto& bus = p.auxiliaries[a];
                    if ((bus.inputs && inputs[in++] != arrangement(bus.inputs)) ||
                        (bus.outputs && outputs[out++] != arrangement(bus.outputs))) declared = false;
                }
                if (!declared) continue;
                if (bool(l.inputs) != bool(layout.inputs) || bool(l.outputs) != bool(layout.outputs)) {
                    removeAudioBusses();
                    if (mi) addAudioInput(STR16("Input"), inputs[0]);
                    if (mo) addAudioOutput(STR16("Output"), outputs[0]);
                    addAuxiliaryBuses();
                } else {
                    if (mi) getAudioInput(0)->setArrangement(inputs[0]);
                    if (mo) getAudioOutput(0)->setArrangement(outputs[0]);
                }
                layout = l; return kResultTrue;
            }
        }
        return kResultFalse;
    }
    tresult PLUGIN_API activateBus(MediaType type, BusDirection dir, int32 index, TBool state) override {
        const auto result = AudioEffect::activateBus(type, dir, index, state);
        if (result == kResultTrue && type == kAudio && (dir == kInput || dir == kOutput)) {
            const int32 aux = auxiliaryIndex(dir, index);
            if (aux >= 0) auxiliaryActive[dir][uint32_t(aux)].store(state != 0, std::memory_order_relaxed);
        }
        return result;
    }
    tresult PLUGIN_API setupProcessing(ProcessSetup& setup) override {
        if (active || setup.symbolicSampleSize != kSample32 || setup.maxSamplesPerBlock < 1 ||
            setup.maxSamplesPerBlock > 65536 || !std::isfinite(setup.sampleRate) || setup.sampleRate <= 0.) return kResultFalse;
        // Product preparation is the authority for rates and resource limits.
        // Reject here without replacing the last accepted VST3 setup. The host
        // may reactivate that setup after a rejected negotiation (SDK validator).
        if (!prepareProduct(setup)) return kResultFalse;
        if (instance.api->deactivate(instance.handle)) return kResultFalse;
        return AudioEffect::setupProcessing(setup);
    }
    tresult PLUGIN_API setActive(TBool state) override {
        if (state && !active) {
            if (!prepareProduct(processSetup)) return kResultFalse;
        } else if (!state && active && instance.api->deactivate(instance.handle)) return kResultFalse;
        active = bool(state); return AudioEffect::setActive(state);
    }
    tresult PLUGIN_API setProcessing(TBool state) override { return state && !active ? kResultFalse : kResultOk; }
    uint32 PLUGIN_API getLatencySamples() override { return latency.load(std::memory_order_relaxed); }
    uint32 PLUGIN_API getTailSamples() override { return tail.load(std::memory_order_relaxed); }
    tresult PLUGIN_API getState(IBStream* stream) override {
        return guarded<tresult>(kResultFalse, [&] { return save(instance, stream) ? kResultOk : kResultFalse; });
    }
    tresult PLUGIN_API setState(IBStream* stream) override {
        return guarded<tresult>(kResultFalse, [&] {
            std::vector<uint8_t> bytes;
            if (!readState(stream, bytes) || instance.api->load_state(instance.handle, bytes.data(), uint32_t(bytes.size()))) return kResultFalse;
            reflected.fill(std::numeric_limits<double>::quiet_NaN()); return kResultOk;
        });
    }
    tresult PLUGIN_API notify(IMessage* message) override {
        return guarded<tresult>(kResultFalse, [&] { return receive(message); });
    }
    tresult receive(IMessage* message) {
        if (message && !std::strcmp(message->getMessageID(), "RAVET.Value")) {
            int64 index = -1; double value = 0.;
            auto a = message->getAttributes();
            if (a->getInt("index", index) != kResultOk || a->getFloat("value", value) != kResultOk ||
                index < 0 || index >= instance.api->product->parameter_count ||
                instance.api->set_value(instance.handle, uint32_t(index), value)) return kResultFalse;
            return kResultOk;
        }
        if (message && !std::strcmp(message->getMessageID(), "RAVET.State")) {
            const void* bytes = nullptr; uint32 size = 0;
            if (message->getAttributes()->getBinary("state", bytes, size) != kResultOk || size > stateLimit ||
                instance.api->load_state(instance.handle, static_cast<const uint8_t*>(bytes), size)) return kResultFalse;
            reflected.fill(std::numeric_limits<double>::quiet_NaN()); return kResultOk;
        }
        if (message && !std::strcmp(message->getMessageID(), "RAVET.Poll")) { idle(); return kResultOk; }
        return AudioEffect::notify(message);
    }
    // AUDIO. Not exception-guarded because nothing here allocates or throws:
    // events use fixed member arrays, host counts are bounded by `capacity`
    // before use, and the stable insertion sort is bounded (n <= 1024).
    tresult PLUGIN_API process(ProcessData& data) override {
        // With auxiliary buses, the shared map checks the main bus (SDK bus 0
        // of each direction it uses) and mapAuxiliary handles the rest.
        ProcessData main = data;
        if (auxiliaryCount) {
            main.numInputs = std::min<int32>(data.numInputs, layout.inputs ? 1 : 0);
            main.numOutputs = std::min<int32>(data.numOutputs, layout.outputs ? 1 : 0);
        }
        auto fail = [&] { audio.silenceSupportedOutputs(main); silenceAuxiliaryOutputs(data); return kResultFalse; };
        std::array<RavetNativeProcessorBusV1, 9> buses{}; RavetNativeTransportV1 transport{};
        std::array<int32, maxAuxiliaries> mapped{};
        if (!active || !audio.map(main, buses) || !mapAuxiliary(data, buses, mapped) ||
            !ravet::vst3::nativeTransport(data.processContext, transport)) return fail();
        uint32_t count = 0, inspected = 0;
        auto append = [&](const RavetNativeEventV1& e) {
            if (count == capacity) return false;
            // Stable insertion: parameter queue order, then input-event order at ties.
            uint32_t at = count++;
            while (at && input[at - 1].offset > e.offset) { input[at] = input[at - 1]; --at; }
            input[at] = e; return true;
        };
        if (data.inputParameterChanges) {
            auto queues = data.inputParameterChanges->getParameterCount();
            if (queues < 0 || queues > int32_t(capacity + 16 * midiControllers)) return fail();
            for (int32 q = 0; q < queues; ++q) {
                auto queue = data.inputParameterChanges->getParameterData(q);
                if (!queue) return fail();
                auto points = queue->getPointCount(); auto id = queue->getParameterId();
                if (points < 0 || uint32_t(points) > capacity - inspected) return fail();
                inspected += uint32_t(points); int32 previous = -1;
                for (int32 j = 0; j < points; ++j) {
                    int32 offset = -1; double value = 0.;
                    if (queue->getPoint(j, offset, value) != kResultOk || !offsetValid(offset, data.numSamples) ||
                        offset < previous || !normalized(value)) return fail();
                    previous = offset;
                    RavetNativeEventV1 e{}; e.offset = uint32_t(offset);
                    const int32 index = instance.api->parameter_index(id);
                    if (index >= 0) {
                        RavetPluginParameterV1 meta{};
                        if (instance.api->parameter(uint32_t(index), &meta)) return fail();
                        if (meta.flags & RAVET_PLUGIN_PARAMETER_READ_ONLY) continue;
                        e.kind = RAVET_NATIVE_PARAMETER_VALUE; e.parameter_id = id;
                        if (instance.api->denormalize(uint32_t(index), value, &e.value)) return fail();
                    } else if (instance.api->product->note_input && id >= midiBegin && id - midiBegin < 16 * midiControllers) {
                        uint32_t lane = id - midiBegin, controller = lane % midiControllers, channel = lane / midiControllers;
                        e.kind = RAVET_NATIVE_MIDI;
                        if (controller < 128) { e.detail = 3; e.words[0] = (0xb0 | channel) | (controller << 8) | (uint32_t(std::lround(value * 127.)) << 16); }
                        else if (controller == 128) { e.detail = 2; e.words[0] = (0xd0 | channel) | (uint32_t(std::lround(value * 127.)) << 8); }
                        else { auto bend = uint32_t(std::lround(value * 16383.)); e.detail = 3;
                            e.words[0] = (0xe0 | channel) | ((bend & 127) << 8) | ((bend >> 7) << 16); }
                    } else continue;
                    if (!append(e)) return fail();
                }
            }
        }
        if (data.inputEvents && instance.api->product->note_input) {
            auto rawCount = data.inputEvents->getEventCount();
            if (rawCount < 0 || uint32_t(rawCount) > capacity - inspected) return fail();
            for (int32 i = 0; i < rawCount; ++i) {
                Event e{}; std::array<RavetNativeEventV1, 2> mapped{}; size_t used = 0;
                if (data.inputEvents->getEvent(i, e) != kResultOk ||
                    ravet::vst3::nativePerformanceInput(e, 0, data.numSamples, ravet::vst3::UnknownExpressionPolicy::Ignore, mapped, used)
                        == ravet::vst3::PerformanceInputResult::Rejected) return fail();
                for (size_t j = 0; j < used; ++j) if (!append(mapped[j])) return fail();
            }
        }
        RavetNativeProcessorResultV1 result{};
        if (instance.api->process(instance.handle, buses.data(), 1 + auxiliaryCount, uint32_t(data.numSamples), input.data(), count,
            data.processContext ? &transport : nullptr, output.data(), instance.api->product->note_output ? capacity : 0, &result) || result.status > 1 || result.event_count > capacity) return fail();
        const bool changed = instance.api->latency_changed(instance.handle) != 0;
        refreshTiming();
        if (changed) latencyPending.store(true, std::memory_order_release);
        if (!deliverOutput(data, result.event_count)) return kResultFalse;
        if (layout.outputs && data.numOutputs && data.outputs) data.outputs[0].silenceFlags = result.status == 1
            ? channelMask(layout.outputs) : 0;
        for (uint32_t i = 0; i < auxiliaryCount; ++i) if (mapped[i] >= 0)
            data.outputs[mapped[i]].silenceFlags = result.status == 1 ? channelMask(instance.api->product->auxiliaries[i].outputs) : 0;
        return kResultOk;
    }
private:
    // Declared after the main bus in each direction, in table order, and not
    // default-active: hosts activate them with activateBus. A bus with inputs
    // and outputs appears in both directions under its name. Re-added buses
    // keep the activation the host last set.
    void addAuxiliaryBuses() {
        const auto& p = *instance.api->product;
        for (uint32_t i = 0; i < auxiliaryCount; ++i) {
            const auto& a = p.auxiliaries[i];
            String128 name{}; utf16(a.name, name, 128);
            if (a.inputs) addAudioInput(name, arrangement(a.inputs), kAux, 0)
                ->setActive(auxiliaryActive[kInput][i].load(std::memory_order_relaxed));
            if (a.outputs) addAudioOutput(name, arrangement(a.outputs), kAux, 0)
                ->setActive(auxiliaryActive[kOutput][i].load(std::memory_order_relaxed));
        }
    }
    // Table index of the auxiliary bus at an SDK audio bus index, or -1.
    int32 auxiliaryIndex(BusDirection dir, int32 index) const {
        const auto& p = *instance.api->product;
        int32 sdk = (dir == kInput ? layout.inputs : layout.outputs) ? 1 : 0;
        for (uint32_t i = 0; i < auxiliaryCount; ++i) {
            if (!(dir == kInput ? p.auxiliaries[i].inputs : p.auxiliaries[i].outputs)) continue;
            if (sdk++ == index) return int32(i);
        }
        return -1;
    }
    // AUDIO. The host's channels for an active auxiliary bus, or nullptr when
    // the bus is inactive, omitted (hosts may pass fewer buses) or not exactly
    // shaped (channel count, nonnull aligned pointers); the caller then uses
    // prepared buffers. Zero-frame blocks always use prepared buffers.
    Sample32** hostChannels(const ProcessData& data, BusDirection dir, int32 index, uint32_t aux, uint32_t channels) const {
        const int32 count = dir == kInput ? data.numInputs : data.numOutputs;
        const AudioBusBuffers* list = dir == kInput ? data.inputs : data.outputs;
        if (!auxiliaryActive[dir][aux].load(std::memory_order_relaxed) || data.numSamples <= 0 ||
            uint32_t(data.numSamples) > auxiliary.frames || data.symbolicSampleSize != kSample32 ||
            index >= count || !aligned(list)) return nullptr;
        const auto& bus = list[index];
        if (bus.numChannels != int32(channels) || !aligned(bus.channelBuffers32)) return nullptr;
        const auto bytes = uintptr_t(data.numSamples) * sizeof(Sample32);
        for (uint32_t c = 0; c < channels; ++c) {
            const auto* pointer = bus.channelBuffers32[c];
            if (!aligned(pointer) || uintptr_t(pointer) > std::numeric_limits<uintptr_t>::max() - bytes) return nullptr;
        }
        return bus.channelBuffers32;
    }
    // AUDIO. Fills buses[1..] for every auxiliary bus with the prepared channel
    // counts: host buffers where hostChannels accepts them, otherwise silent
    // inputs and scratch outputs. mapped[i] is the SDK output index whose host
    // buffers bus i writes, or -1. Rejects host output ranges that overlap one
    // another or the main outputs (the table requires disjoint outputs).
    bool mapAuxiliary(const ProcessData& data, std::array<RavetNativeProcessorBusV1, 9>& buses,
                      std::array<int32, maxAuxiliaries>& mapped) const {
        mapped.fill(-1);
        if (!auxiliaryCount) return true;
        struct Range { uintptr_t begin, end; };
        std::array<Range, 64 * (1 + maxAuxiliaries)> ranges; size_t used = 0;
        const auto bytes = uintptr_t(std::max<int32>(data.numSamples, 0)) * sizeof(Sample32);
        auto collect = [&](Sample32* const* channels, uint32_t count) {
            for (uint32_t c = 0; c < count; ++c) ranges[used++] = {uintptr_t(channels[c]), uintptr_t(channels[c]) + bytes};
        };
        if (bytes && layout.outputs && buses[0].outputs) collect(static_cast<Sample32* const*>(buses[0].outputs), layout.outputs);
        const auto& p = *instance.api->product;
        int32 in = layout.inputs ? 1 : 0, out = layout.outputs ? 1 : 0;
        for (uint32_t i = 0; i < auxiliaryCount; ++i) {
            const auto& a = p.auxiliaries[i]; auto& bus = buses[1 + i];
            bus.input_channels = a.inputs; bus.output_channels = a.outputs;
            if (a.inputs) {
                auto host = hostChannels(data, kInput, in++, i, a.inputs);
                bus.inputs = host ? static_cast<const void*>(host) : static_cast<const void*>(auxiliary.silent.data());
            }
            if (a.outputs) {
                const int32 index = out++;
                auto host = hostChannels(data, kOutput, index, i, a.outputs);
                if (host) { mapped[i] = index; collect(host, a.outputs); }
                bus.outputs = host ? static_cast<const void*>(host) : static_cast<const void*>(auxiliary.outputs[i].data());
            }
        }
        std::sort(ranges.begin(), ranges.begin() + used, [](const Range& a, const Range& b) { return a.begin < b.begin; });
        for (size_t i = 1; i < used; ++i) if (ranges[i].begin < ranges[i - 1].end) return false;
        return true;
    }
    // AUDIO, on failure: silence the host outputs of active auxiliary buses.
    void silenceAuxiliaryOutputs(ProcessData& data) const {
        const auto& p = *instance.api->product;
        int32 out = layout.outputs ? 1 : 0;
        for (uint32_t i = 0; i < auxiliaryCount; ++i) {
            const auto& a = p.auxiliaries[i];
            if (!a.outputs) continue;
            const int32 index = out++;
            if (auto host = hostChannels(data, kOutput, index, i, a.outputs)) {
                for (uint32_t c = 0; c < a.outputs; ++c) std::fill_n(host[c], data.numSamples, 0.f);
                data.outputs[index].silenceFlags = channelMask(a.outputs);
            }
        }
    }
    bool prepareProduct(const ProcessSetup& setup) {
        if (setup.symbolicSampleSize != kSample32 || setup.maxSamplesPerBlock < 1 || setup.maxSamplesPerBlock > 65536) return false;
        RavetNativeProcessorConfigV1 config{}; config.version = 1;
        config.maximum_frames = uint32_t(setup.maxSamplesPerBlock);
        config.maximum_events = capacity; config.maximum_output_events = instance.api->product->note_output ? capacity : 0;
        config.bus_count = 1 + auxiliaryCount; config.sample_rate = setup.sampleRate;
        config.buses[0] = {layout.inputs, layout.outputs};
        const auto& p = *instance.api->product;
        for (uint32_t i = 0; i < auxiliaryCount; ++i) config.buses[1 + i] = {p.auxiliaries[i].inputs, p.auxiliaries[i].outputs};
        // The shared map covers the main bus; auxiliary buses use mapAuxiliary.
        auto main = config; main.bus_count = 1;
        for (uint32_t i = 1; i < 9; ++i) main.buses[i] = {};
        ravet::vst3::NativeAudioBusMap candidate; AuxiliaryBuffers buffers;
        if (!candidate.configure(main, 32) || !buffers.allocate(p, config.maximum_frames) ||
            instance.api->prepare(instance.handle, &config)) return false;
        audio = candidate; auxiliary = std::move(buffers); refreshTiming(); return true;
    }
    void refreshTiming() {
        RavetNativeProcessorTimingV1 timing{};
        if (!instance.api->timing(instance.handle, &timing)) {
            latency.store(timing.latency_samples, std::memory_order_relaxed);
            tail.store(timing.tail_kind == 2 ? kInfiniteTail : timing.tail_kind == 1
                ? uint32_t(std::min<uint64_t>(timing.tail_samples, kInfiniteTail - 1)) : 0, std::memory_order_relaxed);
        }
    }
    bool deliverOutput(ProcessData& data, uint32_t count) {
        if (count && (!instance.api->product->note_output || !data.outputEvents)) return false;
        uint32_t previous = 0;
        for (uint32_t i = 0; i < count; ++i) {
            auto& e = output[i]; auto& mapped = hostOutput[i]; mapped = {};
            if (e.offset < previous || !offsetValid(int32_t(e.offset), data.numSamples)) return false;
            previous = e.offset;
            if (e.kind != RAVET_NATIVE_MIDI) {
                if (!ravet::vst3::nativePerformanceOutput(e, 0, data.numSamples, mapped)) return false;
                continue;
            }
            const auto word = e.words[0], status = word & 0xf0, channel = word & 15, a = (word >> 8) & 255, b = (word >> 16) & 255;
            if (e.parameter_id || e.note_id || e.scope || e.channel || e.key || e.value != 0. || e.words[1] || e.words[2] ||
                e.words[3] || word >> 24 || a > 127 || b > 127 || (e.detail != 2 && e.detail != 3)) return false;
            mapped.sampleOffset = int32_t(e.offset); mapped.busIndex = 0;
            if ((status == 0x90 || status == 0x80) && e.detail == 3) {
                mapped.type = status == 0x90 && b ? Event::kNoteOnEvent : Event::kNoteOffEvent;
                if (mapped.type == Event::kNoteOnEvent) mapped.noteOn = {int16(channel), int16(a), 0.f, float(b) / 127.f, 0, -1};
                else mapped.noteOff = {int16(channel), int16(a), float(b) / 127.f, -1, 0.f};
            } else {
                mapped.type = Event::kLegacyMIDICCOutEvent;
                auto& cc = mapped.midiCCOut; cc.channel = int8(channel);
                if (status == 0xb0 && e.detail == 3) { cc.controlNumber = uint8(a); cc.value = int8(b); }
                else if (status == 0xd0 && e.detail == 2 && !b) { cc.controlNumber = kAfterTouch; cc.value = int8(a); }
                else if (status == 0xe0 && e.detail == 3) { cc.controlNumber = kPitchBend; cc.value = int8(a); cc.value2 = int8(b); }
                else if (status == 0xa0 && e.detail == 3) { cc.controlNumber = kCtrlPolyPressure; cc.value = int8(a); cc.value2 = int8(b); }
                else if (status == 0xc0 && e.detail == 2 && !b) { cc.controlNumber = kCtrlProgramChange; cc.value = int8(a); }
                else return false;
            }
        }
        for (uint32_t i = 0; i < count; ++i) if (data.outputEvents->addEvent(hostOutput[i]) != kResultOk) return false;
        return true;
    }
    void idle() {
        IPtr<Component> keepAlive(this);
        if (!getPeer()) return;
        if (latencyPending.exchange(false, std::memory_order_acq_rel)) sendMessageID("RAVET.Latency");
        for (uint32_t i = 0; i < instance.api->product->parameter_count; ++i) {
            auto value = instance.api->get_value(instance.handle, i);
            if (value != reflected[i] && sendValue(*this, i, value)) reflected[i] = value;
        }
    }
    Instance instance;
    const uint32_t auxiliaryCount = instance.api->product->auxiliary_count;
    const ParamID midiBegin = midiStart(); // Constructed on MAIN, never lazily on AUDIO.
    RavetPluginLayoutV1 layout{};
    ravet::vst3::NativeAudioBusMap audio;
    AuxiliaryBuffers auxiliary;
    // Host activation per direction (kInput, kOutput) and table index. Written
    // on MAIN by activateBus, read on AUDIO.
    std::array<std::array<std::atomic<bool>, maxAuxiliaries>, 2> auxiliaryActive{};
    bool active = false;
    IPtr<MainTimer> timer;
    std::atomic<uint32_t> latency{0}, tail{0};
    std::atomic<bool> latencyPending{false};
    std::array<double, capacity> reflected{};
    std::array<RavetNativeEventV1, capacity> input{}, output{};
    std::array<Event, capacity> hostOutput{};
};

class TableParameter final : public Parameter {
public:
    TableParameter(uint32_t index, const ParameterInfo& info) : Parameter(info), index(index) {}
    ParamValue toPlain(ParamValue value) const override {
        double physical = 0.; entry()->denormalize(index, value, &physical); return physical;
    }
    ParamValue toNormalized(ParamValue value) const override {
        double normalized = 0.; entry()->normalize(index, value, &normalized); return normalized;
    }
    void toString(ParamValue value, String128 text) const override {
        char buffer[512]{};
        if (entry()->value_to_text(index, toPlain(value), buffer, sizeof(buffer)) ||
            !guarded(false, [&] { utf16(buffer, text, 128); return true; })) text[0] = 0;
    }
    bool fromString(const TChar* text, ParamValue& value) const override {
        return guarded(false, [&] {
            double physical = 0.;
            if (!text || entry()->text_to_value(index, utf8(text).c_str(), &physical)) return false;
            return entry()->normalize(index, physical, &value) == 0;
        });
    }
private:
    uint32_t index;
};
struct EditorLife {
    Instance instance;
    bool alive = true, open = false;
    ~EditorLife() { close(); }
    void close() { if (open) { instance.api->editor_close(instance.handle); open = false; } }
};
class Controller;
class View final : public CPluginView {
public:
    explicit View(const std::shared_ptr<EditorLife>& life, std::function<void(FUnknown*)> timer)
        : life(life), startTimer(std::move(timer)) {
        const auto& p = *life->instance.api->product;
        rect = ViewRect(0, 0, int32(p.editor_width), int32(p.editor_height));
    }
    ~View() override { removed(); }
    tresult PLUGIN_API isPlatformTypeSupported(FIDString type) override {
        return type && !std::strcmp(type, platform()) ? kResultTrue : kResultFalse;
    }
    tresult PLUGIN_API attached(void* parent, FIDString type) override {
        auto l = life.lock();
        if (!l || !l->alive || l->open || !parent || isPlatformTypeSupported(type) != kResultTrue) return kResultFalse;
        if (l->instance.api->editor_open(l->instance.handle, viewKind(), parent, uint32_t(rect.getWidth()), uint32_t(rect.getHeight()))) return kResultFalse;
        l->open = true; attachedHere = true; systemWindow = parent;
        return kResultTrue;
    }
    tresult PLUGIN_API removed() override {
        if (attachedHere) { if (auto l = life.lock()) l->close(); attachedHere = false; }
        systemWindow = nullptr; return kResultTrue;
    }
    tresult PLUGIN_API setFrame(IPlugFrame* frame) override {
        auto l = life.lock();
        if (l && l->alive && frame) startTimer(frame);
        return CPluginView::setFrame(frame);
    }
    tresult PLUGIN_API onSize(ViewRect* size) override {
        auto l = life.lock();
        if (!l || !l->alive || !validSize(size)) return kResultFalse;
        if (attachedHere && l->instance.api->editor_resize(l->instance.handle, uint32_t(size->getWidth()), uint32_t(size->getHeight()))) return kResultFalse;
        rect = *size; return kResultTrue;
    }
    tresult PLUGIN_API canResize() override {
        auto l = life.lock(); return l && l->alive && l->instance.api->product->editor_resizable ? kResultTrue : kResultFalse;
    }
    tresult PLUGIN_API checkSizeConstraint(ViewRect* size) override {
        auto l = life.lock();
        if (!l || !l->alive || !size) return kResultFalse;
        const auto& p = *l->instance.api->product;
        const int64_t width = p.editor_resizable ? std::clamp<int64_t>(int64_t(size->right) - size->left,
            p.editor_width, std::min<uint32_t>(4 * p.editor_width, 16384)) : p.editor_width;
        const int64_t height = p.editor_resizable ? std::clamp<int64_t>(int64_t(size->bottom) - size->top,
            p.editor_height, std::min<uint32_t>(4 * p.editor_height, 16384)) : p.editor_height;
        if (int64_t(size->left) + width > INT32_MAX || int64_t(size->top) + height > INT32_MAX) return kResultFalse;
        size->right = int32_t(int64_t(size->left) + width);
        size->bottom = int32_t(int64_t(size->top) + height);
        return validSize(size) ? kResultTrue : kResultFalse;
    }
private:
    bool validSize(const ViewRect* size) const {
        if (!size) return false;
        const int64_t width = int64_t(size->right) - size->left, height = int64_t(size->bottom) - size->top;
        if (width < 1 || height < 1 || width > 16384 || height > 16384) return false;
        auto l = life.lock(); if (!l) return false;
        auto& p = *l->instance.api->product;
        return p.editor_resizable ? width >= p.editor_width && width <= 4 * p.editor_width &&
            height >= p.editor_height && height <= 4 * p.editor_height :
            width == p.editor_width && height == p.editor_height;
    }
    static const char* platform() {
#if SMTG_OS_MACOS
        return kPlatformTypeNSView;
#elif SMTG_OS_WINDOWS
        return kPlatformTypeHWND;
#else
        return kPlatformTypeX11EmbedWindowID;
#endif
    }
    static uint32_t viewKind() {
#if SMTG_OS_MACOS
        return RAVET_PLUGIN_VIEW_COCOA;
#elif SMTG_OS_WINDOWS
        return RAVET_PLUGIN_VIEW_WIN32;
#else
        return RAVET_PLUGIN_VIEW_X11;
#endif
    }
    std::weak_ptr<EditorLife> life;
    std::function<void(FUnknown*)> startTimer;
    bool attachedHere = false;
};

class Controller final : public EditController, public IMidiMapping {
public:
    ~Controller() override { close(); }
    static FUnknown* create(void*) {
        return guarded<FUnknown*>(nullptr, [] { return static_cast<IEditController*>(new Controller); });
    }
    tresult PLUGIN_API initialize(FUnknown* context) override {
        return guarded<tresult>(kOutOfMemory, [&] { return initializeController(context); });
    }
    tresult initializeController(FUnknown* context) {
        auto result = EditController::initialize(context);
        if (result != kResultOk) return result;
        if (!life->alive) { life = std::make_shared<EditorLife>(); pending.fill(false); pendingState.clear(); }
        auto& instance = life->instance;
        if (!instance.handle) return kOutOfMemory;
        const auto& p = *instance.api->product;
        for (uint32_t i = 0; i < p.parameter_count; ++i) {
            RavetPluginParameterV1 meta{}; instance.api->parameter(i, &meta);
            ParameterInfo info{}; info.id = meta.id; info.unitId = kRootUnitId;
            utf16(meta.name, info.title, 128); utf16(meta.name, info.shortTitle, 128); utf16(meta.unit, info.units, 128);
            info.stepCount = int32(meta.steps);
            instance.api->normalize(i, meta.default_value, &info.defaultNormalizedValue);
            if (meta.flags & RAVET_PLUGIN_PARAMETER_AUTOMATABLE) info.flags |= ParameterInfo::kCanAutomate;
            if (meta.flags & RAVET_PLUGIN_PARAMETER_READ_ONLY) info.flags = (info.flags & ~ParameterInfo::kCanAutomate) | ParameterInfo::kIsReadOnly;
            if (meta.flags & RAVET_PLUGIN_PARAMETER_HIDDEN) info.flags |= ParameterInfo::kIsHidden;
            if (int32_t(i) == p.bypass_index) info.flags |= ParameterInfo::kIsBypass;
            parameters.addParameter(new TableParameter(i, info));
        }
        if (p.note_input) {
            for (uint32_t channel = 0; channel < 16; ++channel) for (uint32_t control = 0; control < midiControllers; ++control) {
                ParameterInfo info{}; info.id = midiStart() + channel * midiControllers + control;
                info.unitId = kRootUnitId; info.flags = ParameterInfo::kCanAutomate | ParameterInfo::kIsHidden;
                info.stepCount = control == 129 ? 16383 : 127;
                info.defaultNormalizedValue = control == 129 ? 8192. / 16383. : 0.;
                auto title = std::string("MIDI ") + std::to_string(channel + 1) + " / " + std::to_string(control);
                utf16(title.c_str(), info.title, 128); utf16(title.c_str(), info.shortTitle, 128);
                parameters.addParameter(info);
            }
        }
        syncParameters();
        timer = owned(new MainTimer([this] { idle(); })); timer->start(context);
        return kResultOk;
    }
    tresult PLUGIN_API terminate() override { close(); return EditController::terminate(); }
    tresult PLUGIN_API setParamNormalized(ParamID id, ParamValue value) override {
        return guarded<tresult>(kResultFalse, [&] { return setNormalized(id, value); });
    }
    tresult setNormalized(ParamID id, ParamValue value) {
        if (!life->alive || !normalized(value)) return kResultFalse;
        auto& instance = life->instance;
        auto index = instance.api->parameter_index(id);
        if (index < 0) return EditController::setParamNormalized(id, value);
        double physical = 0.;
        if (instance.api->denormalize(uint32_t(index), value, &physical) ||
            instance.api->set_value(instance.handle, uint32_t(index), physical)) return kResultFalse;
        pending[uint32_t(index)] = true;
        sendPending();
        // Re-read the snapped/clamped physical value from the table.
        instance.api->normalize(uint32_t(index), instance.api->get_value(instance.handle, uint32_t(index)), &value);
        return EditController::setParamNormalized(id, value);
    }
    tresult PLUGIN_API getState(IBStream* stream) override {
        return guarded<tresult>(kResultFalse, [&] { return save(life->instance, stream) ? kResultOk : kResultFalse; });
    }
    tresult PLUGIN_API setComponentState(IBStream* stream) override { return guarded<tresult>(kResultFalse, [&] { return load(stream, false); }); }
    tresult PLUGIN_API setState(IBStream* stream) override { return guarded<tresult>(kResultFalse, [&] { return load(stream, true); }); }
    tresult PLUGIN_API connect(IConnectionPoint* other) override {
        return guarded<tresult>(kResultFalse, [&] {
            auto result = EditController::connect(other);
            if (result == kResultOk) sendPending(); return result;
        });
    }
    tresult PLUGIN_API notify(IMessage* message) override {
        return guarded<tresult>(kResultFalse, [&] { return receive(message); });
    }
    tresult receive(IMessage* message) {
        if (!message || !life->alive) return kResultFalse;
        if (!std::strcmp(message->getMessageID(), "RAVET.Latency")) {
            return componentHandler ? componentHandler->restartComponent(kLatencyChanged) : kResultFalse;
        }
        if (!std::strcmp(message->getMessageID(), "RAVET.Value")) {
            int64 index = -1; double physical = 0.; auto a = message->getAttributes();
            auto& instance = life->instance;
            if (a->getInt("index", index) != kResultOk || a->getFloat("value", physical) != kResultOk || index < 0 ||
                index >= instance.api->product->parameter_count || instance.api->set_value(instance.handle, uint32_t(index), physical)) return kResultFalse;
            RavetPluginParameterV1 meta{}; double value = 0.; instance.api->parameter(uint32_t(index), &meta);
            instance.api->normalize(uint32_t(index), physical, &value);
            EditController::setParamNormalized(meta.id, value); return kResultOk;
        }
        return EditController::notify(message);
    }
    tresult PLUGIN_API getMidiControllerAssignment(int32 bus, int16 channel, CtrlNumber controller, ParamID& id) override {
        if (!life->instance.api->product->note_input || bus != 0 || channel < 0 || channel > 15 || controller < 0 || controller > kPitchBend) return kResultFalse;
        id = midiStart() + uint32_t(channel) * midiControllers + uint32_t(controller); return kResultTrue;
    }
    IPlugView* PLUGIN_API createView(FIDString name) override {
        return guarded<IPlugView*>(nullptr, [&] { return makeView(name); });
    }
    IPlugView* makeView(FIDString name) {
        auto p = life->instance.api->product;
        if (!life->alive || !name || std::strcmp(name, ViewType::kEditor) || !p->editor_width || !p->editor_height) return nullptr;
        std::weak_ptr<EditorLife> weak = life;
        return new View(life, [this, weak](FUnknown* context) { auto l = weak.lock(); if (l && l->alive && timer) timer->start(context); });
    }
    OBJ_METHODS(Controller, EditController)
    DEFINE_INTERFACES
        DEF_INTERFACE(IMidiMapping)
    END_DEFINE_INTERFACES(EditController)
    REFCOUNT_METHODS(EditController)
    // Public to the independent in-process fixture, never an exported SDK API.
    void idle() {
        IPtr<Controller> keepAlive(this);
        if (!life->alive) return;
        if (getPeer()) sendMessageID("RAVET.Poll");
        auto& instance = life->instance;
        if (life->open) instance.api->editor_idle(instance.handle);
        std::array<RavetPluginEditV1, 256> edits{};
        auto count = instance.api->take_edits(instance.handle, edits.data(), uint32_t(edits.size()));
        if (count > edits.size()) return;
        for (uint32_t i = 0; i < count; ++i) {
            if (!life->alive) return;
            const auto& edit = edits[i];
            if (edit.index >= instance.api->product->parameter_count) continue;
            RavetPluginParameterV1 meta{}; instance.api->parameter(edit.index, &meta);
            if (edit.kind == RAVET_PLUGIN_EDIT_BEGIN) beginEdit(meta.id);
            else if (edit.kind == RAVET_PLUGIN_EDIT_END) endEdit(meta.id);
            else if (edit.kind == RAVET_PLUGIN_EDIT_VALUE) {
                double value = 0.;
                if (!instance.api->normalize(edit.index, edit.value, &value)) {
                    if (setParamNormalized(meta.id, value) == kResultOk && life->alive) performEdit(meta.id, value);
                }
            }
        }
        if (life->alive) sendPending();
    }
private:
    void close() {
        if (timer) { timer->stop(); timer = nullptr; }
        life->alive = false; life->close();
    }
    void syncParameters() {
        auto& instance = life->instance;
        for (uint32_t i = 0; i < instance.api->product->parameter_count; ++i) {
            RavetPluginParameterV1 meta{}; double value = 0.; instance.api->parameter(i, &meta);
            instance.api->normalize(i, instance.api->get_value(instance.handle, i), &value);
            EditController::setParamNormalized(meta.id, value);
        }
    }
    tresult load(IBStream* stream, bool send) {
        if (!life->alive) return kResultFalse;
        std::vector<uint8_t> bytes; auto& instance = life->instance;
        if (!readState(stream, bytes) || instance.api->load_state(instance.handle, bytes.data(), uint32_t(bytes.size()))) return kResultFalse;
        syncParameters();
        pending.fill(false); pendingState.clear();
        if (send) { pendingState = std::move(bytes); sendPending(); }
        return kResultOk;
    }
    void sendPending() {
        if (!getPeer()) return;
        if (!pendingState.empty()) {
            auto message = owned(allocateMessage());
            if (!message) return;
            message->setMessageID("RAVET.State");
            message->getAttributes()->setBinary("state", pendingState.data(), uint32_t(pendingState.size()));
            if (sendMessage(message) != kResultOk) return;
            pendingState.clear();
        }
        for (uint32_t i = 0; i < life->instance.api->product->parameter_count; ++i) if (pending[i]) {
            if (sendValue(*this, i, life->instance.api->get_value(life->instance.handle, i))) pending[i] = false;
        }
    }
    std::shared_ptr<EditorLife> life = std::make_shared<EditorLife>();
    IPtr<MainTimer> timer;
    std::array<bool, capacity> pending{};
    std::vector<uint8_t> pendingState;
};
} // namespace ravet::plugin::vst3

#ifndef RAVET_VST3_CONTRACT_TEST
namespace ravet::plugin::vst3 {
IPluginFactory* createFactory();
}
SMTG_EXPORT_SYMBOL IPluginFactory* PLUGIN_API GetPluginFactory() {
    // Loading the product library and building the class records allocate.
    return ravet::plugin::vst3::guarded<IPluginFactory*>(nullptr, ravet::plugin::vst3::createFactory);
}
IPluginFactory* ravet::plugin::vst3::createFactory() {
    using namespace ravet::plugin::vst3;
    if (!gPluginFactory) {
        auto e = entry(); if (!validTable(e)) return nullptr;
        const auto& p = *e->product;
        PFactoryInfo info(p.vendor, p.url, "", PFactoryInfo::kUnicode);
        // Publish only a complete factory: a failure part-way releases it.
        auto factory = owned(new CPluginFactory(info));
        factory->addHostContextCallback([](FUnknown* context) { factoryContext = context; });
        bool mono = false, stereo = false, surround = false;
        for (uint32_t i = 0; i < p.layout_count; ++i) {
            auto channels = std::max(p.layouts[i].inputs, p.layouts[i].outputs);
            mono |= channels == 1; stereo |= channels == 2; surround |= channels > 2;
        }
        std::string category = p.category == RAVET_PLUGIN_INSTRUMENT ? "Instrument|Synth" : "Fx";
        if (p.category == RAVET_PLUGIN_MIDI_EFFECT) category += "|Tools";
        if (mono) category += "|Mono";
        if (stereo) category += "|Stereo";
        if (surround) category += "|Surround";
        auto processor = cid(p.vst3_processor_cid), controller = cid(p.vst3_controller_cid);
        PClassInfo2 componentClass(processor, PClassInfo::kManyInstances, kVstAudioEffectClass, p.name,
            kDistributable, category.c_str(), p.vendor, p.version, kVstVersionString);
        PClassInfo2 controllerClass(controller, PClassInfo::kManyInstances, kVstComponentControllerClass,
            p.name, 0, "", p.vendor, p.version, kVstVersionString);
        factory->registerClass(&componentClass, ravet::plugin::vst3::Component::create);
        factory->registerClass(&controllerClass, Controller::create);
        gPluginFactory = factory.take();
    } else gPluginFactory->addRef();
    return gPluginFactory;
}
bool InitModule() { return true; }
bool DeinitModule() { ravet::plugin::vst3::factoryContext = nullptr; return true; }
#endif
