#include "RavetAAudio.hpp"
#include <cstdlib>
#include <cstdio>
#include <vector>
#include <thread>

struct AAudioStream {
    int32_t direction = 0, format = 2, rate = 48000, channels = 2;
    AAudioStream_dataCallback callback = nullptr;
    AAudioStream_errorCallback error = nullptr;
    void *context = nullptr;
};
struct AAudioStreamBuilder : AAudioStream {};
static AAudioStream *outputStream;
static int shortRead = 0, readError = 0, closes = 0;
static bool mismatchedInput = false, failedOpen = false;
aaudio_result_t AAudio_createStreamBuilder(AAudioStreamBuilder **out) { *out = new AAudioStreamBuilder; return 0; }
#define SETTER(name, member) void AAudioStreamBuilder_set##name(AAudioStreamBuilder *b, int32_t v) { b->member = v; }
SETTER(Direction, direction) SETTER(Format, format) SETTER(ChannelCount, channels) SETTER(SampleRate, rate)
void AAudioStreamBuilder_setDeviceId(AAudioStreamBuilder *, int32_t) {}
void AAudioStreamBuilder_setPerformanceMode(AAudioStreamBuilder *, int32_t) {}
void AAudioStreamBuilder_setSharingMode(AAudioStreamBuilder *, int32_t) {}
void AAudioStreamBuilder_setDataCallback(AAudioStreamBuilder *b, AAudioStream_dataCallback c, void *context) { b->callback = c; b->context = context; }
void AAudioStreamBuilder_setErrorCallback(AAudioStreamBuilder *b, AAudioStream_errorCallback c, void *context) { b->error = c; b->context = context; }
aaudio_result_t AAudioStreamBuilder_openStream(AAudioStreamBuilder *b, AAudioStream **out) {
    if (failedOpen) return -42;
    *out = new AAudioStream(*b);
    if (b->direction == AAUDIO_DIRECTION_OUTPUT) outputStream = *out;
    else if (mismatchedInput) (*out)->rate += 1;
    return 0;
}
aaudio_result_t AAudioStreamBuilder_delete(AAudioStreamBuilder *b) { delete b; return 0; }
int32_t AAudioStream_getSampleRate(AAudioStream *s) { return s->rate; }
int32_t AAudioStream_getChannelCount(AAudioStream *s) { return s->channels; }
int32_t AAudioStream_getFormat(AAudioStream *s) { return s->format; }
aaudio_result_t AAudioStream_setBufferSizeInFrames(AAudioStream *, int32_t frames) { return frames > 0 ? frames : -43; }
aaudio_result_t AAudioStream_requestStart(AAudioStream *) { return 0; }
aaudio_result_t AAudioStream_requestStop(AAudioStream *) { return 0; }
aaudio_result_t AAudioStream_close(AAudioStream *s) { if (s == outputStream) outputStream = nullptr; ++closes; delete s; return 0; }
aaudio_result_t AAudioStream_read(AAudioStream *s, void *data, int32_t frames, int64_t timeout) {
    if (timeout != 0) std::abort();
    if (readError) return readError;
    const int count = std::max(0, frames - shortRead);
    for (int f = 0; f < count; ++f) for (int c = 0; c < s->channels; ++c) static_cast<float *>(data)[f * s->channels + c] = c ? -0.25f : 0.5f;
    return count;
}
static void check(bool condition, const char *message) {
    if (!condition) { std::fprintf(stderr, "FAIL: %s\n", message); std::exit(1); }
}
int main() {
    ravet::AAudioHost host(ravet_native_state_v1(32));
    std::vector<float> samples(10004, 99);
    for (unsigned cycle = 0; cycle < 3; ++cycle) {
        check(host.session.set(1, 1), "retained stopped controls");
        check(host.start(48000, 256), "start");
        check(outputStream->callback(outputStream, outputStream->context, samples.data(), 5000) == AAUDIO_CALLBACK_RESULT_CONTINUE, "oversize chunking");
        check(samples[9998] == 0.5f && samples[9999] == -0.25f && samples[10000] == 99, "planar/interleaved prefixes/tails");
        check(host.session.set(1, 2), "control edit");
        check(outputStream->callback(outputStream, outputStream->context, samples.data(), 5000) == 0 && samples[9998] == 1, "live gain");
        const auto saved = host.session.save();
        auto invalid = saved; invalid[0] ^= 1;
        check(!host.session.restore(invalid.data(), uint32_t(invalid.size())) && host.session.save() == saved, "atomic invalid recall");
        const uint8_t note[] = {0x90, 60, 100}, bad[] = {0x90, 128, 100};
        check(!host.midi(bad, 3), "MIDI validation");
        for (unsigned i = 0; i < 1024; ++i) check(host.midi(note, 3), "queue fill");
        check(!host.midi(note, 3), "bounded overflow");
        check(outputStream->callback(outputStream, outputStream->context, samples.data(), 5000) == 0, "queue delivery");
        shortRead = 10;
        check(outputStream->callback(outputStream, outputStream->context, samples.data(), 512) == 0, "short input render");
        check(samples[1023] == 0 && host.inputShortages.load() == 10, "shortage silence/count");
        shortRead = 0;
        outputStream->error(outputStream, outputStream->context, -999);
        check(outputStream->callback(outputStream, outputStream->context, samples.data(), 512) == AAUDIO_CALLBACK_RESULT_STOP && samples[0] == 0, "error publication and callback silence");
        host.stop(); check(!host.session.prepared && outputStream == nullptr, "stopped retirement");
    }
    mismatchedInput = true;
    check(!host.start(48000, 256) && !host.session.prepared && outputStream == nullptr, "rate mismatch cleanup");
    mismatchedInput = false; failedOpen = true;
    check(!host.start(48000, 256) && host.failure.load() == -42, "open failure");
    failedOpen = false;
    check(host.start(48000, 256), "recovery"); readError = -998;
    check(outputStream->callback(outputStream, outputStream->context, samples.data(), 512) == AAUDIO_CALLBACK_RESULT_STOP && samples[0] == 0, "capture failure");
    host.stop(); check(closes == 10, "stream ownership balance");
    readError = 0;
    check(host.start(48000, 256), "concurrent-control start");
    std::thread control([&] {
        for (unsigned i = 0; i < 10000; ++i) host.session.set(1, float(i % 5));
    });
    for (unsigned i = 0; i < 1000; ++i) {
        check(outputStream->callback(outputStream, outputStream->context, samples.data(), 64) == 0, "racing publication retains audio progress");
        check(std::isfinite(samples[0]) && samples[0] >= 0 && samples[0] <= 2, "coherent finite control rendering");
    }
    control.join(); host.stop();
    std::puts("Mobile control/state/queue/AAudio-double lifecycle contracts passed");
}
