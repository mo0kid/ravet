#pragma once
#include "RavetPlugin.h"
#include <stdexcept>

namespace ravet::auv2 {
// Fail at construction, before any host-visible object borrows a partial table.
inline const RavetPluginEntryV1 &checked(const RavetPluginEntryV1 *entry) {
    if (!entry || entry->version != 1 || entry->byte_size < sizeof(*entry) || !entry->product ||
        !entry->parameter || !entry->parameter_index || !entry->value_to_text || !entry->text_to_value ||
        !entry->normalize || !entry->denormalize || !entry->create || !entry->destroy ||
        !entry->get_value || !entry->set_value || !entry->save_state || !entry->load_state ||
        !entry->prepare || !entry->process || !entry->reset || !entry->deactivate ||
        !entry->timing || !entry->latency_changed || !entry->take_edits || !entry->editor_open ||
        !entry->editor_resize || !entry->editor_close || !entry->editor_idle)
        throw std::runtime_error("incomplete ravet_plugin_v1 table");
    const auto &p = *entry->product;
    if (!p.name || !p.vendor || !p.version || !p.plugin_id || !p.layouts || !p.layout_count ||
        p.layout_count > 1024 || p.parameter_count > 1024 || p.category > RAVET_PLUGIN_MIDI_EFFECT ||
        p.bypass_index < -1 || p.bypass_index >= int32_t(p.parameter_count))
        throw std::runtime_error("invalid RAVET product");
    if (p.au_type != 0x61756678 && p.au_type != 0x61756d75 && p.au_type != 0x61756d66)
        throw std::runtime_error("AUv2 supports aufx, aumu and aumf product types");
    for (uint32_t i = 0; i < p.layout_count; ++i)
        if (p.layouts[i].inputs > 64 || !p.layouts[i].outputs || p.layouts[i].outputs > 64 ||
            bool(p.layouts[i].inputs) != bool(p.layouts[0].inputs))
            throw std::runtime_error("AUv2 requires 0..64 input and 1..64 output channels");
    return *entry;
}
} // namespace ravet::auv2
