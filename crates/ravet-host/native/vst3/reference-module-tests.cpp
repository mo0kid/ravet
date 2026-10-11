// Actual RAVET reference VST3 modules through the native hosted-audio provider
// (ADR 0274). Arguments: /abs/ravet_sidechain_vst3.vst3 /abs/ravet_event_probe_vst3.vst3
// CONTROL runs on the process main thread; AUDIO runs on one worker per scenario
// whose C++ operator new/delete calls are counted (not malloc, not the module's
// own heap use, not deadlines). Rust heap operations inside each loaded module
// are read from its allocation-probe counter (modules built with the
// ravet-vst3-reference-native `allocation-probe` feature) around every process
// call and must stay zero. Expected audio, events and parameters come from
// closed forms and an independent FNV-1a implementation in this file.
#include "RavetVst3HostedAudio.h"
#include <dlfcn.h>
#include <algorithm>
#include <array>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <exception>
#include <iostream>
#include <memory>
#include <new>
#include <stdexcept>
#include <string>
#include <thread>
#include <type_traits>
#include <vector>

namespace {
thread_local bool measured = false;
thread_local size_t allocations = 0, deallocations = 0;
#define RAVET_STR2(x) #x
#define RAVET_STR(x) RAVET_STR2(x)
#define REQUIRE(x)                                                             \
  do {                                                                         \
    if (!(x))                                                                  \
      throw std::runtime_error("requirement failed line " RAVET_STR(           \
          __LINE__) ": " #x);                                                  \
  } while (0)

using Record = RavetVst3EventRecord;
constexpr uint64_t mono = 1ull << 19, stereo = 3, surround51 = 0x3f;
constexpr uint32_t maximumFrames = 64, iterations = 2000;

struct Api {
  const RavetVst3ProviderV2 *instance = ravet_vst3_provider_v2();
  const RavetVst3ParameterProviderV1 *parameters =
      ravet_vst3_parameter_provider_v1();
  const RavetVst3ProcessingProviderV1 *processing =
      ravet_vst3_processing_provider_v1();
  const RavetVst3HostedAudioProviderV1 *hosted =
      ravet_vst3_hosted_audio_provider_v1();
  Api() {
    REQUIRE(instance && parameters && processing && hosted);
    REQUIRE(hosted->instance_origin == instance &&
            hosted->processing_origin == processing &&
            processing->instance_provider == instance &&
            parameters->provider == instance);
  }
};
const Api &api() {
  static const Api value;
  return value;
}

// Rust heap-operation counter exported by a loaded reference bundle built with
// the allocation-probe feature. RTLD_NOLOAD finds the image the provider
// already loaded; the extra reference is released at once.
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

template <class T> T sized() {
  T value{};
  value.struct_size = sizeof(T);
  return value;
}

// One loaded module and one created child of its audio class.
struct Child {
  RavetVst3Module *module = nullptr;
  RavetVst3Instance *child = nullptr;
  RavetVst3Audio *audio = nullptr;
  bool started = false;
  uint32_t parameterCount = 0;
  RavetVst3HostedAudioInfo audioInfo = sized<RavetVst3HostedAudioInfo>();
  explicit Child(const std::string &path, uint32_t inputMask,
                 uint32_t outputMask) {
    int32_t native = 0;
    REQUIRE(api().instance->open(
                reinterpret_cast<const uint8_t *>(path.data()),
                uint32_t(path.size()), &module, &native) == RAVET_VST3_OK);
    auto factory = sized<RavetVst3FactoryRecord>();
    REQUIRE(api().instance->factory(module, &factory) == RAVET_VST3_OK);
    REQUIRE(std::strcmp(factory.vendor, "RAVET") == 0 &&
            factory.class_count == 2);
    uint32_t audioClass = UINT32_MAX;
    for (uint32_t i = 0; i < factory.class_count; ++i) {
      auto info = sized<RavetVst3ClassRecord>();
      REQUIRE(api().instance->class_info(module, i, &info) == RAVET_VST3_OK);
      if (std::strcmp(info.category, "Audio Module Class") == 0)
        audioClass = i;
    }
    REQUIRE(audioClass != UINT32_MAX);
    REQUIRE(api().instance->create(module, audioClass, &child, &native) ==
            RAVET_VST3_OK);
    auto parameterReport = sized<RavetVst3ParameterReport>();
    REQUIRE(api().parameters->refresh(child, &parameterCount,
                                      &parameterReport) == RAVET_VST3_OK);
    RavetVst3ProcessingSelection selection{sizeof(selection), inputMask,
                                           outputMask, 0};
    auto info = sized<RavetVst3ProcessingInfo>();
    auto report = sized<RavetVst3ProcessingReport>();
    REQUIRE(api().processing->refresh(child, &selection, &info, &report) ==
            RAVET_VST3_OK);
    REQUIRE(info.selected_input_mask == inputMask &&
            info.selected_output_mask == outputMask);
    REQUIRE(info.input_bus_count == (inputMask ? 1u : 0u) &&
            info.output_bus_count == (outputMask ? 1u : 0u));
    auto audioReport = sized<RavetVst3HostedAudioReport>();
    REQUIRE(api().hosted->refresh_audio(child, &audioInfo, &audioReport) ==
            RAVET_VST3_OK);
  }
  RavetVst3HostedAudioBus bus(uint32_t direction, uint32_t index) const {
    auto bus = sized<RavetVst3HostedAudioBus>();
    auto report = sized<RavetVst3HostedAudioReport>();
    REQUIRE(api().hosted->audio_bus(child, direction, index, &bus, &report) ==
            RAVET_VST3_OK);
    return bus;
  }
  int32_t prepare(const RavetVst3HostedAudioProfile &profile,
                  RavetVst3HostedAudioReport &report) {
    report = sized<RavetVst3HostedAudioReport>();
    return api().hosted->prepare(child, &profile, &report);
  }
  void start() {
    int32_t native = 0;
    auto timing = sized<RavetVst3HostedAudioTiming>();
    auto report = sized<RavetVst3HostedAudioReport>();
    REQUIRE(api().hosted->timing(child, &timing, &report) == RAVET_VST3_OK);
    REQUIRE(timing.latency_samples == 0 && timing.tail_kind == 0 &&
            timing.tail_samples == 0);
    REQUIRE(api().instance->start(child, &native) == RAVET_VST3_OK);
    started = true;
    REQUIRE(api().instance->begin_audio(child, &audio, &native) ==
            RAVET_VST3_OK);
  }
  ~Child() noexcept(false) {
    // Teardown order: end AUDIO, stop, destroy, close. Never throw while
    // another failure is already unwinding.
    int32_t native = 0;
    bool ok = true;
    if (audio)
      ok &= api().instance->end_audio(audio, &native) == RAVET_VST3_OK;
    if (child) {
      if (started)
        ok &= api().instance->stop(child, &native) == RAVET_VST3_OK;
      ok &= api().instance->destroy(child, &native) == RAVET_VST3_OK;
    }
    if (module)
      ok &= api().instance->close(module, &native) == RAVET_VST3_OK;
    if (!ok && std::uncaught_exceptions() == 0)
      throw std::runtime_error("ordered child/module teardown failed");
  }
};

RavetVst3HostedAudioProfile profile(uint32_t bits, uint32_t inputs,
                                    uint32_t outputs) {
  auto p = sized<RavetVst3HostedAudioProfile>();
  p.maximum_frames = maximumFrames;
  p.sample_bits = bits;
  p.sample_rate = 48000.;
  p.input_buses = inputs;
  p.output_buses = outputs;
  return p;
}

template <class S> struct Traits;
template <> struct Traits<float> {
  using Block = RavetVst3AudioBlock;
  using In = RavetVst3InputChannel;
  using Out = RavetVst3OutputChannel;
  static int32_t process(RavetVst3Audio *audio, const Block &block,
                         const RavetVst3ParameterPoint *points,
                         uint32_t pointCount, const Record *events,
                         uint32_t eventCount, const uint8_t *payload,
                         uint32_t payloadBytes,
                         RavetVst3ProcessingOutputs &outputs,
                         RavetVst3ProcessingReport &report) {
    return api().hosted->process32(audio, &block, points, pointCount, events,
                                   eventCount, payload, payloadBytes, nullptr,
                                   &outputs, &report);
  }
};
template <> struct Traits<double> {
  using Block = RavetVst3HostedAudioBlock64;
  using In = RavetVst3HostedInputChannel64;
  using Out = RavetVst3HostedOutputChannel64;
  static int32_t process(RavetVst3Audio *audio, const Block &block,
                         const RavetVst3ParameterPoint *points,
                         uint32_t pointCount, const Record *events,
                         uint32_t eventCount, const uint8_t *payload,
                         uint32_t payloadBytes,
                         RavetVst3ProcessingOutputs &outputs,
                         RavetVst3ProcessingReport &report) {
    return api().hosted->process64(audio, &block, points, pointCount, events,
                                   eventCount, payload, payloadBytes, nullptr,
                                   &outputs, &report);
  }
};

// Complete caller-owned output storage (maximum protocol capacities).
struct OutputStorage {
  std::vector<Record> events = std::vector<Record>(2048);
  std::vector<uint8_t> payload = std::vector<uint8_t>(65536);
  std::vector<uint32_t> ids = std::vector<uint32_t>(64);
  std::vector<RavetVst3ParameterPoint> points =
      std::vector<RavetVst3ParameterPoint>(2048);
  RavetVst3ProcessingOutputs wire{};
  RavetVst3ProcessingReport report{};
  void reset() {
    wire = {sizeof(wire),         0,
            events.data(),        uint32_t(events.size()),
            0,                    payload.data(),
            uint32_t(payload.size()), 0,
            ids.data(),           uint32_t(ids.size()),
            0,                    points.data(),
            uint32_t(points.size()), 0};
    report = sized<RavetVst3ProcessingReport>();
  }
};

double mainSample(uint32_t call, uint32_t channel, uint32_t frame) {
  return double((frame * 37 + call * 11 + channel * 5) % 101) / 50. - 1. +
         1e-10 * double(frame + 1);
}
double keySample(uint32_t call, uint32_t channel, uint32_t frame) {
  return double((frame * 13 + call * 7 + channel * 29) % 89) / 44. - 1.;
}

enum class Aux { Stereo, Mono, InactiveStereo, InactiveMono };

template <class S> void sidechain(const std::string &path, Aux aux) {
  using T = Traits<S>;
  constexpr uint32_t bits = sizeof(S) * 8;
  const bool active = aux == Aux::Stereo || aux == Aux::Mono;
  const uint64_t arrangement =
      aux == Aux::Stereo || aux == Aux::InactiveStereo ? stereo : mono;
  const uint32_t keys =
      active ? (arrangement == stereo ? 2u : 1u) : 0u;
  Child c(path, 0, 0);
  const auto rust = rustCounter(path);
  uint64_t rustOperations = 0;
  REQUIRE(c.parameterCount == 0);
  REQUIRE(c.audioInfo.input_buses == 2 && c.audioInfo.output_buses == 1);
  const auto main = c.bus(0, 0), side = c.bus(0, 1), out = c.bus(1, 0);
  REQUIRE(main.bus_type == 0 && main.channels == 2 &&
          main.arrangement == stereo && std::strcmp(main.name, "Main") == 0);
  REQUIRE(side.bus_type == 1 && side.channels == 2 &&
          side.arrangement == stereo && side.flags == 0 &&
          std::strcmp(side.name, "Sidechain") == 0);
  REQUIRE(out.bus_type == 0 && out.channels == 2 && out.flags == 1);
  auto p = profile(bits, 2, 1);
  p.inputs[0] = {stereo, 1, 0};
  p.inputs[1] = {arrangement, active ? 1u : 0u, 0};
  p.outputs[0] = {stereo, 1, 0};
  RavetVst3HostedAudioReport report{};
  REQUIRE(c.prepare(p, report) == RAVET_VST3_OK && report.failure == 0);
  c.start();
  const uint32_t inputs = 2 + keys;
  std::vector<std::array<S, maximumFrames + 4>> in(inputs), outBuffers(2);
  std::vector<typename T::In> inChannels(inputs);
  std::array<typename T::Out, 2> outChannels{};
  for (uint32_t i = 0; i < inputs; ++i)
    inChannels[i] = {in[i].data(), uint32_t(in[i].size()), 0};
  for (uint32_t i = 0; i < 2; ++i)
    outChannels[i] = {outBuffers[i].data(), uint32_t(outBuffers[i].size()), 0};
  typename T::Block block{sizeof(block), 0, inputs, 2, inChannels.data(),
                          outChannels.data()};
  OutputStorage storage;
  size_t compared = 0, zero = 0, wide = 0;
  std::exception_ptr failure;
  std::thread worker([&] {
   try {
    allocations = deallocations = 0;
    for (uint32_t call = 0; call < iterations; ++call) {
      const uint32_t frames = (call * 7) % (maximumFrames + 1);
      zero += frames == 0;
      for (uint32_t f = 0; f < maximumFrames + 4; ++f) {
        for (uint32_t ch = 0; ch < 2; ++ch) {
          in[ch][f] = S(mainSample(call, ch, f));
          outBuffers[ch][f] = S(9);
        }
        for (uint32_t k = 0; k < keys; ++k)
          in[2 + k][f] = S(keySample(call, k, f));
      }
      block.frames = frames;
      storage.reset();
      const uint64_t rustBefore = rust();
      measured = true;
      const auto status = T::process(c.audio, block, nullptr, 0, nullptr, 0,
                                     nullptr, 0, storage.wire, storage.report);
      measured = false;
      rustOperations += rust() - rustBefore;
      REQUIRE(status == RAVET_VST3_OK);
      REQUIRE(storage.wire.event_count == 0 &&
              storage.wire.parameter_queue_count == 0);
      for (uint32_t f = 0; f < frames; ++f) {
        const S key = keys == 2   ? (in[2][f] + in[3][f]) * S(0.5)
                      : keys == 1 ? in[2][f]
                                  : S(1);
        for (uint32_t ch = 0; ch < 2; ++ch) {
          REQUIRE(outBuffers[ch][f] == in[ch][f] * key);
          ++compared;
        }
      }
      for (uint32_t f = frames; f < maximumFrames + 4; ++f)
        REQUIRE(outBuffers[0][f] == S(9) && outBuffers[1][f] == S(9));
      if constexpr (std::is_same_v<S, double>)
        for (uint32_t f = 0; f < frames; ++f)
          wide += outBuffers[0][f] != double(float(outBuffers[0][f]));
    }
    REQUIRE(allocations == 0 && deallocations == 0);
    REQUIRE(rustOperations == 0);
   } catch (...) {
    measured = false;
    failure = std::current_exception();
   }
  });
  worker.join();
  if (failure)
    std::rethrow_exception(failure);
  // f64 keeps sub-f32 detail: most products are not f32-representable.
  REQUIRE(bits == 32 || wide > compared / 4);
  std::printf("PASS sidechain %ubit aux=%s keys=%u: %u blocks (%zu zero-frame, "
              "frames 0..64), %zu samples exact vs closed form, 0 counted "
              "C++ new/delete on AUDIO, 0 Rust heap operations in module\n",
              bits,
              aux == Aux::Stereo         ? "stereo-active"
              : aux == Aux::Mono         ? "mono-active"
              : aux == Aux::InactiveMono ? "mono-inactive"
                                         : "stereo-inactive",
              keys, iterations, zero, compared);
}

// Native arrangement refusal keeps its stage and returned SDK result.
void sidechainRefusal(const std::string &path) {
  Child c(path, 0, 0);
  auto p = profile(32, 2, 1);
  p.inputs[0] = {stereo, 1, 0};
  p.inputs[1] = {surround51, 1, 0};
  p.outputs[0] = {stereo, 1, 0};
  RavetVst3HostedAudioReport report{};
  REQUIRE(c.prepare(p, report) != RAVET_VST3_OK);
  REQUIRE(report.stage == 106 && report.failure == 2 &&
          (report.flags & RAVET_VST3_HOSTED_RETURNED) &&
          report.native_result == 1 /* kResultFalse */);
  std::printf("PASS sidechain 5.1 aux refused at stage 106 with returned "
              "kResultFalse\n");
}

// Independent canonical serialization and FNV-1a (see Rust record_hash docs).
struct Fnv {
  uint32_t h = 0x811c9dc5u;
  void byte(uint8_t b) { h = (h ^ b) * 0x01000193u; }
  void u16(uint16_t v) {
    byte(uint8_t(v));
    byte(uint8_t(v >> 8));
  }
  void u32(uint32_t v) {
    for (int i = 0; i < 4; ++i)
      byte(uint8_t(v >> (8 * i)));
  }
  void u64(uint64_t v) {
    for (int i = 0; i < 8; ++i)
      byte(uint8_t(v >> (8 * i)));
  }
  void f32(float v) {
    uint32_t b;
    std::memcpy(&b, &v, 4);
    u32(b);
  }
  void f64(double v) {
    uint64_t b;
    std::memcpy(&b, &v, 8);
    u64(b);
  }
  void text(const uint8_t *bytes, uint32_t count) {
    u32(count / 2);
    for (uint32_t i = 0; i < count; i += 2) {
      uint16_t unit;
      std::memcpy(&unit, bytes + i, 2);
      u16(unit);
    }
  }
  uint32_t fold() const { return (h ^ (h >> 20)) & 0xfffffu; }
};
uint32_t hashEvent(const Record &r, const uint8_t *payload) {
  Fnv f;
  f.u16(r.type);
  f.u16(r.flags);
  f.u32(r.bus);
  f.f64(r.ppq_position);
  const uint8_t *bytes = payload + r.payload_offset;
  switch (r.type) {
  case 0:
    for (int32_t v : {r.channel, r.pitch, r.note_id, r.note_length})
      f.u32(uint32_t(v));
    f.f32(r.tuning);
    f.f32(r.velocity);
    break;
  case 1:
    for (int32_t v : {r.channel, r.pitch, r.note_id})
      f.u32(uint32_t(v));
    f.f32(r.tuning);
    f.f32(r.velocity);
    break;
  case 2:
    f.u32(r.data_type);
    f.u32(r.payload_bytes);
    for (uint32_t i = 0; i < r.payload_bytes; ++i)
      f.byte(bytes[i]);
    break;
  case 3:
    for (int32_t v : {r.channel, r.pitch, r.note_id})
      f.u32(uint32_t(v));
    f.f32(r.velocity);
    break;
  case 4:
    f.u32(r.expression_id);
    f.u32(uint32_t(r.note_id));
    f.f64(r.value);
    break;
  case 5:
    f.u32(r.expression_id);
    f.u32(uint32_t(r.note_id));
    f.text(bytes, r.payload_bytes);
    break;
  case 6:
    f.u32(uint32_t(r.root));
    f.u32(uint32_t(r.bass));
    f.u32(r.mask);
    f.text(bytes, r.payload_bytes);
    break;
  case 7:
    f.u32(uint32_t(r.root));
    f.u32(r.mask);
    f.text(bytes, r.payload_bytes);
    break;
  case 8:
    f.u32(r.expression_id);
    f.u32(uint32_t(r.note_id));
    f.u64(r.integer_value);
    break;
  default:
    throw std::runtime_error("unexpected kind");
  }
  return f.fold();
}
uint32_t hashPoint(uint32_t id, double value) {
  Fnv f;
  f.u16(16);
  f.u16(0);
  f.u32(0);
  f.f64(0.);
  f.u32(id);
  f.f64(value);
  return f.fold();
}
uint32_t digestStep(uint32_t digest, uint32_t hash) {
  return (digest * 0x01000193u ^ hash) & 0xfffffu;
}

// Deterministic per-call batch: one event of each of the nine kinds plus up
// to two Marker points.
struct Batch {
  std::array<Record, 9> events{};
  alignas(8) std::array<uint8_t, 256> payload{};
  uint32_t payloadUsed = 0;
  bool loneNoteOn = false;
  std::array<RavetVst3ParameterPoint, 2> points{};
  uint32_t pointCount = 0;
  uint32_t put(const void *bytes, uint32_t count) {
    payloadUsed = (payloadUsed + 1) & ~1u;
    const auto offset = payloadUsed;
    std::memcpy(payload.data() + offset, bytes, count);
    payloadUsed += count;
    return offset;
  }
  void fill(uint32_t call, uint32_t frames) {
    payloadUsed = 0;
    static const char16_t *texts[] = {u"", u"A", u"Cmaj7", u"\U0001D11E x",
                                      u"été"};
    const auto text = [&](uint32_t which, Record &r) {
      const char16_t *t = texts[which % 5];
      uint32_t units = 0;
      while (t[units])
        ++units;
      r.payload_offset = put(t, units * 2);
      r.payload_bytes = units * 2;
    };
    for (uint32_t i = 0; i < 9; ++i) {
      Record r{};
      r.struct_size = sizeof(r);
      r.type = uint16_t(i);
      r.bus = 0;
      r.offset = frames ? i * frames / 9 : 0;
      r.flags = (call + i) % 3 == 0 ? 0xc001 : (call % 2);
      r.ppq_position = double(call) * 0.25 - double(i) * 0.125;
      events[i] = r;
    }
    auto &on = events[0];
    on.channel = int32_t(call % 16), on.pitch = int32_t(call % 128),
    on.note_id = call % 5 == 0 ? -1 : int32_t(call), on.note_length = 9;
    on.tuning = -3.25f + float(call % 7), on.velocity = float(call % 9) / 8.f;
    auto &off = events[1];
    off.channel = 15, off.pitch = int32_t((call * 3) % 128),
    off.note_id = int32_t(call), off.tuning = 0.5f, off.velocity = 0.25f;
    auto &data = events[2];
    const uint8_t sysex[] = {0xf0, 0x7d, uint8_t(call & 0x7f),
                             uint8_t((call >> 7) & 0x7f), 0xf7};
    data.data_type = 0;
    data.payload_bytes = 2 + call % 4;
    data.payload_offset = put(sysex, data.payload_bytes);
    auto &poly = events[3];
    poly.channel = 2, poly.pitch = 64, poly.note_id = -1,
    poly.velocity = float(call % 5) / 4.f;
    auto &value = events[4];
    value.expression_id = call % 8, value.note_id = int32_t(call),
    value.value = double(call % 101) / 100.;
    auto &textEvent = events[5];
    textEvent.expression_id = 100008, textEvent.note_id = int32_t(call);
    text(call, textEvent);
    auto &chord = events[6];
    chord.root = int32_t(call % 128), chord.bass = 57,
    chord.mask = 0xffffu - call % 3;
    text(call + 1, chord);
    auto &scale = events[7];
    scale.root = 60, scale.mask = 0xa5a5u ^ (call & 0xff);
    text(call + 2, scale);
    auto &integer = events[8];
    integer.expression_id = 100009, integer.note_id = 0,
    integer.integer_value = UINT64_MAX - call;
    pointCount = frames > 1 ? 2 : 1;
    // Marker point 0 takes the first offset below frames - 1 that no event
    // uses, so NoteOn at offset 0 is observed alone whenever frames >= 11.
    // Otherwise it stays at offset 0. Point offsets must be strictly
    // increasing per ID, so point 0 never shares point 1's last-frame offset.
    uint32_t free = 0;
    for (uint32_t o = 0; o + 1 < frames; ++o) {
      bool used = false;
      for (uint32_t i = 0; i < 9; ++i)
        used = used || events[i].offset == o;
      if (!used) {
        free = o;
        break;
      }
    }
    points[0] = {1, free, double(call % 17) / 16.};
    points[1] = {1, frames ? frames - 1 : 0, 0.75};
    loneNoteOn = free != 0 && events[1].offset != 0;
  }
};

template <class S> void probe(const std::string &path) {
  using T = Traits<S>;
  constexpr uint32_t bits = sizeof(S) * 8;
  Child c(path, 1, 1);
  const auto rust = rustCounter(path);
  uint64_t rustOperations = 0;
  REQUIRE(c.parameterCount == 2);
  for (uint32_t i = 0; i < 2; ++i) {
    auto record = sized<RavetVst3ParameterRecord>();
    REQUIRE(api().parameters->info(c.child, i, &record) == RAVET_VST3_OK);
    REQUIRE(record.id == i + 1 && record.flags == (i ? 2u : 1u));
  }
  for (uint32_t direction = 0; direction < 2; ++direction) {
    auto bus = sized<RavetVst3EventBusRecord>();
    REQUIRE(api().processing->bus_info(c.child, direction, 0, &bus) ==
            RAVET_VST3_OK);
    REQUIRE(bus.channels == 16 &&
            std::strcmp(bus.name, direction ? "Echo" : "Events") == 0);
  }
  REQUIRE(c.audioInfo.input_buses == 0 && c.audioInfo.output_buses == 1);
  auto p = profile(bits, 0, 1);
  p.outputs[0] = {stereo, 1, 0};
  RavetVst3HostedAudioReport report{};
  REQUIRE(c.prepare(p, report) == RAVET_VST3_OK);
  c.start();
  std::array<std::array<S, maximumFrames + 4>, 2> out{};
  std::array<typename T::Out, 2> outChannels{};
  for (uint32_t i = 0; i < 2; ++i)
    outChannels[i] = {out[i].data(), uint32_t(out[i].size()), 0};
  typename T::Block block{sizeof(block), 0, 0, 2, nullptr, outChannels.data()};
  OutputStorage storage;
  auto batch = std::make_unique<Batch>();
  size_t zero = 0, events = 0, points = 0, impulses = 0, lone = 0;
  std::exception_ptr failure;
  std::thread worker([&] {
   try {
    allocations = deallocations = 0;
    uint32_t digest = 0;
    for (uint32_t call = 0; call < iterations; ++call) {
      const uint32_t frames = (call * 11) % (maximumFrames + 1);
      zero += frames == 0;
      batch->fill(call, frames);
      for (auto &channel : out)
        channel.fill(S(9));
      block.frames = frames;
      storage.reset();
      const uint64_t rustBefore = rust();
      measured = true;
      const auto status =
          T::process(c.audio, block, batch->points.data(), batch->pointCount,
                     batch->events.data(), 9, batch->payload.data(),
                     batch->payloadUsed, storage.wire, storage.report);
      measured = false;
      rustOperations += rust() - rustBefore;
      REQUIRE(status == RAVET_VST3_OK);
      // Independent expected audio, digest, echo, count and parameter.
      std::array<S, maximumFrames> left{}, right{};
      uint32_t lastOffset = 0;
      for (const auto &r : batch->events) {
        const auto h = hashEvent(r, batch->payload.data());
        digest = digestStep(digest, h);
        lastOffset = std::max(lastOffset, r.offset);
        if (frames) {
          left[r.offset] += S(r.type + 1);
          right[r.offset] += S(h) / S(1048576);
        }
      }
      for (uint32_t i = 0; i < batch->pointCount; ++i) {
        const auto h = hashPoint(1, batch->points[i].value);
        digest = digestStep(digest, h);
        if (frames) {
          left[batch->points[i].offset] += S(17);
          right[batch->points[i].offset] += S(h) / S(1048576);
        }
      }
      for (uint32_t f = 0; f < frames; ++f) {
        REQUIRE(out[0][f] == left[f] && out[1][f] == right[f]);
        impulses += left[f] != S(0);
      }
      for (uint32_t f = frames; f < maximumFrames + 4; ++f)
        REQUIRE(out[0][f] == S(9) && out[1][f] == S(9));
      REQUIRE(storage.wire.event_count == 10);
      for (uint32_t i = 0; i < 9; ++i) {
        const auto &e = storage.events[i];
        const auto &r = batch->events[i];
        REQUIRE(e.type == r.type && e.bus == 0 && e.offset == r.offset &&
                e.flags == r.flags && e.ppq_position == r.ppq_position);
        REQUIRE(e.payload_bytes == r.payload_bytes &&
                std::memcmp(storage.payload.data() + e.payload_offset,
                            batch->payload.data() + r.payload_offset,
                            r.payload_bytes) == 0);
        REQUIRE(hashEvent(e, storage.payload.data()) ==
                hashEvent(r, batch->payload.data()));
      }
      const auto &cc = storage.events[9];
      REQUIRE(cc.type == RAVET_VST3_EVENT_LEGACY_CC_OUTPUT &&
              cc.controller == 16 && cc.channel == 0 && cc.midi_value == 9 &&
              cc.midi_value2 == 0 && cc.offset == lastOffset);
      REQUIRE(storage.wire.parameter_queue_count == 1 && storage.ids[0] == 2 &&
              storage.wire.parameter_point_count == 1);
      REQUIRE(storage.points[0].id == 2 &&
              storage.points[0].offset == (frames ? frames - 1 : 0) &&
              storage.points[0].value == double(digest) / 1048576.);
      events += 9;
      points += batch->pointCount;
      lone += batch->loneNoteOn;
    }
    REQUIRE(allocations == 0 && deallocations == 0);
    REQUIRE(rustOperations == 0);
   } catch (...) {
    measured = false;
    failure = std::current_exception();
   }
  });
  worker.join();
  if (failure)
    std::rethrow_exception(failure);
  std::printf("PASS event probe %ubit: %u blocks (%zu zero-frame, frames "
              "0..64), %zu events of all nine kinds + %zu Marker points, %zu "
              "nonzero impulse samples exact (%zu blocks with a lone NoteOn "
              "impulse), echo/CC/digest parameter exact, 0 counted C++ "
              "new/delete on AUDIO, 0 Rust heap operations in module\n",
              bits, iterations, zero, events, points, impulses, lone);
}
} // namespace

void *operator new(std::size_t size) {
  if (measured)
    ++allocations;
  if (auto *p = std::malloc(size ? size : 1))
    return p;
  throw std::bad_alloc();
}
void *operator new[](std::size_t size) { return ::operator new(size); }
void operator delete(void *pointer) noexcept {
  if (pointer && measured)
    ++deallocations;
  std::free(pointer);
}
void operator delete[](void *pointer) noexcept { ::operator delete(pointer); }
void operator delete(void *pointer, std::size_t) noexcept {
  ::operator delete(pointer);
}
void operator delete[](void *pointer, std::size_t) noexcept {
  ::operator delete(pointer);
}

int main(int argc, char **argv) {
  if (argc != 3) {
    std::cerr << "usage: reference-module-tests /abs/sidechain.vst3 "
                 "/abs/event-probe.vst3\n";
    return 2;
  }
  try {
    const std::string sidechainPath = argv[1], probePath = argv[2];
    for (auto aux :
         {Aux::Stereo, Aux::Mono, Aux::InactiveStereo, Aux::InactiveMono}) {
      sidechain<float>(sidechainPath, aux);
      sidechain<double>(sidechainPath, aux);
    }
    sidechainRefusal(sidechainPath);
    probe<float>(probePath);
    probe<double>(probePath);
    std::printf("11 actual reference-module scenarios passed\n");
  } catch (const std::exception &error) {
    std::cerr << error.what() << '\n';
    return 1;
  }
  return 0;
}
