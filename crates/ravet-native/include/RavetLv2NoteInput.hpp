#pragma once
#include "RavetNativeEvent.h"
#include "lv2/atom/atom.h"
#include <array>
#include <cstring>
#include <limits>
namespace ravet::native {
// External-SDK MIDI1 short-note adapter. Foreign Sequence storage must be live,
// 8-byte aligned and cover its declared body; output/count remain disjoint and
// live. Whole bounded sequence preflight precedes any output/count mutation.
// Default/explicit audio-frame timestamps only; same-frame order is preserved.
// NoteOn velocity0 uses MIDI1 NoteOff convention. Native NoteOn otherwise keeps
// the exact widened f32(byte/127); no per-note identity is invented. NoteOff
// selects channel+key. Defined non-note short MIDI and other bounded Atom bodies
// are ignored; running status, undefined statuses, SysEx and long MIDI reject.
class Lv2NoteInput final {
public:
    static constexpr uint32_t maximumBodyBytes = 65536, maximumEvents = 1024;
    static bool convert(const LV2_Atom_Sequence *sequence, uint32_t sequenceType,
                        uint32_t midiType, uint32_t frameTimeType, uint32_t frames,
                        RavetNativeEventV1 *output, uint32_t capacity, uint32_t &count) {
        if (!sequence || reinterpret_cast<uintptr_t>(sequence) % 8 || !sequenceType || !midiType || !frameTimeType ||
            frames > 65536 || capacity > maximumEvents || (capacity && !output)) return false;
        LV2_Atom_Sequence header{}; std::memcpy(&header, sequence, sizeof(header));
        if (header.atom.type != sequenceType || header.atom.size < sizeof(header.body) || header.atom.size > maximumBodyBytes ||
            (header.body.unit && header.body.unit != frameTimeType) || header.body.pad) return false;
        const auto address = reinterpret_cast<uintptr_t>(sequence);
        const uint64_t total = sizeof(LV2_Atom) + uint64_t(header.atom.size);
        if (total > std::numeric_limits<uintptr_t>::max() - address) return false;
        const auto *bytes = reinterpret_cast<const uint8_t *>(sequence);
        uint64_t cursor = sizeof(header); uint32_t rawCount = 0, notes = 0, previous = 0;
        std::array<RavetNativeEventV1, maximumEvents> staged{};
        while (cursor < total) {
            if (++rawCount > maximumEvents || total - cursor < sizeof(LV2_Atom_Event)) return false;
            LV2_Atom_Event event{}; std::memcpy(&event, bytes + cursor, sizeof(event));
            const uint64_t payload = event.body.size;
            const uint64_t padded = (payload + 7) & ~uint64_t(7);
            if (padded > total - cursor - sizeof(event) || event.time.frames < 0 ||
                (frames ? uint64_t(event.time.frames) >= frames : event.time.frames != 0)) return false;
            const auto offset = static_cast<uint32_t>(event.time.frames);
            if (rawCount > 1 && offset < previous) return false;
            previous = offset;
            if (event.body.type == midiType) {
                if (payload == 0 || payload > 3) return false;
                const auto *midi = bytes + cursor + sizeof(event);
                const auto status = midi[0];
                uint32_t length = 0;
                if (status >= 0x80 && status < 0xf0) length = (status & 0xe0) == 0xc0 ? 2 : 3;
                else if (status == 0xf1 || status == 0xf3) length = 2;
                else if (status == 0xf2) length = 3;
                else if (status == 0xf6 || status == 0xf8 || status == 0xfa || status == 0xfb || status == 0xfc || status == 0xfe || status == 0xff) length = 1;
                if (payload != length) return false;
                for (uint32_t i = 1; i < length; ++i) if (midi[i] >= 0x80) return false;
                if ((status & 0xf0) == 0x80 || (status & 0xf0) == 0x90) {
                    if (notes == capacity) return false;
                    auto &note = staged[notes++]; note.offset = offset;
                    note.channel = status & 15; note.key = midi[1];
                    note.value = static_cast<double>(static_cast<float>(midi[2]) / 127.f);
                    if ((status & 0xf0) == 0x90 && midi[2]) note.kind = RAVET_NATIVE_NOTE_ON;
                    else { note.kind = RAVET_NATIVE_NOTE_OFF_SCOPED; note.scope = RAVET_NATIVE_SCOPE_CHANNEL | RAVET_NATIVE_SCOPE_KEY; }
                }
            }
            cursor += sizeof(event) + padded;
        }
        if (cursor != total) return false;
        for (uint32_t i = 0; i < notes; ++i) output[i] = staged[i];
        count = notes; return true;
    }
};
} // namespace ravet::native
