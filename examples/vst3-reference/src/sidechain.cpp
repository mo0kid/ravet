// RAVET Keyed Sidechain: reference VST3 module for hosted auxiliary buses.
// Stereo main in/out plus one mono or stereo auxiliary sidechain input. Output
// is main * key (key = mono sample, stereo mean, or exactly 1 when the
// auxiliary bus is inactive), computed by the portable Rust KeyedGain.
// Input channels flagged in silenceFlags are read as exact zeros (a prepared
// zero buffer replaces them, whatever the buffer holds); output silenceFlags
// are always cleared. Malformed ProcessData (null bus or channel arrays, wrong
// channel counts, frames outside 0..maxSamplesPerBlock) is refused with
// kResultFalse and every reachable output channel zeroed.
// External SDK sources stay external. Not a production dynamics processor.
#include "RavetVst3Reference.h"
#include "public.sdk/source/main/pluginfactory.h"
#include "public.sdk/source/vst/vstaudioeffect.h"
#include "public.sdk/source/vst/vsteditcontroller.h"
#include <algorithm>
#include <cmath>
#include <vector>

using namespace Steinberg;
using namespace Steinberg::Vst;
namespace {
const FUID processorID(0x0EC762B9, 0x94774931, 0xB5F7CA9F, 0x5893AFF1);
const FUID controllerID(0x175307DF, 0x2C3A4DDD, 0x920C7E5F, 0x91D17BDE);

class SidechainProcessor final : public AudioEffect {
public:
  SidechainProcessor() { setControllerClass(controllerID); }
  ~SidechainProcessor() override { ravet_keyed_gain_destroy(dsp); }
  static FUnknown *create(void *) {
    return static_cast<IAudioProcessor *>(new SidechainProcessor);
  }
  tresult PLUGIN_API initialize(FUnknown *context) override {
    const auto status = AudioEffect::initialize(context);
    if (status != kResultOk)
      return status;
    if (!dsp)
      dsp = ravet_keyed_gain_create();
    if (!dsp)
      return kResultFalse;
    addAudioInput(STR16("Main"), SpeakerArr::kStereo, kMain,
                  BusInfo::kDefaultActive);
    addAudioInput(STR16("Sidechain"), SpeakerArr::kStereo, kAux, 0);
    addAudioOutput(STR16("Main"), SpeakerArr::kStereo, kMain,
                   BusInfo::kDefaultActive);
    return kResultOk;
  }
  tresult PLUGIN_API setBusArrangements(SpeakerArrangement *inputs,
                                        int32 inputCount,
                                        SpeakerArrangement *outputs,
                                        int32 outputCount) override {
    if (inputCount != 2 || outputCount != 1 || !inputs || !outputs ||
        inputs[0] != SpeakerArr::kStereo ||
        outputs[0] != SpeakerArr::kStereo ||
        (inputs[1] != SpeakerArr::kMono && inputs[1] != SpeakerArr::kStereo))
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
        !std::isfinite(setup.sampleRate) || setup.sampleRate <= 0.)
      return kResultFalse;
    return AudioEffect::setupProcessing(setup);
  }
  tresult PLUGIN_API setProcessing(TBool) override { return kResultOk; }
  tresult PLUGIN_API setActive(TBool state) override {
    prepared = false;
    if (state) {
      // Buses exist only after initialize; refuse rather than throw.
      if (audioInputs.size() < 2 || !dsp)
        return kResultFalse;
      auto *aux = FCast<AudioBus>(audioInputs[1].get());
      key = aux && aux->isActive()
                ? SpeakerArr::getChannelCount(aux->getArrangement())
                : 0;
      if (ravet_keyed_gain_prepare(
              dsp, processSetup.symbolicSampleSize == kSample64 ? 64 : 32,
              processSetup.sampleRate, uint32_t(key),
              uint32_t(processSetup.maxSamplesPerBlock)) != 0)
        return kResultFalse;
      // Zero buffers substituted for silence-flagged inputs (CONTROL only).
      zero32.assign(size_t(processSetup.maxSamplesPerBlock), 0.f);
      zero64.assign(size_t(processSetup.maxSamplesPerBlock), 0.);
      prepared = true;
    }
    return AudioEffect::setActive(state);
  }
  tresult PLUGIN_API process(ProcessData &data) override {
    const bool wide = data.symbolicSampleSize == kSample64;
    const auto silence = [&] {
      if (data.numOutputs < 1 || !data.outputs || data.numSamples <= 0 ||
          data.numSamples > processSetup.maxSamplesPerBlock)
        return kResultFalse;
      auto &out = data.outputs[0];
      for (int32 c = 0; c < out.numChannels; ++c) {
        if (wide && out.channelBuffers64 && out.channelBuffers64[c])
          std::fill_n(out.channelBuffers64[c], data.numSamples, 0.);
        if (!wide && out.channelBuffers32 && out.channelBuffers32[c])
          std::fill_n(out.channelBuffers32[c], data.numSamples, 0.f);
      }
      out.silenceFlags = 0;
      return kResultFalse;
    };
    if (!prepared || data.symbolicSampleSize != processSetup.symbolicSampleSize ||
        data.numSamples < 0 ||
        data.numSamples > processSetup.maxSamplesPerBlock)
      return silence();
    if (data.numSamples == 0)
      return kResultOk;
    if (data.numInputs < 1 + (key > 0) || !data.inputs ||
        data.numOutputs < 1 || !data.outputs ||
        data.inputs[0].numChannels != 2 || data.outputs[0].numChannels != 2 ||
        (key > 0 && data.inputs[1].numChannels != key))
      return silence();
    const auto frames = uint32_t(data.numSamples);
    // Per-channel input pointers with silence-flagged channels replaced by the
    // prepared zero buffer; a null channel array stays null and is refused.
    const auto route = [&](auto **buffers, uint64 flags, int32 count,
                           const auto *zero, auto *target) {
      if (!buffers)
        return false;
      for (int32 c = 0; c < count; ++c)
        target[c] = (flags >> c) & 1 ? zero : buffers[c];
      return true;
    };
    int32_t status = 1;
    if (wide) {
      const double *main[2] = {}, *keys[2] = {};
      if (route(data.inputs[0].channelBuffers64, data.inputs[0].silenceFlags,
                2, zero64.data(), main) &&
          (key == 0 || route(data.inputs[1].channelBuffers64,
                             data.inputs[1].silenceFlags, key, zero64.data(),
                             keys)))
        status = ravet_keyed_gain_process64(dsp, main, key > 0 ? keys : nullptr,
                                            data.outputs[0].channelBuffers64,
                                            frames);
    } else {
      const float *main[2] = {}, *keys[2] = {};
      if (route(data.inputs[0].channelBuffers32, data.inputs[0].silenceFlags,
                2, zero32.data(), main) &&
          (key == 0 || route(data.inputs[1].channelBuffers32,
                             data.inputs[1].silenceFlags, key, zero32.data(),
                             keys)))
        status = ravet_keyed_gain_process32(dsp, main, key > 0 ? keys : nullptr,
                                            data.outputs[0].channelBuffers32,
                                            frames);
    }
    if (status != 0)
      return silence();
    data.outputs[0].silenceFlags = 0;
    return kResultOk;
  }

private:
  RavetKeyedGain *dsp = nullptr;
  std::vector<float> zero32;
  std::vector<double> zero64;
  int32 key = 0;
  bool prepared = false;
};

class SidechainController final : public EditController {
public:
  static FUnknown *create(void *) {
    return static_cast<IEditController *>(new SidechainController);
  }
};
} // namespace
BEGIN_FACTORY("RAVET", "", "", PFactoryInfo::kNoFlags)
DEF_CLASS2(INLINE_UID_FROM_FUID(processorID), PClassInfo::kManyInstances,
           kVstAudioEffectClass, "RAVET Keyed Sidechain", Vst::kDistributable,
           "Fx|Dynamics", "0.0.1", kVstVersionString,
           SidechainProcessor::create)
DEF_CLASS2(INLINE_UID_FROM_FUID(controllerID), PClassInfo::kManyInstances,
           kVstComponentControllerClass, "RAVET Keyed Sidechain Controller", 0,
           "", "0.0.1", kVstVersionString, SidechainController::create)
END_FACTORY
