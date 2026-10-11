// Graph-bank child editor (decision 0276): the actual RAVET Gain VST3 editor,
// attached into a hidden offscreen NSWindow by the Rust bank in the linked
// ravet-host vst3-editor-bank example staticlib, while the bank processes AUDIO
// on a worker. Argument: /absolute/ravet_gain_vst3.vst3 built with
// RAVET_GAIN_EDITOR=ON. This harness owns the window and injects programmatic
// edits into the Gain editor's own script-message bridge, so the plug-in's
// controller issues beginEdit/performEdit/endEdit itself.
#import <Cocoa/Cocoa.h>
#import <WebKit/WebKit.h>
#include "RavetVst3EditGestures.h"
#include "RavetVst3Editor.h"
#include "RavetVst3HostedAudio.h"
#include "RavetVst3Parameters.h"
#include "RavetVst3Processing.h"
#include <iostream>

namespace {
struct Host {
    void* context;
    void* (*open_window)(void*, uint32_t, uint32_t, uint32_t);
    int32_t (*edit)(void*, const char*);
    void (*pump)(void*, double);
    uint32_t (*subviews)(void*);
    void (*resize_window)(void*, uint32_t, uint32_t);
    void (*close_window)(void*);
};
NSWindow* window = nil;
// The bridge only reads `body`; a plain object stands in for WKScriptMessage.
} // namespace
@interface RavetInjectedMessage : NSObject
@property(nonatomic, strong) id body;
@end
@implementation RavetInjectedMessage
@end
namespace {
void pump(void*, double seconds) {
    @autoreleasepool {
        NSDate* end = [NSDate dateWithTimeIntervalSinceNow:seconds];
        while (end.timeIntervalSinceNow > 0) [[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode beforeDate:end];
    }
}
void* openWindow(void*, uint32_t, uint32_t width, uint32_t height) {
    if (window || !pthread_main_np()) return nullptr;
    window = [[NSWindow alloc] initWithContentRect:NSMakeRect(-20000, -20000, width, height)
                                         styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskResizable
                                           backing:NSBackingStoreBuffered defer:NO];
    window.releasedWhenClosed = NO;
    if (window.visible || window.contentView.subviews.count) return nullptr; // never ordered front
    return (__bridge void*)window.contentView;
}
uint32_t subviews(void*) { return window ? uint32_t(window.contentView.subviews.count) : 0; }
void resizeWindow(void*, uint32_t width, uint32_t height) { [window setContentSize:NSMakeSize(width, height)]; }
void closeWindow(void*) { [window close]; window = nil; }
int32_t edit(void*, const char* json) {
    if (!window || !json) return 1;
    NSData* data = [NSData dataWithBytes:json length:strlen(json)];
    id body = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
    if (![body isKindOfClass:[NSDictionary class]]) return 2;
    for (NSView* view in window.contentView.subviews) {
        id bridge = nil;
        @try { bridge = [view valueForKey:@"bridge"]; } @catch (NSException*) { continue; }
        SEL selector = @selector(userContentController:didReceiveScriptMessage:);
        if (![bridge respondsToSelector:selector]) continue;
        RavetInjectedMessage* message = [RavetInjectedMessage new];
        message.body = body;
        [(id<WKScriptMessageHandler>)bridge userContentController:(WKUserContentController*)[NSNull null]
                                          didReceiveScriptMessage:(WKScriptMessage*)message];
        return 0;
    }
    return 3; // no editor bridge in the window
}
} // namespace
extern "C" int32_t ravet_vst3_editor_bank_run(const RavetVst3ProviderV2*, const RavetVst3ParameterProviderV1*,
    const RavetVst3ProcessingProviderV1*, const RavetVst3HostedAudioProviderV1*, const RavetVst3EditorProviderV1*,
    const RavetVst3EditGestureProviderV1*, const char*, const Host*);
int main(int argc, char** argv) {
    @autoreleasepool {
        if (argc != 2 || !pthread_main_np()) {
            std::cerr << "usage: editor-bank-window-tests /abs/ravet_gain_vst3.vst3 (main thread)\n";
            return 2;
        }
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyProhibited];
        const Host host{nullptr, openWindow, edit, pump, subviews, resizeWindow, closeWindow};
        return ravet_vst3_editor_bank_run(ravet_vst3_provider_v2(), ravet_vst3_parameter_provider_v1(),
            ravet_vst3_processing_provider_v1(), ravet_vst3_hosted_audio_provider_v1(), ravet_vst3_editor_provider_v1(),
            ravet_vst3_edit_gesture_provider_v1(), argv[1], &host);
    }
}
