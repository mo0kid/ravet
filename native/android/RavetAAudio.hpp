#pragma once
#include <aaudio/AAudio.h>
#include "RavetNativeSession.hpp"
#include <atomic>
#include <algorithm>

namespace ravet {
// Android API 26+, f32 mono/stereo input/output. Lifecycle/settings are control
// operations; error callbacks only publish diagnostics. Stop/close streams before
// DSP reconfiguration or destruction. Device clocks are not drift-corrected.
class AAudioHost {
public:
    static_assert(std::atomic<uint64_t>::is_always_lock_free && std::atomic<unsigned>::is_always_lock_free,
                  "The mobile callback profile requires lock-free diagnostics/queue atomics");
    NativeSession session;
    explicit AAudioHost(const RavetNativeStateEntryV1 *entry) : session(entry) {}
    ~AAudioHost() { stop(); }
    std::atomic<int32_t> failure{AAUDIO_OK};
    std::atomic<uint64_t> inputShortages{0}, eventDrops{0};
    int rate = 0, channels = 0, bufferFrames = 0;
    bool start(int preferredRate, int preferredFrames, int inputDevice = 0, int outputDevice = 0) {
        stop(); failure.store(AAUDIO_OK); inputShortages.store(0);
        AAudioStreamBuilder *builder = nullptr;
        if (AAudio_createStreamBuilder(&builder) != AAUDIO_OK) return false;
        AAudioStreamBuilder_setDirection(builder, AAUDIO_DIRECTION_OUTPUT);
        AAudioStreamBuilder_setFormat(builder, AAUDIO_FORMAT_PCM_FLOAT);
        AAudioStreamBuilder_setChannelCount(builder, 2);
        AAudioStreamBuilder_setSampleRate(builder, preferredRate);
        AAudioStreamBuilder_setDeviceId(builder, outputDevice);
        AAudioStreamBuilder_setPerformanceMode(builder, AAUDIO_PERFORMANCE_MODE_LOW_LATENCY);
        AAudioStreamBuilder_setSharingMode(builder, AAUDIO_SHARING_MODE_SHARED);
        AAudioStreamBuilder_setDataCallback(builder, callback, this);
        AAudioStreamBuilder_setErrorCallback(builder, errorCallback, this);
        aaudio_result_t result = AAudioStreamBuilder_openStream(builder, &output);
        AAudioStreamBuilder_delete(builder);
        if (result != AAUDIO_OK) { failure.store(result); return false; }
        rate = AAudioStream_getSampleRate(output); channels = AAudioStream_getChannelCount(output);
        if (AAudioStream_getFormat(output) != AAUDIO_FORMAT_PCM_FLOAT || channels < 1 || channels > 2) return reject(AAUDIO_ERROR_INVALID_FORMAT);
        const int actualFrames = AAudioStream_setBufferSizeInFrames(output, preferredFrames);
        if (actualFrames < 0) return reject(actualFrames);
        bufferFrames = actualFrames;
        if (AAudio_createStreamBuilder(&builder) != AAUDIO_OK) return reject(AAUDIO_ERROR_INTERNAL);
        AAudioStreamBuilder_setDirection(builder, AAUDIO_DIRECTION_INPUT);
        AAudioStreamBuilder_setFormat(builder, AAUDIO_FORMAT_PCM_FLOAT);
        AAudioStreamBuilder_setChannelCount(builder, channels);
        AAudioStreamBuilder_setSampleRate(builder, rate);
        AAudioStreamBuilder_setDeviceId(builder, inputDevice);
        AAudioStreamBuilder_setPerformanceMode(builder, AAUDIO_PERFORMANCE_MODE_LOW_LATENCY);
        AAudioStreamBuilder_setErrorCallback(builder, errorCallback, this);
        result = AAudioStreamBuilder_openStream(builder, &input);
        AAudioStreamBuilder_delete(builder);
        if (result != AAUDIO_OK) return reject(result);
        if (AAudioStream_getFormat(input) != AAUDIO_FORMAT_PCM_FLOAT || AAudioStream_getChannelCount(input) != channels ||
            AAudioStream_getSampleRate(input) != rate) return reject(AAUDIO_ERROR_INVALID_FORMAT);
        if (!session.prepare(rate, uint32_t(channels), uint32_t(channels), maximumFrames)) return reject(AAUDIO_ERROR_INVALID_STATE);
        if (AAudioStream_requestStart(input) != AAUDIO_OK || AAudioStream_requestStart(output) != AAUDIO_OK) return reject(AAUDIO_ERROR_INVALID_STATE);
        return true;
    }
    void stop() {
        // AAudio close joins stream callback activity. No callback accesses DSP
        // after output close returns; input closes before the stopped owner retires.
        if (output) { AAudioStream_requestStop(output); AAudioStream_close(output); output = nullptr; }
        if (input) { AAudioStream_requestStop(input); AAudioStream_close(input); input = nullptr; }
        session.stop();
        readIndex.store(writeIndex.load(std::memory_order_acquire), std::memory_order_release);
    }
    bool midi(const uint8_t *bytes, uint32_t length) {
        if (!bytes || length < 1 || length > 3 || bytes[0] < 0x80) return false;
        const uint8_t status = bytes[0];
        const uint32_t expected = status < 0xf0 ? ((status & 0xe0) == 0xc0 ? 2 : 3) :
            status == 0xf1 || status == 0xf3 ? 2 : status == 0xf2 ? 3 :
            status == 0xf6 || status == 0xf8 || status == 0xfa || status == 0xfb || status == 0xfc || status == 0xfe || status == 0xff ? 1 : 0;
        if (length != expected) return false;
        for (uint32_t i = 1; i < length; ++i) if (bytes[i] > 127) return false;
        std::lock_guard<std::mutex> guard(producerMutex);
        const unsigned write = writeIndex.load(std::memory_order_relaxed), next = (write + 1) % queue.size();
        if (next == readIndex.load(std::memory_order_acquire)) { eventDrops.fetch_add(1); return false; }
        RavetNativeEventV1 event{}; event.kind = RAVET_NATIVE_MIDI; event.detail = length;
        for (uint32_t i = 0; i < length; ++i) event.words[0] |= uint32_t(bytes[i]) << (8 * i);
        queue[write] = event; writeIndex.store(next, std::memory_order_release); return true;
    }
private:
    static constexpr uint32_t maximumFrames = 4096;
    AAudioStream *input = nullptr, *output = nullptr;
    std::array<float, maximumFrames * 2> capture{}, planarInput{}, planarOutput{};
    std::array<RavetNativeEventV1, 1025> queue{};
    std::atomic<unsigned> readIndex{0}, writeIndex{0};
    std::mutex producerMutex;
    bool reject(int32_t code) { stop(); failure.store(code); return false; }
    static void errorCallback(AAudioStream *, void *context, aaudio_result_t error) {
        static_cast<AAudioHost *>(context)->failure.store(error, std::memory_order_release);
    }
    static aaudio_data_callback_result_t callback(AAudioStream *, void *context, void *data, int32_t frames) {
        return static_cast<AAudioHost *>(context)->render(static_cast<float *>(data), frames);
    }
    aaudio_data_callback_result_t render(float *outputData, int32_t total) {
        if (total < 0 || !outputData) return AAUDIO_CALLBACK_RESULT_STOP;
        if (failure.load(std::memory_order_acquire) != AAUDIO_OK) {
            std::fill_n(outputData, size_t(total) * channels, 0); return AAUDIO_CALLBACK_RESULT_STOP;
        }
        for (uint32_t offset = 0; offset < uint32_t(total);) {
            const uint32_t frames = std::min(maximumFrames, uint32_t(total) - offset);
            const int read = AAudioStream_read(input, capture.data(), int32_t(frames), 0);
            if (read < 0) { failure.store(read); std::fill_n(outputData, size_t(total) * channels, 0); return AAUDIO_CALLBACK_RESULT_STOP; }
            if (uint32_t(read) < frames) { inputShortages.fetch_add(frames - read); std::fill(capture.begin() + size_t(read) * channels, capture.begin() + size_t(frames) * channels, 0); }
            std::array<const float *, 2> ins{}; std::array<float *, 2> outs{};
            for (int c = 0; c < channels; ++c) {
                ins[c] = planarInput.data() + c * maximumFrames; outs[c] = planarOutput.data() + c * maximumFrames;
                for (uint32_t f = 0; f < frames; ++f) planarInput[c * maximumFrames + f] = capture[f * channels + c];
            }
            std::array<RavetNativeEventV1, 1024> events{};
            unsigned index = readIndex.load(std::memory_order_relaxed);
            const unsigned write = writeIndex.load(std::memory_order_acquire);
            uint32_t count = 0;
            while (index != write && count < events.size()) { events[count++] = queue[index]; index = (index + 1) % queue.size(); }
            if (session.apply() || session.render(ins.data(), outs.data(), channels, channels, frames, events.data(), count)) {
                failure.store(AAUDIO_ERROR_INTERNAL); std::fill_n(outputData, size_t(total) * channels, 0); return AAUDIO_CALLBACK_RESULT_STOP;
            }
            readIndex.store(index, std::memory_order_release); session.reflect();
            for (uint32_t f = 0; f < frames; ++f) for (int c = 0; c < channels; ++c)
                outputData[(offset + f) * channels + c] = planarOutput[c * maximumFrames + f];
            offset += frames;
        }
        return AAUDIO_CALLBACK_RESULT_CONTINUE;
    }
};
}
