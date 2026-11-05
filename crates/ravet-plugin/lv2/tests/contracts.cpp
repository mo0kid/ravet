// Contract tests for the generic RAVET LV2 adapter, run against a packaged
// bundle. A minimal in-process LV2 host loads the bundle's actual adapter and
// UI modules.
//
//   ravet_lv2_contracts <bundle directory> <adapter file> <ui file or -> [--mock]
//
// --mock runs the full contracts against the mock product (it reads the mock's
// probe). Without it, a product-independent smoke test runs: descriptor URI,
// Turtle port agreement, instantiate/activate/run with silence, state round
// trip and cleanup.
#include "RavetLv2Product.hpp"

#include "lv2/atom/atom.h"
#include "lv2/atom/util.h"
#include "lv2/buf-size/buf-size.h"
#include "lv2/core/lv2.h"
#include "lv2/midi/midi.h"
#include "lv2/options/options.h"
#include "lv2/state/state.h"
#include "lv2/time/time.h"
#include "lv2/ui/ui.h"
#include "lv2/urid/urid.h"

#include <cmath>
#include <cstdio>
#include <cstring>
#include <fstream>
#include <map>
#include <sstream>
#include <string>
#include <vector>

using namespace ravet::lv2;

struct RavetMockLogEntry { uint32_t call, frames; RavetNativeEventV1 event; };
struct RavetMockProbe {
    uint32_t prepare_calls, deactivate_calls, process_calls, last_frames, max_frames_seen;
    RavetNativeProcessorConfigV1 config;
    uint32_t log_count;
    RavetMockLogEntry log[4096];
    uint32_t transport_count;
    RavetNativeTransportV1 transport[64];
    uint32_t editor_open_calls, editor_kind, editor_width, editor_height, editor_close_calls, editor_idle_calls,
        editor_resize_calls;
    void *editor_parent;
    uint32_t load_calls;
};

static int failures = 0, checks = 0;
#define CHECK(condition) do { ++checks; if (!(condition)) { ++failures; std::fprintf(stderr, "FAIL %s:%d: %s\n", __FILE__, __LINE__, #condition); } } while (0)

static void *open(const std::string &path) {
#if defined(_WIN32)
    return LoadLibraryA(path.c_str());
#else
    return dlopen(path.c_str(), RTLD_NOW | RTLD_LOCAL);
#endif
}
static void *symbol(void *module, const char *name) {
#if defined(_WIN32)
    return reinterpret_cast<void *>(GetProcAddress(static_cast<HMODULE>(module), name));
#else
    return dlsym(module, name);
#endif
}

#if defined(__APPLE__)
#include <objc/message.h>
#include <objc/runtime.h>
constexpr uint32_t expectedView = RAVET_PLUGIN_VIEW_COCOA;
// A real (unattached) NSView: the UI messages its parent to find the child view.
static void *nativeParent() {
    dlopen("/System/Library/Frameworks/AppKit.framework/AppKit", RTLD_NOW | RTLD_GLOBAL);
    auto send = reinterpret_cast<id (*)(id, SEL)>(objc_msgSend);
    id view = send(reinterpret_cast<id>(objc_getClass("NSView")), sel_registerName("alloc"));
    return send(view, sel_registerName("init"));
}
#elif defined(_WIN32)
constexpr uint32_t expectedView = RAVET_PLUGIN_VIEW_WIN32;
static int fakeWindow = 0;
static void *nativeParent() { return &fakeWindow; }  // GetWindow on it returns NULL
#else
constexpr uint32_t expectedView = RAVET_PLUGIN_VIEW_X11;
static void *nativeParent() { return reinterpret_cast<void *>(uintptr_t(0x4200001)); }  // XQueryTree fails without a display
#endif

// ---- Minimal host ----
struct Host {
    std::vector<std::string> uris{""};
    LV2_URID_Map map{this, [](LV2_URID_Map_Handle h, const char *uri) -> LV2_URID {
                         auto *host = static_cast<Host *>(h);
                         for (size_t i = 1; i < host->uris.size(); ++i) if (host->uris[i] == uri) return LV2_URID(i);
                         host->uris.push_back(uri);
                         return LV2_URID(host->uris.size() - 1);
                     }};
    LV2_URID id(const char *uri) { return map.map(map.handle, uri); }
    int32_t maxBlock = 256;
    std::vector<LV2_Options_Option> options;
    LV2_Feature mapFeature{LV2_URID__map, &map}, optionsFeature{LV2_OPTIONS__options, nullptr};
    std::vector<const LV2_Feature *> features;
    Host() {
        options = {{LV2_OPTIONS_INSTANCE, 0, id(LV2_BUF_SIZE__maxBlockLength), sizeof(int32_t), id(LV2_ATOM__Int), &maxBlock},
                   {LV2_OPTIONS_INSTANCE, 0, 0, 0, 0, nullptr}};
        optionsFeature.data = options.data();
        features = {&mapFeature, &optionsFeature, nullptr};
    }
};

