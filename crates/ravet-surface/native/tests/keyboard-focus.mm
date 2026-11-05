// Keyboard focus for embedded editors whose host window does not become key
// on its own (as in Pro Tools). Three hosts are modelled with real AppKit
// windows: a panel that becomes key only if needed, a window that refuses key
// status, and an ordinary key window (as in Logic or Live). Run with
// ctest (ravet-web-keyboard-focus). This is not a Pro Tools run.
#import <Cocoa/Cocoa.h>
#import <WebKit/WebKit.h>
#include "RavetWebView.h"
#include <iostream>
#include <stdexcept>
#include <string>

@interface RefusingWindow : NSWindow
@end
@implementation RefusingWindow
- (BOOL)canBecomeKeyWindow { return NO; }
@end

static void require(bool value, const char* text) { if (!value) throw std::runtime_error(text); }
static void pump(double seconds) {
    NSDate* until = [NSDate dateWithTimeIntervalSinceNow:seconds];
    while ([until timeIntervalSinceNow] > 0) {
        NSEvent* event = [NSApp nextEventMatchingMask:NSEventMaskAny untilDate:[NSDate dateWithTimeIntervalSinceNow:0.01]
                                               inMode:NSDefaultRunLoopMode dequeue:YES];
        if (event) [NSApp sendEvent:event];
    }
}
static void message(void*, const char*) {}
static size_t snapshot(void*, char*, size_t) { return 0; }
static std::string evaluate(WKWebView* web, NSString* script) {
    __block std::string out; __block bool done = false;
    [web evaluateJavaScript:script completionHandler:^(id result, NSError*) {
        if ([result isKindOfClass:NSString.class]) out = [result UTF8String];
        else if (result) out = [[result description] UTF8String];
        done = true;
    }];
    for (int i = 0; i < 500 && !done; ++i) pump(0.01);
    require(done, "script finished");
    return out;
}
static NSString* const kPage = @"<!doctype html><html><body><input id=t type=text><button id=b>b</button>"
    "<script>document.getElementById('t').addEventListener('blur',()=>{window.blurred=(window.blurred||0)+1});</script>"
    "</body></html>";

struct Editor { void* view; WKWebView* web; };
static Editor open(NSWindow* window) {
    void* view = ravet_web_create_html(kPage.UTF8String, 300, 200, nullptr, message, snapshot);
    require(view, "editor created");
    ravet_web_attach(view, (__bridge void*)window.contentView);
    WKWebView* web = (WKWebView*)((__bridge NSView*)view).subviews.firstObject;
    require([web isKindOfClass:WKWebView.class], "web view found");
    for (int i = 0; i < 500 && evaluate(web, @"document.readyState") != "complete"; ++i) pump(0.01);
    return {view, web};
}
static NSEvent* key(NSWindow* window, NSString* text, NSEventType type) {
    return [NSEvent keyEventWithType:type location:NSZeroPoint modifierFlags:0 timestamp:0
                        windowNumber:window.windowNumber context:nil characters:text
         charactersIgnoringModifiers:text isARepeat:NO keyCode:0];
}
static void type(NSWindow* target, NSString* text) {
    [NSApp postEvent:key(target, text, NSEventTypeKeyDown) atStart:NO];
    [NSApp postEvent:key(target, text, NSEventTypeKeyUp) atStart:NO];
    pump(0.3);
}
static NSWindow* window(Class kind, NSRect frame) {
    NSWindow* w = [[kind alloc] initWithContentRect:frame styleMask:NSWindowStyleMaskTitled
                                            backing:NSBackingStoreBuffered defer:NO];
    w.releasedWhenClosed = NO;
    return w;
}
static int groups = 0;
static void pass(const char* text) { ++groups; std::cout << text << " PASS\n"; }

