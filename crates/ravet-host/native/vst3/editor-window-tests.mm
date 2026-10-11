// Actual VST3 modules through the exported editor companion, attached into a
// hidden offscreen NSWindow on the main thread. Arguments:
//   editor:/absolute/module.vst3  module whose controller supplies an NSView editor
//   none:/absolute/module.vst3    module without an editor (must report UNAVAILABLE)
// Each module runs three create/attach/resize/detach/retire cycles; cycle 1 runs
// audio on a worker while the editor is open, and cycle 2 destroys while attached.
#import <Cocoa/Cocoa.h>
#include "RavetVst3Editor.h"
#include <array>
#include <atomic>
#include <cmath>
#include <cstring>
#include <iostream>
#include <stdexcept>
#include <string>
#include <thread>

namespace {
void requireAt(bool value, int line) { if (!value) throw std::runtime_error("editor window assertion line " + std::to_string(line)); }
#define require(value) requireAt(bool(value), __LINE__)
const RavetVst3ProviderV2* api = nullptr;
const RavetVst3EditorProviderV1* editors = nullptr;
void pump(double seconds) {
    @autoreleasepool {
        NSDate* end = [NSDate dateWithTimeIntervalSinceNow:seconds];
        while (end.timeIntervalSinceNow > 0) [[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode beforeDate:end];
    }
}
RavetVst3EditorReport report() { RavetVst3EditorReport r{}; r.struct_size = sizeof(r); return r; }
RavetVst3EditorSize extent(uint32_t w = 0, uint32_t h = 0) { return RavetVst3EditorSize{sizeof(RavetVst3EditorSize), w, h, 0}; }
bool bounded(const RavetVst3EditorSize& s) { return s.width >= 1 && s.width <= 16384 && s.height >= 1 && s.height <= 16384; }
uint32_t audioClass(RavetVst3Module* parent) {
    RavetVst3FactoryRecord factory{}; factory.struct_size = sizeof(factory);
    require(api->factory(parent, &factory) == RAVET_VST3_OK);
    for (uint32_t index = 0; index < factory.class_count; ++index) {
        RavetVst3ClassRecord info{}; info.struct_size = sizeof(info);
        require(api->class_info(parent, index, &info) == RAVET_VST3_OK);
        if (std::string(info.category) == "Audio Module Class") return index;
    }
    throw std::runtime_error("no audio module class");
}
struct Worker {
    std::atomic<bool> stop{false};
    std::atomic<uint64_t> blocks{0};
    std::atomic<bool> failed{false};
    std::thread thread;
    void run(RavetVst3Audio* lease) {
        thread = std::thread([this, lease] {
            std::array<std::array<float, 256>, 2> inputs{}, outputs{};
            std::array<RavetVst3InputChannel, 2> in{}; std::array<RavetVst3OutputChannel, 2> out{};
            for (uint32_t ch = 0; ch < 2; ++ch) {
                for (uint32_t f = 0; f < 256; ++f) inputs[ch][f] = std::sin(float(f) * 0.05f) * 0.25f;
                in[ch] = {inputs[ch].data(), 256, 0}; out[ch] = {outputs[ch].data(), 256, 0};
            }
            RavetVst3AudioBlock block{sizeof(block), 256, 2, 2, in.data(), out.data()};
            while (!stop.load(std::memory_order_acquire)) {
                int32_t native = 0;
                if (api->process(lease, &block, &native) != RAVET_VST3_OK) { failed = true; return; }
                for (uint32_t ch = 0; ch < 2; ++ch)
                    for (uint32_t f = 0; f < 256; ++f) if (!std::isfinite(outputs[ch][f])) { failed = true; return; }
                blocks.fetch_add(1, std::memory_order_relaxed);
                std::this_thread::sleep_for(std::chrono::microseconds(500));
            }
        });
    }
};
struct Counts { uint32_t editors = 0, unavailable = 0, cycles = 0; uint64_t blocks = 0; };
void cycle(const std::string& path, bool expectEditor, int index, Counts& counts) {
    RavetVst3Module* parent = nullptr; int32_t native = 0;
    require(api->open(reinterpret_cast<const uint8_t*>(path.data()), uint32_t(path.size()), &parent, &native) == RAVET_VST3_OK);
    RavetVst3Instance* child = nullptr;
    require(api->create(parent, audioClass(parent), &child, &native) == RAVET_VST3_OK);
    RavetVst3Editor* editor = nullptr; auto initial = extent(); auto r = report();
    const auto created = editors->create(child, RAVET_VST3_EDITOR_PLATFORM_NSVIEW, &editor, &initial, &r);
    if (!expectEditor) {
        require(created == RAVET_VST3_EDITOR_UNAVAILABLE && !editor && r.stage == 201 && r.failure == 7 && !r.native_result);
        ++counts.unavailable;
        require(api->destroy(child, &native) == RAVET_VST3_OK);
        require(api->close(parent, &native) == RAVET_VST3_OK);
        ++counts.cycles;
        return;
    }
    require(created == RAVET_VST3_OK && editor && bounded(initial));
    ++counts.editors;
    NSWindow* window = [[NSWindow alloc] initWithContentRect:NSMakeRect(-20000, -20000, initial.width, initial.height)
                                                   styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskResizable
                                                     backing:NSBackingStoreBuffered defer:NO];
    window.releasedWhenClosed = NO;
    require(!window.visible); // never ordered front
    NSView* content = window.contentView;
    require(content.subviews.count == 0);
    Worker worker; RavetVst3Audio* lease = nullptr;
    if (index == 1) {
        RavetVst3AudioProfile profile{sizeof(profile), 256, 2, 2, 48000.};
        require(api->prepare(child, &profile, &native) == RAVET_VST3_OK);
        require(api->start(child, &native) == RAVET_VST3_OK);
        require(api->begin_audio(child, &lease, &native) == RAVET_VST3_OK && lease);
        worker.run(lease);
    }
    auto applied = extent();
    require(editors->attach(editor, (__bridge void*)content, &applied, &r) == RAVET_VST3_OK);
    require(r.flags & RAVET_VST3_EDITOR_ATTACHED && bounded(applied));
    require(content.subviews.count >= 1);
    pump(0.3);
    auto size = extent();
    require(editors->size(editor, &size, &r) == RAVET_VST3_OK && bounded(size));
    // Host-initiated resizes through canResize/checkSizeConstraint/onSize.
    RavetVst3EditorState state{}; state.struct_size = sizeof(state);
    require(editors->state(editor, &state) == RAVET_VST3_OK && (state.flags & 1));
    std::array<RavetVst3EditorSize, 2> proposals{extent(640, 480), extent(100, 100)};
    for (auto proposed : proposals) {
        const auto status = editors->resize(editor, &proposed, &applied, &r);
        if (status == RAVET_VST3_EDITOR_UNSUPPORTED) { require(r.stage == 206 && r.failure == 8); continue; }
        require(status == RAVET_VST3_OK && bounded(applied) && r.stage == 208);
        [window setContentSize:NSMakeSize(applied.width, applied.height)]; // consumer resizes its container
        auto again = extent();
        require(editors->size(editor, &again, &r) == RAVET_VST3_OK && again.width == applied.width && again.height == applied.height);
        auto repeat = applied, stable = extent();
        require(editors->resize(editor, &repeat, &stable, &r) == RAVET_VST3_OK && stable.width == applied.width && stable.height == applied.height);
        std::cout << "resize " << proposed.width << "x" << proposed.height << " -> " << applied.width << "x" << applied.height << '\n';
        pump(0.05);
    }
    const auto scaled = editors->set_scale(editor, 2.f, &applied, &r);
    require(scaled == RAVET_VST3_OK || (scaled == RAVET_VST3_EDITOR_UNSUPPORTED && r.stage == 209 && r.failure == 8));
    auto request = extent();
    const auto taken = editors->take_request(editor, &request);
    require(taken == RAVET_VST3_EDITOR_EMPTY || (taken == RAVET_VST3_OK && bounded(request)));
    // Ordering: neither the child nor the module retires while the view lives.
    if (!lease) require(api->destroy(child, &native) == RAVET_VST3_LIFECYCLE);
    require(api->close(parent, &native) == RAVET_VST3_CHILDREN);
    if (index == 2) {
        r = report();
        require(editors->destroy(editor, &r) == RAVET_VST3_OK && r.stage == 212 && r.failure == 0);
    } else {
        require(editors->detach(editor, &r) == RAVET_VST3_OK && !(r.flags & RAVET_VST3_EDITOR_ATTACHED));
        require(content.subviews.count == 0);
        r = report();
        require(editors->destroy(editor, &r) == RAVET_VST3_OK && r.failure == 0);
    }
    pump(0.1);
    require(content.subviews.count == 0);
    if (lease) {
        pump(0.1);
        worker.stop = true; worker.thread.join();
        require(!worker.failed && worker.blocks > 0);
        counts.blocks += worker.blocks;
        require(api->end_audio(lease, &native) == RAVET_VST3_OK);
        require(api->stop(child, &native) == RAVET_VST3_OK);
    }
    require(api->destroy(child, &native) == RAVET_VST3_OK);
    require(api->close(parent, &native) == RAVET_VST3_OK);
    [window close];
    pump(0.05);
    ++counts.cycles;
}
} // namespace
int main(int argc, char** argv) {
    @autoreleasepool {
        try {
            require(argc >= 2 && pthread_main_np());
            api = ravet_vst3_provider_v2(); editors = ravet_vst3_editor_provider_v1();
            require(editors->struct_size == sizeof(*editors) && editors->version == 1 && editors->instance_origin == api);
            [NSApplication sharedApplication];
            [NSApp setActivationPolicy:NSApplicationActivationPolicyProhibited];
            Counts counts;
            for (int argument = 1; argument < argc; ++argument) {
                const std::string value = argv[argument];
                const auto colon = value.find(':');
                require(colon != std::string::npos);
                const auto kind = value.substr(0, colon), path = value.substr(colon + 1);
                require(kind == "editor" || kind == "none");
                for (int index = 0; index < 3; ++index) cycle(path, kind == "editor", index, counts);
                std::cout << kind << " " << path << ": 3 cycles\n";
            }
            std::cout << "PASS actual VST3 editors in a hidden offscreen NSWindow: " << counts.cycles << " cycles, "
                      << counts.editors << " attached editors, " << counts.unavailable << " UNAVAILABLE refusals, "
                      << counts.blocks << " concurrent audio blocks, child/module retirement refused while open\n";
        } catch (const std::exception& error) {
            std::cerr << error.what() << '\n';
            return 1;
        }
    }
    return 0;
}
