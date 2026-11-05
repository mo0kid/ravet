// Generic RAVET LV2 plug-in (DSP side), written once against RavetPlugin.h.
//
// The module loads the product library copied beside it in the bundle
// (ravet_product.{so,dylib,dll}), reads its RavetPluginEntryV1 table and exposes
// one LV2 descriptor whose URI, ports and state come from that table. The
// port layout is shared with the Turtle generator (RavetLv2Product.hpp).
//
// run() never allocates, locks or blocks: every buffer is sized at
// instantiate/activate. Denormal handling is inside the table's process.
#include "RavetLv2Product.hpp"
#include "RavetNativeMidiOutput.hpp"

#include "lv2/atom/atom.h"
#include "lv2/atom/util.h"
#include "lv2/buf-size/buf-size.h"
#include "lv2/core/lv2.h"
#include "lv2/midi/midi.h"
#include "lv2/options/options.h"
#include "lv2/state/state.h"
#include "lv2/time/time.h"
#include "lv2/urid/urid.h"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <limits>
#include <memory>
#include <mutex>
#include <string>
#include <vector>

namespace {
using namespace ravet::lv2;

constexpr uint32_t maxEvents = 1024;          // RavetNativeProcessor event limit
constexpr uint32_t defaultBlock = 4096;        // without bufsz options
constexpr uint32_t maxBlockLimit = 65536;
constexpr uint32_t maxPositions = 32;          // time:Position objects per run

struct Library {
    const RavetPluginEntryV1 *entry = nullptr;
    std::string error;
    LV2_Descriptor descriptor{};
    std::string uri;
};

Library &library();

struct Uris {
    LV2_URID atomSequence, atomChunk, atomObject, atomBlank, atomInt, atomLong, atomFloat, atomDouble,
        atomFrameTime, midiEvent, timePosition, timeFrame, timeSpeed, timeBeat, timeBar, timeBarBeat,
        timeBeatsPerBar, timeBeatUnit, timeBpm, bufMax, bufNominal, state;
};

// Host time:Position values are arbitrary finite doubles. Converting one that is
// out of range to an integer is undefined behaviour in C++, so every
// host-derived integer goes through this saturating conversion (NaN → 0).
template <class T> T saturate(double value) {
    if (!(value == value)) return 0;
    if (value <= double(std::numeric_limits<T>::min())) return std::numeric_limits<T>::min();
    // 2^63 and 2^31 are exact doubles; anything at or above them saturates.
    if (value >= -2.0 * double(std::numeric_limits<T>::min())) return std::numeric_limits<T>::max();
    return T(value);
}

// Host transport as last reported by time:Position, advanced between reports.
struct Clock {
    bool known = false, hasFrame = false, hasSpeed = false, hasBeat = false, hasTempo = false,
         hasBar = false, hasBarBeat = false, hasBeatsPerBar = false, hasBeatUnit = false;
    int64_t frame = 0, bar = 0;
    double speed = 0, beat = 0, tempo = 0, barBeat = 0, beatsPerBar = 0;
    int32_t beatUnit = 0;

    void advance(uint32_t frames, double rate) {
        if (!known || !hasSpeed || speed == 0.0) return;
        // Saturating: a host may report any finite speed, and int64 overflow is UB.
        if (hasFrame) frame = saturate<int64_t>(double(frame) + std::round(double(frames) * speed));
        if (!hasTempo || !(rate > 0)) return;
        const double beats = double(frames) / rate * tempo / 60.0 * speed;
        if (hasBeat) beat += beats;
        if (hasBarBeat) {
            barBeat += beats;
            if (hasBeatsPerBar && beatsPerBar > 0 && barBeat >= beatsPerBar) {
                const double bars = std::floor(barBeat / beatsPerBar);
                barBeat -= bars * beatsPerBar;
                if (hasBar) bar = saturate<int64_t>(double(bar) + bars);
            }
        }
    }

