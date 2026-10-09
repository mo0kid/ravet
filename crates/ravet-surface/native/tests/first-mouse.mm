// Exercise an initial drag through AppKit while another owned window is key.
#import <Cocoa/Cocoa.h>
#include "RavetWebView.h"
#include <iostream>
#include <stdexcept>
#include <vector>
#include <string>
static void require(bool ok,const char* message) { if(!ok) throw std::runtime_error(message); }
static void pump(double seconds) {
    NSDate* end=[NSDate dateWithTimeIntervalSinceNow:seconds];
    while(end.timeIntervalSinceNow>0) {
        NSEvent* event=[NSApp nextEventMatchingMask:NSEventMaskAny untilDate:[NSDate dateWithTimeIntervalSinceNow:.01]
            inMode:NSDefaultRunLoopMode dequeue:YES];
        if(event) [NSApp sendEvent:event];
    }
}
static void mouse(NSWindow* window,NSEventType type,NSPoint point) {
    NSEvent* event=[NSEvent mouseEventWithType:type location:point modifierFlags:0
        timestamp:NSProcessInfo.processInfo.systemUptime windowNumber:window.windowNumber
        context:nil eventNumber:1 clickCount:1 pressure:type==NSEventTypeLeftMouseUp?0:1];
    [NSApp sendEvent:event];
    pump(.1);
}
int main() {
    @autoreleasepool { try {
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];
        [NSApp finishLaunching];
        [NSApp activateIgnoringOtherApps:YES];
        auto* window=[[NSWindow alloc] initWithContentRect:NSMakeRect(200,200,320,160)
            styleMask:NSWindowStyleMaskTitled backing:NSBackingStoreBuffered defer:NO];
        window.title=@"RAVET first-drag probe";
        auto* other=[[NSWindow alloc] initWithContentRect:NSMakeRect(550,200,160,160)
            styleMask:NSWindowStyleMaskTitled backing:NSBackingStoreBuffered defer:NO];
        other.title=@"RAVET focus probe";
        std::vector<std::string> messages;
        const char* html="<!doctype html><html><body style='margin:0'><div id='fader' style='width:320px;height:160px;touch-action:none;background:#123456'></div><script>const f=document.querySelector('#fader');let dragging=false;const send=phase=>webkit.messageHandlers.ravet.postMessage({phase});f.onpointerdown=e=>{dragging=true;f.setPointerCapture(e.pointerId);send('begin')};f.onpointermove=e=>{if(dragging)send('change')};f.onpointerup=e=>{if(dragging){dragging=false;send('end')}};send('ready');</script></body></html>";
        void* view=ravet_web_create_html(html,320,160,&messages,
            [](void* context,const char* json){ static_cast<std::vector<std::string>*>(context)->emplace_back(json); },nullptr);
        require(view,"create editor");
        ravet_web_attach(view,(__bridge void*)window.contentView);
        [window makeKeyAndOrderFront:nil];
        for(int attempt=0;attempt<100&&messages.empty();++attempt) pump(.03);
        require(!messages.empty(),"page not ready");
        [other makeKeyAndOrderFront:nil];pump(.1);
        require(other.isKeyWindow&&!window.isKeyWindow,"test window did not lose focus");
        messages.clear();
        mouse(window,NSEventTypeLeftMouseDown,NSMakePoint(100,80));
        mouse(window,NSEventTypeLeftMouseDragged,NSMakePoint(100,110));
        mouse(window,NSEventTypeLeftMouseUp,NSMakePoint(100,110));
        bool begin=false,change=false,end=false;
        for(const auto& message:messages) {
            if(message.find("begin")!=std::string::npos) begin=true;
            if(message.find("change")!=std::string::npos) change=true;
            if(message.find("end")!=std::string::npos) end=true;
        }
        std::cout<<"first inactive drag: begin="<<begin<<" change="<<change<<" end="<<end<<'\n';
        ravet_web_destroy(view);[window orderOut:nil];[other orderOut:nil];
        require(begin&&change&&end,"inactive-window first drag did not reach the page");
        return 0;
    } catch(const std::exception& error) { std::cerr<<error.what()<<'\n';return 1; } }
}