// Atom sequence builder over an 8-byte aligned buffer.
struct Sequence {
    std::vector<uint64_t> storage;
    explicit Sequence(size_t bytes = 8192) : storage(bytes / 8, 0) {}
    LV2_Atom_Sequence *seq() { return reinterpret_cast<LV2_Atom_Sequence *>(storage.data()); }
    void clear(Host &host) {
        std::fill(storage.begin(), storage.end(), 0);
        seq()->atom.type = host.id(LV2_ATOM__Sequence);
        seq()->atom.size = sizeof(LV2_Atom_Sequence_Body);
        seq()->body.unit = host.id(LV2_ATOM__frameTime);
    }
    void prepareOutput() { std::fill(storage.begin(), storage.end(), 0); seq()->atom.size = uint32_t(storage.size() * 8); }
    uint8_t *append(int64_t frame, LV2_URID type, uint32_t size) {
        auto *base = reinterpret_cast<uint8_t *>(seq());
        auto *event = reinterpret_cast<LV2_Atom_Event *>(base + sizeof(LV2_Atom) + seq()->atom.size);
        event->time.frames = frame;
        event->body.type = type;
        event->body.size = size;
        seq()->atom.size += uint32_t(sizeof(LV2_Atom_Event) + lv2_atom_pad_size(size));
        return reinterpret_cast<uint8_t *>(event + 1);
    }
    void midi(Host &host, int64_t frame, std::vector<uint8_t> bytes) {
        std::memcpy(append(frame, host.id(LV2_MIDI__MidiEvent), uint32_t(bytes.size())), bytes.data(), bytes.size());
    }
    // time:Position with Float bpm/speed/beatsPerBar, Double beat, Long frame, Int beatUnit.
    void position(Host &host, int64_t frame, float bpm, float speed, double beat, int64_t sample) {
        struct Property { LV2_URID key; LV2_URID type; uint32_t size; uint64_t bits; };
        std::vector<Property> properties;
        auto f = [&](const char *key, float v) { uint64_t b = 0; std::memcpy(&b, &v, 4); properties.push_back({host.id(key), host.id(LV2_ATOM__Float), 4, b}); };
        f(LV2_TIME__beatsPerMinute, bpm);
        f(LV2_TIME__speed, speed);
        f(LV2_TIME__beatsPerBar, 4.f);
        { uint64_t b = 0; std::memcpy(&b, &beat, 8); properties.push_back({host.id(LV2_TIME__beat), host.id(LV2_ATOM__Double), 8, b}); }
        { uint64_t b = 0; std::memcpy(&b, &sample, 8); properties.push_back({host.id(LV2_TIME__frame), host.id(LV2_ATOM__Long), 8, b}); }
        { int32_t unit = 4; uint64_t b = 0; std::memcpy(&b, &unit, 4); properties.push_back({host.id(LV2_TIME__beatUnit), host.id(LV2_ATOM__Int), 4, b}); }
        const uint32_t size = uint32_t(sizeof(LV2_Atom_Object_Body) + properties.size() * (sizeof(LV2_Atom_Property_Body) + 8));
        auto *body = append(frame, host.id(LV2_ATOM__Object), size);
        auto *object = reinterpret_cast<LV2_Atom_Object_Body *>(body);
        object->id = 0;
        object->otype = host.id(LV2_TIME__Position);
        auto *cursor = body + sizeof(LV2_Atom_Object_Body);
        for (const auto &p : properties) {
            auto *property = reinterpret_cast<LV2_Atom_Property_Body *>(cursor);
            property->key = p.key; property->context = 0; property->value.type = p.type; property->value.size = p.size;
            std::memcpy(cursor + sizeof(LV2_Atom_Property_Body), &p.bits, 8);
            cursor += sizeof(LV2_Atom_Property_Body) + 8;
        }
    }
};

