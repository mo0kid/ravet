// Loads a built RAVET .aaxplugin and exercises every registered effect through
// the public AAX SDK interfaces with a synthetic host: exports and SDK version,
// descriptors, Initialize, parameters and text, processing, MIDI nodes, the state
// chunk round trip, and teardown. Works for any product built with the generic
// adapter. It is NOT Avid's validator (DSH/aaxval) and NOT a Pro Tools run.
//
//   ravet_aax_check <path/to/Name.aaxplugin> [--editor]
//
// --editor also opens each effect's editor through its AAX_IEffectGUI in a real
// parent (an off-screen NSView on macOS, a visible top-level HWND on Windows),
// runs the GUI timer, and closes it. RAVET_AAX_EDITOR_HOLD=<seconds> keeps it
// open that long first (for a screenshot).
#include "FakeHost.hpp"
#include "AAX_CString.h"
#include "AAX_IEffectGUI.h"
#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <cmath>
#include <cstring>
#include <filesystem>
#include <iostream>
#if defined(_WIN32)
#include <windows.h>
#else
#include <dlfcn.h>
#endif

#if defined(__APPLE__)
extern "C" void *ravet_check_make_view(double width, double height);
extern "C" unsigned ravet_check_subviews(void *view);
extern "C" void ravet_check_pump(double seconds);
#elif defined(_WIN32)
// A visible top-level window (WebView2 needs one), the message pump, and the
// number of descendant windows (the editor's, then WebView2's).
static void *ravet_check_make_view(double width, double height) {
    RECT rect{0, 0, LONG(width), LONG(height)};
    AdjustWindowRect(&rect, WS_OVERLAPPEDWINDOW, FALSE);
    HWND window = CreateWindowExW(0, L"STATIC", L"RAVET AAX editor check", WS_OVERLAPPEDWINDOW | WS_VISIBLE, 40, 40,
                                  rect.right - rect.left, rect.bottom - rect.top, nullptr, nullptr,
                                  GetModuleHandleW(nullptr), nullptr);
    return window;
}
static unsigned ravet_check_subviews(void *view) {
    unsigned count = 0;
    EnumChildWindows(static_cast<HWND>(view), [](HWND, LPARAM data) { ++*reinterpret_cast<unsigned *>(data); return TRUE; },
                     reinterpret_cast<LPARAM>(&count));
    return count;
}
static void ravet_check_pump(double seconds) {
    const ULONGLONG end = GetTickCount64() + ULONGLONG(seconds * 1000);
    do {
        MSG message;
        while (PeekMessageW(&message, nullptr, 0, 0, PM_REMOVE)) { TranslateMessage(&message); DispatchMessageW(&message); }
        Sleep(5);
    } while (GetTickCount64() < end);
}
#endif

using fake::require;
bool openEditors = false;
namespace fs = std::filesystem;

