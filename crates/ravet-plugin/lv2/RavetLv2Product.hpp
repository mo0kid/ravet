#pragma once
// Shared by the generic LV2 adapter, its UI and the Turtle generator: loading
// the product library beside the module and the port layout derived from the
// RavetPluginEntryV1 table. The layout is computed identically at build time
// (Turtle) and at run time (adapter), so port indexes always agree.
#include "RavetPlugin.h"
#include <cctype>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <string>
#include <vector>
#if defined(_WIN32)
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#else
#include <dlfcn.h>
#endif

namespace ravet::lv2 {

// The product library is copied into the bundle under this fixed name, so the
// adapter and UI binaries are identical for every product.
#if defined(_WIN32)
inline constexpr const char *productFile = "ravet_product.dll";
#elif defined(__APPLE__)
inline constexpr const char *productFile = "ravet_product.dylib";
#else
inline constexpr const char *productFile = "ravet_product.so";
#endif

// Key of the single state property holding the product's saved-state envelope.
inline constexpr const char *stateKey = "urn:ravet:lv2:state";

// Validate the table before any use. Returns an empty string when usable.
inline std::string checkEntry(const RavetPluginEntryV1 *entry) {
    if (!entry) return "ravet_plugin_v1 returned NULL";
    if (entry->version != RAVET_PLUGIN_VERSION_1) return "unsupported RavetPluginEntryV1 version";
    if (entry->byte_size < sizeof(RavetPluginEntryV1)) return "RavetPluginEntryV1 is smaller than expected";
    const RavetPluginProductV1 *product = entry->product;
    if (!product) return "product description is NULL";
    if (!product->name || !product->lv2_uri || !*product->lv2_uri) return "product name or LV2 URI missing";
    if (product->layout_count < 1 || !product->layouts) return "product has no layouts";
    if (product->parameter_count > 1024) return "more than 1024 parameters";
    if (product->bypass_index >= int32_t(product->parameter_count)) return "bypass index out of range";
    const auto &layout = product->layouts[0];
    if (layout.inputs > 64 || layout.outputs > 64 || layout.outputs == 0) return "preferred layout unsupported (1..64 outputs, 0..64 inputs)";
    if (product->auxiliary_count > 8) return "more than 8 auxiliary buses";
    if (product->auxiliary_count && !product->auxiliaries) return "auxiliary buses missing";
    for (uint32_t i = 0; i < product->auxiliary_count; ++i) {
        const auto &bus = product->auxiliaries[i];
        if (!bus.name || !*bus.name) return "auxiliary bus name missing";
        if (bus.inputs > 64 || bus.outputs > 64 || (bus.inputs == 0 && bus.outputs == 0))
            return "auxiliary bus unsupported (0..64 inputs and outputs, not both 0)";
    }
    if (!entry->parameter || !entry->value_to_text || !entry->create || !entry->destroy || !entry->get_value ||
        !entry->set_value || !entry->save_state || !entry->load_state || !entry->prepare || !entry->process ||
        !entry->reset || !entry->deactivate || !entry->timing || !entry->take_edits || !entry->editor_open ||
        !entry->editor_resize || !entry->editor_close || !entry->editor_idle)
        return "table function missing";
    return {};
}

// Loads `path` and resolves ravet_plugin_v1. The library is never unloaded:
// product libraries may own thread-local destructors and editor classes that
// outlive any one instance.
inline const RavetPluginEntryV1 *loadProduct(const std::string &path, std::string &error) {
    using Getter = const RavetPluginEntryV1 *(*)();
    Getter getter = nullptr;
#if defined(_WIN32)
    const int wide = MultiByteToWideChar(CP_UTF8, 0, path.c_str(), -1, nullptr, 0);
    std::wstring widePath(wide > 0 ? size_t(wide) : 0, L'\0');
    if (wide > 0) MultiByteToWideChar(CP_UTF8, 0, path.c_str(), -1, widePath.data(), wide);
    HMODULE module = LoadLibraryExW(widePath.c_str(), nullptr, LOAD_WITH_ALTERED_SEARCH_PATH);
    if (!module) { error = "cannot load " + path; return nullptr; }
    getter = reinterpret_cast<Getter>(reinterpret_cast<void *>(GetProcAddress(module, "ravet_plugin_v1")));
#else
    void *module = dlopen(path.c_str(), RTLD_NOW | RTLD_LOCAL);
    if (!module) { const char *reason = dlerror(); error = reason ? reason : ("cannot load " + path); return nullptr; }
    getter = reinterpret_cast<Getter>(dlsym(module, "ravet_plugin_v1"));
#endif
    if (!getter) { error = path + " does not export ravet_plugin_v1"; return nullptr; }
    const RavetPluginEntryV1 *entry = getter();
    error = checkEntry(entry);
    return error.empty() ? entry : nullptr;
}

// Directory (with trailing separator) of the module containing `address`.
inline std::string moduleDirectory(const void *address) {
#if defined(_WIN32)
    HMODULE module = nullptr;
    if (!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                            reinterpret_cast<LPCWSTR>(address), &module)) return {};
    std::wstring buffer(32768, L'\0');
    const DWORD length = GetModuleFileNameW(module, buffer.data(), DWORD(buffer.size()));
    if (!length || length >= buffer.size()) return {};
    buffer.resize(length);
    const int bytes = WideCharToMultiByte(CP_UTF8, 0, buffer.c_str(), -1, nullptr, 0, nullptr, nullptr);
    std::string path(bytes > 0 ? size_t(bytes) : 0, '\0');
    if (bytes > 0) WideCharToMultiByte(CP_UTF8, 0, buffer.c_str(), -1, path.data(), bytes, nullptr, nullptr);
    if (!path.empty() && path.back() == '\0') path.pop_back();
    const auto slash = path.find_last_of("\\/");
#else
    Dl_info info{};
    if (!dladdr(address, &info) || !info.dli_fname) return {};
    std::string path = info.dli_fname;
    const auto slash = path.find_last_of('/');
#endif
    return slash == std::string::npos ? std::string() : path.substr(0, slash + 1);
}

