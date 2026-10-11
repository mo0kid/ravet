#import <Cocoa/Cocoa.h>
#import <WebKit/WebKit.h>
#include "RavetWebView.h"
#include <iostream>
#include <stdexcept>
#include <thread>
#include <chrono>
static void require(bool ok,const char* message) { if(!ok)throw std::runtime_error(message); }
static void pump() {
    [[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode beforeDate:[NSDate dateWithTimeIntervalSinceNow:.025]];
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
}
static NSString* evaluate(WKWebView* web,NSString* script) {
    __block bool done=false;__block id value=nil;__block NSError* failure=nil;
    [web evaluateJavaScript:script completionHandler:^(id result,NSError* error){value=result;failure=error;done=true;}];
    for(int i=0;i<200&&!done;i++)pump();
    require(done&&!failure,"pointer probe JavaScript failed");
    return [value isKindOfClass:[NSString class]]?value:nil;
}
static void mouse(NSWindow* window,NSEventType type,NSPoint point) {
    [NSApp sendEvent:[NSEvent mouseEventWithType:type location:point modifierFlags:0
        timestamp:NSProcessInfo.processInfo.systemUptime windowNumber:window.windowNumber
        context:nil eventNumber:1 clickCount:1 pressure:type==NSEventTypeLeftMouseUp?0:1]];
    for(int i=0;i<5;i++)pump();
}
int main(int argc,char** argv) {
    @autoreleasepool { try {
        require(argc==2,"control semantic fixture path required");[NSApplication sharedApplication];
        void* view=ravet_web_create(argv[1],1100,900,nullptr,nullptr,nullptr);
        require(view,"create control semantic WebKit fixture");
        auto* container=(__bridge NSView*)view;
        auto* web=static_cast<WKWebView*>(container.subviews.firstObject);
        __block bool finished=false,passed=false;__block NSString* evidence=nil;
        for(int attempt=0;attempt<1500&&!finished;attempt++) {
            __block bool evaluated=false;
            [web evaluateJavaScript:@"window.ravetSemanticResults ? JSON.stringify(window.ravetSemanticResults) : null" completionHandler:^(id result,NSError* error) {
                if(!error&&[result isKindOfClass:[NSString class]]) {
                    evidence=result;NSDictionary* report=[NSJSONSerialization JSONObjectWithData:[evidence dataUsingEncoding:NSUTF8StringEncoding] options:0 error:nil];
                    finished=true;passed=[report[@"pass"] boolValue];
                }
                evaluated=true;
            }];
            for(int wait=0;wait<20&&!evaluated;wait++)pump();pump();
        }
        if(!finished){__block bool sampled=false;[web evaluateJavaScript:@"JSON.stringify(window.ravetSemanticProgress)" completionHandler:^(id result,NSError* error){if(error)std::cerr<<error.localizedDescription.UTF8String<<'\n';if(result)std::cout<<[(NSString*)result UTF8String]<<'\n';sampled=true;}];for(int i=0;i<50&&!sampled;i++)pump();}
        if(evidence)std::cout<<evidence.UTF8String<<'\n';
        require(finished&&passed,"WebKit control semantic contracts failed");
        // Native range dragging must survive; cancelling pointerdown would also
        // disable the browser's actual fader interaction.
        [NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];[NSApp finishLaunching];[NSApp activateIgnoringOtherApps:YES];
        auto* window=[[NSWindow alloc] initWithContentRect:NSMakeRect(200,200,1100,900)
            styleMask:NSWindowStyleMaskTitled backing:NSBackingStoreBuffered defer:NO];
        window.title=@"RAVET pointer control probe";ravet_web_attach(view,(__bridge void*)window.contentView);
        [window makeKeyAndOrderFront:nil];for(int i=0;i<10;i++)pump();
        for(NSString* kind in @[@"knob",@"fader"]){
            NSString* position=evaluate(web,[NSString stringWithFormat:@"JSON.stringify(ravetPointerNative.setup('%@'))",kind]);
            require(position,"native pointer setup");
            NSDictionary* point=[NSJSONSerialization JSONObjectWithData:[position dataUsingEncoding:NSUTF8StringEncoding] options:0 error:nil];
            require([point[@"focused"] boolValue],"native number must initially be focused");
            NSPoint start=NSMakePoint([point[@"x"] doubleValue],900-[point[@"y"] doubleValue]);
            NSPoint end=[kind isEqualToString:@"knob"]?NSMakePoint(start.x,start.y+30):NSMakePoint(start.x+60,start.y);
            mouse(window,NSEventTypeLeftMouseDown,start);
            require([evaluate(web,@"JSON.stringify(ravetPointerNative.started())") isEqualToString:@"true"],"native drag starts without numeric edit/focus highlight");
            mouse(window,NSEventTypeLeftMouseDragged,end);mouse(window,NSEventTypeLeftMouseUp,end);
            NSString* report=evaluate(web,@"JSON.stringify(ravetPointerNative.finished())");
            std::cout<<kind.UTF8String<<" AppKit pointer drag: "<<report.UTF8String<<'\n';
            NSDictionary* receipt=[NSJSONSerialization JSONObjectWithData:[report dataUsingEncoding:NSUTF8StringEncoding] options:0 error:nil];
            require([receipt[@"pass"] boolValue],"native knob/fader drag completion");
            require([evaluate(web,@"JSON.stringify(ravetPointerNative.editing())") isEqualToString:@"true"],"native number entry and keyboard focus styling");
        }
        ravet_web_destroy(view);[window orderOut:nil];
        std::cout<<"PASS WebKit semantic contracts and AppKit knob/fader pointer drags (no devices)\n";return 0;
    }catch(const std::exception& error){std::cerr<<error.what()<<'\n';return 1;} }
}
