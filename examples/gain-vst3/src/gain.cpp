// Public reference adapter. No private consumer code or SDK source is vendored.
#include "RavetGain.h"
#include "pluginterfaces/base/ibstream.h"
#include "pluginterfaces/vst/ivstparameterchanges.h"
#include "pluginterfaces/vst/ivsthostapplication.h"
#include "public.sdk/source/main/pluginfactory.h"
#include "public.sdk/source/vst/vstaudioeffect.h"
#include "public.sdk/source/vst/vsteditcontroller.h"
#include <algorithm>
#include <array>
#include <atomic>
#include <cmath>
#include <cstring>
#if defined(RAVET_GAIN_PROGRAMS)
#include "RavetGainPrograms.h"
#include "pluginterfaces/base/ustring.h"
#endif
#if defined(__APPLE__) && defined(RAVET_NATIVE_WEB_EDITOR)
#include "RavetGainEditor.h"
#include "RavetGainMeters.h"
#include "RavetWebEditor.h"
#include "public.sdk/source/common/pluginview.h"
#include <optional>
#endif

using namespace Steinberg;
using namespace Steinberg::Vst;
namespace {
const FUID processorID(0xAFF1AE72, 0xCC374810, 0x89136FEE, 0xF4348390);
const FUID controllerID(0xBFF1AE72, 0xCC374810, 0x89136FEE, 0xF4348390);
constexpr ParamID gainID = 1, bypassID = 2;
#if defined(RAVET_GAIN_PROGRAMS)
constexpr ParamID programID = 3;
#endif
constexpr size_t maximumEvents = 1024;
#if defined(__APPLE__) && defined(RAVET_NATIVE_WEB_EDITOR)
constexpr ParamID firstMeterID = 10;
constexpr int32 meterParameterCount = 12;
// Telemetry saturates at four, retaining the native DSP/audio precision.
double meterLevel(double value) {
  return std::isfinite(value) ? std::clamp(value / 4., 0., 1.) : (value > 0. ? 1. : 0.);
}
#endif
constexpr const char *controllerRestoreID = "ravet.controller.restore.v1";
uint64_t pack(float gain, uint32_t bypass, uint32_t generation) {
  uint32_t bits;
  std::memcpy(&bits, &gain, sizeof(bits));
  return uint64_t(bits) | (uint64_t(bypass) << 32) |
         (uint64_t(generation & 0x7fffffff) << 33);
}
float unpackGain(uint64_t word) {
  uint32_t bits = uint32_t(word);
  float value;
  std::memcpy(&value, &bits, sizeof(value));
  return value;
}
uint32_t unpackBypass(uint64_t word) { return uint32_t((word >> 32) & 1); }
uint32_t generation(uint64_t word) { return uint32_t(word >> 33); }
static_assert(std::atomic<uint64_t>::is_always_lock_free,
              "Native gain requires lock-free 64-bit atomics");
bool readState(IBStream *stream, float &gain, uint32_t &bypass) {
  if (!stream)
    return false;
  std::array<uint8_t, 33> bytes{};
  size_t used = 0;
  while (used < bytes.size()) {
    int32 count = 0;
    auto result =
        stream->read(bytes.data() + used, int32(bytes.size() - used), &count);
    if (count < 0 || size_t(count) > bytes.size() - used)
      return false;
    used += size_t(count);
    if (result != kResultOk && result != kResultFalse)
      return false;
    if (count == 0)
      break;
  }
  return ravet_gain_decode(bytes.data(), used, &gain, &bypass) == 0;
}
bool writeState(IBStream *stream, float gain, uint32_t bypass) {
  if (!stream)
    return false;
  std::array<uint8_t, 32> bytes{};
  if (ravet_gain_encode(gain, bypass, bytes.data(), bytes.size()) != 0)
    return false;
  size_t used = 0;
  while (used < bytes.size()) {
    int32 count = 0;
    if (stream->write(bytes.data() + used, int32(bytes.size() - used),
                      &count) != kResultOk ||
        count <= 0 || size_t(count) > bytes.size() - used)
      return false;
    used += size_t(count);
  }
  return true;
}
void silence(ProcessData &data) {
  if (data.numSamples < 0 || !data.outputs)
    return;
  for (int32 bus = 0; bus < data.numOutputs; ++bus) {
    auto &output = data.outputs[bus];
    if (data.symbolicSampleSize == kSample32 && output.channelBuffers32) {
      for (int32 channel = 0; channel < output.numChannels; ++channel)
        if (output.channelBuffers32[channel])
          std::fill_n(output.channelBuffers32[channel], data.numSamples, 0.f);
    } else if (data.symbolicSampleSize == kSample64 &&
               output.channelBuffers64) {
      for (int32 channel = 0; channel < output.numChannels; ++channel)
        if (output.channelBuffers64[channel])
          std::fill_n(output.channelBuffers64[channel], data.numSamples, 0.);
    }
    output.silenceFlags = ~uint64_t(0);
  }
}
class GainProcessor final : public AudioEffect {
public:
  GainProcessor() : dsp(ravet_gain_create()), dsp64(ravet_gain64_create()) {
    setControllerClass(controllerID);
  }
  ~GainProcessor() override { ravet_gain_destroy(dsp); ravet_gain64_destroy(dsp64); }
  static FUnknown *create(void *) {
    return static_cast<IAudioProcessor *>(new GainProcessor);
  }
  tresult PLUGIN_API initialize(FUnknown *context) override {
    auto result = AudioEffect::initialize(context);
    if (result != kResultOk || !dsp || !dsp64)
      return kResultFalse;
    addAudioInput(STR16("Input"), SpeakerArr::kStereo);
    addAudioOutput(STR16("Output"), SpeakerArr::kStereo);
    return kResultOk;
  }
  tresult PLUGIN_API setBusArrangements(SpeakerArrangement *inputs,
                                        int32 numInputs,
                                        SpeakerArrangement *outputs,
                                        int32 numOutputs) override {
    if (!inputs || !outputs || numInputs != 1 || numOutputs != 1 ||
        inputs[0] != outputs[0] ||
        (inputs[0] != SpeakerArr::kMono && inputs[0] != SpeakerArr::kStereo))
      return kResultFalse;
    auto result =
        AudioEffect::setBusArrangements(inputs, numInputs, outputs, numOutputs);
    if (result == kResultOk) {
      if (channels != SpeakerArr::getChannelCount(outputs[0]))
        prepared = false;
      channels = SpeakerArr::getChannelCount(outputs[0]);
    }
    return result;
  }
  tresult PLUGIN_API canProcessSampleSize(int32 size) override {
    return size == kSample32 || size == kSample64 ? kResultOk : kResultFalse;
  }
  tresult PLUGIN_API setupProcessing(ProcessSetup &setup) override {
    prepared = false;
    if ((setup.symbolicSampleSize != kSample32 && setup.symbolicSampleSize != kSample64) ||
        setup.maxSamplesPerBlock <= 0 || setup.maxSamplesPerBlock > 65536 ||
        !std::isfinite(setup.sampleRate) || setup.sampleRate <= 0.)
      return kResultFalse;
    sampleSize = setup.symbolicSampleSize;
    const auto preparation = sampleSize == kSample64
        ? ravet_gain64_prepare(dsp64, setup.sampleRate, uint32_t(channels), uint32_t(setup.maxSamplesPerBlock))
        : ravet_gain_prepare(dsp, setup.sampleRate, uint32_t(channels), uint32_t(setup.maxSamplesPerBlock));
    if (preparation != 0)
      return kResultFalse;
    maximumFrames = setup.maxSamplesPerBlock;
    auto current = state.load(std::memory_order_acquire);
    if (restoreDSP(unpackGain(current), unpackBypass(current)) !=
        0)
      return kResultFalse;
    appliedGeneration = generation(current);
    prepared = true;
    return AudioEffect::setupProcessing(setup);
  }
  tresult PLUGIN_API setProcessing(TBool active) override {
    if (!active) {
      ravet_gain_reset(dsp);
      ravet_gain64_reset(dsp64);
    }
    return kResultOk;
  }
  tresult PLUGIN_API setState(IBStream *stream) override {
    float gain;
    uint32_t bypass;
    if (!readState(stream, gain, bypass))
      return kResultFalse;
    publishRestore(gain, bypass);
    return kResultOk;
  }
  tresult PLUGIN_API notify(IMessage *message) override {
    if (!message || !message->getMessageID())
      return kInvalidArgument;
    if (std::strcmp(message->getMessageID(), controllerRestoreID) != 0)
      return AudioEffect::notify(message);
    const void *bytes = nullptr;
    uint32 size = 0;
    auto *attributes = message->getAttributes();
    float gain;
    uint32_t bypass;
    if (!attributes || attributes->getBinary("state", bytes, size) != kResultOk ||
        !bytes || ravet_gain_decode(static_cast<const uint8_t *>(bytes), size,
                                   &gain, &bypass) != 0)
      return kResultFalse;
    publishRestore(gain, bypass);
    return kResultOk;
  }
  void publishRestore(float gain, uint32_t bypass) {
    auto old = state.load(std::memory_order_acquire);
    while (!state.compare_exchange_weak(
        old, pack(gain, bypass, generation(old) + 1), std::memory_order_acq_rel,
        std::memory_order_acquire)) {
    }
  }
  tresult PLUGIN_API getState(IBStream *stream) override {
    auto current = state.load(std::memory_order_acquire);
    return writeState(stream, unpackGain(current), unpackBypass(current))
               ? kResultOk
               : kResultFalse;
  }
  tresult PLUGIN_API process(ProcessData &data) override {
    if (!prepared || data.symbolicSampleSize != sampleSize ||
        data.numSamples < 0 || data.numSamples > maximumFrames) {
      silence(data);
      return kResultFalse;
    }
    if (data.numSamples > 0 &&
        (data.numInputs != 1 || data.numOutputs != 1 || !data.inputs ||
         !data.outputs || data.inputs[0].numChannels != channels ||
         data.outputs[0].numChannels != channels)) {
      silence(data);
      return kResultFalse;
    }
    if (data.numSamples > 0) {
      if (sampleSize == kSample64) {
        if (!data.inputs[0].channelBuffers64 || !data.outputs[0].channelBuffers64) {
          silence(data); return kResultFalse;
        }
        for (int32 channel = 0; channel < channels; ++channel)
          if (!data.inputs[0].channelBuffers64[channel] || !data.outputs[0].channelBuffers64[channel]) {
            silence(data); return kResultFalse;
          }
      } else {
        if (!data.inputs[0].channelBuffers32 || !data.outputs[0].channelBuffers32) {
          silence(data); return kResultFalse;
        }
        for (int32 channel = 0; channel < channels; ++channel)
          if (!data.inputs[0].channelBuffers32[channel] || !data.outputs[0].channelBuffers32[channel]) {
            silence(data); return kResultFalse;
          }
      }
    }
    size_t used = 0, seen = 0;
    if (data.inputParameterChanges) {
      auto queueCount = data.inputParameterChanges->getParameterCount();
      if (queueCount < 0 || queueCount > int32(maximumEvents)) {
        silence(data);
        return kResultFalse;
      }
      for (int32 index = 0; index < queueCount; ++index) {
        auto *queue = data.inputParameterChanges->getParameterData(index);
        if (!queue) {
          silence(data);
          return kResultFalse;
        }
        int32 count = queue->getPointCount();
        if (count < 0 || size_t(count) > maximumEvents - seen) {
          silence(data);
          return kResultFalse;
        }
        seen += size_t(count);
        auto id = queue->getParameterId();
        int32 previous = 0;
        for (int32 point = 0; point < count; ++point) {
          int32 offset;
          ParamValue value;
          if (queue->getPoint(point, offset, value) != kResultOk ||
              offset < previous || offset < 0 ||
              (data.numSamples == 0 ? offset != 0
                                    : offset >= data.numSamples) ||
              !std::isfinite(value) || value < 0. || value > 1.) {
            silence(data);
            return kResultFalse;
          }
          previous = offset;
          auto insertEvent = [&](RavetGainEvent event) {
            // Stable bounded insertion: preserve queue traversal order on ties.
            size_t insert = used;
            while (insert > 0 && events[insert - 1].offset > event.offset) {
              events[insert] = events[insert - 1]; --insert;
            }
            events[insert] = event; ++used;
          };
#if defined(RAVET_GAIN_PROGRAMS)
          if (id == programID) {
            RavetGainProgramInfo program{};
            const auto selected = uint32_t(std::floor(value * double(ravet_gain_program_count() - 1) + .5));
            if (maximumEvents - used < 2 || ravet_gain_program_info(selected, &program) != 0) {
              silence(data); return kResultFalse;
            }
            insertEvent({uint32_t(offset), gainID, program.gain});
            insertEvent({uint32_t(offset), bypassID, float(program.bypass)});
            continue;
          }
#endif
          if (id != gainID && id != bypassID) continue;
          if (used == maximumEvents) { silence(data); return kResultFalse; }
          insertEvent({uint32_t(offset), id, float(id == gainID ? value * 4. : value)});
        }
      }
    }
    // Validate the entire host stream before applying an accepted restore.
    auto current = state.load(std::memory_order_acquire);
    if (generation(current) != appliedGeneration) {
      if (restoreDSP(unpackGain(current), unpackBypass(current)) !=
          0) {
        silence(data);
        return kResultFalse;
      }
      appliedGeneration = generation(current);
    }
    float gain = unpackGain(current);
    uint32_t bypass = unpackBypass(current);
    int32_t result;
    if (sampleSize == kSample64) {
      const double *const *inputs = data.numSamples == 0 ? nullptr : data.inputs[0].channelBuffers64;
      double *const *outputs = data.numSamples == 0 ? nullptr : data.outputs[0].channelBuffers64;
      result = ravet_gain64_process(dsp64, inputs, outputs, uint32_t(channels), uint32_t(data.numSamples), events.data(), uint32_t(used), &gain, &bypass);
    } else {
      const float *const *inputs = data.numSamples == 0 ? nullptr : data.inputs[0].channelBuffers32;
      float *const *outputs = data.numSamples == 0 ? nullptr : data.outputs[0].channelBuffers32;
      result = ravet_gain_process(dsp, inputs, outputs, uint32_t(channels), uint32_t(data.numSamples), events.data(), uint32_t(used), &gain, &bypass);
    }
    if (result != 0) {
      if (result == 2)
        prepared = false;
      silence(data);
      return kResultFalse;
    }
#if defined(__APPLE__) && defined(RAVET_NATIVE_WEB_EDITOR)
    // The audio owner copies readings, then the host mediates processor to
    // controller delivery. Never expose the live DSP pointer to the editor.
    if (data.numSamples > 0 && data.outputParameterChanges) {
      RavetGainMeters meters{};
      const auto copied = sampleSize == kSample64 ? ravet_gain64_meters(dsp64,&meters) : ravet_gain_meters(dsp,&meters);
      if (copied == 0) {
        const RavetGainMeterReading readings[]{meters.input[0],meters.input[1],meters.output[0],meters.output[1]};
        for (int32 reading=0; reading<4; ++reading) {
          const double values[]{meterLevel(readings[reading].peak),meterLevel(readings[reading].rms),double(readings[reading].nonfinite!=0)};
          for (int32 field=0; field<3; ++field) {
            int32 index=0;
            auto* queue=data.outputParameterChanges->addParameterData(firstMeterID+3*reading+field,index);
            if (queue) queue->addPoint(data.numSamples-1,values[field],index);
          }
        }
      }
    }
#endif
    // A concurrent restore must win over this callback's reflection.
    state.compare_exchange_strong(
        current, pack(gain, bypass, appliedGeneration),
        std::memory_order_release, std::memory_order_relaxed);
    if (data.numOutputs > 0 && data.outputs) {
      uint64_t silentChannels = 0;
      if (data.numSamples > 0) {
        for (int32 channel = 0; channel < channels; ++channel) {
          bool silent = true;
          for (int32 frame = 0; frame < data.numSamples; ++frame) {
            const double sample = sampleSize == kSample64
                ? data.outputs[0].channelBuffers64[channel][frame]
                : data.outputs[0].channelBuffers32[channel][frame];
            if (sample != 0.) { silent = false; break; }
          }
          if (silent) silentChannels |= uint64_t(1) << channel;
        }
      }
      data.outputs[0].silenceFlags = silentChannels;
    }
    return kResultOk;
  }

private:
  int32_t restoreDSP(float gain, uint32_t bypass) {
    return sampleSize == kSample64 ? ravet_gain64_restore(dsp64, gain, bypass) : ravet_gain_restore(dsp, gain, bypass);
  }
  RavetGain *dsp = nullptr;
  RavetGain64 *dsp64 = nullptr;
  int32 channels = 2, maximumFrames = 0, sampleSize = kSample32;
  bool prepared = false;
  uint32_t appliedGeneration = 0;
  std::atomic<uint64_t> state{pack(1.f, 0, 0)};
  std::array<RavetGainEvent, maximumEvents> events{};
};
#if defined(RAVET_GAIN_PROGRAMS)
using GainControllerBase = EditControllerEx1;
#else
using GainControllerBase = EditController;
#endif
class GainController final : public GainControllerBase {
public:
  static FUnknown *create(void *) {
    return static_cast<IEditController *>(new GainController);
  }
  tresult PLUGIN_API initialize(FUnknown *context) override {
    auto result = EditController::initialize(context);
    if (result != kResultOk)
      return result;
    // The AU SDK exposes this marker and owns normalized parameter snapshots
    // separately. Its controller values may lag SetParameter until a UI timer;
    // treating that snapshot as authoritative would overwrite processor recall.
    persistControllerControls = !FUnknownPtr<IVst3ToAUWrapper>(context);
    parameters.addParameter(new RangeParameter(STR16("Gain"), gainID,
                                               STR16("x"), 0., 4., 1., 0,
                                               ParameterInfo::kCanAutomate));
    parameters.addParameter(
        STR16("Bypass"), nullptr, 1, 0.,
        ParameterInfo::kCanAutomate | ParameterInfo::kIsBypass, bypassID);
#if defined(RAVET_GAIN_PROGRAMS)
    addUnit(new Unit(STR16("Gain"), kRootUnitId, kNoParentUnitId, programID));
    auto* list = new ProgramList(STR16("Program"), programID, kRootUnitId);
    addProgramList(list);
    for (uint32_t index = 0; index < ravet_gain_program_count(); ++index) {
      RavetGainProgramInfo info{}; String128 name{};
      if (ravet_gain_program_info(index, &info) != 0) return kResultFalse;
      UString(name,128).fromAscii(info.name);
      list->addProgram(name);
    }
    auto* parameter = list->getParameter();
    parameter->getInfo().flags &= ~ParameterInfo::kCanAutomate;
    parameters.addParameter(parameter);
#endif
#if defined(__APPLE__) && defined(RAVET_NATIVE_WEB_EDITOR)
    const char16* labels[]{u"Input L peak",u"Input L RMS",u"Input L invalid",u"Input R peak",u"Input R RMS",u"Input R invalid",u"Output L peak",u"Output L RMS",u"Output L invalid",u"Output R peak",u"Output R RMS",u"Output R invalid"};
    for (int32 index=0; index<meterParameterCount; ++index) {
      parameters.addParameter(labels[index],nullptr,index%3==2?1:0,0.,
          ParameterInfo::kIsReadOnly|ParameterInfo::kIsHidden,firstMeterID+index);
    }
#endif
    return kResultOk;
  }
#if defined(RAVET_GAIN_PROGRAMS)
  tresult PLUGIN_API setParamNormalized(ParamID id, ParamValue value) override {
    if (programChanging) return kResultFalse;
    if (id != programID) return EditController::setParamNormalized(id, value);
    if (!getParameterObject(programID) || !std::isfinite(value) || value < 0. || value > 1.) return kResultFalse;
    const auto index = uint32_t(std::floor(value * double(ravet_gain_program_count() - 1) + .5));
    RavetGainProgramInfo program{};
    if (ravet_gain_program_info(index, &program) != 0) return kResultFalse;
    const double oldProgram = getParamNormalized(programID), oldGain = getParamNormalized(gainID), oldBypass = getParamNormalized(bypassID);
    EditController::setParamNormalized(programID, double(index) / double(ravet_gain_program_count() - 1));
    EditController::setParamNormalized(gainID, double(program.gain) / 4.);
    EditController::setParamNormalized(bypassID, double(program.bypass));
    programChanging = true;
    const auto notified = componentHandler ? componentHandler->restartComponent(kParamValuesChanged) : kResultOk;
    programChanging = false;
    if (notified != kResultOk) {
      EditController::setParamNormalized(programID, oldProgram);
      EditController::setParamNormalized(gainID, oldGain);
      EditController::setParamNormalized(bypassID, oldBypass);
      return kResultFalse;
    }
    return kResultOk;
  }
#endif
#if defined(__APPLE__) && defined(RAVET_NATIVE_WEB_EDITOR)
  IPlugView* PLUGIN_API createView(FIDString name) override;
#endif
  tresult PLUGIN_API setComponentState(IBStream *stream) override {
#if defined(RAVET_GAIN_PROGRAMS)
    if (programChanging) return kResultFalse;
#endif
    float gain;
    uint32_t bypass;
    if (!readState(stream, gain, bypass))
      return kResultFalse;
    setParamNormalized(gainID, double(gain) / 4.);
    setParamNormalized(bypassID, double(bypass));
    return kResultOk;
  }
  // Hosts may stop process() while their bypass switch is enabled. Persist
  // controller edits as well as processor state, including undelivered queues.
  tresult PLUGIN_API getState(IBStream *stream) override {
    if (!persistControllerControls)
      return EditController::getState(stream);
    return writeState(stream, float(getParamNormalized(gainID) * 4.),
                      getParamNormalized(bypassID) >= .5 ? 1 : 0)
               ? kResultOk : kResultFalse;
  }
  tresult PLUGIN_API setState(IBStream *stream) override {
#if defined(RAVET_GAIN_PROGRAMS)
    if (programChanging) return kResultFalse;
#endif
    if (!persistControllerControls)
      return EditController::setState(stream);
    float gain;
    uint32_t bypass;
    if (!readState(stream, gain, bypass))
      return kResultFalse;
    setParamNormalized(gainID, double(gain) / 4.);
    setParamNormalized(bypassID, double(bypass));
    pendingRestore = true;
    synchronizeRestore();
    return kResultOk;
  }
  tresult PLUGIN_API connect(IConnectionPoint *other) override {
    auto result = EditController::connect(other);
    if (result == kResultOk && pendingRestore)
      synchronizeRestore();
    return result;
  }
private:
#if defined(RAVET_GAIN_PROGRAMS)
  bool programChanging = false;
#endif
  bool persistControllerControls = true;
  bool pendingRestore = false;
  void synchronizeRestore() {
    if (!getPeer())
      return;
    auto message = owned(allocateMessage());
    if (!message)
      return;
    std::array<uint8_t, 32> bytes{};
    if (ravet_gain_encode(float(getParamNormalized(gainID) * 4.),
                         getParamNormalized(bypassID) >= .5 ? 1 : 0,
                         bytes.data(), bytes.size()) != 0)
      return;
    message->setMessageID(controllerRestoreID);
    if (message->getAttributes() &&
        message->getAttributes()->setBinary("state", bytes.data(),
                                             uint32(bytes.size())) == kResultOk &&
        sendMessage(message) == kResultOk)
      pendingRestore = false;
  }
};
#if defined(__APPLE__) && defined(RAVET_NATIVE_WEB_EDITOR)
class GainWebView final : public CPluginView {
public:
  explicit GainWebView(GainController* owner) : controller(owner) {
    rect = ViewRect(0,0,480,480);
  }
  ~GainWebView() override {
    endGesture();
    if(editor) ravet_editor_destroy(editor);
  }
  tresult PLUGIN_API isPlatformTypeSupported(FIDString type) override {
    return type && std::strcmp(type,kPlatformTypeNSView)==0 ? kResultTrue : kResultFalse;
  }
  tresult PLUGIN_API attached(void* parent,FIDString type) override {
    if(!parent || editor || isPlatformTypeSupported(type)!=kResultTrue) return kResultFalse;
    size_t length=0; const auto* html=ravet_gain_editor_html(&length);
    uint32_t status=1;
    auto* candidate=ravet_editor_create_html(html,length,rect.getWidth(),rect.getHeight(),{this,message,snapshot,nullptr},&status);
    if(!candidate || status!=0) return kResultFalse;
    if(ravet_editor_attach(candidate,parent)!=0) {ravet_editor_destroy(candidate);return kResultFalse;}
    editor=candidate; systemWindow=parent;
    return kResultOk;
  }
  tresult PLUGIN_API removed() override {
    if(!endGesture()) return kResultFalse;
    if(editor) {if(ravet_editor_destroy(editor)!=0) return kResultFalse;editor=nullptr;}
    systemWindow=nullptr;return kResultOk;
  }
  tresult PLUGIN_API onSize(ViewRect* size) override {
    if(!size || size->getWidth()<360 || size->getWidth()>1200 || size->getHeight()<300 || size->getHeight()>900) return kResultFalse;
    if(editor && ravet_editor_set_size(editor,size->getWidth(),size->getHeight())!=0) return kResultFalse;
    rect=*size;return kResultOk;
  }
  tresult PLUGIN_API canResize() override {return kResultTrue;}
  tresult PLUGIN_API checkSizeConstraint(ViewRect* size) override {
    if(!size) return kResultFalse;
    const int64 width=int64(size->right)-size->left,height=int64(size->bottom)-size->top;
    size->left=0;size->top=0;
    size->right=int32(std::clamp<int64>(width,360,1200));
    size->bottom=int32(std::clamp<int64>(height,300,900));return kResultTrue;
  }
private:
  IPtr<GainController> controller;
  RavetWebEditor* editor=nullptr;
  std::optional<ParamID> active;
  uint32_t status=0;
  bool endGesture() {
    if(!active) return true;
    if(controller->endEdit(*active)!=kResultOk) {status=2;return false;}
    active.reset();status=0;return true;
  }
  bool beginGesture(ParamID id) {
    if(active && *active==id) return true;
    if(!endGesture()) return false;
    if(controller->beginEdit(id)!=kResultOk) {status=1;return false;}
    active=id;return true;
  }
  bool value(ParamID id,double normalized) {
    const double old=controller->getParamNormalized(id);
    if(controller->setParamNormalized(id,normalized)!=kResultOk) {status=1;return false;}
    if(controller->performEdit(id,normalized)!=kResultOk) {
      controller->setParamNormalized(id,old);status=1;return false;
    }
    status=0;return true;
  }
  void edit(uint32_t kind,ParamID id,double normalized) {
    if(kind==1) {beginGesture(id);return;}
    if(kind==3) {if(!active || *active==id) endGesture();else status=1;return;}
    if(kind==2) {if(active && *active==id)value(id,normalized);else status=1;return;}
    if(!endGesture() || !beginGesture(id)) return;
    const bool accepted=value(id,normalized);
    if(endGesture() && !accepted) status=1;
  }
  static void message(void* raw,const uint8_t* json,size_t length) {
    auto* self=static_cast<GainWebView*>(raw);
    IPtr<GainWebView> retained(self);
    uint32_t kind=0,id=0;double normalized=0;
    if(ravet_gain_editor_message(json,length,&kind,&id,&normalized)!=0) {self->status=1;return;}
    self->edit(kind,id,normalized);
  }
  static size_t snapshot(void* raw,uint8_t* output,size_t capacity) {
    auto* self=static_cast<GainWebView*>(raw);
    IPtr<GainWebView> retained(self);
    if(self->status==2) self->endGesture();
    std::array<double,meterParameterCount> values{};
    for (int32 index=0;index<meterParameterCount;++index) values[index]=self->controller->getParamNormalized(firstMeterID+index);
    return ravet_gain_editor_meter_snapshot(self->controller->getParamNormalized(gainID),self->controller->getParamNormalized(bypassID),self->status,values.data(),output,capacity);
  }
};
IPlugView* GainController::createView(FIDString name) {
  return name && std::strcmp(name,ViewType::kEditor)==0 ? new GainWebView(this) : nullptr;
}
#endif
} // namespace
BEGIN_FACTORY("RAVET", "", "", PFactoryInfo::kNoFlags)
DEF_CLASS2(INLINE_UID_FROM_FUID(processorID), PClassInfo::kManyInstances,
           kVstAudioEffectClass, "RAVET Gain", Vst::kDistributable, "Fx|Tools",
           "0.1.0", kVstVersionString, GainProcessor::create)
DEF_CLASS2(INLINE_UID_FROM_FUID(controllerID), PClassInfo::kManyInstances,
           kVstComponentControllerClass, "RAVET Gain Controller", 0, "",
           "0.1.0", kVstVersionString, GainController::create)
END_FACTORY