enum class PortKind { AudioIn, AudioOut, EventsIn, MidiOut, Latency, Parameter, Enabled, ParameterOut, AuxIn, AuxOut };

struct Port {
    PortKind kind;
    uint32_t channel = 0;      // audio channel (within its bus)
    uint32_t parameter = 0;    // parameter declaration index
    std::string symbol, name;
    uint32_t bus = 0;          // auxiliary bus index (AuxIn, AuxOut)
    bool sideChain = false;    // AuxIn of an input-only auxiliary bus
};

// LV2 port-group symbols (lv2/port-groups), used only by products with
// auxiliary buses: aux<B>_in / aux<B>_out for each direction of auxiliary bus B
// (1-based), and main_in (or main_out when the main bus has no inputs) as the
// group a side-chain is pg:sideChainOf, declared only when a product has an
// input-only auxiliary bus.
inline std::string auxGroupSymbol(uint32_t bus, bool input) {
    return "aux" + std::to_string(bus + 1) + (input ? "_in" : "_out");
}
inline constexpr const char *mainInputGroup = "main_in", *mainOutputGroup = "main_out";

inline bool validSymbol(const char *text) {
    if (!text || !*text) return false;
    if (!(std::isalpha(static_cast<unsigned char>(*text)) || *text == '_')) return false;
    for (const char *c = text; *c; ++c)
        if (!(std::isalnum(static_cast<unsigned char>(*c)) || *c == '_') || (c - text) >= 64) return false;
    return true;
}

// Bypass parameter physical values: bypassed is the maximum, active the minimum.
inline double bypassPhysical(const RavetPluginParameterV1 &bypass, bool bypassed) {
    return bypassed ? bypass.maximum : bypass.minimum;
}
inline bool isBypassed(const RavetPluginParameterV1 &bypass, double physical) {
    return physical >= 0.5 * (bypass.minimum + bypass.maximum) && bypass.maximum > bypass.minimum;
}

