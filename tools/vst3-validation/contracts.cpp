// Reference adapter contract fixture; SDK conformance uses the dynamic
// validator.
#include "../../examples/gain-vst3/src/gain.cpp"
#include "public.sdk/source/common/memorystream.h"
#include "public.sdk/source/vst/hosting/hostclasses.h"
#include "public.sdk/source/vst/hosting/parameterchanges.h"
#include <iostream>
#include <stdexcept>
void require(bool condition, const char *message) {
  if (!condition)
    throw std::runtime_error(message);
}
void restore(GainProcessor &processor, float gain, uint32_t bypass) {
  std::array<uint8_t, 32> record{};
  require(ravet_gain_encode(gain, bypass, record.data(), record.size()) == 0,
          "encode");
  MemoryStream stream(record.data(), record.size());
  require(processor.setState(&stream) == kResultOk, "restore");
}
void snapshot(GainProcessor &processor, float expectedGain,
              uint32_t expectedBypass) {
  MemoryStream stream;
  require(processor.getState(&stream) == kResultOk, "save");
  float gain;
  uint32_t bypass;
  require(ravet_gain_decode(reinterpret_cast<uint8_t *>(stream.getData()),
                            stream.getSize(), &gain, &bypass) == 0,
          "decode");
  require(gain == expectedGain && bypass == expectedBypass, "complete state");
}
void native64Contracts(HostApplication &host) {
  unsigned blocks = 0;
  for (double rate : {44100., 48000., 96000.}) {
    for (int32 channels : {1, 2}) {
      GainProcessor processor;
      require(processor.initialize(&host) == kResultOk, "f64 initialize");
      require(processor.canProcessSampleSize(kSample32) == kResultOk &&
              processor.canProcessSampleSize(kSample64) == kResultOk &&
              processor.canProcessSampleSize(99) == kResultFalse, "sample sizes");
      auto arrangement = channels == 1 ? SpeakerArr::kMono : SpeakerArr::kStereo;
      require(processor.setBusArrangements(&arrangement, 1, &arrangement, 1) == kResultOk, "f64 layout");
      ProcessSetup setup{kRealtime, kSample64, 8, rate};
      require(processor.setupProcessing(setup) == kResultOk, "f64 setup");
      require(processor.setActive(true) == kResultOk, "f64 active");
      require(processor.setProcessing(true) == kResultOk, "f64 start");
      const auto exact = std::nextafter(1., 2.);
      std::array<double, 9> left{}, right{};
      double *inputs[]{left.data(), right.data()};
      double *outputs[]{channels == 1 ? left.data() : right.data(), left.data()};
      AudioBusBuffers input{}, output{};
      input.numChannels = output.numChannels = channels;
      input.channelBuffers64 = inputs; output.channelBuffers64 = outputs;
      ProcessData data{};
      data.symbolicSampleSize = kSample64; data.numSamples = 8;
      data.numInputs = data.numOutputs = 1; data.inputs = &input; data.outputs = &output;
      for (unsigned iteration = 0; iteration < 1000; ++iteration) {
        left.fill(exact); right.fill(-exact);
        restore(processor, 2., 0);
        require(processor.process(data) == kResultOk, "f64 process");
        require(outputs[0][0] == exact * 2., "f64 exact audio");
        if (channels == 2) require(outputs[1][0] == -exact * 2., "f64 cross-channel alias");
        require(left[8] == exact && right[8] == -exact, "f64 tails");
        ++blocks;
      }
      left.fill(0.); right.fill(0.);
      require(processor.process(data) == kResultOk, "f64 silent process");
      require(output.silenceFlags == (uint64_t(1) << channels) - 1, "f64 silent flags");
      left.fill(exact); right.fill(-exact);
      ParameterChanges changes(2); int32 index;
      changes.addParameterData(bypassID, index)->addPoint(3, 1., index);
      data.inputParameterChanges = &changes;
      require(processor.process(data) == kResultOk, "f64 bypass");
      for (int frame = 0; frame < 8; ++frame)
        require(outputs[0][frame] == exact * (frame < 3 ? 2. : 1.), "f64 bypass time");
      snapshot(processor, 2., 1);
      changes.clearQueue();
      changes.addParameterData(gainID,index)->addPoint(0,0.75,index);
      changes.addParameterData(bypassID,index)->addPoint(8,0.,index);
      require(processor.process(data) == kResultFalse, "f64 invalid events");
      for (int channel = 0; channel < channels; ++channel)
        for (int frame = 0; frame < 8; ++frame)
          require(outputs[channel][frame] == 0., "f64 invalid silence");
      snapshot(processor,2.,1);
      changes.clearQueue();
      changes.addParameterData(gainID,index)->addPoint(0,0.125,index);
      data.numSamples = 0; data.numInputs = data.numOutputs = 0;
      data.inputs = data.outputs = nullptr;
      require(processor.process(data) == kResultOk, "f64 zero-frame edit");
      snapshot(processor,0.5f,1);
      ++blocks;
      require(processor.setProcessing(false) == kResultOk, "f64 stop");
      processor.setActive(false);
      setup.symbolicSampleSize = kSample32;
      require(processor.setupProcessing(setup) == kResultOk, "precision switch setup");
      require(processor.setActive(true) == kResultOk, "precision switch active");
      require(processor.setProcessing(true) == kResultOk, "precision switch start");
      std::array<float,8> narrow{}; narrow.fill(0.25f);
      float *narrowChannels[]{narrow.data(),narrow.data()};
      // Use disjoint output ranges in stereo.
      std::array<float,8> other{}; other.fill(-0.25f);
      narrowChannels[1] = other.data();
      input.channelBuffers32 = output.channelBuffers32 = narrowChannels;
      data.symbolicSampleSize = kSample32; data.numSamples = 8;
      data.numInputs = data.numOutputs = 1; data.inputs = &input; data.outputs = &output;
      data.inputParameterChanges = nullptr;
      require(processor.process(data) == kResultOk && narrow[0] == 0.25f, "switched precision state/bypass");
      snapshot(processor,0.5f,1);
      narrow.fill(0.); other.fill(0.);
      require(processor.process(data) == kResultOk, "f32 silent process");
      require(output.silenceFlags == (uint64_t(1) << channels) - 1, "f32 silent flags");
      // A block with another precision is rejected before state/DSP mutation.
      std::array<double,8> wrongLeft{}, wrongRight{};
      wrongLeft.fill(99.); wrongRight.fill(99.);
      double *wrongChannels[]{wrongLeft.data(), wrongRight.data()};
      input.channelBuffers64 = output.channelBuffers64 = wrongChannels;
      data.symbolicSampleSize = kSample64;
      require(processor.process(data) == kResultFalse, "precision mismatch rejection");
      for (int channel = 0; channel < channels; ++channel)
        for (int frame = 0; frame < 8; ++frame)
          require(wrongChannels[channel][frame] == 0., "precision mismatch silence");
      snapshot(processor,0.5f,1);
      processor.setProcessing(false); processor.setActive(false); processor.terminate();
    }
  }
  std::cout << "PASS native VST3 f64: " << blocks << " blocks across six mono/stereo/rate cycles and f32 switching\n";
}
#if defined(__APPLE__) && defined(RAVET_NATIVE_WEB_EDITOR)
void nativeMeterContracts(HostApplication& host) {
  for (int32 precision : {kSample32,kSample64}) {
    GainProcessor processor; GainController controller;
    require(processor.initialize(&host)==kResultOk && controller.initialize(&host)==kResultOk,"meter initialize");
    for (int32 i=0;i<12;++i) {
      const auto* parameter=controller.getParameterObject(firstMeterID+i);
      require(parameter && (parameter->getInfo().flags&ParameterInfo::kIsReadOnly) && (parameter->getInfo().flags&ParameterInfo::kIsHidden) && !(parameter->getInfo().flags&ParameterInfo::kCanAutomate),"meter metadata");
    }
    ProcessSetup setup{kRealtime,precision,8,48000.};
    require(processor.setupProcessing(setup)==kResultOk && processor.setActive(true)==kResultOk && processor.setProcessing(true)==kResultOk,"meter setup");
    restore(processor,2.,0);
    std::array<float,8> left32{},right32{},outL32{},outR32{};left32.fill(.25f);right32.fill(-.5f);
    std::array<double,8> left64{},right64{},outL64{},outR64{};left64.fill(.25);right64.fill(-.5);
    float* ins32[]{left32.data(),right32.data()};float* outs32[]{outL32.data(),outR32.data()};
    double* ins64[]{left64.data(),right64.data()};double* outs64[]{outL64.data(),outR64.data()};
    AudioBusBuffers input{},output{};input.numChannels=output.numChannels=2;
    if (precision==kSample64) {input.channelBuffers64=ins64;output.channelBuffers64=outs64;}
    else {input.channelBuffers32=ins32;output.channelBuffers32=outs32;}
    ParameterChanges changes(12);ProcessData data{};data.symbolicSampleSize=precision;data.numSamples=8;data.numInputs=data.numOutputs=1;data.inputs=&input;data.outputs=&output;data.outputParameterChanges=&changes;
    require(processor.process(data)==kResultOk && changes.getParameterCount()==12,"meter output queues");
    const double expected[]{.0625,.0625,0.,.125,.125,0.,.125,.125,0.,.25,.25,0.};
    for(int32 i=0;i<12;++i) {
      auto* queue=changes.getParameterData(i);int32 offset=-1;double value=-1;
      require(queue && queue->getParameterId()==firstMeterID+i && queue->getPointCount()==1 && queue->getPoint(0,offset,value)==kResultOk && offset==7 && value==expected[i],"meter last sample/levels");
      require(controller.setParamNormalized(queue->getParameterId(),value)==kResultOk && controller.getParamNormalized(queue->getParameterId())==expected[i],"host meter delivery");
    }
    data.outputParameterChanges=nullptr;require(processor.process(data)==kResultOk,"host without meter queues");
    processor.setProcessing(false);processor.setActive(false);processor.terminate();controller.terminate();
  }
  std::cout<<"PASS native VST3 meters: f32/f64 host queues/controller metadata/delivery"<<std::endl;
}
#endif
int main() {
  try {
    HostApplication host;
    native64Contracts(host);
#if defined(__APPLE__) && defined(RAVET_NATIVE_WEB_EDITOR)
    nativeMeterContracts(host);
#endif
    GainProcessor processor;
    GainController controller;
    require(processor.initialize(&host) == kResultOk, "initialize processor");
    require(controller.initialize(&host) == kResultOk, "initialize controller");
    ProcessSetup setup{kRealtime, kSample32, 64, 48000.};
    require(processor.setupProcessing(setup) == kResultOk, "prepare");
    require(processor.setActive(true) == kResultOk, "activate");
    require(processor.setProcessing(true) == kResultOk, "start");
    restore(processor, 2., 0);
    std::array<float, 9> left, right, outLeft, outRight;
    left.fill(0.25f);
    right.fill(-0.5f);
    outLeft.fill(99.f);
    outRight.fill(99.f);
    float *inputChannels[] = {left.data(), right.data()};
    float *outputChannels[] = {outLeft.data(), outRight.data()};
    AudioBusBuffers input{}, output{};
    input.numChannels = output.numChannels = 2;
    input.channelBuffers32 = inputChannels;
    output.channelBuffers32 = outputChannels;
    ProcessData data{};
    data.numSamples = 8;
    data.symbolicSampleSize = kSample32;
    data.numInputs = data.numOutputs = 1;
    data.inputs = &input;
    data.outputs = &output;
    require(processor.process(data) == kResultOk, "process separate");
    for (int i = 0; i < 8; ++i) {
      require(outLeft[i] == 0.5f && outRight[i] == -1.f, "wet audio");
    }
    require(outLeft[8] == 99.f && outRight[8] == 99.f, "backing tail");
    snapshot(processor, 2., 0);
    ParameterChanges automation(2);
    int32 index;
    automation.addParameterData(bypassID, index)->addPoint(3, 1., index);
    data.inputParameterChanges = &automation;
    require(processor.process(data) == kResultOk, "sample bypass");
    for (int i = 0; i < 8; ++i)
      require(outLeft[i] == (i < 3 ? 0.5f : 0.25f), "bypass timing");
    snapshot(processor, 2., 1);
    // A malformed later queue prevents an earlier valid gain edit applying.
    automation.clearQueue();
    automation.addParameterData(gainID, index)->addPoint(0, 0.75, index);
    automation.addParameterData(bypassID, index)->addPoint(8, 0., index);
    require(processor.process(data) == kResultFalse, "malformed rejection");
    for (int i = 0; i < 8; ++i)
      require(outLeft[i] == 0.f && outRight[i] == 0.f, "fault silence");
    require(outLeft[8] == 99.f, "rejected tail");
    snapshot(processor, 2., 1);
    automation.clearQueue();
    automation.addParameterData(gainID, index)->addPoint(0, 0.125, index);
    data.numSamples = 0;
    data.numInputs = data.numOutputs = 0;
    data.inputs = data.outputs = nullptr;
    require(processor.process(data) == kResultOk,
            "zero-frame automation flush");
    snapshot(processor, 0.5f, 1);
    MemoryStream state;
    require(processor.getState(&state) == kResultOk, "controller state");
    state.seek(0, IBStream::kIBSeekSet, nullptr);
    require(controller.setComponentState(&state) == kResultOk,
            "controller recall");
    require(controller.getParamNormalized(gainID) == 0.125 &&
                controller.getParamNormalized(bypassID) == 1.,
            "normalized controller recall");
    data.numSamples = 8;
    data.numInputs = data.numOutputs = 1;
    data.inputs = &input;
    data.outputs = &output;
    data.inputParameterChanges = nullptr;
    restore(processor, 0.5f, 0);
    output.channelBuffers32 = inputChannels;
    require(processor.process(data) == kResultOk, "in-place process");
    for (int i = 0; i < 8; ++i)
      require(left[i] == 0.125f && right[i] == -0.25f, "in-place samples");
    require(left[8] == 0.25f && right[8] == -0.5f, "in-place tails");
    require(processor.setProcessing(false) == kResultOk, "stop");
    processor.setActive(false);
    SpeakerArrangement mono = SpeakerArr::kMono;
    require(processor.setBusArrangements(&mono, 1, &mono, 1) == kResultOk,
            "mono layout");
    require(processor.setupProcessing(setup) == kResultOk, "mono prepare");
    input.numChannels = output.numChannels = 1;
    restore(processor, 1.5f, 0);
    left.fill(0.25f);
    require(processor.process(data) == kResultOk, "mono process");
    for (int i = 0; i < 8; ++i)
      require(left[i] == 0.375f, "mono audio");
    snapshot(processor, 1.5f, 0);
    // Controller-only edits while a host has suspended its bypassed processor.
    controller.setParamNormalized(gainID, .125);
    controller.setParamNormalized(bypassID, 1.);
    MemoryStream suspended;
    require(controller.getState(&suspended) == kResultOk, "suspended controller save");
    controller.setParamNormalized(gainID, .75);
    controller.setParamNormalized(bypassID, 0.);
    suspended.seek(0, IBStream::kIBSeekSet, nullptr);
    require(controller.setState(&suspended) == kResultOk, "controller restore before connect");
    require(controller.connect(&processor) == kResultOk, "connect pending restore");
    snapshot(processor, .5f, 1);
    require(controller.getParamNormalized(gainID) == .125 &&
                controller.getParamNormalized(bypassID) == 1., "suspended edit recall");
    controller.disconnect(&processor);
    processor.setProcessing(false);
    processor.setActive(false);
    processor.terminate();
    controller.terminate();
    std::cout << "VST3 contracts passed: stereo/mono, separate/in-place, "
                 "tails, sample bypass, atomic rejection, zero-frame flush, "
                 "complete state and suspended controller recall\n";
  } catch (const std::exception &error) {
    std::cerr << error.what() << '\n';
    return 1;
  }
}
