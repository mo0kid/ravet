// Robustness of the actual RAVET reference VST3 modules (ADR 0274) against
// malformed and over-capacity ProcessData, driven directly through the SDK
// (IComponent/IAudioProcessor) because the hosted-audio provider never sends
// such data. Arguments: /abs/ravet_sidechain_vst3.vst3 /abs/ravet_event_probe_vst3.vst3
//
// Oracles: a refused block must return kResultFalse, zero every reachable
// output channel, post no events or parameter points, and leave the running
// digest unchanged; the digest is compared with a twin instance that never saw
// the refused block. An ignored input (other bus, unknown kind, non-Marker
// queue) must give bitwise the same audio, events and digest as the same block
// without it on the twin. Every process call, refused or not, must perform
// zero Rust heap operations inside the module (allocation-probe counter).
#include "pluginterfaces/base/funknown.h"
#include "pluginterfaces/vst/ivstaudioprocessor.h"
#include "pluginterfaces/vst/ivstcomponent.h"
#include "pluginterfaces/vst/ivstevents.h"
#include "pluginterfaces/vst/ivstparameterchanges.h"
#include "pluginterfaces/vst/vstspeaker.h"
#include "public.sdk/source/vst/hosting/hostclasses.h"
#include "public.sdk/source/vst/hosting/module.h"
#include <dlfcn.h>
#include <algorithm>
#include <array>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <deque>
#include <exception>
#include <iostream>
#include <limits>
#include <memory>
#include <stdexcept>
#include <string>
#include <type_traits>
#include <vector>

using namespace Steinberg;
using namespace Steinberg::Vst;

