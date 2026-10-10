// Actual public native archive through the SDK reference; the dynamic SDK
// validator separately loads the built VST3 bundle.
#include "../../examples/performance-vst3/src/performance.cpp"
#include "public.sdk/source/common/memorystream.h"
#include "public.sdk/source/vst/hosting/hostclasses.h"
#include "public.sdk/source/vst/hosting/parameterchanges.h"
#include <cstdio>
#include <cstdlib>
#include <new>
#include <stdexcept>
#include <type_traits>

namespace {
bool measuring = false;
size_t allocations = 0, frees = 0;
} // namespace
void *operator new(size_t bytes) {
  if (measuring)
    ++allocations;
  if (auto *p = std::malloc(bytes ? bytes : 1))
    return p;
  throw std::bad_alloc();
}
void *operator new[](size_t bytes) { return ::operator new(bytes); }
void operator delete(void *p) noexcept {
  if (p && measuring)
    ++frees;
  std::free(p);
}
void operator delete[](void *p) noexcept { ::operator delete(p); }
void operator delete(void *p, size_t) noexcept { ::operator delete(p); }
void require(bool condition, const char *message) {
  if (!condition)
    throw std::runtime_error(message);
}
class PerformanceList final : public IEventList {
public:
  std::array<Event, 1025> events{};
  int32 count = 0, failure = -1;
  tresult PLUGIN_API queryInterface(const TUID, void **out) override {
    if (out)
      *out = nullptr;
    return kNoInterface;
  }
  uint32 PLUGIN_API addRef() override { return 1; }
  uint32 PLUGIN_API release() override { return 1; }
  int32 PLUGIN_API getEventCount() override { return count; }
  tresult PLUGIN_API getEvent(int32 index, Event &out) override {
    if (index < 0 || index >= count || index >= 1025 || index == failure)
      return kResultFalse;
    out = events[size_t(index)];
    return kResultOk;
  }
  tresult PLUGIN_API addEvent(Event &) override { return kResultFalse; }
};
Event performanceNote(bool on, int32 id, int16 key = 69, int32 offset = 0) {
  Event e{};
  e.sampleOffset = offset;
  e.type = on ? Event::kNoteOnEvent : Event::kNoteOffEvent;
  if (on)
    e.noteOn = {3, key, 0.f, .12345679f, 0, id};
  else
    e.noteOff = {3, key, .7f, id, 0.f};
  return e;
}
Event performanceExpression(NoteExpressionTypeID id, double value, int32 noteID,
                            int32 offset = 0) {
  Event e{};
  e.sampleOffset = offset;
  e.type = Event::kNoteExpressionValueEvent;
  e.noteExpressionValue = {id, noteID, value};
  return e;
}
void restorePerformance(PerformanceProcessor &processor, float value) {
  LevelCodec codec;
  std::array<uint8_t, 24> bytes{};
  require(codec.encode(value, bytes), "state encode");
  MemoryStream stream(bytes.data(), int32(bytes.size()));
  require(processor.setState(&stream) == kResultOk, "processor recall");
}
float performanceSnapshot(PerformanceProcessor &processor) {
  MemoryStream stream;
  LevelCodec codec;
  float value = -1;
  require(processor.getState(&stream) == kResultOk &&
              codec.decode(reinterpret_cast<const uint8_t *>(stream.getData()),
                           size_t(stream.getSize()), value),
          "processor snapshot");
  return value;
}
void controllerContracts(HostApplication &host) {
  PerformanceController controller;
  require(controller.initialize(&host) == kResultOk, "controller initialize");
  require(controller.getParameterCount() == 1 &&
              controller.getParamNormalized(levelID) == double(.2f),
          "level default");
  void *expressionInterface = nullptr;
  require(controller.queryInterface(INoteExpressionController::iid,
                                    &expressionInterface) == kResultOk &&
              expressionInterface != nullptr,
          "note-expression interface");
  static_cast<INoteExpressionController *>(expressionInterface)->release();
  for (int16 channel = 0; channel < 16; ++channel) {
    require(controller.getNoteExpressionCount(0, channel) == 5,
            "five audible expressions for every channel");
    for (int32 index = 0; index < 5; ++index) {
      NoteExpressionTypeInfo info{};
      require(controller.getNoteExpressionInfo(0, channel, index, info) ==
                      kResultOk &&
                  info.typeId == uint32(index) &&
                  info.valueDesc.minimum == 0. &&
                  info.valueDesc.maximum == 1. &&
                  info.associatedParameterId == kNoParamId &&
                  (info.flags & NoteExpressionTypeInfo::kIsAbsolute),
              "expression metadata");
      for (double value : {0., .25, .5, 1.}) {
        String128 text{};
        double recalled = -1;
        require(controller.getNoteExpressionStringByValue(
                    0, channel, uint32(index), value, text) == kResultOk &&
                    controller.getNoteExpressionValueByString(
                        0, channel, uint32(index), text, recalled) ==
                        kResultOk &&
                    std::abs(recalled - value) < 1e-14,
                "expression text endpoints/roundtrip");
      }
    }
  }
  require(controller.getNoteExpressionCount(1, 0) == 0 &&
              controller.getNoteExpressionCount(0, -1) == 0 &&
              controller.getNoteExpressionCount(0, 16) == 0,
          "expression endpoint bounds");
  NoteExpressionTypeInfo untouched{};
  untouched.typeId = 999;
  require(controller.getNoteExpressionInfo(0, 0, 5, untouched) ==
                  kResultFalse &&
              untouched.typeId == 999,
          "unadvertised brightness metadata untouched");
  for (const TChar *text : {STR16("nan"), STR16("inf"), STR16(" 0"),
                            STR16("0x"), STR16("+0"), STR16("-100000")}) {
    double out = 91.;
    require(controller.getNoteExpressionValueByString(0, 0, kVolumeTypeID, text,
                                                      out) == kResultFalse &&
                out == 91.,
            "malformed volume text leaves output untouched");
  }
  String128 text{};
  std::fill_n(text, 128, TChar(91));
  require(controller.getNoteExpressionStringByValue(
              0, 0, kVolumeTypeID, std::numeric_limits<double>::quiet_NaN(),
              text) == kResultFalse &&
              text[0] == 91,
          "nonfinite expression text rejected");
  double out = 91.;
  String128 unterminated{};
  std::fill_n(unterminated, 128, TChar('0'));
  require(controller.getNoteExpressionValueByString(
              0, 0, kVolumeTypeID, unterminated, out) == kResultFalse &&
              out == 91.,
          "bounded string read");
  require(controller.setParamNormalized(levelID, .75) == kResultOk &&
              controller.setParamNormalized(
                  levelID, std::numeric_limits<double>::infinity()) ==
                  kResultFalse &&
              controller.getParamNormalized(levelID) == .75,
          "invalid level edit atomic");
  MemoryStream saved;
  require(controller.getState(&saved) == kResultOk, "controller save");
  require(controller.setParamNormalized(levelID, .125) == kResultOk,
          "controller edit");
  saved.seek(0, IBStream::kIBSeekSet, nullptr);
  require(controller.setState(&saved) == kResultOk,
          "disconnected controller recall");
  // Component streams must contain exactly one canonical complete record.
  LevelCodec codec;
  std::array<uint8_t, 24> canonical{};
  require(codec.encode(.375f, canonical), "canonical controller fixture");
  for (unsigned fault = 0; fault < 4; ++fault) {
    std::array<uint8_t, 25> damaged{};
    std::copy(canonical.begin(), canonical.end(), damaged.begin());
    int32 bytes = 24;
    if (fault == 0)
      damaged[0] ^= 1;
    if (fault == 1)
      bytes = 23;
    if (fault == 2)
      bytes = 25;
    if (fault == 3)
      damaged[20] = damaged[21] = damaged[22] = damaged[23] = 255;
    MemoryStream invalid(damaged.data(), bytes);
    require(controller.setComponentState(&invalid) == kResultFalse &&
                controller.getParamNormalized(levelID) == .75,
            "malformed complete component state leaves controller unchanged");
  }
  PerformanceProcessor processor;
  require(processor.initialize(&host) == kResultOk,
          "pending processor initialize");
  require(controller.connect(&processor) == kResultOk &&
              performanceSnapshot(processor) == .75f,
          "controller recall synchronizes on connection");
  controller.disconnect(&processor);
  processor.terminate();
  controller.terminate();
}
template <class Sample>
void processingContracts(HostApplication &host, int32 channels) {
  constexpr int32 precision =
      std::is_same_v<Sample, double> ? kSample64 : kSample32;
  PerformanceProcessor processor;
  require(processor.initialize(&host) == kResultOk, "instrument initialize");
  require(processor.getBusCount(kAudio, kInput) == 0 &&
              processor.getBusCount(kAudio, kOutput) == 1 &&
              processor.getBusCount(kEvent, kInput) == 1 &&
              processor.getBusCount(kEvent, kOutput) == 0,
          "explicit instrument bus profile");
  SpeakerArrangement arrangement =
      channels == 1 ? SpeakerArr::kMono : SpeakerArr::kStereo;
  require(processor.setBusArrangements(nullptr, 0, &arrangement, 1) ==
              kResultOk,
          "mono/stereo layout");
  ProcessSetup setup{kRealtime, precision, 4096, 48000.};
  require(processor.setupProcessing(setup) == kResultOk &&
              processor.setActive(true) == kResultOk &&
              processor.setProcessing(true) == kResultOk,
          "instrument prepare");
  std::array<Sample, 4098> left{}, right{};
  Sample *buffers[]{left.data(), right.data()};
  AudioBusBuffers output{};
  output.numChannels = channels;
  if constexpr (std::is_same_v<Sample, double>)
    output.channelBuffers64 = buffers;
  else
    output.channelBuffers32 = buffers;
  ProcessData data{};
  data.symbolicSampleSize = precision;
  data.numOutputs = 1;
  data.outputs = &output;
  PerformanceList list;
  data.inputEvents = &list;
  const auto render = [&](int32 frames) {
    data.numSamples = frames;
    return processor.process(data);
  };
  // Signed SDK plug-in IDs retain exact bits; note cents expand before the
  // following absolute tuning expression. No MIDI quantization is involved.
  for (unsigned cycle = 0; cycle < 250; ++cycle) {
    processor.setProcessing(false);
    processor.setProcessing(true);
    restorePerformance(processor, .5f);
    const int32 id =
        cycle ? -10000 - int32(cycle) : std::numeric_limits<int32>::min();
    list.count = 5;
    list.events[0] = performanceNote(true, id);
    list.events[0].noteOn.tuning = -100.f;
    list.events[1] = performanceExpression(kVolumeTypeID, .5, id);
    list.events[2] = performanceExpression(kPanTypeID, 0., id);
    list.events[3] = performanceExpression(kTuningTypeID, .55, id);
    list.events[4] = performanceExpression(kExpressionTypeID, .7, id);
    left.fill(Sample(99));
    right.fill(Sample(88));
    const auto oldAlloc = allocations, oldFree = frees;
    measuring = true;
    const auto status = render(300);
    measuring = false;
    require(status == kResultOk && allocations == oldAlloc && frees == oldFree,
            "prepared instrument render uses no C++ heap");
    const double frequency = 440. * std::pow(2., 240. * (.55 - .5) / 12.);
    const double pan = channels == 2 ? 2. : 1.;
    const double tolerance = std::is_same_v<Sample, float> ? 1e-7 : 1e-13;
    for (int frame = 0; frame < 240; ++frame) {
      const double expected =
          std::sin(6.2831853071795864769 * frequency / 48000. * frame) *
          (frame + 1.) / 240. * double(.12345679f) * 2. * .7 * .5 / 16. * pan;
      require(std::abs(double(left[size_t(frame)]) - expected) < tolerance &&
                  (channels == 1 || right[size_t(frame)] == Sample(0)),
              "analytic precision, velocity, tuning, pan, expression");
    }
    require(left[300] == Sample(99) &&
                (channels == 1 || right[300] == Sample(88)),
            "backing tails");
    require(performanceSnapshot(processor) == .5f,
            "successful state reflection");
    // A late ID/address conflict must discard automation and a pending recall.
    restorePerformance(processor, .75f);
    ParameterChanges changes(1);
    int32 index = 0;
    changes.addParameterData(levelID, index)->addPoint(0, .125, index);
    data.inputParameterChanges = &changes;
    list.count = 2;
    list.events[0] = performanceExpression(kVolumeTypeID, .25, id);
    list.events[1] = performanceNote(true, id, 70, 127);
    require(
        render(128) == kResultFalse && performanceSnapshot(processor) == .75f,
        "late conflict does not consume pending state or publish automation");
    for (int frame = 0; frame < 128; ++frame)
      require(left[size_t(frame)] == Sample(0), "fault silence");
    data.inputParameterChanges = nullptr;
    list.count = 0;
    require(render(1) == kResultOk && left[0] != Sample(0) &&
                performanceSnapshot(processor) == .75f,
            "pending restore retained and accepted after fault");
    const double continued =
        std::sin(6.2831853071795864769 * frequency / 48000. * 300.) *
        (1. - .2 * 61. / 480.) * double(.12345679f) * 2. * .7 * double(.75f) /
        16. * pan;
    require(std::abs(double(left[0]) - continued) < tolerance,
            "failed block preserves exact oscillator and envelope progress");
    list.count = 1;
    list.events[0] = performanceExpression(kVolumeTypeID, -0., id);
    require(render(128) == kResultOk, "zero volume accepted");
    for (int frame = 0; frame < 128; ++frame)
      require(left[size_t(frame)] == Sample(0), "exact note mute");
    list.events[0] = performanceNote(false, id);
    require(render(2400) == kResultOk &&
                output.silenceFlags == (uint64(1) << channels) - 1,
            "muted release finishes");
    list.events[0] = performanceNote(true, id, 70);
    require(render(2) == kResultOk && left[1] != Sample(0),
            "ID reusable at different address after actual release");
  }
  processor.setProcessing(false);
  processor.setProcessing(true);
  list.count = 0;
  data.inputParameterChanges = nullptr;
  // Adopt pending setup state before exercising the entire shared event budget.
  require(render(0) == kResultOk, "zero-frame pending recall");
  list.count = 1024;
  for (int32 i = 0; i < list.count; ++i)
    list.events[size_t(i)] = performanceNote(true, -1);
  require(render(0) == kResultOk, "complete 1024-event native budget");
  restorePerformance(processor, .625f);
  require(render(0) == kResultFalse && performanceSnapshot(processor) == .625f,
          "pending recall consumes one shared slot; overflow atomic");
  list.count = 1023;
  require(render(0) == kResultOk, "recall plus 1023 events accepted");
  list.count = 1;
  list.events[0] = performanceExpression(kCustomStart, .5, 0);
  require(render(1) == kResultFalse, "unknown selected expression rejects");
  list.events[0].busIndex = 1;
  require(render(1) == kResultOk,
          "unselected payload filtered after header validation");
  list.events[0].sampleOffset = 1;
  require(render(1) == kResultFalse, "unselected invalid timing rejects");
  list.count = 0;
  ParameterChanges malformed(2);
  int32 index = 0;
  malformed.addParameterData(levelID, index)->addPoint(0, .25, index);
  malformed.addParameterData(9999, index)
      ->addPoint(0, std::numeric_limits<double>::quiet_NaN(), index);
  data.inputParameterChanges = &malformed;
  require(render(1) == kResultFalse && performanceSnapshot(processor) == .625f,
          "all raw queues validated before known parameter edits");
  data.inputParameterChanges = nullptr;
  require(processor.getTailSamples() == kInfiniteTail &&
              processor.canProcessSampleSize(99) == kResultFalse,
          "tail and precision metadata");
  processor.setProcessing(false);
  processor.setActive(false);
  processor.terminate();
}
int main() {
  try {
    HostApplication host;
    controllerContracts(host);
    for (int32 channels : {1, 2}) {
      processingContracts<float>(host, channels);
      processingContracts<double>(host, channels);
    }
    std::puts("PASS VST3 performance instrument: controller/state/expressions, "
              "f32/f64 mono/stereo, 1000 prepared cycles, analytic audio, "
              "atomic late faults and shared capacity");
    return 0;
  } catch (const std::exception &error) {
    std::fprintf(stderr, "VST3 performance instrument: %s\n", error.what());
    return 1;
  }
}