// The fixed port layout of the product's single LV2 descriptor:
//   audio inputs and outputs of the preferred (first) layout,
//   events_in (Atom: time:Position, plus MIDI when note_input),
//   midi_out (Atom MIDI, only when note_output),
//   latency (control output, lv2:latency designation),
//   then one control port per parameter in declaration order. The bypass
//   parameter becomes the inverted lv2:enabled port; read-only parameters are
//   control outputs.
//   Last, one audio port per auxiliary-bus channel, bus by bus in table order,
//   inputs before outputs: symbol aux<B>_in_<C> / aux<B>_out_<C> (B the 1-based
//   bus, C the 1-based channel), named after the bus. Products without
//   auxiliary buses therefore keep every port index and symbol.
// Fixed symbols (audio, events, latency, enabled, the auxiliary symbols and,
// for products with auxiliary buses, the port-group symbols) win: a parameter key that is not a valid LV2 symbol or collides with one
// becomes p<host ID> (then '_' is appended until unique).
inline std::vector<Port> portLayout(const RavetPluginEntryV1 &entry) {
    const auto &product = *entry.product;
    const auto &layout = product.layouts[0];
    std::vector<Port> ports;
    auto audioName = [](const char *direction, uint32_t channel, uint32_t count) {
        if (count == 2) return std::string(channel ? "Right " : "Left ") + direction;
        return std::string(direction) + " " + std::to_string(channel + 1);
    };
    for (uint32_t c = 0; c < layout.inputs; ++c)
        ports.push_back({PortKind::AudioIn, c, 0, "in_" + std::to_string(c + 1), audioName("In", c, layout.inputs)});
    for (uint32_t c = 0; c < layout.outputs; ++c)
        ports.push_back({PortKind::AudioOut, c, 0, "out_" + std::to_string(c + 1), audioName("Out", c, layout.outputs)});
    ports.push_back({PortKind::EventsIn, 0, 0, "events_in", "Events In"});
    if (product.note_output) ports.push_back({PortKind::MidiOut, 0, 0, "midi_out", "MIDI Out"});
    ports.push_back({PortKind::Latency, 0, 0, "latency", "Latency"});
    // Auxiliary ports are appended after the parameters, but their symbols are
    // reserved first so parameter keys cannot take them.
    std::vector<Port> auxiliary;
    for (uint32_t b = 0; b < product.auxiliary_count; ++b) {
        const auto &bus = product.auxiliaries[b];
        const std::string base = bus.name ? bus.name : "";
        const std::string prefix = "aux" + std::to_string(b + 1);
        auto name = [&](const char *direction, uint32_t channel, uint32_t count) {
            std::string text = base;
            if (bus.inputs && bus.outputs) text += std::string(" ") + direction;
            if (count == 2) text += channel ? " Right" : " Left";
            else if (count > 2) text += " " + std::to_string(channel + 1);
            return text;
        };
        for (uint32_t c = 0; c < bus.inputs; ++c)
            auxiliary.push_back({PortKind::AuxIn, c, 0, prefix + "_in_" + std::to_string(c + 1),
                                 name("In", c, bus.inputs), b, bus.outputs == 0});
        for (uint32_t c = 0; c < bus.outputs; ++c)
            auxiliary.push_back({PortKind::AuxOut, c, 0, prefix + "_out_" + std::to_string(c + 1),
                                 name("Out", c, bus.outputs), b, false});
    }
    std::vector<std::string> groups;
    if (product.auxiliary_count) {
        groups = {mainInputGroup, mainOutputGroup};
        for (uint32_t b = 0; b < product.auxiliary_count; ++b) {
            groups.push_back(auxGroupSymbol(b, true));
            groups.push_back(auxGroupSymbol(b, false));
        }
    }
    auto taken = [&](const std::string &symbol) {
        for (const auto &port : ports) if (port.symbol == symbol) return true;
        for (const auto &port : auxiliary) if (port.symbol == symbol) return true;
        for (const auto &group : groups) if (group == symbol) return true;
        return false;
    };
    for (uint32_t i = 0; i < product.parameter_count; ++i) {
        RavetPluginParameterV1 parameter{};
        if (entry.parameter(i, &parameter) != 0) continue;
        Port port{PortKind::Parameter, 0, i, {}, parameter.name ? parameter.name : ""};
        if (int32_t(i) == product.bypass_index) {
            port.kind = PortKind::Enabled;
            port.symbol = "enabled";
            port.name = "Enabled";
        } else {
            if (parameter.flags & RAVET_PLUGIN_PARAMETER_READ_ONLY) port.kind = PortKind::ParameterOut;
            port.symbol = validSymbol(parameter.key) ? parameter.key : "";
            if (port.symbol.empty() || taken(port.symbol) || port.symbol == "enabled")
                port.symbol = "p" + std::to_string(parameter.id);
            while (taken(port.symbol)) port.symbol += "_";
        }
        if (port.name.empty()) port.name = port.symbol;
        ports.push_back(std::move(port));
    }
    for (auto &port : auxiliary) ports.push_back(std::move(port));
    return ports;
}

} // namespace ravet::lv2