struct StateStore {
    std::map<uint32_t, std::vector<uint8_t>> values;
    std::map<uint32_t, uint32_t> types, flags;
    static LV2_State_Status store(LV2_State_Handle h, uint32_t key, const void *value, size_t size, uint32_t type, uint32_t flags) {
        auto *self = static_cast<StateStore *>(h);
        self->values[key].assign(static_cast<const uint8_t *>(value), static_cast<const uint8_t *>(value) + size);
        self->types[key] = type;
        self->flags[key] = flags;
        return LV2_STATE_SUCCESS;
    }
    static const void *retrieve(LV2_State_Handle h, uint32_t key, size_t *size, uint32_t *type, uint32_t *flags) {
        auto *self = static_cast<StateStore *>(h);
        auto found = self->values.find(key);
        if (found == self->values.end()) return nullptr;
        *size = found->second.size(); *type = self->types[key]; *flags = self->flags[key];
        return found->second.data();
    }
};

static std::string readFile(const std::string &path) {
    std::ifstream in(path);
    std::stringstream buffer;
    buffer << in.rdbuf();
    return buffer.str();
}

int main(int argc, char **argv) {
    if (argc < 4) { std::fprintf(stderr, "usage: ravet_lv2_contracts <bundle> <adapter> <ui> [--mock]\n"); return 2; }
    const std::string bundle = std::string(argv[1]) + "/";
    const bool mock = argc > 4 && std::string(argv[4]) == "--mock";

    std::string error;
    const RavetPluginEntryV1 *entry = loadProduct(bundle + productFile, error);
    if (!entry) { std::fprintf(stderr, "product: %s\n", error.c_str()); return 1; }
    void *adapterModule = open(bundle + argv[2]);
    if (!adapterModule) { std::fprintf(stderr, "cannot load adapter %s\n", argv[2]); return 1; }
    auto descriptorFn = reinterpret_cast<const LV2_Descriptor *(*)(uint32_t)>(symbol(adapterModule, "lv2_descriptor"));
    CHECK(descriptorFn != nullptr);
    if (!descriptorFn) return 1;
    const LV2_Descriptor *descriptor = descriptorFn(0);
    CHECK(descriptor && std::string(descriptor->URI) == entry->product->lv2_uri);
    CHECK(descriptorFn(1) == nullptr);
    if (!descriptor) return 1;

    // Turtle agrees with the run-time port layout.
    const auto ports = portLayout(*entry);
    const std::string turtle = readFile(bundle + "plugin.ttl");
    const std::string manifest = readFile(bundle + "manifest.ttl");
    CHECK(manifest.find(std::string("<") + entry->product->lv2_uri + ">") != std::string::npos);
    CHECK(manifest.find(std::string("lv2:binary <") + argv[2] + ">") != std::string::npos);
    for (size_t i = 0; i < ports.size(); ++i) {
        const std::string expected = "lv2:index " + std::to_string(i) + " ;\n    lv2:symbol \"" + ports[i].symbol + "\"";
        CHECK(turtle.find(expected) != std::string::npos);
    }

    Host host;
    const uint32_t inputs = entry->product->layouts[0].inputs, outputs = entry->product->layouts[0].outputs;
    const uint32_t frames = 1500;  // > maxBlockLength 256: exercises chunking
    std::vector<std::vector<float>> in(inputs, std::vector<float>(frames)), out(outputs, std::vector<float>(frames));
    std::vector<float> controls(ports.size(), 0.f);
    Sequence events, midiOut(65536);
    LV2_Handle plugin = descriptor->instantiate(descriptor, 48000.0, bundle.c_str(), host.features.data());
    CHECK(plugin != nullptr);
    if (!plugin) return 1;
    void *dsp = nullptr;  // the mock's table instance behind `plugin`
    if (mock) dsp = reinterpret_cast<void *(*)()>(symbol(open(bundle + productFile), "ravet_mock_last_instance"))();
    // Without urid:map (e.g. lv2apply) the plug-in runs; Atom ports and state are unused.
    {
        const LV2_Feature *none[] = {nullptr};
        LV2_Handle bare = descriptor->instantiate(descriptor, 48000.0, bundle.c_str(), none);
        CHECK(bare != nullptr);
        if (bare) {
            std::vector<std::vector<float>> bareIn(inputs, std::vector<float>(64, 0.25f)), bareOut(outputs, std::vector<float>(64, 9.f));
            std::vector<float> bareControls(ports.size(), 0.f);
            float notAnAtom = 0.f;  // lv2apply connects optional Atom ports to floats
            uint32_t i0 = 0, o0 = 0;
            for (uint32_t i = 0; i < ports.size(); ++i) {
                if (ports[i].kind == PortKind::AudioIn) descriptor->connect_port(bare, i, bareIn[i0++].data());
                else if (ports[i].kind == PortKind::AudioOut) descriptor->connect_port(bare, i, bareOut[o0++].data());
                else if (ports[i].kind == PortKind::EventsIn || ports[i].kind == PortKind::MidiOut) descriptor->connect_port(bare, i, &notAnAtom);
                else {
                    RavetPluginParameterV1 parameter{};
                    if (ports[i].kind != PortKind::Latency) entry->parameter(ports[i].parameter, &parameter);
                    bareControls[i] = ports[i].kind == PortKind::Enabled ? 1.f : float(parameter.default_value);
                    descriptor->connect_port(bare, i, &bareControls[i]);
                }
            }
            descriptor->activate(bare);
            for (int block = 0; block < 4; ++block) descriptor->run(bare, block ? 1 : 64);
            bool finite = notAnAtom == 0.f;
            for (auto &channel : bareOut) for (float v : channel) finite &= std::isfinite(v) && v != 9.f;
            CHECK(finite);
            StateStore unused;
            const auto *iface = static_cast<const LV2_State_Interface *>(descriptor->extension_data(LV2_STATE__interface));
            CHECK(iface && iface->save(bare, StateStore::store, &unused, 0, nullptr) == LV2_STATE_ERR_NO_FEATURE);
            descriptor->deactivate(bare);
            descriptor->cleanup(bare);
        }
    }
    int32_t enabledPort = -1, latencyPort = -1, eventsPort = -1, midiPort = -1;
    std::map<std::string, uint32_t> bySymbol;
    uint32_t ai = 0, ao = 0;
    for (uint32_t i = 0; i < ports.size(); ++i) {
        bySymbol[ports[i].symbol] = i;
        switch (ports[i].kind) {
        case PortKind::AudioIn: descriptor->connect_port(plugin, i, in[ai++].data()); break;
        case PortKind::AudioOut: descriptor->connect_port(plugin, i, out[ao++].data()); break;
        case PortKind::EventsIn: eventsPort = int32_t(i); descriptor->connect_port(plugin, i, events.seq()); break;
        case PortKind::MidiOut: midiPort = int32_t(i); descriptor->connect_port(plugin, i, midiOut.seq()); break;
        case PortKind::Latency: latencyPort = int32_t(i); descriptor->connect_port(plugin, i, &controls[i]); break;
        default: {
            RavetPluginParameterV1 parameter{};
            entry->parameter(ports[i].parameter, &parameter);
            controls[i] = float(parameter.default_value);
            if (ports[i].kind == PortKind::Enabled) { enabledPort = int32_t(i); controls[i] = 1.f; }
            descriptor->connect_port(plugin, i, &controls[i]);
        }
        }
    }
    CHECK(eventsPort >= 0 && latencyPort >= 0);
    for (uint32_t c = 0; c < inputs; ++c) for (uint32_t f = 0; f < frames; ++f) in[c][f] = 0.5f * std::sin(0.01f * float(f + 100 * c));

    auto run = [&](uint32_t n) {
        midiOut.prepareOutput();
        descriptor->run(plugin, n);
        events.clear(host);
    };
    const auto *stateIface = static_cast<const LV2_State_Interface *>(descriptor->extension_data(LV2_STATE__interface));
    CHECK(stateIface && stateIface->save && stateIface->restore);
    CHECK(descriptor->extension_data("urn:unknown") == nullptr);
    events.clear(host);
    descriptor->activate(plugin);

    if (!mock) {
        run(frames);
        bool finite = true;
        for (auto &channel : out) for (float v : channel) finite &= std::isfinite(v);
        CHECK(finite);
        StateStore store;
        CHECK(stateIface->save(plugin, StateStore::store, &store, 0, nullptr) == LV2_STATE_SUCCESS);
        CHECK(store.values.size() == 1);
        CHECK(stateIface->restore(plugin, StateStore::retrieve, &store, 0, nullptr) == LV2_STATE_SUCCESS);
        run(frames);
        // UI: open the product's editor in a native parent, idle, resize, close.
        // Only macOS (Cocoa) hosts an editor in this release; elsewhere the UI
        // refuses to instantiate, which is what this checks there.
        const bool shipsUi = std::string(argv[3]) != "-";
        CHECK(shipsUi == (manifest.find("ui:binary") != std::string::npos));
        void *uiModule = shipsUi ? open(bundle + argv[3]) : nullptr;
        auto uiDescriptorFn = uiModule ? reinterpret_cast<const LV2UI_Descriptor *(*)(uint32_t)>(symbol(uiModule, "lv2ui_descriptor")) : nullptr;
        const LV2UI_Descriptor *ui = uiDescriptorFn ? uiDescriptorFn(0) : nullptr;
        CHECK(!shipsUi || (ui != nullptr) == (entry->product->editor_width != 0));
        if (ui) {
            struct Recorder { int writes = 0; } recorder;
            auto write = [](LV2UI_Controller c, uint32_t, uint32_t, uint32_t, const void *) { ++static_cast<Recorder *>(c)->writes; };
            void *parentView = nativeParent();
            LV2_Feature parentFeature{LV2_UI__parent, parentView};
            const LV2_Feature *uiFeatures[] = {&parentFeature, nullptr};
            LV2UI_Widget widget = nullptr;
            LV2UI_Handle handle = ui->instantiate(ui, descriptor->URI, bundle.c_str(), write, &recorder, &widget, uiFeatures);
#if defined(__APPLE__)
            CHECK(handle != nullptr && widget != nullptr);
            std::printf("UI widget %p in parent %p\n", widget, parentView);
#else
            CHECK(handle == nullptr);  // X11/Win32 editors are not hosted by this table release
#endif
            if (handle) {
                for (uint32_t i = 0; i < ports.size(); ++i)
                    if (ports[i].kind == PortKind::Parameter || ports[i].kind == PortKind::Enabled)
                        ui->port_event(handle, i, sizeof(float), 0, &controls[i]);
                const auto *idle = static_cast<const LV2UI_Idle_Interface *>(ui->extension_data(LV2_UI__idleInterface));
                CHECK(idle != nullptr);
                for (int i = 0; idle && i < 10; ++i) CHECK(idle->idle(handle) == 0);
                CHECK(recorder.writes == 0);  // no editor interaction, so nothing written
                const auto *uiResize = static_cast<const LV2UI_Resize *>(ui->extension_data(LV2_UI__resize));
                if (uiResize && entry->product->editor_resizable)
                    CHECK(uiResize->ui_resize(handle, int(entry->product->editor_width * 2), int(entry->product->editor_height * 2)) == 0);
                ui->cleanup(handle);
            }
        }
        descriptor->deactivate(plugin);
        descriptor->cleanup(plugin);
        std::printf("ravet_lv2_contracts (smoke): %d checks, %d failures\n", checks, failures);
        return failures ? 1 : 0;
    }

    // ---- Mock product contracts ----
    auto *probe = reinterpret_cast<RavetMockProbe *(*)()>(symbol(open(bundle + productFile), "ravet_mock_probe"))();
    auto resetProbe = reinterpret_cast<void (*)()>(symbol(open(bundle + productFile), "ravet_mock_probe_reset"));
    auto lastInstance = reinterpret_cast<void *(*)()>(symbol(open(bundle + productFile), "ravet_mock_last_instance"));
    auto pushEdit = reinterpret_cast<void (*)(void *, uint32_t, uint32_t, double)>(symbol(open(bundle + productFile), "ravet_mock_push_edit"));
    CHECK(probe && resetProbe && lastInstance && pushEdit);
    if (!probe || !resetProbe || !lastInstance || !pushEdit) return 1;
    CHECK(dsp != nullptr);
    const uint32_t gainPort = bySymbol.at("gain"), modePort = bySymbol.at("mode"), levelPort = bySymbol.at("level");
    CHECK(enabledPort >= 0 && midiPort >= 0);
    CHECK(ports.size() == 2 + 2 + 1 + 1 + 1 + 4);
    CHECK(turtle.find("lv2:designation lv2:enabled") != std::string::npos);
    CHECK(turtle.find("lv2:designation lv2:latency") != std::string::npos);
    CHECK(turtle.find("rdfs:label \"Fuzz\"") != std::string::npos);
    CHECK(turtle.find("units:unit units:db") != std::string::npos);
    CHECK(turtle.find("atom:supports time:Position, midi:MidiEvent") != std::string::npos);
    CHECK(turtle.find("a ui:") != std::string::npos);
    CHECK(probe->prepare_calls == 2 && probe->config.maximum_frames == 256 && probe->config.buses[0].inputs == 2 &&
          probe->config.buses[0].outputs == 2 && probe->config.sample_rate == 48000.0);

    // 1. First run sends every input control at offset 0, chunked at 256 frames.
    resetProbe();
    controls[gainPort] = -6.f;
    run(frames);
    CHECK(probe->process_calls == 6 && probe->max_frames_seen == 256);
    uint32_t parameterEvents = 0;
    for (uint32_t i = 0; i < probe->log_count; ++i) {
        const auto &e = probe->log[i];
        if (e.event.kind == RAVET_NATIVE_PARAMETER_VALUE) {
            ++parameterEvents;
            CHECK(e.call == 0 && e.event.offset == 0);
            if (e.event.parameter_id == 0x101) CHECK(e.event.value == -6.0);
            if (e.event.parameter_id == 0x102) CHECK(e.event.value == 0.0);  // enabled=1 → bypass off
        }
    }
    CHECK(parameterEvents == 3);  // gain, bypass, mode (level is read-only)
    const double gain = std::pow(10.0, -6.0 / 20.0);
    CHECK(std::fabs(out[0][1000] - float(in[0][1000] * gain)) < 1e-6f);
    CHECK(std::fabs(out[1][17] - float(in[1][17] * gain)) < 1e-6f);
    CHECK(controls[latencyPort] == 32.f);   // mode default 1
    CHECK(controls[levelPort] > 0.f);       // read-only parameter reaches its output port

    // 2. Unchanged controls send nothing; enabled=0 bypasses (inverted).
    resetProbe();
    run(frames);
    CHECK(probe->log_count == 0);
    controls[size_t(enabledPort)] = 0.f;
    run(frames);
    CHECK(probe->log_count == 1 && probe->log[0].event.parameter_id == 0x102 && probe->log[0].event.value == 1.0);
    CHECK(out[0][700] == in[0][700]);
    controls[size_t(enabledPort)] = 1.f;

    // 3. Stepped parameter and latency port.
    resetProbe();
    controls[modePort] = 3.f;
    run(64);
    CHECK(probe->log_count >= 1 && probe->log[probe->log_count - 1].event.parameter_id == 0x103 &&
          probe->log[probe->log_count - 1].event.value == 3.0);
    CHECK(controls[latencyPort] == 96.f);
    controls[modePort] = NAN;  // nonfinite control is ignored
    resetProbe();
    run(64);
    CHECK(probe->log_count == 0);
    controls[modePort] = 3.f;

    // 4. MIDI in (timed, chunk-relative) and echoed out at the absolute frame.
    resetProbe();
    events.clear(host);
    events.midi(host, 10, {0x90, 60, 100});
    events.midi(host, 300, {0x80, 60, 0});
    events.midi(host, 301, {0xF0, 1, 2});      // SysEx: skipped
    events.midi(host, 302, {0x90, 200, 1});    // invalid data byte: skipped
    midiOut.prepareOutput();
    descriptor->run(plugin, 512);
    events.clear(host);
    uint32_t midiEvents = 0;
    for (uint32_t i = 0; i < probe->log_count; ++i) {
        const auto &e = probe->log[i];
        if (e.event.kind != RAVET_NATIVE_MIDI) continue;
        ++midiEvents;
        if (midiEvents == 1) CHECK(e.call == 0 && e.event.offset == 10 && e.event.detail == 3 && e.event.words[0] == (0x90u | 60u << 8 | 100u << 16));
        if (midiEvents == 2) CHECK(e.call == 1 && e.event.offset == 300 - 256 && e.event.words[0] == (0x80u | 60u << 8));
    }
    CHECK(midiEvents == 2);
    {
        std::vector<std::pair<int64_t, uint32_t>> seen;
        LV2_ATOM_SEQUENCE_FOREACH(midiOut.seq(), event) {
            CHECK(event->body.type == host.id(LV2_MIDI__MidiEvent));
            seen.push_back({event->time.frames, event->body.size});
        }
        CHECK(seen.size() == 2 && seen[0].first == 10 && seen[1].first == 300 && seen[0].second == 3);
    }

    // 5. time:Position → transport; extrapolated between reports; mid-block split.
    resetProbe();
    events.clear(host);
    events.position(host, 0, 120.f, 1.f, 8.0, 48000);
    midiOut.prepareOutput();
    descriptor->run(plugin, 240);
    events.clear(host);
    CHECK(probe->transport_count == 1);
    {
        const auto &t = probe->transport[0];
        CHECK((t.present & RAVET_TRANSPORT_TEMPO) && t.tempo == 120.0);
        CHECK((t.present & RAVET_TRANSPORT_PLAYING) && (t.flags & RAVET_TRANSPORT_FLAG_PLAYING));
        CHECK((t.present & RAVET_TRANSPORT_BEATS) && t.beat_position == 8.0);
        CHECK((t.present & RAVET_TRANSPORT_SAMPLES) && t.sample_position == 48000);
        CHECK((t.present & RAVET_TRANSPORT_SIGNATURE) && t.signature_numerator == 4 && t.signature_denominator == 4);
    }
    resetProbe();
    run(240);  // no new position: advanced by 240 frames at 120 bpm / 48 kHz = 0.01 beats
    CHECK(probe->transport_count == 1 && std::fabs(probe->transport[0].beat_position - 8.01) < 1e-9 &&
          probe->transport[0].sample_position == 48240);
    resetProbe();
    events.clear(host);
    events.position(host, 100, 90.f, 0.f, 2.0, 0);  // stops mid-block
    midiOut.prepareOutput();
    descriptor->run(plugin, 200);
    events.clear(host);
    CHECK(probe->process_calls == 2 && probe->transport_count == 2);
    CHECK(probe->transport[0].tempo == 120.0 && (probe->transport[0].flags & RAVET_TRANSPORT_FLAG_PLAYING));
    CHECK(probe->transport[1].tempo == 90.0 && !(probe->transport[1].flags & RAVET_TRANSPORT_FLAG_PLAYING) &&
          probe->transport[1].beat_position == 2.0);

    // 6. State: one atom:Chunk property; restore wins over stale ports.
    StateStore store;
    controls[gainPort] = -12.f;
    run(64);
    CHECK(stateIface->save(plugin, StateStore::store, &store, 0, nullptr) == LV2_STATE_SUCCESS);
    CHECK(store.values.size() == 1 && store.values.count(host.id(stateKey)) &&
          store.types[host.id(stateKey)] == host.id(LV2_ATOM__Chunk) &&
          (store.flags[host.id(stateKey)] & LV2_STATE_IS_POD) && (store.flags[host.id(stateKey)] & LV2_STATE_IS_PORTABLE));
    CHECK(store.values.begin()->second.size() == 40 && !std::memcmp(store.values.begin()->second.data(), "MOCKSTA1", 8));
    controls[gainPort] = 6.f;
    run(64);
    CHECK(entry->get_value(dsp, 0) == 6.0);  // port change reached the cache (saved state includes it)
    CHECK(stateIface->restore(plugin, StateStore::retrieve, &store, 0, nullptr) == LV2_STATE_SUCCESS);
    CHECK(entry->get_value(dsp, 0) == -12.0);
    resetProbe();
    run(64);  // port still at 6 dB: taken as baseline, restore stands
    CHECK(probe->log_count == 0 && entry->get_value(dsp, 0) == -12.0);
    controls[gainPort] = 3.f;
    run(64);  // a later port change wins
    CHECK(entry->get_value(dsp, 0) == 3.0);
    {
        StateStore wrong = store;
        wrong.types[host.id(stateKey)] = host.id(LV2_ATOM__Int);
        CHECK(stateIface->restore(plugin, StateStore::retrieve, &wrong, 0, nullptr) == LV2_STATE_ERR_BAD_TYPE);
        StateStore corrupt = store;
        corrupt.values[host.id(stateKey)][0] = 'X';
        CHECK(stateIface->restore(plugin, StateStore::retrieve, &corrupt, 0, nullptr) == LV2_STATE_ERR_UNKNOWN);
        StateStore empty;
        CHECK(stateIface->restore(plugin, StateStore::retrieve, &empty, 0, nullptr) == LV2_STATE_SUCCESS);
        CHECK(entry->get_value(dsp, 0) == 3.0);
    }

    // 7. Zero-frame run delivers control changes; disconnected input reads silence.
    resetProbe();
    controls[gainPort] = 0.f;
    descriptor->run(plugin, 0);
    CHECK(probe->process_calls == 1 && probe->last_frames == 0 && probe->log_count == 1);
    descriptor->connect_port(plugin, 0, nullptr);
    run(32);
    CHECK(out[0][5] == 0.f);
    descriptor->connect_port(plugin, 0, in[0].data());

    // 8. Deactivate/activate re-prepares.
    descriptor->deactivate(plugin);
    CHECK(probe->deactivate_calls == 1);
    descriptor->activate(plugin);
    CHECK(probe->prepare_calls == 1);  // probe reset above, so this is the re-prepare

    // ---- UI ----
    void *uiModule = open(bundle + argv[3]);
    CHECK(uiModule != nullptr);
    if (uiModule) {
        auto uiDescriptorFn = reinterpret_cast<const LV2UI_Descriptor *(*)(uint32_t)>(symbol(uiModule, "lv2ui_descriptor"));
        const LV2UI_Descriptor *ui = uiDescriptorFn ? uiDescriptorFn(0) : nullptr;
        CHECK(ui && std::string(ui->URI) == std::string(entry->product->lv2_uri) + "#ui");
        struct Written { uint32_t port; float value; };
        struct Recorder { std::vector<Written> writes; std::vector<std::pair<uint32_t, bool>> touches; int width = 0, height = 0; } recorder;
        auto write = [](LV2UI_Controller c, uint32_t port, uint32_t size, uint32_t protocol, const void *buffer) {
            if (size == sizeof(float) && protocol == 0) static_cast<Recorder *>(c)->writes.push_back({port, *static_cast<const float *>(buffer)});
        };
        LV2UI_Touch touch{&recorder, [](LV2UI_Feature_Handle h, uint32_t port, bool grabbed) {
                              static_cast<Recorder *>(h)->touches.push_back({port, grabbed});
                          }};
        LV2UI_Resize resize{&recorder, [](LV2UI_Feature_Handle h, int w, int height) {
                                static_cast<Recorder *>(h)->width = w; static_cast<Recorder *>(h)->height = height; return 0;
                            }};
        void *parentView = nativeParent();
        LV2_Feature parentFeature{LV2_UI__parent, parentView}, touchFeature{LV2_UI__touch, &touch}, resizeFeature{LV2_UI__resize, &resize};
        const LV2_Feature *uiFeatures[] = {&parentFeature, &touchFeature, &resizeFeature, nullptr};
        const LV2_Feature *noParent[] = {&touchFeature, nullptr};
        LV2UI_Widget widget = nullptr;
        if (ui) {
            CHECK(ui->instantiate(ui, descriptor->URI, bundle.c_str(), write, &recorder, &widget, noParent) == nullptr);
            CHECK(ui->instantiate(ui, "urn:other", bundle.c_str(), write, &recorder, &widget, uiFeatures) == nullptr);
            const uint32_t opensBefore = probe->editor_open_calls;
            LV2UI_Handle handle = ui->instantiate(ui, descriptor->URI, bundle.c_str(), write, &recorder, &widget, uiFeatures);
            CHECK(handle != nullptr);
            CHECK(probe->editor_open_calls == opensBefore + 1 && probe->editor_kind == expectedView &&
                  probe->editor_parent == parentView && probe->editor_width == 640 && probe->editor_height == 360);
            CHECK(widget == parentView);  // the mock adds no child view
            CHECK(recorder.width == 640 && recorder.height == 360);
            if (handle) {
                void *mirror = lastInstance();
                CHECK(mirror != dsp);
                const float gainValue = -20.f, enabledValue = 0.f;
                ui->port_event(handle, gainPort, sizeof(float), 0, &gainValue);
                ui->port_event(handle, uint32_t(enabledPort), sizeof(float), 0, &enabledValue);
                CHECK(entry->get_value(mirror, 0) == -20.0 && entry->get_value(mirror, 1) == 1.0);
                pushEdit(mirror, RAVET_PLUGIN_EDIT_BEGIN, 0, 0);
                pushEdit(mirror, RAVET_PLUGIN_EDIT_VALUE, 0, -3.0);
                pushEdit(mirror, RAVET_PLUGIN_EDIT_END, 0, 0);
                pushEdit(mirror, RAVET_PLUGIN_EDIT_VALUE, 1, 0.0);  // bypass off → enabled 1
                const auto *idle = static_cast<const LV2UI_Idle_Interface *>(ui->extension_data(LV2_UI__idleInterface));
                CHECK(idle && idle->idle(handle) == 0);
                CHECK(recorder.writes.size() == 2 && recorder.writes[0].port == gainPort && recorder.writes[0].value == -3.f &&
                      recorder.writes[1].port == uint32_t(enabledPort) && recorder.writes[1].value == 1.f);
                CHECK(recorder.touches.size() == 2 && recorder.touches[0].first == gainPort && recorder.touches[0].second &&
                      !recorder.touches[1].second);
                CHECK(probe->editor_idle_calls >= 1);
                const auto *uiResize = static_cast<const LV2UI_Resize *>(ui->extension_data(LV2_UI__resize));
                CHECK(uiResize && uiResize->ui_resize(handle, 800, 500) == 0 && probe->editor_width == 800);
                // The DSP side applies the written value as a normal control change.
                controls[gainPort] = recorder.writes[0].value;
                run(32);
                CHECK(entry->get_value(dsp, 0) == -3.0);
                const uint32_t closes = probe->editor_close_calls;
                ui->cleanup(handle);
                CHECK(probe->editor_close_calls == closes + 1);
            }
        }
    }

    descriptor->deactivate(plugin);
    descriptor->cleanup(plugin);
    std::printf("ravet_lv2_contracts (mock): %d checks, %d failures\n", checks, failures);
    return failures ? 1 : 0;
}
