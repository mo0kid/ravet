// RAVET Event Probe: reference VST3 module for hosted rich events.
// Stereo instrument output, one 16-channel event input and one 16-channel
// event output. Every admitted input event (all nine SDK input kinds) and every
// Marker parameter point is rendered by the portable Rust EventProbe as an
// impulse at its exact sample offset: left = code + 1, right = hash20 / 2^20.
// Each admitted input event is echoed unchanged (payload copied) to the event
// output. When at least one event was echoed, one legacy MIDI CC output
// (controller 16, channel 0) follows at the latest echoed offset, carrying the
// number of echoed events (14 bits across value/value2). Marker points, events
// on buses other than 0 and unknown or output-only kinds are not echoed and
// not counted. The read-only Digest parameter reports the running 20-bit
// digest at the last frame (offset 0 in zero-frame blocks), every block.
//
// Admission (per block): at most 2048 input events and at most
// 4096 - events Marker points; event payloads (data bytes, UTF-16 text plus a
// terminator, each 2-aligned) share 65536 + 4096 + 16 bytes of storage.
// Events on buses other than 0 and unknown kinds are ignored before their
// offset is examined; queues other than Marker are ignored unread. Anything
// else malformed (event or point count over capacity or negative, offsets
// outside 0..frames-1 or nonzero in a zero-frame block, null payload with a
// nonzero size, payload overflow, failed getEvent/getPoint, null queue,
// null bus or channel arrays, wrong channel count, frames outside
// 0..maxSamplesPerBlock, mismatched sample size) refuses the block with
// kResultFalse, zeroes the reachable outputs, posts no events and leaves the
// digest unchanged. Exception: the digest is advanced by the Rust render before
// output events and the Digest point are posted, so if the host's output lists
// refuse addEvent/addPoint the block returns kResultFalse with the digest
// already advanced and the events posted so far left in place.
// External SDK sources stay external. Not a production instrument.
#include "RavetVst3Reference.h"
#include "pluginterfaces/vst/ivstevents.h"
#include "pluginterfaces/vst/ivstparameterchanges.h"
#include "public.sdk/source/main/pluginfactory.h"
#include "public.sdk/source/vst/vstaudioeffect.h"
#include "public.sdk/source/vst/vsteditcontroller.h"
#include <algorithm>
#include <array>
#include <cmath>
#include <cstring>
#include <memory>