namespace {
#define RAVET_STR2(x) #x
#define RAVET_STR(x) RAVET_STR2(x)
#define REQUIRE(x)                                                             \
  do {                                                                         \
    if (!(x))                                                                  \
      throw std::runtime_error("requirement failed line " RAVET_STR(           \
          __LINE__) ": " #x);                                                  \
  } while (0)

constexpr int32 maximumFrames = 64;
constexpr size_t payloadCapacity = 65536 + 2 * 2048 + 16;
size_t checks = 0;

// Stack-owned host objects: reference counting is a no-op.
#define RAVET_UNKNOWN(Interface)                                               \
  tresult PLUGIN_API queryInterface(const TUID iid, void **object) override {  \
    if (FUnknownPrivate::iidEqual(iid, Interface::iid) ||                      \
        FUnknownPrivate::iidEqual(iid, FUnknown::iid)) {                       \
      *object = this;                                                          \
      return kResultOk;                                                        \
    }                                                                          \
    *object = nullptr;                                                         \
    return kNoInterface;                                                       \
  }                                                                            \
  uint32 PLUGIN_API addRef() override { return 1; }                            \
  uint32 PLUGIN_API release() override { return 1; }

struct Events final : IEventList {
  std::vector<Event> events;
  bool overrideCount = false;
  int32 count = 0;
  int32 failAt = -1;
  size_t addLimit = std::numeric_limits<size_t>::max();
  int32 PLUGIN_API getEventCount() override {
    return overrideCount ? count : int32(events.size());
  }
  tresult PLUGIN_API getEvent(int32 index, Event &e) override {
    if (index == failAt || index < 0 || size_t(index) >= events.size())
      return kResultFalse;
    e = events[size_t(index)];
    return kResultOk;
  }
  tresult PLUGIN_API addEvent(Event &e) override {
    if (events.size() >= addLimit)
      return kResultFalse;
    events.push_back(e);
    return kResultOk;
  }
  RAVET_UNKNOWN(IEventList)
};

struct Queue final : IParamValueQueue {
  ParamID id = 0;
  std::vector<std::pair<int32, ParamValue>> points;
  bool overrideCount = false;
  int32 count = 0;
  int32 failAt = -1;
  bool refuseAdd = false;
  ParamID PLUGIN_API getParameterId() override { return id; }
  int32 PLUGIN_API getPointCount() override {
    return overrideCount ? count : int32(points.size());
  }
  tresult PLUGIN_API getPoint(int32 index, int32 &offset,
                              ParamValue &value) override {
    if (index == failAt || index < 0 || size_t(index) >= points.size())
      return kResultFalse;
    offset = points[size_t(index)].first;
    value = points[size_t(index)].second;
    return kResultOk;
  }
  tresult PLUGIN_API addPoint(int32 offset, ParamValue value,
                              int32 &index) override {
    if (refuseAdd)
      return kResultFalse;
    points.emplace_back(offset, value);
    index = int32(points.size()) - 1;
    return kResultOk;
  }
  RAVET_UNKNOWN(IParamValueQueue)
};

struct Changes final : IParameterChanges {
  std::vector<Queue *> queues;
  std::deque<Queue> owned;
  bool overrideCount = false;
  int32 count = 0;
  bool refuseQueues = false, refusePoints = false;
  int32 PLUGIN_API getParameterCount() override {
    return overrideCount ? count : int32(queues.size());
  }
  IParamValueQueue *PLUGIN_API getParameterData(int32 index) override {
    return index >= 0 && size_t(index) < queues.size() ? queues[size_t(index)]
                                                       : nullptr;
  }
  IParamValueQueue *PLUGIN_API addParameterData(const ParamID &id,
                                                int32 &index) override {
    if (refuseQueues)
      return nullptr;
    owned.emplace_back();
    owned.back().id = id;
    owned.back().refuseAdd = refusePoints;
    queues.push_back(&owned.back());
    index = int32(queues.size()) - 1;
    return &owned.back();
  }
  RAVET_UNKNOWN(IParameterChanges)
};

// Rust heap-operation counter exported by an allocation-probe build.
using RustCounter = uint64_t (*)();
RustCounter rustCounter(const std::string &bundle) {
  const auto name = bundle.substr(bundle.find_last_of('/') + 1);
  const auto binary =
      bundle + "/Contents/MacOS/" + name.substr(0, name.size() - 5);
  void *image = dlopen(binary.c_str(), RTLD_LAZY | RTLD_NOLOAD);
  REQUIRE(image != nullptr);
  auto *counter = reinterpret_cast<RustCounter>(
      dlsym(image, "ravet_vst3_reference_allocation_count"));
  dlclose(image);
  REQUIRE(counter != nullptr);
  return counter;
}

HostApplication &host() {
  static HostApplication application;
  return application;
}

// One component of the module's audio class, driven directly.
struct Plugin {
  VST3::Hosting::Module::Ptr module;
  IPtr<IComponent> component;
  IPtr<IAudioProcessor> processor;
  RustCounter rust = nullptr;
  bool active = false;
  explicit Plugin(const std::string &path, bool initialize = true) {
    std::string error;
    module = VST3::Hosting::Module::create(path, error);
    REQUIRE(module != nullptr);
    for (const auto &info : module->getFactory().classInfos())
      if (info.category() == kVstAudioEffectClass)
        component = module->getFactory().createInstance<IComponent>(info.ID());
    REQUIRE(component != nullptr);
    processor = FUnknownPtr<IAudioProcessor>(component);
    REQUIRE(processor != nullptr);
    rust = rustCounter(path);
    if (initialize)
      REQUIRE(component->initialize(&host()) == kResultOk);
  }
  void start(int32 bits) {
    ProcessSetup setup{kRealtime, bits == 64 ? kSample64 : kSample32,
                       maximumFrames, 48000.};
    REQUIRE(processor->setupProcessing(setup) == kResultOk);
    REQUIRE(component->setActive(true) == kResultOk);
    REQUIRE(processor->setProcessing(true) == kResultOk);
    active = true;
  }
  void stop() {
    if (active) {
      processor->setProcessing(false);
      component->setActive(false);
      active = false;
    }
  }
  ~Plugin() {
    stop();
    if (component)
      component->terminate();
  }
};

// One process call with fully controllable ProcessData.
template <class S> struct Call {
  int32 frames = 16;
  int32 numOutputs = 1, numInputs = 0, outChannels = 2;
  bool outputsNull = false, outChannelArrayNull = false;
  int nullOutChannel = -1;
  int32 symbolic = sizeof(S) == 8 ? kSample64 : kSample32;
  IEventList *inputEvents = nullptr;
  IParameterChanges *inputChanges = nullptr;
  bool outputEvents = true, outputChanges = true;
  size_t addLimit = std::numeric_limits<size_t>::max();
  bool refusePoints = false;
  // Buffers follow the declared sample size; with a mismatched size the
  // refusal zeroes them as the other type, so typed zero checks are skipped.
  bool checkSilence = true;
  // Inputs (sidechain): per-bus channel vectors and flags.
  std::vector<std::vector<std::vector<S>>> in;
  std::vector<uint64> inFlags;
  std::vector<int32> inChannelsOverride;
  bool inputsNull = false;
  int nullInChannelArray = -1;
  // Results.
  std::array<std::vector<S>, 2> out{std::vector<S>(maximumFrames + 4, S(9)),
                                    std::vector<S>(maximumFrames + 4, S(9))};
  Events posted;
  Changes digest;
  tresult result = kInternalError;
  uint64_t rustOperations = 0;

  tresult run(Plugin &p) {
    posted.addLimit = addLimit;
    digest.refusePoints = refusePoints;
    std::array<S *, 2> outPointers{out[0].data(), out[1].data()};
    if (nullOutChannel >= 0)
      outPointers[size_t(nullOutChannel)] = nullptr;
    AudioBusBuffers outBus{};
    outBus.numChannels = outChannels;
    outBus.silenceFlags = ~uint64(0);
    S **outArray = outChannelArrayNull ? nullptr : outPointers.data();
    std::vector<std::vector<S *>> inPointers(in.size());
    std::vector<AudioBusBuffers> inBuses(in.size());
    for (size_t b = 0; b < in.size(); ++b) {
      for (auto &channel : in[b])
        inPointers[b].push_back(channel.data());
      inBuses[b].numChannels = b < inChannelsOverride.size() &&
                                       inChannelsOverride[b] >= 0
                                   ? inChannelsOverride[b]
                                   : int32(in[b].size());
      inBuses[b].silenceFlags = b < inFlags.size() ? inFlags[b] : 0;
      S **array = int(b) == nullInChannelArray ? nullptr : inPointers[b].data();
      if constexpr (std::is_same_v<S, double>)
        inBuses[b].channelBuffers64 = array;
      else
        inBuses[b].channelBuffers32 = array;
    }
    if constexpr (std::is_same_v<S, double>)
      outBus.channelBuffers64 = outArray;
    else
      outBus.channelBuffers32 = outArray;
    ProcessData data{};
    data.processMode = kRealtime;
    data.symbolicSampleSize = symbolic;
    data.numSamples = frames;
    data.numInputs = numInputs;
    data.inputs = inputsNull || in.empty() ? nullptr : inBuses.data();
    data.numOutputs = numOutputs;
    data.outputs = outputsNull ? nullptr : &outBus;
    data.inputEvents = inputEvents;
    data.inputParameterChanges = inputChanges;
    data.outputEvents = outputEvents ? &posted : nullptr;
    data.outputParameterChanges = outputChanges ? &digest : nullptr;
    const uint64_t before = p.rust();
    result = p.processor->process(data);
    rustOperations = p.rust() - before;
    REQUIRE(rustOperations == 0);
    ++checks;
    return result;
  }
  bool silent(int32 n) const {
    return std::all_of(out[0].begin(), out[0].begin() + n,
                       [](S v) { return v == S(0); }) &&
           std::all_of(out[1].begin(), out[1].begin() + n,
                       [](S v) { return v == S(0); });
  }
  bool tailUntouched(int32 n) const {
    for (const auto &channel : out)
      for (size_t f = size_t(std::max(n, 0)); f < channel.size(); ++f)
        if (channel[f] != S(9))
          return false;
    return true;
  }
};

// ------------------------------------------------------------- Event Probe

Event noteOn(int32 offset, int32 bus = 0) {
  Event e{};
  e.busIndex = bus;
  e.sampleOffset = offset;
  e.type = Event::kNoteOnEvent;
  e.noteOn.channel = 1;
  e.noteOn.pitch = 60;
  e.noteOn.velocity = 0.5f;
  e.noteOn.noteId = 7;
  return e;
}
Event dataEvent(int32 offset, const uint8 *bytes, uint32 size) {
  Event e{};
  e.sampleOffset = offset;
  e.type = Event::kDataEvent;
  e.data.type = DataEvent::kMidiSysEx;
  e.data.size = size;
  e.data.bytes = bytes;
  return e;
}
Event textEvent(int32 offset, const TChar *text, uint32 units) {
  Event e{};
  e.sampleOffset = offset;
  e.type = Event::kNoteExpressionTextEvent;
  e.noteExpressionText.typeId = 3;
  e.noteExpressionText.noteId = 9;
  e.noteExpressionText.textLen = units;
  e.noteExpressionText.text = text;
  return e;
}

// A well-formed block: four kinds at distinct offsets plus one Marker point.
struct Good {
  std::array<uint8, 5> sysex{0xf0, 0x7d, 0x11, 0x22, 0xf7};
  std::u16string text = u"probe";
  Events events;
  Queue marker;
  Changes changes;
  explicit Good(uint32_t variant) {
    events.events.push_back(noteOn(1));
    events.events.back().noteOn.pitch = int16(40 + variant % 50);
    events.events.push_back(dataEvent(4, sysex.data(), 2 + variant % 4));
    events.events.push_back(textEvent(
        8, reinterpret_cast<const TChar *>(text.data()), uint32(text.size())));
    Event poly{};
    poly.sampleOffset = 12;
    poly.type = Event::kPolyPressureEvent;
    poly.polyPressure.pressure = float(variant % 7) / 8.f;
    events.events.push_back(poly);
    marker.id = 1;
    marker.points = {{14, double(variant % 9) / 8.}};
    changes.queues = {&marker};
  }
};

template <class S> double digestOf(const Call<S> &c) {
  REQUIRE(c.digest.queues.size() == 1 && c.digest.queues[0]->id == 2 &&
          c.digest.queues[0]->points.size() == 1);
  return c.digest.queues[0]->points[0].second;
}

// Refused block: kResultFalse, outputs zeroed (when frames are in range), no
// events, no Digest point.
template <class S> void zeroedOutputs(const Call<S> &c) {
  if (c.checkSilence && c.frames > 0 && c.frames <= maximumFrames &&
      !c.outputsNull && c.numOutputs >= 1 && !c.outChannelArrayNull) {
    for (int ch = 0; ch < std::min(2, int(c.outChannels)); ++ch)
      if (ch != c.nullOutChannel)
        REQUIRE(std::all_of(c.out[size_t(ch)].begin(),
                            c.out[size_t(ch)].begin() + c.frames,
                            [](S v) { return v == S(0); }));
  }
}

template <class S> void refused(Plugin &p, Call<S> &c, const char *label) {
  if (c.run(p) != kResultFalse)
    throw std::runtime_error(std::string("not refused: ") + label);
  zeroedOutputs(c);
  if (!c.posted.events.empty() || !c.digest.queues.empty())
    throw std::runtime_error(std::string("posted output: ") + label);
}

// Twin comparison: after any case on `a`, the same good block on `a` and on
// the twin `b` must agree bitwise (audio, echo events, Digest).
template <class S> void sameGood(Plugin &a, Plugin &b, uint32_t variant) {
  Good g(variant);
  Call<S> x, y;
  x.inputEvents = y.inputEvents = &g.events;
  x.inputChanges = y.inputChanges = &g.changes;
  REQUIRE(x.run(a) == kResultOk);
  REQUIRE(y.run(b) == kResultOk);
  REQUIRE(x.out == y.out && digestOf(x) == digestOf(y));
  REQUIRE(x.posted.events.size() == 5 && y.posted.events.size() == 5);
  // Payload pointers differ per instance; compare kinds and offsets.
  for (size_t i = 0; i < 5; ++i)
    REQUIRE(x.posted.events[i].type == y.posted.events[i].type &&
            x.posted.events[i].sampleOffset ==
                y.posted.events[i].sampleOffset);
}

template <class S> void probe(const std::string &path) {
  constexpr int32 bits = sizeof(S) * 8;
  Plugin a(path), b(path);
  // Sequencing: process before setupProcessing/setActive is refused.
  {
    Good g(0);
    Call<S> c;
    c.inputEvents = &g.events;
    refused(a, c, "process before activation");
  }
  a.start(bits);
  b.start(bits);
  // First good block on a fresh activation: reference digest d1.
  double d1 = 0;
  {
    Good g(1);
    Call<S> x, y;
    x.inputEvents = y.inputEvents = &g.events;
    x.inputChanges = y.inputChanges = &g.changes;
    REQUIRE(x.run(a) == kResultOk && y.run(b) == kResultOk);
    d1 = digestOf(x);
    REQUIRE(d1 == digestOf(y) && d1 != 0.);
    // Impulse positions: NoteOn@1 code 1, Data@4 code 3, Text@8 code 6,
    // Poly@12 code 4, Marker@14 code 17; everything else zero.
    for (int32 f = 0; f < 16; ++f) {
      const S expected = f == 1    ? S(1)
                         : f == 4  ? S(3)
                         : f == 8  ? S(6)
                         : f == 12 ? S(4)
                         : f == 14 ? S(17)
                                   : S(0);
      REQUIRE(x.out[0][size_t(f)] == expected);
      REQUIRE((x.out[1][size_t(f)] != S(0)) == (expected != S(0)));
    }
    REQUIRE(x.tailUntouched(16));
    // Echo: 4 events then the count CC (4 echoed; Marker excluded).
    REQUIRE(x.posted.events.size() == 5);
    const auto &cc = x.posted.events[4];
    REQUIRE(cc.type == Event::kLegacyMIDICCOutEvent &&
            cc.midiCCOut.controlNumber == 16 && cc.midiCCOut.value == 4 &&
            cc.midiCCOut.value2 == 0 && cc.sampleOffset == 12);
  }
  uint32_t variant = 2;
  const auto check = [&] { sameGood<S>(a, b, variant++); };

  // ---- Over capacity: events.
  {
    Events many;
    for (int i = 0; i < 2049; ++i)
      many.events.push_back(noteOn(i % 16));
    Call<S> c;
    c.inputEvents = &many;
    refused(a, c, "2049 events");
    check();
    Events negative;
    negative.overrideCount = true;
    negative.count = -1;
    Call<S> n;
    n.inputEvents = &negative;
    refused(a, n, "negative event count");
    check();
    Events failing;
    for (int i = 0; i < 5; ++i)
      failing.events.push_back(noteOn(i));
    failing.failAt = 3;
    Call<S> f;
    f.inputEvents = &failing;
    refused(a, f, "getEvent failure");
    check();
    // Exactly 2048 events plus 2048 points: admitted at capacity.
    Events full;
    for (int i = 0; i < 2048; ++i)
      full.events.push_back(noteOn(i % 16));
    Queue points;
    points.id = 1;
    for (int i = 0; i < 2048; ++i)
      points.points.emplace_back(15, 0.5);
    Changes changes;
    changes.queues = {&points};
    Call<S> ok, twin;
    ok.inputEvents = twin.inputEvents = &full;
    ok.inputChanges = twin.inputChanges = &changes;
    REQUIRE(ok.run(a) == kResultOk && twin.run(b) == kResultOk);
    REQUIRE(ok.posted.events.size() == 2049);
    const auto &cc = ok.posted.events[2048];
    REQUIRE(cc.midiCCOut.value == 0 && cc.midiCCOut.value2 == 16);
    REQUIRE(digestOf(ok) == digestOf(twin));
  }
  // ---- Over capacity: parameter points (records = events + points <= 4096).
  {
    const auto pointsCase = [&](size_t events, size_t pointCount,
                                bool admitted, const char *label) {
      Events list;
      for (size_t i = 0; i < events; ++i)
        list.events.push_back(noteOn(int32(i % 16)));
      Queue q;
      q.id = 1;
      for (size_t i = 0; i < pointCount; ++i)
        q.points.emplace_back(15, 0.25);
      Changes changes;
      changes.queues = {&q};
      Call<S> c, twin;
      c.inputEvents = twin.inputEvents = &list;
      c.inputChanges = twin.inputChanges = &changes;
      if (admitted) {
        REQUIRE(c.run(a) == kResultOk && twin.run(b) == kResultOk);
        REQUIRE(digestOf(c) == digestOf(twin));
      } else {
        refused(a, c, label);
      }
      check();
    };
    pointsCase(0, 4096, true, "4096 points");
    pointsCase(0, 4097, false, "4097 points");
    pointsCase(1, 4096, false, "1 event + 4096 points");
    pointsCase(2048, 2049, false, "2048 events + 2049 points");
    Queue negative;
    negative.id = 1;
    negative.overrideCount = true;
    negative.count = -1;
    Changes nc;
    nc.queues = {&negative};
    Call<S> n;
    n.inputChanges = &nc;
    refused(a, n, "negative point count");
    check();
    Queue failing;
    failing.id = 1;
    failing.points = {{0, 0.}, {1, 0.}, {2, 0.}};
    failing.failAt = 1;
    Changes fc;
    fc.queues = {&failing};
    Call<S> f;
    f.inputChanges = &fc;
    refused(a, f, "getPoint failure");
    check();
    Changes nullQueue;
    nullQueue.queues = {nullptr};
    Call<S> nq;
    nq.inputChanges = &nullQueue;
    refused(a, nq, "null queue");
    check();
    Changes negativeQueues;
    negativeQueues.overrideCount = true;
    negativeQueues.count = -1;
    Call<S> nqs;
    nqs.inputChanges = &negativeQueues;
    refused(a, nqs, "negative queue count");
    check();
    // A non-Marker queue is ignored unread, even when malformed.
    Good g(variant);
    Queue other;
    other.id = 99;
    other.overrideCount = true;
    other.count = -7;
    Changes withOther;
    withOther.queues = {&other, &g.marker};
    Call<S> c, twin;
    c.inputEvents = twin.inputEvents = &g.events;
    c.inputChanges = &withOther;
    twin.inputChanges = &g.changes;
    REQUIRE(c.run(a) == kResultOk && twin.run(b) == kResultOk);
    REQUIRE(c.out == twin.out && digestOf(c) == digestOf(twin));
    ++variant;
  }
  // ---- Payload overflow and null payloads.
  {
    std::vector<uint8> big(payloadCapacity + 1, 0x5a);
    // Echoed payloads point into module storage, valid until the next
    // process call on that instance, so they are verified before check().
    const auto dataCase = [&](std::vector<Event> list, bool admitted,
                              const char *label, auto verify) {
      Events events;
      events.events = std::move(list);
      Call<S> c, twin;
      c.inputEvents = twin.inputEvents = &events;
      if (admitted) {
        REQUIRE(c.run(a) == kResultOk && twin.run(b) == kResultOk);
        REQUIRE(digestOf(c) == digestOf(twin));
        REQUIRE(c.posted.events.size() == events.events.size() + 1);
        verify(c.posted.events);
      } else {
        refused(a, c, label);
      }
      check();
    };
    const auto none = [](const std::vector<Event> &) {};
    dataCase({dataEvent(0, big.data(), uint32(payloadCapacity))}, true,
             "payload at capacity", [&](const std::vector<Event> &echoed) {
               REQUIRE(echoed[0].data.size == payloadCapacity &&
                       std::memcmp(echoed[0].data.bytes, big.data(),
                                   payloadCapacity) == 0);
             });
    dataCase({dataEvent(0, big.data(), uint32(payloadCapacity + 1))}, false,
             "payload over capacity", none);
    dataCase({dataEvent(0, big.data(), 40000), dataEvent(1, big.data(), 40000)},
             false, "combined payload over capacity", none);
    std::u16string longText(40000, u'x');
    dataCase({textEvent(0, reinterpret_cast<const TChar *>(longText.data()),
                        40000)},
             false, "text over capacity", none);
    dataCase({dataEvent(0, nullptr, 3)}, false, "null data bytes", none);
    dataCase({textEvent(0, nullptr, 2)}, false, "null text", none);
    dataCase({dataEvent(2, nullptr, 0), textEvent(3, nullptr, 0)}, true,
             "empty payloads", [](const std::vector<Event> &empty) {
               REQUIRE(empty[0].data.size == 0 &&
                       empty[1].noteExpressionText.textLen == 0);
             });
  }
  // ---- Offsets.
  {
    const auto offsetCase = [&](int32 offset, int32 frames, bool point,
                                const char *label) {
      Events events;
      Queue q;
      q.id = 1;
      Changes changes;
      if (point) {
        q.points = {{offset, 0.5}};
        changes.queues = {&q};
      } else {
        events.events = {noteOn(offset)};
      }
      Call<S> c;
      c.frames = frames;
      c.inputEvents = &events;
      c.inputChanges = &changes;
      refused(a, c, label);
      check();
    };
    for (bool point : {false, true}) {
      offsetCase(-1, 16, point, "negative offset");
      offsetCase(16, 16, point, "offset at frames");
      offsetCase(1000, 16, point, "offset past frames");
      offsetCase(std::numeric_limits<int32>::min(), 16, point, "minimum offset");
      offsetCase(1, 0, point, "nonzero offset in zero-frame block");
    }
  }
  // ---- Ignored inputs: other buses and unknown or output-only kinds, even
  //      with offsets that would otherwise be refused.
  {
    Good g(variant);
    Events noisy;
    noisy.events = g.events.events;
    noisy.events.insert(noisy.events.begin() + 1, noteOn(3, 1));
    noisy.events.push_back(noteOn(-5, 2));
    noisy.events.push_back(noteOn(9, -1));
    Event cc{};
    cc.sampleOffset = 5;
    cc.type = Event::kLegacyMIDICCOutEvent;
    noisy.events.push_back(cc);
    Event unknown{};
    unknown.sampleOffset = 99999;
    unknown.type = 1000;
    noisy.events.push_back(unknown);
    Call<S> c, twin;
    c.inputEvents = &noisy;
    twin.inputEvents = &g.events;
    c.inputChanges = twin.inputChanges = &g.changes;
    REQUIRE(c.run(a) == kResultOk && twin.run(b) == kResultOk);
    REQUIRE(c.out == twin.out && digestOf(c) == digestOf(twin));
    REQUIRE(c.posted.events.size() == 5 &&
            c.posted.events[4].midiCCOut.value == 4);
    ++variant;
  }
  // ---- Empty and parameter-only blocks; null optional lists.
  {
    Events none;
    Call<S> c, twin;
    c.inputEvents = &none; // present but empty
    twin.inputEvents = nullptr;
    REQUIRE(c.run(a) == kResultOk && twin.run(b) == kResultOk);
    REQUIRE(c.silent(16) && c.tailUntouched(16));
    REQUIRE(c.posted.events.empty() && digestOf(c) == digestOf(twin));
    REQUIRE(c.digest.queues[0]->points[0].first == 15);
    Queue q;
    q.id = 1;
    q.points = {{2, 0.}, {9, 1.}};
    Changes changes;
    changes.queues = {&q};
    Call<S> p, ptwin;
    p.inputChanges = ptwin.inputChanges = &changes;
    REQUIRE(p.run(a) == kResultOk && ptwin.run(b) == kResultOk);
    REQUIRE(p.out[0][2] == S(17) && p.out[0][9] == S(17) &&
            p.posted.events.empty() && digestOf(p) == digestOf(ptwin));
    check();
    Good g(variant);
    Call<S> quiet, qtwin;
    quiet.inputEvents = qtwin.inputEvents = &g.events;
    quiet.inputChanges = qtwin.inputChanges = &g.changes;
    quiet.outputEvents = false;
    quiet.outputChanges = false;
    REQUIRE(quiet.run(a) == kResultOk && qtwin.run(b) == kResultOk);
    REQUIRE(quiet.out == qtwin.out);
    ++variant;
    check();
  }
  // ---- Null and malformed buffers, sizes and sample format.
  {
    Good g(variant);
    const auto bufferCase = [&](auto edit, const char *label) {
      Call<S> c;
      c.inputEvents = &g.events;
      c.inputChanges = &g.changes;
      edit(c);
      refused(a, c, label);
      check();
    };
    bufferCase([](Call<S> &c) { c.outputsNull = true; }, "null outputs");
    bufferCase([](Call<S> &c) { c.numOutputs = 0; }, "no output bus");
    bufferCase([](Call<S> &c) { c.outChannelArrayNull = true; },
               "null channel array");
    bufferCase([](Call<S> &c) { c.nullOutChannel = 0; }, "null left channel");
    bufferCase([](Call<S> &c) { c.nullOutChannel = 1; }, "null right channel");
    bufferCase([](Call<S> &c) { c.outChannels = 1; }, "mono output");
    bufferCase([](Call<S> &c) { c.frames = maximumFrames + 1; },
               "frames above maximum");
    bufferCase([](Call<S> &c) { c.frames = -1; }, "negative frames");
    bufferCase(
        [](Call<S> &c) {
          c.symbolic = sizeof(S) == 8 ? kSample32 : kSample64;
          c.checkSilence = false;
        },
        "sample size mismatch");
    // Zero frames need no output bus at all.
    Call<S> z, ztwin;
    z.frames = ztwin.frames = 0;
    z.outputsNull = true;
    z.numOutputs = 0;
    Events atZero;
    atZero.events = {noteOn(0)};
    z.inputEvents = ztwin.inputEvents = &atZero;
    REQUIRE(z.run(a) == kResultOk && ztwin.run(b) == kResultOk);
    REQUIRE(digestOf(z) == digestOf(ztwin) &&
            z.digest.queues[0]->points[0].first == 0);
    check();
  }
  // ---- Host output refusal (documented): the digest has already advanced.
  {
    Good g(variant);
    Call<S> c, twin;
    c.inputEvents = twin.inputEvents = &g.events;
    c.inputChanges = twin.inputChanges = &g.changes;
    c.addLimit = 2;
    REQUIRE(c.run(a) == kResultFalse && c.silent(16));
    REQUIRE(c.posted.events.size() == 2 && c.digest.queues.empty());
    REQUIRE(twin.run(b) == kResultOk);
    check(); // agrees: a advanced exactly as the successful twin did
    Call<S> p, ptwin;
    p.inputEvents = ptwin.inputEvents = &g.events;
    p.refusePoints = true;
    REQUIRE(p.run(a) == kResultFalse && ptwin.run(b) == kResultOk);
    check();
    ++variant;
  }
  // ---- Reactivation resets the digest.
  {
    a.stop();
    a.start(bits);
    Good g(1);
    Call<S> c;
    c.inputEvents = &g.events;
    c.inputChanges = &g.changes;
    REQUIRE(c.run(a) == kResultOk && digestOf(c) == d1);
    b.stop();
    b.start(bits);
    Call<S> t;
    t.inputEvents = &g.events;
    t.inputChanges = &g.changes;
    REQUIRE(t.run(b) == kResultOk && digestOf(t) == d1);
    check();
  }
  std::printf("PASS event probe robustness %dbit: %zu checked process calls, "
              "every refusal silent with no output and an unchanged digest, "
              "0 Rust heap operations in module\n",
              bits, checks);
}

// ------------------------------------------------------------ Keyed Sidechain

template <class S> std::vector<std::vector<S>> channels(int n, double base) {
  std::vector<std::vector<S>> v(size_t(n), std::vector<S>(maximumFrames + 4));
  for (int c = 0; c < n; ++c)
    for (int f = 0; f < maximumFrames + 4; ++f)
      v[size_t(c)][size_t(f)] = S(base + 0.125 * c - 0.0078125 * f);
  return v;
}

template <class S> void sidechain(const std::string &path) {
  constexpr int32 bits = sizeof(S) * 8;
  checks = 0;
  // setActive before initialize is refused, not thrown.
  {
    Plugin raw(path, false);
    REQUIRE(raw.component->setActive(true) == kResultFalse);
  }
  for (const int keys : {0, 1, 2}) {
    Plugin p(path);
    SpeakerArrangement inputs[2] = {
        SpeakerArr::kStereo, keys == 1 ? SpeakerArr::kMono : SpeakerArr::kStereo};
    SpeakerArrangement output = SpeakerArr::kStereo;
    REQUIRE(p.processor->setBusArrangements(inputs, 2, &output, 1) == kResultOk);
    REQUIRE(p.component->activateBus(kAudio, kInput, 1, keys > 0) == kResultOk);
    p.start(bits);
    const auto base = [&] {
      Call<S> c;
      c.numInputs = keys > 0 ? 2 : 1;
      c.in.push_back(channels<S>(2, 0.5));
      if (keys > 0)
        c.in.push_back(channels<S>(keys, -0.25));
      return c;
    };
    const auto keyAt = [&](const Call<S> &c, int f, uint64 keyFlags) {
      if (keys == 0)
        return S(1);
      const auto k = [&](int ch) {
        return (keyFlags >> ch) & 1 ? S(0) : c.in[1][size_t(ch)][size_t(f)];
      };
      return keys == 1 ? k(0) : (k(0) + k(1)) * S(0.5);
    };
    // Valid block, then silence flags with garbage under the flagged channels.
    for (const uint64 mainFlags : {uint64(0), uint64(1), uint64(2), uint64(3)})
      for (const uint64 keyFlags : {uint64(0), uint64(1), uint64(3)}) {
        if (keys == 0 && keyFlags)
          continue;
        auto c = base();
        c.inFlags = {mainFlags, keyFlags};
        for (int ch = 0; ch < 2; ++ch)
          if ((mainFlags >> ch) & 1)
            std::fill(c.in[0][size_t(ch)].begin(), c.in[0][size_t(ch)].end(), S(7));
        if (keys > 0)
          for (int ch = 0; ch < keys; ++ch)
            if ((keyFlags >> ch) & 1)
              std::fill(c.in[1][size_t(ch)].begin(), c.in[1][size_t(ch)].end(), S(7));
        const auto expectedMain = [&](int ch, int f) {
          return (mainFlags >> ch) & 1 ? S(0) : c.in[0][size_t(ch)][size_t(f)];
        };
        REQUIRE(c.run(p) == kResultOk);
        for (int f = 0; f < c.frames; ++f)
          for (int ch = 0; ch < 2; ++ch)
            REQUIRE(c.out[size_t(ch)][size_t(f)] ==
                    expectedMain(ch, f) * keyAt(c, f, keyFlags));
        REQUIRE(c.tailUntouched(c.frames));
      }
    const auto refuse = [&](auto edit, const char *label) {
      auto c = base();
      edit(c);
      if (c.run(p) != kResultFalse)
        throw std::runtime_error(std::string("sidechain not refused: ") + label);
      zeroedOutputs(c);
    };
    refuse([](Call<S> &c) { c.inputsNull = true; }, "null inputs");
    refuse([](Call<S> &c) { c.numInputs = 0; }, "no input bus");
    refuse([](Call<S> &c) { c.nullInChannelArray = 0; }, "null main channel array");
    refuse([](Call<S> &c) { c.inChannelsOverride = {1}; }, "mono main input");
    refuse([](Call<S> &c) { c.outputsNull = true; }, "null outputs");
    refuse([](Call<S> &c) { c.numOutputs = 0; }, "no output bus");
    refuse([](Call<S> &c) { c.outChannelArrayNull = true; }, "null output channel array");
    refuse([](Call<S> &c) { c.nullOutChannel = 1; }, "null output channel");
    refuse([](Call<S> &c) { c.outChannels = 1; }, "mono output");
    refuse([](Call<S> &c) { c.frames = maximumFrames + 1; }, "frames above maximum");
    refuse([](Call<S> &c) { c.frames = -1; }, "negative frames");
    refuse([](Call<S> &c) {
      c.symbolic = sizeof(S) == 8 ? kSample32 : kSample64;
      c.checkSilence = false;
    }, "sample size mismatch");
    if (keys > 0) {
      refuse([](Call<S> &c) { c.numInputs = 1; }, "missing key bus");
      refuse([](Call<S> &c) { c.nullInChannelArray = 1; }, "null key channel array");
      refuse([&](Call<S> &c) { c.inChannelsOverride = {2, keys == 1 ? 2 : 1}; }, "key channel count");
    }
    // Zero frames need no buffers at all.
    Call<S> z;
    z.frames = 0;
    z.numInputs = 0;
    z.numOutputs = 0;
    z.outputsNull = true;
    REQUIRE(z.run(p) == kResultOk);
  }
  std::printf("PASS sidechain robustness %dbit: %zu checked process calls "
              "(keys 0/1/2, input silence flags read as zero, malformed buffers "
              "refused silent), setActive before initialize refused, 0 Rust "
              "heap operations in module\n",
              bits, checks);
}
} // namespace

int main(int argc, char **argv) {
  if (argc != 3) {
    std::cerr << "usage: reference-robustness-tests /abs/sidechain.vst3 "
                 "/abs/event-probe.vst3\n";
    return 2;
  }
  try {
    sidechain<float>(argv[1]);
    sidechain<double>(argv[1]);
    checks = 0;
    probe<float>(argv[2]);
    checks = 0;
    probe<double>(argv[2]);
    std::printf("4 actual reference-module robustness scenarios passed\n");
  } catch (const std::exception &error) {
    std::cerr << error.what() << '\n';
    return 1;
  }
  return 0;
}
