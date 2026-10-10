#ifndef RAVET_EMBEDDED_CPP_H
#define RAVET_EMBEDDED_CPP_H
#include <stddef.h>
#include <stdint.h>
#include <math.h>

// Native C++ subset for Arduino cores without a supported Rust toolchain.
// All storage is caller/instance owned. No heap, OS, atomics, HAL or device I/O.
// C++11. Prepare/deactivate while callbacks are stopped. Exclusively own each
// instance in one callback context; consumers own interrupt/core synchronization.
namespace ravet { namespace embedded {
struct Config {
    double sampleRate;
    size_t maximumFrames;
    size_t maximumEvents;
};
struct ParameterEvent {
    size_t frameOffset;
    uint32_t id;
    float value;
};
enum class Status { Continue, Silence, Fault };
struct StereoBlock {
    const float* input[2];
    float* output[2];
    size_t frames;
    const ParameterEvent* events;
    size_t eventCount;
};

// Processor contract: bool prepare(Config), Status process(StereoBlock&),
// void reset(), void deactivate(). The adapter validates bounds/order/finite
// event values before calling DSP. DSP validates its narrower parameter policy
// before mutation and initializes every active output. Unknown IDs may be ignored.
// Processor code remains responsible for no allocation/blocking and timing.
template<class Processor, size_t Capacity>
class StereoAdapter {
    static_assert(Capacity > 0, "nonzero fixed capacity required");
    Processor processor_;
    Config config_;
    bool prepared_;
    float input_[2][Capacity];
    float output_[2][Capacity];
    StereoAdapter(const StereoAdapter&) = delete;
    StereoAdapter& operator=(const StereoAdapter&) = delete;
public:
    StereoAdapter() : processor_(), config_{0, 0, 0}, prepared_(false), input_{}, output_{} {}
    bool prepare(Config config) {
        deactivate();
        if (!isfinite(config.sampleRate) || config.sampleRate <= 0
            || config.maximumFrames == 0 || config.maximumFrames > Capacity) return false;
        if (!processor_.prepare(config)) return false;
        config_ = config;
        prepared_ = true;
        return true;
    }
    void reset() { processor_.reset(); }
    void deactivate() { prepared_ = false; processor_.deactivate(); }
    const Processor& processor() const { return processor_; }
    // Stereo f32 lengths are samples, frames are not samples. Null is allowed
    // only with zero length. Input/output can overlap; neither overlaps this
    // instance. Nonnull spans must be live for their lengths. Active output is
    // silenced on rejection; backing tails are retained. Zero-frame events work.
    Status process(const float* input, size_t inputSamples, float* output,
                   size_t outputSamples, size_t frames,
                   const ParameterEvent* events = nullptr, size_t eventCount = 0) {
        const size_t active = frames > SIZE_MAX / 2 ? outputSamples
                                                  : (frames * 2 < outputSamples ? frames * 2 : outputSamples);
        bool valid = prepared_ && frames <= config_.maximumFrames
            && (inputSamples == 0 || input) && (outputSamples == 0 || output)
            && frames <= SIZE_MAX / 2 && inputSamples >= frames * 2 && outputSamples >= frames * 2
            && eventCount <= config_.maximumEvents && (eventCount == 0 || events);
        size_t previous = 0;
        if (valid) for (size_t i = 0; i < eventCount; ++i) {
            const ParameterEvent& event = events[i];
            if (!isfinite(event.value) || event.frameOffset < previous
                || (frames == 0 ? event.frameOffset != 0 : event.frameOffset >= frames)) {
                valid = false;
                break;
            }
            previous = event.frameOffset;
        }
        if (!valid) {
            if (output) for (size_t i = 0; i < active; ++i) output[i] = 0;
            return Status::Fault;
        }
        for (size_t i = 0; i < frames; ++i)
            for (size_t c = 0; c < 2; ++c) input_[c][i] = input[2 * i + c];
        StereoBlock block{{input_[0], input_[1]}, {output_[0], output_[1]}, frames, events, eventCount};
        const Status status = processor_.process(block);
        if (status != Status::Continue) {
            for (size_t i = 0; i < active; ++i) output[i] = 0;
        } else {
            for (size_t i = 0; i < frames; ++i)
                for (size_t c = 0; c < 2; ++c) output[2 * i + c] = output_[c][i];
        }
        return status;
    }
};

// Public reference only: stereo immediate gain ID 0, amplitude 0..4, default 1.
// No smoothing or arbitrary Rust processor translation. Timed parameter events
// take effect before the corresponding sample, in supplied same-frame order.
class Gain {
    float gain_;
public:
    Gain() : gain_(1) {}
    bool prepare(Config) { return true; }
    void reset() {}
    void deactivate() {}
    float gain() const { return gain_; }
    Status process(StereoBlock& block) {
        for (size_t i = 0; i < block.eventCount; ++i) {
            const ParameterEvent& e = block.events[i];
            if (e.id == 0 && (!isfinite(e.value) || e.value < 0 || e.value > 4)) return Status::Fault;
        }
        size_t next = 0;
        for (size_t frame = 0; frame < block.frames; ++frame) {
            while (next < block.eventCount && block.events[next].frameOffset == frame) {
                if (block.events[next].id == 0) gain_ = block.events[next].value;
                ++next;
            }
            for (size_t c = 0; c < 2; ++c) block.output[c][frame] = block.input[c][frame] * gain_;
        }
        // All remaining events belong to a zero-frame flush.
        for (; next < block.eventCount; ++next)
            if (block.events[next].id == 0) gain_ = block.events[next].value;
        return Status::Continue;
    }
};
} }
#endif
