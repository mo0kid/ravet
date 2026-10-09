#include <AudioToolbox/AudioToolbox.h>
#include <CoreFoundation/CoreFoundation.h>
#include <array>
#include <cstring>
#include <iostream>
#include <stdexcept>
void require(bool condition, const char *message) {
  if (!condition)
    throw std::runtime_error(message);
}
void okay(OSStatus status, const char *message) {
  if (status != noErr) {
    std::cerr << message << ": " << status << '\n';
    throw std::runtime_error(message);
  }
}
OSStatus input(void *, AudioUnitRenderActionFlags *, const AudioTimeStamp *,
               UInt32, UInt32 frames, AudioBufferList *buffers) {
  for (UInt32 channel = 0; channel < buffers->mNumberBuffers; ++channel) {
    auto *output = static_cast<float *>(buffers->mBuffers[channel].mData);
    if (!output)
      return kAudioUnitErr_InvalidPropertyValue;
    for (UInt32 frame = 0; frame < frames; ++frame)
      output[frame] = channel == 0 ? 0.25f : -0.5f;
  }
  return noErr;
}
int main(int argc, char **argv) {
  try {
    require(argc == 2, "usage: contracts component-path");
    auto url = CFURLCreateFromFileSystemRepresentation(
        nullptr, reinterpret_cast<const UInt8 *>(argv[1]), std::strlen(argv[1]),
        true);
    auto bundle = CFBundleCreate(nullptr, url);
    CFRelease(url);
    require(bundle && CFBundleLoadExecutable(bundle),
            "load actual component bundle");
    auto factory = reinterpret_cast<AudioComponentFactoryFunction>(
        CFBundleGetFunctionPointerForName(bundle, CFSTR("AUWrapperFactory")));
    require(factory, "factory symbol");
    AudioComponentDescription desc{'aufx', 'RvGn', 'Ravt', 0, 0};
    auto component =
        AudioComponentRegister(&desc, CFSTR("RAVET: Gain"), 256, factory);
    require(component, "in-process registration");
    AudioUnit unit = nullptr;
    okay(AudioComponentInstanceNew(component, &unit), "instantiate");
    AudioStreamBasicDescription format{};
    format.mSampleRate = 48000.;
    format.mFormatID = kAudioFormatLinearPCM;
    format.mFormatFlags =
        kAudioFormatFlagsNativeFloatPacked | kAudioFormatFlagIsNonInterleaved;
    format.mBytesPerPacket = format.mBytesPerFrame = 4;
    format.mFramesPerPacket = 1;
    format.mChannelsPerFrame = 2;
    format.mBitsPerChannel = 32;
    okay(AudioUnitSetProperty(unit, kAudioUnitProperty_StreamFormat,
                              kAudioUnitScope_Input, 0, &format,
                              sizeof(format)),
         "input format");
    okay(AudioUnitSetProperty(unit, kAudioUnitProperty_StreamFormat,
                              kAudioUnitScope_Output, 0, &format,
                              sizeof(format)),
         "output format");
    UInt32 maximum = 64;
    okay(AudioUnitSetProperty(unit, kAudioUnitProperty_MaximumFramesPerSlice,
                              kAudioUnitScope_Global, 0, &maximum,
                              sizeof(maximum)),
         "maximum frames");
    AURenderCallbackStruct callback{input, nullptr};
    okay(AudioUnitSetProperty(unit, kAudioUnitProperty_SetRenderCallback,
                              kAudioUnitScope_Input, 0, &callback,
                              sizeof(callback)),
         "input callback");
    AudioUnitParameterInfo info{};
    UInt32 size = sizeof(info);
    okay(AudioUnitGetProperty(unit, kAudioUnitProperty_ParameterInfo,
                              kAudioUnitScope_Global, 1, &info, &size),
         "gain metadata");
    require(info.minValue == 0.f && info.maxValue == 1.f &&
                info.defaultValue == 0.25f,
            "normalized gain metadata");
    okay(AudioUnitInitialize(unit), "initialize");
    std::array<float, 65> left, right;
    struct StereoBuffers {
      UInt32 count;
      AudioBuffer buffers[2];
    } buffers{2,
              {{1, 64 * sizeof(float), left.data()},
               {1, 64 * sizeof(float), right.data()}}};
    AudioTimeStamp time{};
    time.mFlags = kAudioTimeStampSampleTimeValid;
    auto render = [&](float expectedLeft, float expectedRight, bool check) {
      left.fill(99.f);
      right.fill(99.f);
      AudioUnitRenderActionFlags flags = 0;
      okay(AudioUnitRender(unit, &flags, &time, 0, 64,
                           reinterpret_cast<AudioBufferList *>(&buffers)),
           "render");
      time.mSampleTime += 64;
      if (check)
        for (size_t i = 0; i < 64; ++i)
          if (left[i] != expectedLeft || right[i] != expectedRight) {
            std::cerr << "sample time " << time.mSampleTime << " frame " << i
                      << " actual " << left[i] << "," << right[i]
                      << " expected " << expectedLeft << "," << expectedRight
                      << "\n";
            require(false, "rendered samples");
          }
      require(left[64] == 99.f && right[64] == 99.f, "active-prefix tails");
    };
    okay(AudioUnitSetParameter(unit, 1, kAudioUnitScope_Global, 0, 0.5f, 0),
         "gain change");
    for (int i = 0; i < 4; ++i)
      render(0.f, 0.f, false);
    render(0.5f, -1.f, true);
    UInt32 bypass = 1;
    okay(AudioUnitSetProperty(unit, kAudioUnitProperty_BypassEffect,
                              kAudioUnitScope_Global, 0, &bypass,
                              sizeof(bypass)),
         "bypass on");
    render(0.25f, -0.5f, true);
    CFPropertyListRef saved = nullptr;
    size = sizeof(saved);
    okay(AudioUnitGetProperty(unit, kAudioUnitProperty_ClassInfo,
                              kAudioUnitScope_Global, 0, &saved, &size),
         "save state");
    require(saved, "state record");
    bypass = 0;
    okay(AudioUnitSetProperty(unit, kAudioUnitProperty_BypassEffect,
                              kAudioUnitScope_Global, 0, &bypass,
                              sizeof(bypass)),
         "bypass off");
    okay(AudioUnitSetParameter(unit, 1, kAudioUnitScope_Global, 0, 0.125f, 0),
         "alternate gain");
    for (int i = 0; i < 4; ++i)
      render(0.f, 0.f, false);
    render(0.125f, -0.25f, true);
    okay(AudioUnitSetProperty(unit, kAudioUnitProperty_ClassInfo,
                              kAudioUnitScope_Global, 0, &saved, sizeof(saved)),
         "recall state");
    CFRelease(saved);
    // AUv2 keeps host bypass separate from preset state; the SDK wrapper
    // deliberately preserves the current host bypass during preset recall.
    render(0.5f, -1.f, true);
    size = sizeof(bypass);
    okay(AudioUnitGetProperty(unit, kAudioUnitProperty_BypassEffect,
                              kAudioUnitScope_Global, 0, &bypass, &size),
         "host bypass after recall");
    require(bypass == 0, "preset preserves current host bypass");
    Float32 requested = 0.;
    okay(AudioUnitGetParameter(unit, 1, kAudioUnitScope_Global, 0, &requested),
         "requested gain");
    require(requested == 0.5f, "complete gain recall");
    okay(AudioUnitUninitialize(unit), "uninitialize");
    okay(AudioComponentInstanceDispose(unit), "dispose");
    CFRelease(bundle);
    std::cout
        << "AUv2 actual-bundle contracts passed: in-process "
           "registration, normalized parameters, stereo rendering, bypass, "
           "gain state recall, preserved host bypass and tails\n";
  } catch (const std::exception &error) {
    std::cerr << error.what() << '\n';
    return 1;
  }
}