using namespace Steinberg;
using namespace Steinberg::Vst;
namespace {
const FUID processorID(0xEA756CA8, 0x15B5401C, 0xA6274ACD, 0x04CC1C6E);
const FUID controllerID(0xDEC1B7F1, 0xE2A344E0, 0xAB46B99F, 0xB9A9A103);
constexpr ParamID markerID = 1, digestID = 2;
constexpr size_t maximumEvents = 2048, maximumPoints = 2048;
constexpr size_t payloadCapacity = 65536 + 2 * maximumEvents + 16;
constexpr uint8 countController = 16; // General Purpose 1

struct Storage {
  std::array<RavetEventProbeRecord, maximumEvents + maximumPoints> records{};
  std::array<Event, maximumEvents> echo{};
  alignas(8) std::array<uint8_t, payloadCapacity> payload{};
};

class ProbeProcessor final : public AudioEffect {
public:
  ProbeProcessor() { setControllerClass(controllerID); }
  ~ProbeProcessor() override { ravet_event_probe_destroy(dsp); }
  static FUnknown *create(void *) {
    return static_cast<IAudioProcessor *>(new ProbeProcessor);
  }
  tresult PLUGIN_API initialize(FUnknown *context) override {
    const auto status = AudioEffect::initialize(context);
    if (status != kResultOk)
      return status;
    if (!dsp)
      dsp = ravet_event_probe_create();
    if (!storage)
      storage.reset(new (std::nothrow) Storage);
    if (!dsp || !storage)
      return kResultFalse;
    addAudioOutput(STR16("Probe"), SpeakerArr::kStereo, kMain,
                   BusInfo::kDefaultActive);
    addEventInput(STR16("Events"), 16);
    addEventOutput(STR16("Echo"), 16);
    return kResultOk;
  }
  tresult PLUGIN_API setBusArrangements(SpeakerArrangement *inputs,
                                        int32 inputCount,
                                        SpeakerArrangement *outputs,
                                        int32 outputCount) override {
    if (inputCount != 0 || outputCount != 1 || !outputs ||
        outputs[0] != SpeakerArr::kStereo)
      return kResultFalse;
    return AudioEffect::setBusArrangements(inputs, inputCount, outputs,
                                           outputCount);
  }
  tresult PLUGIN_API canProcessSampleSize(int32 size) override {
    return size == kSample32 || size == kSample64 ? kResultOk : kResultFalse;
  }
  uint32 PLUGIN_API getLatencySamples() override { return 0; }
  uint32 PLUGIN_API getTailSamples() override { return kNoTail; }
  tresult PLUGIN_API setupProcessing(ProcessSetup &setup) override {
    prepared = false;
    if (canProcessSampleSize(setup.symbolicSampleSize) != kResultOk ||
        setup.maxSamplesPerBlock <= 0 || setup.maxSamplesPerBlock > 65536 ||
        !std::isfinite(setup.sampleRate) || setup.sampleRate <= 0. ||
        ravet_event_probe_prepare(dsp, uint32_t(setup.maxSamplesPerBlock)) != 0)
      return kResultFalse;
    return AudioEffect::setupProcessing(setup);
  }
  tresult PLUGIN_API setProcessing(TBool) override { return kResultOk; }
  tresult PLUGIN_API setActive(TBool state) override {
    prepared = state != 0;
    if (state)
      ravet_event_probe_reset(dsp);
    return AudioEffect::setActive(state);
  }
  tresult PLUGIN_API process(ProcessData &data) override {
    const bool wide = data.symbolicSampleSize == kSample64;
    const int32 frames = data.numSamples;
    const auto fail = [&] {
      if (data.numOutputs >= 1 && data.outputs && frames > 0 &&
          frames <= processSetup.maxSamplesPerBlock) {
        auto &out = data.outputs[0];
        for (int32 c = 0; c < out.numChannels; ++c) {
          if (wide && out.channelBuffers64 && out.channelBuffers64[c])
            std::fill_n(out.channelBuffers64[c], frames, 0.);
          if (!wide && out.channelBuffers32 && out.channelBuffers32[c])
            std::fill_n(out.channelBuffers32[c], frames, 0.f);
        }
        out.silenceFlags = 0;
      }
      return kResultFalse;
    };
    if (!prepared || data.symbolicSampleSize != processSetup.symbolicSampleSize ||
        frames < 0 || frames > processSetup.maxSamplesPerBlock ||
        (frames > 0 && (data.numOutputs < 1 || !data.outputs ||
                        data.outputs[0].numChannels != 2)))
      return fail();
    const auto inRange = [&](int32 offset) {
      return frames ? offset >= 0 && offset < frames : offset == 0;
    };
    auto &s = *storage;
    size_t records = 0, echoes = 0, used = 0;
    int32 lastOffset = 0;
    // Copy UTF-16 text (with terminator) or bytes into module-owned storage.
    const auto copy = [&](const void *source, size_t bytes, size_t terminator,
                          const void *&target) {
      used = (used + 1) & ~size_t(1);
      if (bytes + terminator > s.payload.size() - used ||
          (bytes && !source))
        return false;
      if (bytes)
        std::memcpy(s.payload.data() + used, source, bytes);
      std::memset(s.payload.data() + used + bytes, 0, terminator);
      target = s.payload.data() + used;
      used += bytes + terminator;
      return true;
    };
    const int32 count = data.inputEvents ? data.inputEvents->getEventCount() : 0;
    if (count < 0 || size_t(count) > maximumEvents)
      return fail();
    for (int32 i = 0; i < count; ++i) {
      Event e{};
      if (data.inputEvents->getEvent(i, e) != kResultOk)
        return fail();
      if (e.busIndex != 0)
        continue; // Only bus 0 exists; anything else is ignored.
      if (e.type > Event::kNoteExpressionIntValueEvent)
        continue; // Unknown or output-only kinds are ignored before any check.
      if (!inRange(e.sampleOffset))
        return fail();
      RavetEventProbeRecord r{};
      r.offset = uint32_t(e.sampleOffset);
      r.code = e.type;
      r.flags = e.flags;
      r.bus = e.busIndex;
      r.ppq_position = e.ppqPosition;
      Event echo = e;
      const void *target = nullptr;
      switch (e.type) {
      case Event::kNoteOnEvent:
        r.a = e.noteOn.channel, r.b = e.noteOn.pitch, r.c = e.noteOn.noteId;
        r.d = e.noteOn.length, r.f0 = e.noteOn.tuning,
        r.f1 = e.noteOn.velocity;
        break;
      case Event::kNoteOffEvent:
        r.a = e.noteOff.channel, r.b = e.noteOff.pitch,
        r.c = e.noteOff.noteId;
        r.f0 = e.noteOff.tuning, r.f1 = e.noteOff.velocity;
        break;
      case Event::kDataEvent:
        r.a = int32_t(e.data.type);
        if (!copy(e.data.bytes, e.data.size, 0, target))
          return fail();
        r.payload = target, r.payload_count = e.data.size;
        echo.data.bytes = static_cast<const uint8 *>(target);
        break;
      case Event::kPolyPressureEvent:
        r.a = e.polyPressure.channel, r.b = e.polyPressure.pitch;
        r.c = e.polyPressure.noteId, r.f1 = e.polyPressure.pressure;
        break;
      case Event::kNoteExpressionValueEvent:
        r.a = int32_t(e.noteExpressionValue.typeId);
        r.c = e.noteExpressionValue.noteId;
        r.value = e.noteExpressionValue.value;
        break;
      case Event::kNoteExpressionTextEvent:
        r.a = int32_t(e.noteExpressionText.typeId);
        r.c = e.noteExpressionText.noteId;
        if (!copy(e.noteExpressionText.text,
                  size_t(e.noteExpressionText.textLen) * sizeof(TChar),
                  sizeof(TChar), target))
          return fail();
        r.payload = target, r.payload_count = e.noteExpressionText.textLen;
        echo.noteExpressionText.text = static_cast<const TChar *>(target);
        break;
      case Event::kChordEvent:
        r.a = e.chord.root, r.b = e.chord.bassNote;
        r.d = int32_t(uint16_t(e.chord.mask));
        if (!copy(e.chord.text, size_t(e.chord.textLen) * sizeof(TChar),
                  sizeof(TChar), target))
          return fail();
        r.payload = target, r.payload_count = e.chord.textLen;
        echo.chord.text = static_cast<const TChar *>(target);
        break;
      case Event::kScaleEvent:
        r.a = e.scale.root, r.d = int32_t(uint16_t(e.scale.mask));
        if (!copy(e.scale.text, size_t(e.scale.textLen) * sizeof(TChar),
                  sizeof(TChar), target))
          return fail();
        r.payload = target, r.payload_count = e.scale.textLen;
        echo.scale.text = static_cast<const TChar *>(target);
        break;
      case Event::kNoteExpressionIntValueEvent:
        r.a = int32_t(e.noteExpressionIntValue.typeId);
        r.c = e.noteExpressionIntValue.noteId;
        r.integer = e.noteExpressionIntValue.value;
        break;
      default:
        continue; // Unknown or output-only kinds are not probe records.
      }
      s.records[records++] = r;
      s.echo[echoes++] = echo;
      lastOffset = std::max(lastOffset, e.sampleOffset);
    }
    if (auto *changes = data.inputParameterChanges) {
      const int32 queues = changes->getParameterCount();
      if (queues < 0)
        return fail();
      for (int32 q = 0; q < queues; ++q) {
        auto *queue = changes->getParameterData(q);
        if (!queue)
          return fail();
        if (queue->getParameterId() != markerID)
          continue;
        const int32 points = queue->getPointCount();
        if (points < 0 || size_t(points) > s.records.size() - records)
          return fail();
        for (int32 p = 0; p < points; ++p) {
          int32 offset = 0;
          ParamValue value = 0.;
          if (queue->getPoint(p, offset, value) != kResultOk || !inRange(offset))
            return fail();
          RavetEventProbeRecord r{};
          r.offset = uint32_t(offset);
          r.code = 16;
          r.a = int32_t(markerID);
          r.value = value;
          s.records[records++] = r;
        }
      }
    }
    uint32_t digest = 0;
    int32_t status = 1;
    // A null channel array yields null channels, which the render refuses.
    // Zero-frame blocks may carry no output bus at all.
    AudioBusBuffers *bus = frames ? &data.outputs[0] : nullptr;
    const auto channel = [&](auto **buffers, int32 c) {
      return bus && buffers ? buffers[c] : nullptr;
    };
    if (wide)
      status = ravet_event_probe_process64(
          dsp, s.records.data(), uint32_t(records),
          channel(bus ? bus->channelBuffers64 : nullptr, 0),
          channel(bus ? bus->channelBuffers64 : nullptr, 1), uint32_t(frames),
          &digest);
    else
      status = ravet_event_probe_process32(
          dsp, s.records.data(), uint32_t(records),
          channel(bus ? bus->channelBuffers32 : nullptr, 0),
          channel(bus ? bus->channelBuffers32 : nullptr, 1), uint32_t(frames),
          &digest);
    if (status != 0)
      return fail();
    if (frames > 0)
      data.outputs[0].silenceFlags = 0;
    if (data.outputEvents && echoes > 0) {
      for (size_t i = 0; i < echoes; ++i)
        if (data.outputEvents->addEvent(s.echo[i]) != kResultOk)
          return fail();
      Event cc{};
      cc.busIndex = 0;
      cc.sampleOffset = lastOffset;
      cc.type = Event::kLegacyMIDICCOutEvent;
      cc.midiCCOut.controlNumber = countController;
      cc.midiCCOut.channel = 0;
      cc.midiCCOut.value = int8(echoes & 127);
      cc.midiCCOut.value2 = int8((echoes >> 7) & 127);
      if (data.outputEvents->addEvent(cc) != kResultOk)
        return fail();
    }
    if (auto *changes = data.outputParameterChanges) {
      int32 index = 0;
      auto *queue = changes->addParameterData(digestID, index);
      if (!queue ||
          queue->addPoint(frames ? frames - 1 : 0, double(digest) / 1048576.,
                          index) != kResultOk)
        return fail();
    }
    return kResultOk;
  }

private:
  RavetEventProbe *dsp = nullptr;
  std::unique_ptr<Storage> storage;
  bool prepared = false;
};

class ProbeController final : public EditController {
public:
  static FUnknown *create(void *) {
    return static_cast<IEditController *>(new ProbeController);
  }
  tresult PLUGIN_API initialize(FUnknown *context) override {
    const auto status = EditController::initialize(context);
    if (status != kResultOk)
      return status;
    parameters.addParameter(STR16("Marker"), nullptr, 0, 0.,
                            ParameterInfo::kCanAutomate, markerID);
    parameters.addParameter(STR16("Digest"), nullptr, 0, 0.,
                            ParameterInfo::kIsReadOnly, digestID);
    return kResultOk;
  }
};
} // namespace
BEGIN_FACTORY("RAVET", "", "", PFactoryInfo::kNoFlags)
DEF_CLASS2(INLINE_UID_FROM_FUID(processorID), PClassInfo::kManyInstances,
           kVstAudioEffectClass, "RAVET Event Probe", Vst::kDistributable,
           "Instrument", "0.0.1", kVstVersionString, ProbeProcessor::create)
DEF_CLASS2(INLINE_UID_FROM_FUID(controllerID), PClassInfo::kManyInstances,
           kVstComponentControllerClass, "RAVET Event Probe Controller", 0, "",
           "0.0.1", kVstVersionString, ProbeController::create)
END_FACTORY
