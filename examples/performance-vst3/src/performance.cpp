// Public identified-performance reference. External SDK sources stay external.
#include "RavetNativeClient.hpp"
#include "RavetPerformance.h"
#include "RavetVst3Audio.hpp"
#include "RavetVst3ParameterInput.hpp"
#include "RavetVst3Parameters.hpp"
#include "RavetVst3PerformanceInput.hpp"
#include "RavetVst3Transport.hpp"
#include "pluginterfaces/base/ibstream.h"
#include "public.sdk/source/main/pluginfactory.h"
#include "public.sdk/source/vst/vstaudioeffect.h"
#include "public.sdk/source/vst/vsteditcontroller.h"
#include <algorithm>
#include <array>
#include <atomic>
#include <charconv>
#include <cmath>
#include <cstring>
#include <limits>

using namespace Steinberg;
using namespace Steinberg::Vst;
namespace {
const FUID processorID(0xCF831A21, 0x64C94718, 0xA66CC589, 0x3461309B);
const FUID controllerID(0xDF831A21, 0x64C94718, 0xA66CC589, 0x3461309B);
constexpr ParamID levelID = 1;
constexpr size_t maximumEvents = 1024;
constexpr const char *restoreMessage = "ravet.performance.restore.v1";

// CONTROL-owned codec instance; schema/codecs never run on the audio callback.
class LevelCodec final {
public:
  LevelCodec()
      : table(ravet_performance_parameters_v1(32)),
        core(table ? table->processor_entry() : nullptr),
        owner(core ? core->create() : nullptr) {}
  ~LevelCodec() {
    if (owner)
      core->destroy(owner);
  }
  LevelCodec(const LevelCodec &) = delete;
  LevelCodec &operator=(const LevelCodec &) = delete;
  bool valid() const {
    return owner && table->state_bytes == 24 && table->parameter_count == 1;
  }
  bool decode(const uint8_t *bytes, size_t count, float &value) {
    RavetNativeParameterValueV1 state{};
    if (!valid() || count != 24 ||
        table->load_state(owner, bytes, uint32_t(count)) != 0 ||
        table->snapshot(owner, &state, 1) != 0 || state.id != levelID)
      return false;
    value = state.value;
    return true;
  }
  bool encode(float value, std::array<uint8_t, 24> &bytes) {
    if (!valid() || !ravet::native::scalar::finite(value) || value < 0.f ||
        value > 1.f)
      return false;
    // Canonical schema codec builds the header/default, then substitutes the
    // known complete physical level through its validated decoder.
    std::array<uint8_t, 24> candidate{};
    if (table->save_state(owner, candidate.data(), 24) != 0)
      return false;
    uint32_t bits;
    std::memcpy(&bits, &value, 4);
    for (unsigned i = 0; i < 4; ++i)
      candidate[20 + i] = uint8_t(bits >> (8 * i));
    float verified = 0.f;
    if (!decode(candidate.data(), candidate.size(), verified) ||
        table->save_state(owner, candidate.data(), 24) != 0)
      return false;
    bytes = candidate;
    return true;
  }
  bool read(IBStream *stream, float &value) {
    if (!stream)
      return false;
    std::array<uint8_t, 25> bytes{};
    size_t used = 0;
    while (used < bytes.size()) {
      int32 got = 0;
      const auto status =
          stream->read(bytes.data() + used, int32(bytes.size() - used), &got);
      if (got < 0 || size_t(got) > bytes.size() - used ||
          (status != kResultOk && status != kResultFalse))
        return false;
      used += size_t(got);
      if (!got)
        break;
    }
    return decode(bytes.data(), used, value);
  }
  bool write(IBStream *stream, float value) {
    std::array<uint8_t, 24> bytes{};
    if (!stream || !encode(value, bytes))
      return false;
    size_t used = 0;
    while (used < bytes.size()) {
      int32 written = 0;
      if (stream->write(bytes.data() + used, int32(bytes.size() - used),
                        &written) != kResultOk ||
          written <= 0 || size_t(written) > bytes.size() - used)
        return false;
      used += size_t(written);
    }
    return true;
  }

private:
  const RavetNativeParameterEntryV1 *table = nullptr;
  const RavetNativeProcessorEntryV1 *core = nullptr;
  void *owner = nullptr;
};
uint64_t pack(float value, uint32_t generation) {
  uint32_t bits;
  std::memcpy(&bits, &value, 4);
  return uint64_t(bits) | (uint64_t(generation) << 32);
}
float unpack(uint64_t word) {
  uint32_t bits = uint32_t(word);
  float value;
  std::memcpy(&value, &bits, 4);
  return value;
}
static_assert(std::atomic<uint64_t>::is_always_lock_free,
              "reference requires lock-free64-bit publication");

class PerformanceProcessor final : public AudioEffect {
public:
  PerformanceProcessor()
      : dsp32(ravet_performance_processor_v1(32),
              ravet_performance_audio_state_v1(32)),
        dsp64(ravet_performance_processor_v1(64),
              ravet_performance_audio_state_v1(64)),
        cache32(ravet_performance_cached_parameters_v1(32)),
        cache64(ravet_performance_cached_parameters_v1(64)) {
    setControllerClass(controllerID);
  }
  static FUnknown *create(void *) {
    return static_cast<IAudioProcessor *>(new PerformanceProcessor);
  }
  tresult PLUGIN_API initialize(FUnknown *context) override {
    const auto status = AudioEffect::initialize(context);
    if (status != kResultOk || !codec.valid() || !dsp32.valid() ||
        !dsp64.valid() || !cache32.valid() || !cache64.valid())
      return kResultFalse;
    addAudioOutput(STR16("Output"), SpeakerArr::kStereo);
    addEventInput(STR16("Performance"), 16);
    return kResultOk;
  }
  tresult PLUGIN_API setBusArrangements(SpeakerArrangement *inputs,
                                        int32 inputCount,
                                        SpeakerArrangement *outputs,
                                        int32 outputCount) override {
    if (inputCount != 0 || outputCount != 1 || !outputs ||
        (outputs[0] != SpeakerArr::kMono && outputs[0] != SpeakerArr::kStereo))
      return kResultFalse;
    const auto status = AudioEffect::setBusArrangements(inputs, inputCount,
                                                        outputs, outputCount);
    if (status == kResultOk) {
      const auto next = SpeakerArr::getChannelCount(outputs[0]);
      if (next != channels)
        prepared = false;
      channels = next;
    }
    return status;
  }
  tresult PLUGIN_API canProcessSampleSize(int32 size) override {
    return size == kSample32 || size == kSample64 ? kResultOk : kResultFalse;
  }
  uint32 PLUGIN_API getProcessContextRequirements() override {
    return ravet::vst3::nativeTransportRequirements;
  }
  tresult PLUGIN_API setupProcessing(ProcessSetup &setup) override {
    prepared = false;
    dsp32.deactivate();
    dsp64.deactivate();
    if (canProcessSampleSize(setup.symbolicSampleSize) != kResultOk ||
        setup.maxSamplesPerBlock <= 0 || setup.maxSamplesPerBlock > 65536 ||
        !ravet::native::scalar::finite(setup.sampleRate) ||
        setup.sampleRate <= 0.)
      return kResultFalse;
    size = setup.symbolicSampleSize;
    RavetNativeProcessorConfigV1 config{};
    config.version = 1;
    config.maximum_frames = uint32_t(setup.maxSamplesPerBlock);
    config.maximum_events = 1024;
    config.bus_count = 1;
    config.sample_rate = setup.sampleRate;
    config.buses[0] = {0, uint32_t(channels)};
    // VST3 has no NoteEnd representation. No generated event bus is declared;
    // this consumer explicitly suppresses completion output, never sends Off.
    if (!audioMap.configure(config, size == kSample64 ? 64u : 32u) ||
        selected().prepare(config) != 0)
      return kResultFalse;
    const auto status = AudioEffect::setupProcessing(setup);
    if (status != kResultOk) {
      selected().deactivate();
      return status;
    }
    pending = true;
    prepared = true;
    return kResultOk;
  }
  tresult PLUGIN_API setProcessing(TBool active) override {
    if (!active) {
      dsp32.reset();
      dsp64.reset();
    }
    return kResultOk;
  }
  uint32 PLUGIN_API getTailSamples() override { return kInfiniteTail; }
  tresult PLUGIN_API setState(IBStream *stream) override {
    float value;
    if (!codec.read(stream, value))
      return kResultFalse;
    publish(value);
    return kResultOk;
  }
  tresult PLUGIN_API getState(IBStream *stream) override {
    return codec.write(stream,
                       unpack(publication.load(std::memory_order_acquire)))
               ? kResultOk
               : kResultFalse;
  }
  tresult PLUGIN_API notify(IMessage *message) override {
    if (!message || !message->getMessageID())
      return kInvalidArgument;
    if (std::strcmp(message->getMessageID(), restoreMessage))
      return AudioEffect::notify(message);
    auto *attributes = message->getAttributes();
    const void *bytes = nullptr;
    uint32 count = 0;
    float value;
    if (!attributes ||
        attributes->getBinary("state", bytes, count) != kResultOk || !bytes ||
        !codec.decode(static_cast<const uint8_t *>(bytes), count, value))
      return kResultFalse;
    publish(value);
    return kResultOk;
  }
  tresult PLUGIN_API process(ProcessData &data) override {
    std::array<RavetNativeProcessorBusV1, 9> buses{};
    const auto fail = [&] {
      audioMap.silenceSupportedOutputs(data);
      return kResultFalse;
    };
    if (!prepared || !audioMap.map(data, buses))
      return fail();
    RavetNativeTransportV1 transport{};
    if (!ravet::vst3::nativeTransport(data.processContext, transport))
      return fail();
    struct Tagged {
      RavetNativeEventV1 event;
      size_t order;
    };
    std::array<Tagged, maximumEvents> tagged{};
    size_t count = 0;
    const auto append = [&](const RavetNativeEventV1 &event) {
      if (count == tagged.size())
        return false;
      tagged[count] = {event, count};
      ++count;
      return true;
    };
    const auto requested = publication.load(std::memory_order_acquire);
    const auto generation = uint32_t(requested >> 32);
    const bool recall = pending || requested != accepted;
    if (recall) {
      RavetNativeEventV1 value{};
      value.kind = RAVET_NATIVE_PARAMETER_VALUE;
      value.parameter_id = levelID;
      value.value = double(unpack(requested));
      if (!append(value))
        return fail();
    }
    const auto &cache = size == kSample64 ? cache64 : cache32;
    if (!ravet::vst3::visitValidatedGlobalParameters(
            data.inputParameterChanges, data.numSamples,
            [&](const auto &point) {
              float physical = 0.f;
              const auto status =
                  cache.map(point.id, point.normalized, physical, false);
              if (status == ravet::vst3::ParameterInputResult::Rejected)
                return false;
              if (status == ravet::vst3::ParameterInputResult::Unknown)
                return true;
              RavetNativeEventV1 event{};
              event.kind = RAVET_NATIVE_PARAMETER_VALUE;
              event.parameter_id = point.id;
              event.offset = point.offset;
              event.value = double(physical);
              return append(event);
            }))
      return fail();
    if (!ravet::vst3::visitValidatedPerformanceInput(
            data.inputEvents, 0, data.numSamples,
            ravet::vst3::UnknownExpressionPolicy::Reject,
            [&](const auto &event) { return append(event); }))
      return fail();
    std::sort(tagged.begin(), tagged.begin() + count,
              [](const auto &a, const auto &b) {
                return a.event.offset < b.event.offset ||
                       (a.event.offset == b.event.offset && a.order < b.order);
              });
    std::array<RavetNativeEventV1, maximumEvents> events{};
    for (size_t index = 0; index < count; ++index)
      events[index] = tagged[index].event;
    RavetNativeProcessorResultV1 result{};
    // Recall is an ordered event inside transactional DSP, so late semantic
    // failure cannot consume a pending restore or change global level/history.
    if (selected().process(buses.data(), 1, uint32_t(data.numSamples),
                           events.data(), uint32_t(count), &transport, nullptr,
                           0, result) != 0 ||
        result.status == 2)
      return fail();
    RavetNativeParameterValueV1 base{};
    if (selected().snapshotState(&base, 1) != 0 || base.id != levelID)
      return fail();
    auto expected = requested;
    const auto reflected = pack(base.value, generation);
    const bool current = publication.compare_exchange_strong(
        expected, reflected, std::memory_order_acq_rel,
        std::memory_order_acquire);
    accepted = current ? reflected : requested;
    pending = false;
    if (data.numSamples > 0) {
      auto &output = data.outputs[0];
      uint64 flags = 0;
      for (int32 channel = 0; channel < channels; ++channel) {
        bool silent = true;
        for (int32 frame = 0; frame < data.numSamples; ++frame) {
          if (size == kSample64
                  ? output.channelBuffers64[channel][frame] != 0.
                  : output.channelBuffers32[channel][frame] != 0.f) {
            silent = false;
            break;
          }
        }
        if (silent)
          flags |= uint64(1) << channel;
      }
      output.silenceFlags = flags;
    }
    return kResultOk;
  }

private:
  ravet::native::NativeProcessorClient dsp32, dsp64;
  ravet::vst3::CachedParameterInput cache32, cache64;
  ravet::vst3::NativeAudioBusMap audioMap;
  LevelCodec codec;
  std::atomic<uint64_t> publication{pack(.2f, 0)};
  uint64_t accepted = 0;
  bool pending = true, prepared = false;
  int32 channels = 2, size = kSample32;
  ravet::native::NativeProcessorClient &selected() {
    return size == kSample64 ? dsp64 : dsp32;
  }
  void publish(float value) {
    auto old = publication.load(std::memory_order_acquire);
    while (!publication.compare_exchange_weak(
        old, pack(value, uint32_t(old >> 32) + 1u), std::memory_order_acq_rel,
        std::memory_order_acquire)) {
    }
  }
};

bool validEndpoint(int32 bus, int16 channel) {
  return bus == 0 && channel >= 0 && channel < 16;
}
bool advertised(NoteExpressionTypeID id) { return id <= kExpressionTypeID; }
bool ascii(const TChar *text, std::array<char, 128> &out, size_t &used) {
  if (!text)
    return false;
  used = 0;
  while (used < out.size()) {
    const auto value = text[used];
    if (!value)
      return used > 0;
    if (uint32_t(value) > 127)
      return false;
    out[used++] = char(value);
  }
  return false;
}
class PerformanceController final : public EditController,
                                    public INoteExpressionController {
public:
  static FUnknown *create(void *) {
    return static_cast<IEditController *>(new PerformanceController);
  }
  tresult PLUGIN_API initialize(FUnknown *context) override {
    const auto status = EditController::initialize(context);
    return status == kResultOk && codec.valid() &&
                   ravet::vst3::registerParameters(
                       parameters, ravet_performance_parameters_v1(32))
               ? kResultOk
               : kResultFalse;
  }
  tresult PLUGIN_API setParamNormalized(ParamID id, ParamValue value) override {
    if (!ravet::native::scalar::finite(value) || value < 0. || value > 1.)
      return kResultFalse;
    return EditController::setParamNormalized(id, value);
  }
  tresult PLUGIN_API setComponentState(IBStream *stream) override {
    float value;
    if (!codec.read(stream, value))
      return kResultFalse;
    return setParamNormalized(levelID, value);
  }
  tresult PLUGIN_API getState(IBStream *stream) override {
    return codec.write(stream, float(getParamNormalized(levelID)))
               ? kResultOk
               : kResultFalse;
  }
  tresult PLUGIN_API setState(IBStream *stream) override {
    float value;
    if (!codec.read(stream, value))
      return kResultFalse;
    setParamNormalized(levelID, value);
    pending = true;
    synchronize();
    return kResultOk;
  }
  tresult PLUGIN_API connect(IConnectionPoint *other) override {
    const auto status = EditController::connect(other);
    if (status == kResultOk && pending)
      synchronize();
    return status;
  }
  int32 PLUGIN_API getNoteExpressionCount(int32 bus, int16 channel) override {
    return validEndpoint(bus, channel) ? 5 : 0;
  }
  tresult PLUGIN_API
  getNoteExpressionInfo(int32 bus, int16 channel, int32 index,
                        NoteExpressionTypeInfo &out) override {
    if (!validEndpoint(bus, channel) || index < 0 || index >= 5)
      return kResultFalse;
    const TChar *titles[] = {STR16("Volume"), STR16("Pan"), STR16("Tuning"),
                             STR16("Vibrato"), STR16("Expression")};
    NoteExpressionTypeInfo next{};
    next.typeId = NoteExpressionTypeID(index);
    next.unitId = kRootUnitId;
    next.associatedParameterId = kNoParamId;
    next.flags = NoteExpressionTypeInfo::kIsAbsolute;
    UString(next.title, 128).assign(titles[index]);
    UString(next.shortTitle, 128).assign(titles[index]);
    const double defaults[] = {.25, .5, .5, 0., 1.};
    next.valueDesc = {defaults[index], 0., 1., 0};
    if (index == 0)
      UString(next.units, 128).assign(STR16("dB"));
    if (index == 2 || index == 3)
      UString(next.units, 128).assign(STR16("st"));
    if (index == 1 || index == 2)
      next.flags |= NoteExpressionTypeInfo::kIsBipolar;
    out = next;
    return kResultOk;
  }
  tresult PLUGIN_API getNoteExpressionStringByValue(int32 bus, int16 channel,
                                                    NoteExpressionTypeID id,
                                                    double value,
                                                    String128 out) override {
    if (!validEndpoint(bus, channel) || !advertised(id) ||
        !ravet::native::scalar::finite(value) || value < 0. || value > 1.)
      return kResultFalse;
    String128 next{};
    if (id == kVolumeTypeID && value == 0.) {
      UString(next, 128).assign(STR16("-inf"));
      std::copy_n(next, 128, out);
      return kResultOk;
    }
    const double physical = id == kVolumeTypeID   ? 20. * std::log10(4. * value)
                            : id == kTuningTypeID ? 240. * (value - .5)
                            : id == kPanTypeID    ? 2. * value - 1.
                                                  : value;
    std::array<char, 128> text{};
    const auto result = std::to_chars(
        text.data(), text.data() + 127, physical, std::chars_format::general,
        std::numeric_limits<double>::max_digits10);
    if (result.ec != std::errc{})
      return kResultFalse;
    for (size_t i = 0; i < size_t(result.ptr - text.data()); ++i)
      next[i] = TChar(text[i]);
    std::copy_n(next, 128, out);
    return kResultOk;
  }
  tresult PLUGIN_API getNoteExpressionValueByString(int32 bus, int16 channel,
                                                    NoteExpressionTypeID id,
                                                    const TChar *text,
                                                    double &out) override {
    if (!validEndpoint(bus, channel) || !advertised(id))
      return kResultFalse;
    std::array<char, 128> bytes{};
    size_t used = 0;
    if (!ascii(text, bytes, used))
      return kResultFalse;
    if (id == kVolumeTypeID && used == 4 &&
        std::memcmp(bytes.data(), "-inf", 4) == 0) {
      out = 0.;
      return kResultOk;
    }
    double physical = 0.;
    const auto result = std::from_chars(bytes.data(), bytes.data() + used,
                                        physical, std::chars_format::general);
    if (result.ec != std::errc{} || result.ptr != bytes.data() + used ||
        !ravet::native::scalar::finite(physical))
      return kResultFalse;
    const double maximumDb = 20. * std::log10(4.);
    if (id == kVolumeTypeID && physical > maximumDb)
      return kResultFalse;
    if (id == kVolumeTypeID && physical == maximumDb) {
      out = 1.;
      return kResultOk;
    }
    const double value = id == kVolumeTypeID
                             ? std::pow(10., physical / 20.) / 4.
                         : id == kTuningTypeID ? physical / 240. + .5
                         : id == kPanTypeID    ? (physical + 1.) / 2.
                                               : physical;
    if (!ravet::native::scalar::finite(value) || value < 0. || value > 1. ||
        (id == kVolumeTypeID && value == 0.))
      return kResultFalse;
    out = value;
    return kResultOk;
  }
  OBJ_METHODS(PerformanceController, EditController)
  DEFINE_INTERFACES
  DEF_INTERFACE(INoteExpressionController)
  END_DEFINE_INTERFACES(EditController)
  REFCOUNT_METHODS(EditController)
private:
  LevelCodec codec;
  bool pending = false;
  void synchronize() {
    if (!getPeer())
      return;
    auto message = owned(allocateMessage());
    std::array<uint8_t, 24> bytes{};
    if (!message || !codec.encode(float(getParamNormalized(levelID)), bytes))
      return;
    message->setMessageID(restoreMessage);
    auto *attributes = message->getAttributes();
    if (attributes &&
        attributes->setBinary("state", bytes.data(), 24) == kResultOk &&
        sendMessage(message) == kResultOk)
      pending = false;
  }
};
} // namespace
BEGIN_FACTORY("RAVET", "", "", PFactoryInfo::kNoFlags)
DEF_CLASS2(INLINE_UID_FROM_FUID(processorID), PClassInfo::kManyInstances,
           kVstAudioEffectClass, "RAVET Performance Sine", Vst::kDistributable,
           "Instrument|Synth", "0.0.1", kVstVersionString,
           PerformanceProcessor::create)
DEF_CLASS2(INLINE_UID_FROM_FUID(controllerID), PClassInfo::kManyInstances,
           kVstComponentControllerClass, "RAVET Performance Sine Controller", 0,
           "", "0.0.1", kVstVersionString, PerformanceController::create)
END_FACTORY
