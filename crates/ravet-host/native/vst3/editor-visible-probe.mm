// MANUAL ONLY: opens one actual VST3 editor in a visible, resizable NSWindow for
// a person to inspect. Not registered with CTest; build the EXCLUDE_FROM_ALL
// target explicitly and run:
//   ravet-vst3-editor-visible-probe /absolute/module.vst3 [seconds]
// Window resizes go through resize(); plug-in frame requests are polled with
// take_request() and applied to the window. Closing the window or the timeout
// detaches, destroys the editor, then retires the child and module.
#import <Cocoa/Cocoa.h>
#include "RavetVst3Editor.h"
#include <cstdlib>
#include <iostream>
#include <string>

namespace {
const RavetVst3ProviderV2* api = nullptr;
const RavetVst3EditorProviderV1* editors = nullptr;
RavetVst3EditorReport report() { RavetVst3EditorReport r{}; r.struct_size = sizeof(r); return r; }
RavetVst3EditorSize extent(uint32_t w = 0, uint32_t h = 0) { return RavetVst3EditorSize{sizeof(RavetVst3EditorSize), w, h, 0}; }
int fail(const char* what, int32_t status, const RavetVst3EditorReport* r = nullptr) {
    std::cerr << what << " failed: status " << status;
    if (r) std::cerr << " stage " << r->stage << " failure " << r->failure << " native " << r->native_result;
    std::cerr << '\n';
    return 1;
}
}
int main(int argc, char** argv) {
    @autoreleasepool {
        if (argc < 2) { std::cerr << "usage: ravet-vst3-editor-visible-probe /absolute/module.vst3 [seconds]\n"; return 2; }
        const std::string path = argv[1];
        const double seconds = argc > 2 ? std::atof(argv[2]) : 30.0;
        api = ravet_vst3_provider_v2(); editors = ravet_vst3_editor_provider_v1();
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
        RavetVst3Module* parent = nullptr; int32_t native = 0;
        if (auto s = api->open(reinterpret_cast<const uint8_t*>(path.data()), uint32_t(path.size()), &parent, &native)) return fail("open", s);
        RavetVst3FactoryRecord factory{}; factory.struct_size = sizeof(factory);
        api->factory(parent, &factory);
        uint32_t index = 0;
        for (; index < factory.class_count; ++index) {
            RavetVst3ClassRecord info{}; info.struct_size = sizeof(info);
            if (api->class_info(parent, index, &info) == RAVET_VST3_OK && std::string(info.category) == "Audio Module Class") break;
        }
        RavetVst3Instance* child = nullptr;
        if (auto s = api->create(parent, index, &child, &native)) { api->close(parent, &native); return fail("create instance", s); }
        RavetVst3Editor* editor = nullptr; auto size = extent(); auto r = report();
        if (auto s = editors->create(child, RAVET_VST3_EDITOR_PLATFORM_NSVIEW, &editor, &size, &r)) {
            api->destroy(child, &native); api->close(parent, &native);
            return fail("create editor", s, &r);
        }
        NSWindow* window = [[NSWindow alloc] initWithContentRect:NSMakeRect(200, 200, size.width, size.height)
            styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskClosable | NSWindowStyleMaskResizable
            backing:NSBackingStoreBuffered defer:NO];
        window.releasedWhenClosed = NO;
        window.title = @"RAVET hosted VST3 editor (manual probe)";
        [window makeKeyAndOrderFront:nil];
        [NSApp activateIgnoringOtherApps:YES];
        auto applied = extent();
        if (auto s = editors->attach(editor, (__bridge void*)window.contentView, &applied, &r)) {
            editors->destroy(editor, &r); api->destroy(child, &native); api->close(parent, &native);
            return fail("attach", s, &r);
        }
        __block RavetVst3EditorSize last = applied;
        __block bool finished = false;
        NSDate* deadline = [NSDate dateWithTimeIntervalSinceNow:seconds];
        while (!finished) {
            @autoreleasepool {
                [[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.05]];
                NSEvent* event;
                while ((event = [NSApp nextEventMatchingMask:NSEventMaskAny untilDate:nil inMode:NSDefaultRunLoopMode dequeue:YES]))
                    [NSApp sendEvent:event];
                auto request = extent();
                if (editors->take_request(editor, &request) == RAVET_VST3_OK) {
                    [window setContentSize:NSMakeSize(request.width, request.height)];
                    last = request;
                }
                const NSSize content = window.contentView.frame.size;
                if (uint32_t(content.width) != last.width || uint32_t(content.height) != last.height) {
                    auto proposed = extent(uint32_t(content.width), uint32_t(content.height)); auto adjusted = extent();
                    if (editors->resize(editor, &proposed, &adjusted, &r) == RAVET_VST3_OK) {
                        last = adjusted;
                        if (adjusted.width != proposed.width || adjusted.height != proposed.height)
                            [window setContentSize:NSMakeSize(adjusted.width, adjusted.height)];
                    } else {
                        [window setContentSize:NSMakeSize(last.width, last.height)];
                    }
                }
                finished = !window.visible || deadline.timeIntervalSinceNow <= 0;
            }
        }
        int32_t detached = editors->detach(editor, &r);
        int32_t destroyed = editors->destroy(editor, &r);
        int32_t child_status = api->destroy(child, &native);
        int32_t module_status = api->close(parent, &native);
        [window close];
        std::cout << "manual probe finished: detach " << detached << " destroy " << destroyed << " child " << child_status
                  << " module " << module_status << " last " << last.width << "x" << last.height << '\n';
        return detached == RAVET_VST3_OK && destroyed == RAVET_VST3_OK && child_status == RAVET_VST3_OK && module_status == RAVET_VST3_OK ? 0 : 1;
    }
}
