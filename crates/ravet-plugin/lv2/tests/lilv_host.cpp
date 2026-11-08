// Independent host check through lilv (the reference LV2 host library):
// discovery and verification of the generated Turtle, designations, latency,
// UI discovery, processing, and a state round trip through lilv's state API
// (lilv_state_new_from_instance → Turtle string → lilv_state_restore).
//
//   ravet_lv2_lilv_host <bundle directory> [--mock]
#include <lilv/lilv.h>
#include <lv2/atom/atom.h>
#include <lv2/core/lv2.h>
#include <lv2/port-groups/port-groups.h>
#include <lv2/state/state.h>
#include <lv2/ui/ui.h>
#include <lv2/urid/urid.h>

#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

static int failures = 0, checks = 0;
#define CHECK(condition) do { ++checks; if (!(condition)) { ++failures; std::fprintf(stderr, "FAIL %s:%d: %s\n", __FILE__, __LINE__, #condition); } } while (0)

struct Uris {
    std::vector<std::string> names{""};
    LV2_URID map(const char *uri) {
        for (size_t i = 1; i < names.size(); ++i) if (names[i] == uri) return LV2_URID(i);
        names.push_back(uri);
        return LV2_URID(names.size() - 1);
    }
} uris;

struct Host {
    const LilvPlugin *plugin;
    std::vector<float> controls;
};

static const void *getPort(const char *symbol, void *data, uint32_t *size, uint32_t *type) {
    auto *host = static_cast<Host *>(data);
    LilvWorld *world = nullptr;
    (void)world;
    for (uint32_t i = 0; i < lilv_plugin_get_num_ports(host->plugin); ++i) {
        const LilvPort *port = lilv_plugin_get_port_by_index(host->plugin, i);
        if (!std::strcmp(lilv_node_as_string(lilv_port_get_symbol(host->plugin, port)), symbol) &&
            !std::isnan(host->controls[i])) {
            *size = sizeof(float);
            *type = uris.map(LV2_ATOM__Float);
            return &host->controls[i];
        }
    }
    *size = 0;
    *type = 0;
    return nullptr;
}

static void setPort(const char *symbol, void *data, const void *value, uint32_t size, uint32_t type) {
    auto *host = static_cast<Host *>(data);
    if (size != sizeof(float) || type != uris.map(LV2_ATOM__Float)) return;
    for (uint32_t i = 0; i < lilv_plugin_get_num_ports(host->plugin); ++i) {
        const LilvPort *port = lilv_plugin_get_port_by_index(host->plugin, i);
        if (!std::strcmp(lilv_node_as_string(lilv_port_get_symbol(host->plugin, port)), symbol))
            std::memcpy(&host->controls[i], value, sizeof(float));
    }
}