int main() { @autoreleasepool {
    try {
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];
        [NSApp finishLaunching];
        [NSApp activateIgnoringOtherApps:YES];
        NSWindow* host = window(NSWindow.class, NSMakeRect(100, 100, 400, 300));
        [host makeKeyAndOrderFront:nil];
        pump(0.2);
        require(host.isKeyWindow, "host window is key");

        // A panel that becomes key only when needed: the editor takes key
        // status for the text field and hands it back afterwards.
        NSPanel* panel = (NSPanel*)window(NSPanel.class, NSMakeRect(200, 200, 300, 200));
        panel.becomesKeyOnlyIfNeeded = YES;
        [panel orderFront:nil];
        Editor a = open(panel);
        require(host.isKeyWindow, "panel did not take key status on open");
        evaluate(a.web, @"document.getElementById('b').focus(),''");
        pump(0.3);
        require(host.isKeyWindow, "a button does not take key status");
        evaluate(a.web, @"document.getElementById('t').focus(),''");
        pump(0.3);
        require(panel.isKeyWindow, "panel became key for the text field");
        require([(NSView*)panel.firstResponder isKindOfClass:NSView.class] &&
                [(NSView*)panel.firstResponder isDescendantOf:a.web], "web view is first responder");
        type(panel, @"4");
        require(evaluate(a.web, @"document.getElementById('t').value") == "4", "typed into the panel");
        evaluate(a.web, @"document.getElementById('t').blur(),''");
        pump(0.3);
        require(host.isKeyWindow, "host key status restored after blur");
        // A field removed while focused (Enter in the delay) also releases.
        evaluate(a.web, @"document.getElementById('t').focus(),''");
        pump(0.3);
        require(panel.isKeyWindow, "panel key again");
        evaluate(a.web, @"document.getElementById('t').remove(),''");
        pump(0.4);
        require(host.isKeyWindow, "host key status restored after removal");
        ravet_web_destroy(a.view);
        [panel close];
        pass("panel");

        // A window that refuses key status: plain typing reaches the field
        // while it has focus; Command keys and later typing stay with the host.
        [host makeKeyAndOrderFront:nil];
        NSWindow* refusing = window(RefusingWindow.class, NSMakeRect(220, 220, 300, 200));
        [refusing orderFront:nil];
        Editor b = open(refusing);
        evaluate(b.web, @"document.getElementById('t').focus(),''");
        pump(0.3);
        require(host.isKeyWindow, "host keeps key status");
        type(host, @"7");
        require(evaluate(b.web, @"document.getElementById('t').value") == "7", "typing forwarded");
        NSEvent* command = [NSEvent keyEventWithType:NSEventTypeKeyDown location:NSZeroPoint
            modifierFlags:NSEventModifierFlagCommand timestamp:0 windowNumber:host.windowNumber context:nil
            characters:@"s" charactersIgnoringModifiers:@"s" isARepeat:NO keyCode:1];
        [NSApp postEvent:command atStart:NO];
        pump(0.3);
        require(evaluate(b.web, @"document.getElementById('t').value") == "7", "Command keys stay with the host");
        // A click in another window ends typing and blurs the field.
        NSEvent* click = [NSEvent mouseEventWithType:NSEventTypeLeftMouseDown location:NSMakePoint(10, 10)
            modifierFlags:0 timestamp:0 windowNumber:host.windowNumber context:nil eventNumber:0 clickCount:1 pressure:1];
        [NSApp postEvent:click atStart:NO];
        [NSApp postEvent:[NSEvent mouseEventWithType:NSEventTypeLeftMouseUp location:NSMakePoint(10, 10)
            modifierFlags:0 timestamp:0 windowNumber:host.windowNumber context:nil eventNumber:0 clickCount:1 pressure:0]
                 atStart:NO];
        pump(0.4);
        require(evaluate(b.web, @"String(window.blurred||0)") == "1", "click elsewhere blurred the field");
        type(host, @"8");
        require(evaluate(b.web, @"document.getElementById('t').value") == "7", "typing not forwarded after blur");
        ravet_web_destroy(b.view);
        [refusing close];
        pass("refusing window");

        // An ordinary window that is already key: nothing changes.
        NSWindow* plain = window(NSWindow.class, NSMakeRect(240, 240, 300, 200));
        [plain makeKeyAndOrderFront:nil];
        Editor c = open(plain);
        pump(0.2);
        require(plain.isKeyWindow, "plain window key");
        evaluate(c.web, @"document.getElementById('t').focus(),''");
        pump(0.3);
        type(plain, @"5");
        require(evaluate(c.web, @"document.getElementById('t').value") == "5", "typed normally");
        evaluate(c.web, @"document.getElementById('t').blur(),''");
        pump(0.3);
        require(plain.isKeyWindow, "plain window stays key after blur");
        ravet_web_destroy(c.view);
        [plain close];
        [host close];
        pass("key window");
    } catch (const std::exception& error) {
        std::cerr << "FAIL: " << error.what() << "\n";
        return 1;
    }
    std::cout << groups << " groups PASS\n";
    return 0;
} }