    bool snapshot(RavetNativeTransportV1 &out) const {
        if (!known) return false;
        out = RavetNativeTransportV1{};
        if (hasSpeed) {
            out.present |= RAVET_TRANSPORT_PLAYING;
            if (speed != 0.0) out.flags |= RAVET_TRANSPORT_FLAG_PLAYING;
        }
        if (hasFrame) { out.present |= RAVET_TRANSPORT_SAMPLES; out.sample_position = frame; }
        if (hasBeat && std::isfinite(beat)) { out.present |= RAVET_TRANSPORT_BEATS; out.beat_position = beat; }
        if (hasTempo && std::isfinite(tempo) && tempo > 0) { out.present |= RAVET_TRANSPORT_TEMPO; out.tempo = tempo; }
        if (hasBeatsPerBar && hasBeatUnit && beatsPerBar >= 1 && beatsPerBar <= 65535 &&
            std::trunc(beatsPerBar) == beatsPerBar && beatUnit >= 1 && beatUnit <= 65535) {
            // Recording, loop and pre-roll are unknown in LV2 time: their DETAILS
            // flags stay at the default false and must not be read as host facts.
            out.present |= RAVET_TRANSPORT_DETAILS | RAVET_TRANSPORT_SIGNATURE;
            out.signature_numerator = uint32_t(beatsPerBar);
            out.signature_denominator = uint32_t(beatUnit);
        }
        if ((out.present & RAVET_TRANSPORT_BEATS) && hasBar && hasBarBeat && bar >= INT32_MIN && bar <= INT32_MAX &&
            std::isfinite(barBeat)) {
            out.present |= RAVET_TRANSPORT_BAR;
            out.bar_number = int32_t(bar);
            out.bar_start = beat - barBeat;
        }
        return out.present != 0;
    }
};

struct Parameter {
    uint32_t port, index, id;
    bool enabled, output;
    double minimum, maximum;
    float last;
    bool seen;
};

struct Instance {
    const RavetPluginEntryV1 *entry = nullptr;
    void *plugin = nullptr;
    double rate = 0;
    uint32_t maxFrames = defaultBlock;
    bool prepared = false;
    Uris uris{};
    LV2_URID_Map *map = nullptr;  // optional: without it, Atom ports and state are unused
    uint32_t inputs = 0, outputs = 0;
    std::vector<Port> ports;
    std::vector<void *> data;
    std::vector<uint32_t> audioIn, audioOut;
    std::vector<Parameter> parameters;
    int32_t eventsIn = -1, midiOut = -1, latency = -1;
    std::vector<const float *> inPointers;
    std::vector<float *> outPointers;
    std::vector<float> silence, discard;
    std::vector<RavetNativeEventV1> events, output, midi;
    uint32_t midiCount = 0;
    struct Position { uint32_t offset; Clock clock; };
    std::array<Position, maxPositions> positions{};
    uint32_t positionCount = 0;
    Clock clock;
    bool baseline = false;  // after restore: take port values as given
};

const LV2_Feature *findFeature(const LV2_Feature *const *features, const char *uri) {
    for (auto f = features; f && *f; ++f) if (!std::strcmp((*f)->URI, uri)) return *f;
    return nullptr;
}

LV2_Handle instantiate(const LV2_Descriptor *, double rate, const char *, const LV2_Feature *const *features) {
    const auto *entry = library().entry;
    if (!entry || !(rate > 0)) return nullptr;
    const auto *mapFeature = findFeature(features, LV2_URID__map);
    std::unique_ptr<Instance> self;
    try {
        self = std::make_unique<Instance>();
        self->entry = entry;
        self->rate = rate;
        self->map = mapFeature ? static_cast<LV2_URID_Map *>(mapFeature->data) : nullptr;
        auto map = [&](const char *uri) { return self->map ? self->map->map(self->map->handle, uri) : LV2_URID(0); };
        auto &u = self->uris;
        u.atomSequence = map(LV2_ATOM__Sequence); u.atomChunk = map(LV2_ATOM__Chunk);
        u.atomObject = map(LV2_ATOM__Object); u.atomBlank = map(LV2_ATOM__Blank);
        u.atomInt = map(LV2_ATOM__Int); u.atomLong = map(LV2_ATOM__Long);
        u.atomFloat = map(LV2_ATOM__Float); u.atomDouble = map(LV2_ATOM__Double);
        u.atomFrameTime = map(LV2_ATOM__frameTime); u.midiEvent = map(LV2_MIDI__MidiEvent);
        u.timePosition = map(LV2_TIME__Position); u.timeFrame = map(LV2_TIME__frame);
        u.timeSpeed = map(LV2_TIME__speed); u.timeBeat = map(LV2_TIME__beat);
        u.timeBar = map(LV2_TIME__bar); u.timeBarBeat = map(LV2_TIME__barBeat);
        u.timeBeatsPerBar = map(LV2_TIME__beatsPerBar); u.timeBeatUnit = map(LV2_TIME__beatUnit);
        u.timeBpm = map(LV2_TIME__beatsPerMinute); u.bufMax = map(LV2_BUF_SIZE__maxBlockLength);
        u.bufNominal = map(LV2_BUF_SIZE__nominalBlockLength); u.state = map(stateKey);

        const auto *optionsFeature = self->map ? findFeature(features, LV2_OPTIONS__options) : nullptr;
        if (optionsFeature) {
            uint32_t nominal = 0, maximum = 0;
            for (auto *o = static_cast<const LV2_Options_Option *>(optionsFeature->data); o && o->key; ++o) {
                if (o->type != u.atomInt || o->size != sizeof(int32_t) || !o->value) continue;
                const int32_t value = *static_cast<const int32_t *>(o->value);
                if (value <= 0) continue;
                if (o->key == u.bufMax) maximum = uint32_t(value);
                else if (o->key == u.bufNominal) nominal = uint32_t(value);
            }
            if (maximum) self->maxFrames = maximum;
            else if (nominal) self->maxFrames = nominal;
            self->maxFrames = std::min(self->maxFrames, maxBlockLimit);
        }

        const auto &layout = entry->product->layouts[0];
        self->inputs = layout.inputs;
        self->outputs = layout.outputs;
        self->ports = portLayout(*entry);
        self->data.assign(self->ports.size(), nullptr);
        for (uint32_t i = 0; i < self->ports.size(); ++i) {
            const auto &port = self->ports[i];
            switch (port.kind) {
            case PortKind::AudioIn: self->audioIn.push_back(i); break;
            case PortKind::AudioOut: self->audioOut.push_back(i); break;
            case PortKind::EventsIn: self->eventsIn = int32_t(i); break;
            case PortKind::MidiOut: self->midiOut = int32_t(i); break;
            case PortKind::Latency: self->latency = int32_t(i); break;
            case PortKind::Parameter: case PortKind::Enabled: case PortKind::ParameterOut: {
                RavetPluginParameterV1 info{};
                entry->parameter(port.parameter, &info);
                self->parameters.push_back({i, port.parameter, info.id, port.kind == PortKind::Enabled,
                                            port.kind == PortKind::ParameterOut, info.minimum, info.maximum, 0.f, false});
                break;
            }
            }
        }
        self->inPointers.assign(self->inputs, nullptr);
        self->outPointers.assign(self->outputs, nullptr);
        self->events.assign(maxEvents, RavetNativeEventV1{});
        self->output.assign(maxEvents, RavetNativeEventV1{});
        self->midi.assign(maxEvents, RavetNativeEventV1{});
        self->plugin = entry->create();
        if (!self->plugin) return nullptr;
    } catch (...) {
        return nullptr;
    }
    return self.release();
}

void connectPort(LV2_Handle handle, uint32_t port, void *data) {
    auto *self = static_cast<Instance *>(handle);
    if (port < self->data.size()) self->data[port] = data;
}

void activate(LV2_Handle handle) {
    auto *self = static_cast<Instance *>(handle);
    try {
        self->silence.assign(self->maxFrames, 0.f);
        self->discard.assign(self->maxFrames, 0.f);
    } catch (...) {
        self->prepared = false;
        return;
    }
    RavetNativeProcessorConfigV1 config{};
    config.version = 1;
    config.maximum_frames = self->maxFrames;
    config.maximum_events = maxEvents;
    config.maximum_output_events = self->entry->product->note_output ? maxEvents : 0;
    config.bus_count = 1;
    config.sample_rate = self->rate;
    config.buses[0] = {self->inputs, self->outputs};
    self->prepared = self->entry->prepare(self->plugin, &config) == 0;
    self->clock = Clock{};
    // Resend every connected control value after activation.
    for (auto &parameter : self->parameters) if (!self->baseline) parameter.seen = false;
}

void deactivate(LV2_Handle handle) {
    auto *self = static_cast<Instance *>(handle);
    if (self->prepared) self->entry->deactivate(self->plugin);
    self->prepared = false;
}

double number(const Uris &u, const LV2_Atom *atom, bool &ok) {
    ok = true;
    if (atom->type == u.atomFloat && atom->size >= sizeof(float)) return reinterpret_cast<const LV2_Atom_Float *>(atom)->body;
    if (atom->type == u.atomDouble && atom->size >= sizeof(double)) return reinterpret_cast<const LV2_Atom_Double *>(atom)->body;
    if (atom->type == u.atomInt && atom->size >= sizeof(int32_t)) return reinterpret_cast<const LV2_Atom_Int *>(atom)->body;
    if (atom->type == u.atomLong && atom->size >= sizeof(int64_t)) return double(reinterpret_cast<const LV2_Atom_Long *>(atom)->body);
    ok = false;
    return 0;
}

// Apply one time:Position object to the clock. Fields absent from the object
// keep their previous values, as LV2 hosts send only what they know.
void applyPosition(const Uris &u, const LV2_Atom_Object *object, Clock &clock) {
    const LV2_Atom *frame = nullptr, *speed = nullptr, *beat = nullptr, *bar = nullptr, *barBeat = nullptr,
                   *beatsPerBar = nullptr, *beatUnit = nullptr, *bpm = nullptr;
    lv2_atom_object_get(object, u.timeFrame, &frame, u.timeSpeed, &speed, u.timeBeat, &beat, u.timeBar, &bar,
                        u.timeBarBeat, &barBeat, u.timeBeatsPerBar, &beatsPerBar, u.timeBeatUnit, &beatUnit,
                        u.timeBpm, &bpm, 0);
    bool ok = false;
    double value = 0;
    clock.known = true;
    if (frame && (value = number(u, frame, ok), ok)) {
        clock.hasFrame = true;
        clock.frame = frame->type == u.atomLong ? reinterpret_cast<const LV2_Atom_Long *>(frame)->body : saturate<int64_t>(value);
    }
    if (speed && (value = number(u, speed, ok), ok) && std::isfinite(value)) { clock.hasSpeed = true; clock.speed = value; }
    if (beat && (value = number(u, beat, ok), ok) && std::isfinite(value)) { clock.hasBeat = true; clock.beat = value; }
    if (bar && (value = number(u, bar, ok), ok) && std::isfinite(value)) { clock.hasBar = true; clock.bar = saturate<int64_t>(value); }
    if (barBeat && (value = number(u, barBeat, ok), ok) && std::isfinite(value)) { clock.hasBarBeat = true; clock.barBeat = value; }
    if (beatsPerBar && (value = number(u, beatsPerBar, ok), ok) && std::isfinite(value)) { clock.hasBeatsPerBar = true; clock.beatsPerBar = value; }
    if (beatUnit && (value = number(u, beatUnit, ok), ok) && std::isfinite(value)) { clock.hasBeatUnit = true; clock.beatUnit = saturate<int32_t>(value); }
    if (bpm && (value = number(u, bpm, ok), ok) && std::isfinite(value) && value > 0) { clock.hasTempo = true; clock.tempo = value; }
}

uint32_t midiLength(uint8_t status) { return ravet::native::midi_output_detail::length(status); }

// Collect MIDI and time:Position events from the input sequence. Malformed
// MIDI (SysEx, running status, wrong length) is skipped, not fatal.
void readEvents(Instance &self, uint32_t frames) {
    self.midiCount = 0;
    self.positionCount = 0;
    if (!self.map || self.eventsIn < 0 || !self.data[size_t(self.eventsIn)]) return;
    const auto *sequence = static_cast<const LV2_Atom_Sequence *>(self.data[size_t(self.eventsIn)]);
    const auto &u = self.uris;
    if (sequence->atom.type != u.atomSequence || sequence->atom.size < sizeof(LV2_Atom_Sequence_Body)) return;
    if (sequence->body.unit && sequence->body.unit != u.atomFrameTime) return;  // beat time unsupported
    const bool notes = self.entry->product->note_input != 0;
    uint32_t previous = 0;
    LV2_ATOM_SEQUENCE_FOREACH(sequence, event) {
        int64_t at = event->time.frames;
        uint32_t offset = at < 0 ? 0 : (frames && at >= int64_t(frames) ? frames - 1 : uint32_t(at));
        if (!frames) offset = 0;
        offset = std::max(offset, previous);
        previous = offset;
        if (event->body.type == u.midiEvent) {
            if (!notes || event->body.size < 1 || event->body.size > 3 || self.midiCount == maxEvents) continue;
            const auto *bytes = reinterpret_cast<const uint8_t *>(LV2_ATOM_BODY_CONST(&event->body));
            const uint32_t size = event->body.size;
            if (midiLength(bytes[0]) != size) continue;
            bool valid = true;
            uint32_t word = 0;
            for (uint32_t i = 0; i < size; ++i) {
                if (i && bytes[i] >= 0x80) valid = false;
                word |= uint32_t(bytes[i]) << (8 * i);
            }
            if (!valid) continue;
            RavetNativeEventV1 midi{};
            midi.offset = offset;
            midi.kind = RAVET_NATIVE_MIDI;
            midi.detail = size;
            midi.words[0] = word;
            self.midi[self.midiCount++] = midi;
        } else if (event->body.type == u.atomObject || event->body.type == u.atomBlank) {
            const auto *object = reinterpret_cast<const LV2_Atom_Object *>(&event->body);
            if (event->body.size < sizeof(LV2_Atom_Object_Body) || object->body.otype != u.timePosition) continue;
            Clock clock = self.positionCount ? self.positions[self.positionCount - 1].clock : self.clock;
            applyPosition(u, object, clock);
            if (self.positionCount && self.positions[self.positionCount - 1].offset == offset)
                self.positions[self.positionCount - 1].clock = clock;
            else if (self.positionCount < maxPositions)
                self.positions[self.positionCount++] = {offset, clock};
            else
                self.positions[maxPositions - 1].clock = clock;
        }
    }
}

struct MidiWriter {
    LV2_Atom_Sequence *sequence = nullptr;
    uint32_t capacity = 0;
    void begin(void *port, const Uris &u) {
        sequence = static_cast<LV2_Atom_Sequence *>(port);
        if (!sequence) return;
        capacity = sequence->atom.size;  // host-provided buffer size in bytes
        if (capacity < sizeof(LV2_Atom_Sequence)) { sequence = nullptr; return; }
        sequence->atom.type = u.atomSequence;
        sequence->atom.size = sizeof(LV2_Atom_Sequence_Body);
        sequence->body.unit = u.atomFrameTime;
        sequence->body.pad = 0;
    }
    void write(uint32_t frame, const uint8_t *bytes, uint32_t size, LV2_URID type) {
        if (!sequence) return;
        const uint32_t used = sizeof(LV2_Atom) + sequence->atom.size;
        const uint32_t needed = uint32_t(sizeof(LV2_Atom_Event)) + lv2_atom_pad_size(size);
        if (capacity < used || capacity - used < needed) return;
        auto *event = reinterpret_cast<LV2_Atom_Event *>(reinterpret_cast<uint8_t *>(sequence) + used);
        event->time.frames = frame;
        event->body.type = type;
        event->body.size = size;
        auto *body = reinterpret_cast<uint8_t *>(event + 1);
        std::memset(body, 0, lv2_atom_pad_size(size));
        std::memcpy(body, bytes, size);
        sequence->atom.size += needed;
    }
};

void silenceOutputs(Instance &self, uint32_t start, uint32_t frames) {
    for (auto port : self.audioOut)
        if (auto *out = static_cast<float *>(self.data[port])) std::fill(out + start, out + start + frames, 0.f);
}

void run(LV2_Handle handle, uint32_t frames) {
    auto *self = static_cast<Instance *>(handle);
    const auto *entry = self->entry;
    MidiWriter writer;
    if (self->map && self->midiOut >= 0) writer.begin(self->data[size_t(self->midiOut)], self->uris);

    // Control ports: changed values become parameter events at offset 0.
    uint32_t controls = 0;
    for (auto &parameter : self->parameters) {
        auto *port = static_cast<float *>(self->data[parameter.port]);
        if (!port) continue;
        if (parameter.output) continue;
        const float value = *port;
        if (!std::isfinite(value)) continue;
        if (self->baseline) { parameter.last = value; parameter.seen = true; continue; }
        if (parameter.seen && value == parameter.last) continue;
        parameter.last = value;
        parameter.seen = true;
        if (controls == maxEvents) continue;
        RavetNativeEventV1 event{};
        event.kind = RAVET_NATIVE_PARAMETER_VALUE;
        event.parameter_id = parameter.id;
        event.value = parameter.enabled ? (value < 0.5f ? parameter.maximum : parameter.minimum) : double(value);
        self->events[controls++] = event;
    }
    self->baseline = false;

    readEvents(*self, frames);

    if (!self->prepared) {
        if (frames) silenceOutputs(*self, 0, frames);
    } else {
        uint32_t start = 0, nextMidi = 0, nextPosition = 0;
        bool first = true;
        while (first || start < frames) {
            // Positions at or before this chunk's start set the clock.
            while (nextPosition < self->positionCount && self->positions[nextPosition].offset <= start)
                self->clock = self->positions[nextPosition++].clock;
            uint32_t end = frames ? std::min(frames, start + self->maxFrames) : 0;
            if (nextPosition < self->positionCount) end = std::min(end, self->positions[nextPosition].offset);
            const uint32_t length = end - start;

            uint32_t count = first ? controls : 0;
            while (nextMidi < self->midiCount && (self->midi[nextMidi].offset < end || (!frames)) && count < maxEvents) {
                auto event = self->midi[nextMidi++];
                event.offset -= start;
                self->events[count++] = event;
            }
            // Events beyond the native capacity of 1024 per block are dropped.
            while (nextMidi < self->midiCount && (self->midi[nextMidi].offset < end || !frames)) ++nextMidi;
            if (length || count) {
                for (uint32_t c = 0; c < self->inputs; ++c) {
                    const auto *in = static_cast<const float *>(self->data[self->audioIn[c]]);
                    self->inPointers[c] = length ? (in ? in + start : self->silence.data()) : nullptr;
                }
                for (uint32_t c = 0; c < self->outputs; ++c) {
                    auto *out = static_cast<float *>(self->data[self->audioOut[c]]);
                    self->outPointers[c] = length ? (out ? out + start : self->discard.data()) : nullptr;
                }
                RavetNativeProcessorBusV1 bus{self->inputs, self->outputs,
                                              self->inputs ? static_cast<const void *>(self->inPointers.data()) : nullptr,
                                              static_cast<const void *>(self->outPointers.data())};
                RavetNativeTransportV1 transport{};
                const bool hasTransport = self->clock.snapshot(transport);
                RavetNativeProcessorResultV1 result{};
                const uint32_t capacity = self->midiOut >= 0 ? maxEvents : 0;
                const int32_t code = entry->process(self->plugin, &bus, 1, length, count ? self->events.data() : nullptr, count,
                                                    hasTransport ? &transport : nullptr,
                                                    capacity ? self->output.data() : nullptr, capacity, &result);
                if (code != 0 || result.status != 0) {
                    if (length) silenceOutputs(*self, start, length);
                } else if (self->midiOut >= 0) {
                    for (uint32_t i = 0; i < result.event_count && i < capacity; ++i) {
                        ravet::native::NativeMidiMessage message{};
                        if (ravet::native::nativeMidiOutput(self->output[i], length, message))
                            writer.write(start + message.offset, message.bytes.data(), message.size, self->uris.midiEvent);
                    }
                }
            }
            self->clock.advance(length, self->rate);
            first = false;
            if (!frames) break;
            start = end;
        }
    }

    // Read-only parameters report the value after this block.
    for (const auto &parameter : self->parameters)
        if (parameter.output)
            if (auto *port = static_cast<float *>(self->data[parameter.port]))
                *port = float(entry->get_value(self->plugin, parameter.index));

    if (self->latency >= 0) {
        if (auto *port = static_cast<float *>(self->data[size_t(self->latency)])) {
            RavetNativeProcessorTimingV1 timing{};
            *port = self->prepared && entry->timing(self->plugin, &timing) == 0 ? float(timing.latency_samples) : 0.f;
        }
    }
}

void cleanup(LV2_Handle handle) {
    auto *self = static_cast<Instance *>(handle);
    if (self->prepared) self->entry->deactivate(self->plugin);
    self->entry->destroy(self->plugin);
    delete self;
}

LV2_State_Status save(LV2_Handle handle, LV2_State_Store_Function store, LV2_State_Handle state, uint32_t,
                      const LV2_Feature *const *) {
    auto *self = static_cast<Instance *>(handle);
    if (!self->map) return LV2_STATE_ERR_NO_FEATURE;
    uint32_t length = 0;
    if (self->entry->save_state(self->plugin, nullptr, 0, &length) != 0) return LV2_STATE_ERR_UNKNOWN;
    std::vector<uint8_t> bytes;
    try { bytes.resize(length); } catch (...) { return LV2_STATE_ERR_UNKNOWN; }
    uint32_t written = 0;
    if (self->entry->save_state(self->plugin, bytes.data(), length, &written) != 0 || written > length)
        return LV2_STATE_ERR_UNKNOWN;
    return store(state, self->uris.state, bytes.data(), written, self->uris.atomChunk,
                 LV2_STATE_IS_POD | LV2_STATE_IS_PORTABLE);
}

LV2_State_Status restore(LV2_Handle handle, LV2_State_Retrieve_Function retrieve, LV2_State_Handle state, uint32_t,
                         const LV2_Feature *const *) {
    auto *self = static_cast<Instance *>(handle);
    if (!self->map) return LV2_STATE_ERR_NO_FEATURE;
    size_t size = 0;
    uint32_t type = 0, flags = 0;
    const void *value = retrieve(state, self->uris.state, &size, &type, &flags);
    if (!value) return LV2_STATE_SUCCESS;  // no saved envelope: control ports carry the values
    if (type != self->uris.atomChunk) return LV2_STATE_ERR_BAD_TYPE;
    if (size > std::numeric_limits<uint32_t>::max()) return LV2_STATE_ERR_UNKNOWN;
    if (self->entry->load_state(self->plugin, static_cast<const uint8_t *>(value), uint32_t(size)) != 0)
        return LV2_STATE_ERR_UNKNOWN;
    // Control ports still holding pre-restore values must not undo the restore:
    // the values present at the next run become the baseline.
    self->baseline = true;
    return LV2_STATE_SUCCESS;
}

const void *extensionData(const char *uri) {
    static const LV2_State_Interface stateInterface{save, restore};
    if (!std::strcmp(uri, LV2_STATE__interface)) return &stateInterface;
    return nullptr;
}

Library &library() {
    static Library instance;
    static std::once_flag once;
    std::call_once(once, [] {
        const std::string directory = moduleDirectory(reinterpret_cast<const void *>(&extensionData));
        instance.entry = loadProduct(directory + productFile, instance.error);
        if (!instance.entry) {
            std::fprintf(stderr, "RAVET LV2: %s\n", instance.error.c_str());
            return;
        }
        instance.uri = instance.entry->product->lv2_uri;
        instance.descriptor = {instance.uri.c_str(), instantiate, connectPort, activate, run, deactivate, cleanup,
                               extensionData};
    });
    return instance;
}

} // namespace

LV2_SYMBOL_EXPORT const LV2_Descriptor *lv2_descriptor(uint32_t index) {
    // Loading allocates (paths, error text) and std::call_once may throw; no
    // C++ exception may reach the C host.
    try {
        auto &loaded = library();
        return index == 0 && loaded.entry ? &loaded.descriptor : nullptr;
    } catch (...) {
        return nullptr;
    }
}