int main(int argc, char **argv) {
    if (argc < 2) { std::fprintf(stderr, "usage: ravet_lv2_lilv_host <bundle> [--mock]\n"); return 2; }
    const bool mock = argc > 2 && std::string(argv[2]) == "--mock";
    std::string path = argv[1];
    if (path.back() != '/') path += '/';

    LilvWorld *world = lilv_world_new();
    LilvNode *bundle = lilv_new_file_uri(world, nullptr, path.c_str());
    lilv_world_load_specifications(world);
    lilv_world_load_bundle(world, bundle);
    const LilvPlugins *plugins = lilv_world_get_all_plugins(world);
    CHECK(lilv_plugins_size(plugins) == 1);
    const LilvPlugin *plugin = nullptr;
    LILV_FOREACH (plugins, i, plugins) plugin = lilv_plugins_get(plugins, i);
    if (!plugin) { std::fprintf(stderr, "no plug-in found\n"); return 1; }
    CHECK(lilv_plugin_verify(plugin));
    std::printf("plug-in %s (%s), %u ports\n", lilv_node_as_uri(lilv_plugin_get_uri(plugin)),
                lilv_node_as_string(lilv_plugin_get_name(plugin)), lilv_plugin_get_num_ports(plugin));

    LilvNode *inputPort = lilv_new_uri(world, LV2_CORE__InputPort);
    LilvNode *outputPort = lilv_new_uri(world, LV2_CORE__OutputPort);
    LilvNode *audioPort = lilv_new_uri(world, LV2_CORE__AudioPort);
    LilvNode *controlPort = lilv_new_uri(world, LV2_CORE__ControlPort);
    LilvNode *atomPort = lilv_new_uri(world, LV2_ATOM__AtomPort);
    LilvNode *enabled = lilv_new_uri(world, LV2_CORE__enabled);
    CHECK(lilv_plugin_has_latency(plugin));
    // Side-chain ports (auxiliary input-only buses) are optional audio inputs
    // in a port group that is pg:sideChainOf another group. Every group a port
    // names has a symbol and a label.
    {
        LilvNode *sideChain = lilv_new_uri(world, LV2_CORE_PREFIX "isSideChain");
        LilvNode *optional = lilv_new_uri(world, LV2_CORE__connectionOptional);
        LilvNode *group = lilv_new_uri(world, LV2_PORT_GROUPS__group);
        LilvNode *sideChainOf = lilv_new_uri(world, LV2_PORT_GROUPS__sideChainOf);
        LilvNode *symbol = lilv_new_uri(world, LV2_CORE__symbol);
        LilvNode *label = lilv_new_uri(world, LILV_NS_RDFS "label");
        uint32_t sideChains = 0, grouped = 0;
        for (uint32_t i = 0; i < lilv_plugin_get_num_ports(plugin); ++i) {
            const LilvPort *port = lilv_plugin_get_port_by_index(plugin, i);
            LilvNodes *groups = lilv_port_get_value(plugin, port, group);
            const LilvNode *portGroup = groups && lilv_nodes_size(groups) ? lilv_nodes_get_first(groups) : nullptr;
            if (portGroup) {
                ++grouped;
                LilvNode *groupSymbol = lilv_world_get(world, portGroup, symbol, nullptr);
                LilvNode *groupLabel = lilv_world_get(world, portGroup, label, nullptr);
                CHECK(groupSymbol && groupLabel);
                std::printf("port %u in group %s (%s)\n", i, groupSymbol ? lilv_node_as_string(groupSymbol) : "?",
                            groupLabel ? lilv_node_as_string(groupLabel) : "?");
                lilv_node_free(groupSymbol);
                lilv_node_free(groupLabel);
            }
            if (lilv_port_has_property(plugin, port, sideChain)) {
                ++sideChains;
                CHECK(lilv_port_is_a(plugin, port, audioPort) && lilv_port_is_a(plugin, port, inputPort) &&
                      lilv_port_has_property(plugin, port, optional));
                LilvNode *main = portGroup ? lilv_world_get(world, portGroup, sideChainOf, nullptr) : nullptr;
                CHECK(main != nullptr);
                lilv_node_free(main);
            }
            lilv_nodes_free(groups);
        }
        std::printf("side-chain ports: %u, grouped ports: %u\n", sideChains, grouped);
        for (LilvNode *node : {sideChain, optional, group, sideChainOf, symbol, label}) lilv_node_free(node);
    }
    const LilvPort *enabledPort = lilv_plugin_get_port_by_designation(plugin, inputPort, enabled);

    // UI discovery.
    LilvUIs *uis = lilv_plugin_get_uis(plugin);
    std::printf("UIs: %u\n", lilv_uis_size(uis));
    LILV_FOREACH (uis, i, uis) {
        const LilvUI *ui = lilv_uis_get(uis, i);
        const LilvNodes *classes = lilv_ui_get_classes(ui);
        LILV_FOREACH (nodes, c, classes) std::printf("  %s %s\n", lilv_node_as_uri(lilv_ui_get_uri(ui)), lilv_node_as_uri(lilv_nodes_get(classes, c)));
    }

    const uint32_t count = lilv_plugin_get_num_ports(plugin);
    std::vector<float> minimum(count), maximum(count), defaults(count);
    lilv_plugin_get_port_ranges_float(plugin, minimum.data(), maximum.data(), defaults.data());
    Host host{plugin, std::vector<float>(count, NAN)};
    const uint32_t frames = 512;
    std::vector<std::vector<float>> audio;
    std::vector<std::vector<uint64_t>> atoms;
    LV2_URID_Map map{&uris, [](LV2_URID_Map_Handle, const char *uri) { return uris.map(uri); }};
    LV2_URID_Unmap unmap{&uris, [](LV2_URID_Unmap_Handle, LV2_URID id) {
                             return id < uris.names.size() ? uris.names[id].c_str() : static_cast<const char *>(nullptr);
                         }};
    LV2_Feature mapFeature{LV2_URID__map, &map}, unmapFeature{LV2_URID__unmap, &unmap};
    const LV2_Feature *features[] = {&mapFeature, &unmapFeature, nullptr};
    LilvInstance *instance = lilv_plugin_instantiate(plugin, 48000.0, features);
    CHECK(instance != nullptr);
    if (!instance) return 1;
    audio.reserve(count);
    atoms.reserve(count);
    int32_t gainPort = -1, firstIn = -1, firstOut = -1;
    for (uint32_t i = 0; i < count; ++i) {
        const LilvPort *port = lilv_plugin_get_port_by_index(plugin, i);
        const std::string symbol = lilv_node_as_string(lilv_port_get_symbol(plugin, port));
        if (lilv_port_is_a(plugin, port, audioPort)) {
            audio.emplace_back(frames, 0.f);
            if (lilv_port_is_a(plugin, port, inputPort)) {
                if (firstIn < 0) firstIn = int32_t(audio.size() - 1);
                for (uint32_t f = 0; f < frames; ++f) audio.back()[f] = 0.5f;
            } else if (firstOut < 0) firstOut = int32_t(audio.size() - 1);
            lilv_instance_connect_port(instance, i, audio.back().data());
        } else if (lilv_port_is_a(plugin, port, controlPort)) {
            host.controls[i] = std::isnan(defaults[i]) ? 0.f : defaults[i];
            if (symbol == "gain") gainPort = int32_t(i);
            lilv_instance_connect_port(instance, i, &host.controls[i]);
        } else if (lilv_port_is_a(plugin, port, atomPort)) {
            atoms.emplace_back(1024, 0);
            auto *sequence = reinterpret_cast<LV2_Atom_Sequence *>(atoms.back().data());
            if (lilv_port_is_a(plugin, port, inputPort)) {
                sequence->atom.type = uris.map(LV2_ATOM__Sequence);
                sequence->atom.size = sizeof(LV2_Atom_Sequence_Body);
            } else {
                sequence->atom.size = uint32_t(atoms.back().size() * 8);
            }
            lilv_instance_connect_port(instance, i, sequence);
        }
    }
    lilv_instance_activate(instance);
    lilv_instance_run(instance, frames);
    bool finite = true;
    for (auto &channel : audio) for (float v : channel) finite &= std::isfinite(v);
    CHECK(finite);

    if (mock) {
        CHECK(enabledPort != nullptr && gainPort >= 0 && firstOut >= 0);
        if (gainPort >= 0) host.controls[size_t(gainPort)] = -6.f;
        lilv_instance_run(instance, frames);
        CHECK(std::fabs(audio[size_t(firstOut)][100] - 0.5f * std::pow(10.f, -6.f / 20.f)) < 1e-5f);
    }

    // State round trip through lilv.
    LilvState *state = lilv_state_new_from_instance(plugin, instance, &map, nullptr, nullptr, nullptr, nullptr, getPort,
                                                    &host, LV2_STATE_IS_POD | LV2_STATE_IS_PORTABLE, nullptr);
    CHECK(state != nullptr);
    char *text = state ? lilv_state_to_string(world, &map, &unmap, state, "urn:ravet:lv2:test-state", nullptr) : nullptr;
    CHECK(text && std::strstr(text, "urn:ravet:lv2:state"));
    if (mock && gainPort >= 0) {
        host.controls[size_t(gainPort)] = 0.f;  // changed after saving
        lilv_instance_run(instance, frames);
    }
    if (text) {
        LilvState *loaded = lilv_state_new_from_string(world, &map, text);
        CHECK(loaded != nullptr);
        if (loaded) {
            lilv_state_restore(loaded, instance, setPort, &host, 0, features);
            if (mock && gainPort >= 0) {
                CHECK(host.controls[size_t(gainPort)] == -6.f);  // port value restored by lilv
                lilv_instance_run(instance, frames);
                CHECK(std::fabs(audio[size_t(firstOut)][100] - 0.5f * std::pow(10.f, -6.f / 20.f)) < 1e-5f);
            }
            lilv_state_free(loaded);
        }
        lilv_free(text);
    }
    if (state) lilv_state_free(state);

    lilv_instance_deactivate(instance);
    lilv_instance_free(instance);
    lilv_uis_free(uis);
    for (LilvNode *node : {inputPort, outputPort, audioPort, controlPort, atomPort, enabled, bundle}) lilv_node_free(node);
    lilv_world_free(world);
    std::printf("ravet_lv2_lilv_host%s: %d checks, %d failures\n", mock ? " (mock)" : "", checks, failures);
    return failures ? 1 : 0;
}