namespace {

struct Module {
#if defined(_WIN32)
    HMODULE handle = nullptr;
    explicit Module(const fs::path &path) : handle(LoadLibraryW(path.c_str())) { require(handle != nullptr, "LoadLibrary failed"); }
    void *symbol(const char *name) const { return reinterpret_cast<void *>(GetProcAddress(handle, name)); }
#else
    void *handle = nullptr;
    explicit Module(const fs::path &path) : handle(dlopen(path.c_str(), RTLD_NOW | RTLD_LOCAL)) {
        if (!handle) throw std::runtime_error(dlerror());
    }
    void *symbol(const char *name) const { return dlsym(handle, name); }
#endif
};

fs::path binary(const fs::path &bundle) {
#if defined(_WIN32)
    return bundle / "Contents" / "x64" / bundle.filename();
#else
    for (const auto &item : fs::directory_iterator(bundle / "Contents" / "MacOS")) return item.path();
    throw std::runtime_error("bundle has no Contents/MacOS binary");
#endif
}

std::string fourcc(uint32_t v) {
    char hex[16];
    std::snprintf(hex, sizeof(hex), "0x%08x '", v);
    std::string s = hex;
    for (int shift = 24; shift >= 0; shift -= 8) {
        const char c = char((v >> shift) & 0xff);
        s += (c >= 32 && c < 127) ? c : '?';
    }
    return s + "'";
}

void exercise(const std::string &id, fake::Descriptor &descriptor) {
    auto &component = descriptor.component;
    require(descriptor.componentAdded && component.process && descriptor.createParameters, id + ": incomplete descriptor");
    const auto &p = *component.properties;
    const auto input = fake::stem(p.at(AAX_eProperty_InputStemFormat)), output = fake::stem(p.at(AAX_eProperty_OutputStemFormat));
    const uint32_t inputs = input == AAX_eStemFormat_None ? 0 : AAX_STEM_FORMAT_CHANNEL_COUNT(input);
    const uint32_t outputs = AAX_STEM_FORMAT_CHANNEL_COUNT(output);
    require(component.privateField == 0 && component.privateSize == int32_t(sizeof(void *)) && component.frames == 1 && component.audioOut == 2,
            id + ": context fields");
    require((component.audioIn >= 0) == (inputs > 0), id + ": audio input field matches stem");
    std::cout << "effect " << id << ": \"" << descriptor.name << "\" " << inputs << "x" << outputs
              << " manufacturer " << fourcc(uint32_t(p.at(AAX_eProperty_ManufacturerID)))
              << " product " << fourcc(uint32_t(p.at(AAX_eProperty_ProductID)))
              << " type " << fourcc(uint32_t(p.at(AAX_eProperty_PlugInID_Native)))
              << " category 0x" << std::hex << descriptor.category << std::dec
              << " midi-nodes " << component.midi.size() << " editor " << (descriptor.createGui ? "yes" : "no") << "\n";

    TransportControllerHost host;
    host.effect = id;
    host.input = input;
    host.output = output;
    std::unique_ptr<AAX_IEffectParameters> model(descriptor.newParameters());
    require(bool(model), id + ": factory");
    require(model->Initialize(host.unknown()) == AAX_SUCCESS, id + ": Initialize");
    int32_t count = 0;
    require(model->GetNumberOfParameters(&count) == AAX_SUCCESS, id + ": parameter count");
    AAX_CString bypass;
    model->GetMasterBypassParameter(&bypass);
    std::cout << "  parameters " << count << ", master bypass \"" << bypass.Get() << "\", registered with host " << host.registered.size() << "\n";
    for (int32_t i = 0; i < count; ++i) {
        AAX_CString pid, name, text;
        require(model->GetParameterIDFromIndex(i, &pid) == AAX_SUCCESS, id + ": parameter ID");
        double normalized = -1, back = -1;
        int32_t steps = 0;
        require(model->GetParameterNormalizedValue(pid.Get(), &normalized) == AAX_SUCCESS && normalized >= 0 && normalized <= 1, id + ": value");
        model->GetParameterName(pid.Get(), &name);
        model->GetParameterNumberOfSteps(pid.Get(), &steps);
        require(model->GetParameterStringFromValue(pid.Get(), normalized, &text, 32) == AAX_SUCCESS, id + ": text for " + pid.Get());
        if (model->GetParameterValueFromString(pid.Get(), &back, text) == AAX_SUCCESS)
            require(std::fabs(back - normalized) < 1e-3, id + ": text round trip for " + pid.Get() + " (" + text.Get() + ")");
        std::cout << "  " << pid.Get() << " \"" << name.Get() << "\" = \"" << text.Get() << "\" steps " << steps << "\n";
    }

    const bool synth = descriptor.name == "RAVET Evidence Synth";
    if (synth) require(component.midi.size()==2 && inputs==0 && outputs==2,"synth MIDI nodes/layout");
    fake::Context context(component, inputs, outputs);
    require(model->ResetFieldData(0, &context.owner, sizeof(void *)) == AAX_SUCCESS && context.owner, id + ": private owner");
    for (int block = 0; block < 8; ++block) {
        for (auto &channel : context.inputs)
            for (size_t f = 0; f < channel.size(); ++f) channel[f] = 0.25f * float(std::sin(0.01 * double(f + block * 512)));
        if (!context.midiIn.in.empty() || component.fieldIndex(AAX_eMIDINodeType_LocalInput) >= 0) {
            context.midiIn.in.clear();
            AAX_CMidiPacket on{};
            on.mTimestamp = 3; on.mLength = 3; on.mData[0] = 0x90; on.mData[1] = 60; on.mData[2] = 100;
            if (synth) {
                auto cc=on; cc.mTimestamp=0; cc.mData[0]=0xb0; cc.mData[1]=1; cc.mData[2]=64;
                context.midiIn.in.push_back(cc);
            }
            context.midiIn.in.push_back(on);
            if (synth) {
                auto off=on; off.mTimestamp=17; off.mData[0]=0x80; off.mData[2]=0;
                context.midiIn.in.push_back(off);
            }
        }
        context.run(!synth && block == 3 ? 64 : 512);
        for (auto &channel : context.outputs)
            for (int32_t f = 0; f < context.frames; ++f) require(std::isfinite(channel[size_t(f)]), id + ": finite output");
        if (synth) {
            const float amplitude=(100.f/127.f)*.25f*(64.f/127.f);
            for (uint32_t c=0;c<outputs;++c) for (int f=0;f<context.frames;++f) {
                const float expected = f>=35 && f<99 ? ((f-35)%2 ? -amplitude : amplitude) : 0.f;
                require(std::fabs(context.outputs[c][f]-expected)<1e-7,"synth AAX exact onset/CC/tail");
            }
            require(context.midiOut.posted.size()==size_t((block+1)*3),"synth AAX output packet count");
            const auto &cc=context.midiOut.posted[size_t(block)*3];
            require(cc.mTimestamp==0 && cc.mLength==3 && cc.mData[0]==0xb0 && cc.mData[1]==1 && cc.mData[2]==64,"synth AAX CC echo");
            const auto &off=context.midiOut.posted.back();
            require(off.mTimestamp==17 && off.mLength==3 && off.mData[0]==0x80 && off.mData[1]==60 && off.mData[2]==0,"synth AAX note-off echo");
            const auto &on=context.midiOut.posted[context.midiOut.posted.size()-2];
            require(on.mTimestamp==3 && on.mLength==3 && on.mData[0]==0x90 && on.mData[1]==60 && on.mData[2]==100,"synth AAX note echo");
        }
        model->TimerWakeup();
    }
    if (synth) {
        // Drain the preceding click before changing latency. Parameter IDs are
        // native permanent IDs encoded by the generic AAX adapter.
        context.midiIn.in.clear(); context.run(512);
        const auto latencyUpdates=host.latencyUpdates;
        require(model->UpdateParameterNormalizedValue("p2",0.,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS,"synth lookahead edit");
        context.run(512); model->TimerWakeup(); context.run(512); model->TimerWakeup();
        require(host.latencyUpdates==latencyUpdates+1,"synth AAX latency change notification");
        std::cout << "  PASS synth MIDI input/output, CC audio, exact 32-sample onset and finite audio tail; latency change processed\n";
    }
    context.run(0);
    std::cout << "  processed 8 blocks + 1 empty block; MIDI out packets " << context.midiOut.posted.size() << "\n";

    int32_t chunks = 0;
    AAX_CTypeID chunkId = 0;
    uint32_t size = 0;
    require(model->GetNumberOfChunks(&chunks) == AAX_SUCCESS && chunks == 1, id + ": one chunk");
    require(model->GetChunkIDFromIndex(0, &chunkId) == AAX_SUCCESS && model->GetChunkSize(chunkId, &size) == AAX_SUCCESS && size > 0, id + ": chunk size");
    std::vector<uint8_t> storage(sizeof(AAX_SPlugInChunk) + size);
    auto *chunk = reinterpret_cast<AAX_SPlugInChunk *>(storage.data());
    chunk->fChunkID = chunkId;
    require(model->GetChunk(chunkId, chunk) == AAX_SUCCESS && uint32_t(chunk->fSize) == size, id + ": GetChunk");
    require(std::memcmp(chunk->fData, "RAVETSTA", 8) == 0 || std::memcmp(chunk->fData, "MOCKSTAT", 8) == 0, id + ": chunk carries the state envelope");
    AAX_CBoolean equal = false;
    require(model->CompareActiveChunk(chunk, &equal) == AAX_SUCCESS && equal, id + ": chunk compares equal");
    require(model->SetChunk(chunkId, chunk) == AAX_SUCCESS, id + ": SetChunk");
    chunk->fData[0] ^= 0x5a;
    require(model->SetChunk(chunkId, chunk) != AAX_SUCCESS, id + ": corrupt chunk rejected");
    std::cout << "  state chunk " << fourcc(chunkId) << " " << size << " bytes: save/compare/load ok, corrupt rejected\n";

#if defined(__APPLE__) || defined(_WIN32)
    if (openEditors && descriptor.createGui) {
        using Create = AAX_IEffectGUI *(AAX_CALLBACK *)();
        std::unique_ptr<AAX_IEffectGUI> gui(reinterpret_cast<Create>(descriptor.createGui)());
        fake::GuiHost guiHost(host, model.get());
        require(gui && gui->Initialize(&guiHost) == AAX_SUCCESS, id + ": GUI Initialize");
        AAX_Point size(0, 0);
        require(gui->GetViewSize(&size) == AAX_SUCCESS && size.horz > 0 && size.vert > 0, id + ": GUI size");
        void *parent = ravet_check_make_view(size.horz, size.vert);
#if defined(_WIN32)
        const AAX_EViewContainer_Type type = AAX_eViewContainer_Type_HWND;
        const char *kind = "an HWND";
#else
        const AAX_EViewContainer_Type type = AAX_eViewContainer_Type_NSView;
        const char *kind = "an NSView";
#endif
        fake::ViewContainer view(type, parent);
        require(gui->SetViewContainer(&view) == AAX_SUCCESS, id + ": SetViewContainer");
        const unsigned attached = ravet_check_subviews(parent);
        const char *hold = std::getenv("RAVET_AAX_EDITOR_HOLD");
        const int ticks = 10 + (hold ? int(std::atof(hold) * 20) : 0);
        unsigned loaded = attached;
        for (int tick = 0; tick < ticks; ++tick) {
            ravet_check_pump(0.05);
            gui->TimerWakeup();
            model->TimerWakeup();
            loaded = (std::max)(loaded, ravet_check_subviews(parent));
        }
        require(attached > 0, id + ": editor_open attached a view to the host parent");
        require(gui->SetViewContainer(nullptr) == AAX_SUCCESS && gui->Uninitialize() == AAX_SUCCESS, id + ": GUI close");
        ravet_check_pump(0.05);
        std::cout << "  editor " << size.horz << "x" << size.vert << " opened in " << kind << " (" << attached
                  << " child view, " << loaded << " descendants while open), " << ticks << " timer ticks, closed\n";
#if defined(_WIN32)
        DestroyWindow(static_cast<HWND>(parent));
#endif
    }
#endif
    require(model->Uninitialize() == AAX_SUCCESS, id + ": Uninitialize");
    model.reset();
}

} // namespace

int main(int argc, char **argv) {
    if (argc < 2 || argc > 3 || (argc == 3 && std::string(argv[2]) != "--editor")) {
        std::cerr << "usage: ravet_aax_check <Name.aaxplugin> [--editor]\n";
        return 2;
    }
    openEditors = argc == 3;
    try {
        const fs::path bundle = fs::absolute(argv[1]);
        Module module(binary(bundle));
        for (const char *name : {"ACFRegisterPlugin", "ACFRegisterComponent", "ACFGetClassFactory", "ACFCanUnloadNow",
                                 "ACFStartup", "ACFShutdown", "ACFGetSDKVersion"})
            require(module.symbol(name) != nullptr, std::string("missing export ") + name);
        using Version = int32_t (*)(uint64_t *);
        uint64_t version = 0;
        require(reinterpret_cast<Version>(module.symbol("ACFGetSDKVersion"))(&version) == 0 && version, "ACFGetSDKVersion");
#if defined(_WIN32)
        void *describeSymbol = module.symbol("?GetEffectDescriptions@@YAHPEAVAAX_ICollection@@@Z");
#else
        void *describeSymbol = module.symbol("_Z21GetEffectDescriptionsP15AAX_ICollection");
#endif
        require(describeSymbol != nullptr, "GetEffectDescriptions export");
        std::cout << "loaded " << bundle.filename().string() << "; ACF exports present; SDK version 0x" << std::hex << version << std::dec << "\n";
        fake::Collection collection;
        using Describe = AAX_Result (*)(AAX_ICollection *);
        require(reinterpret_cast<Describe>(describeSymbol)(&collection) == AAX_SUCCESS && !collection.effects.empty(), "GetEffectDescriptions");
        std::cout << "package \"" << collection.package << "\" by \"" << collection.manufacturer << "\" version 0x" << std::hex
                  << collection.version << std::dec << ", " << collection.effects.size() << " effect(s)\n";
        for (auto &effect : collection.effects) exercise(effect.first, *effect.second);
        std::cout << "PASS\n";
        return 0;
    } catch (const std::exception &error) {
        std::cerr << "FAIL: " << error.what() << "\n";
        return 1;
    }
}
