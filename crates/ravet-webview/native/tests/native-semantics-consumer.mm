#import <Cocoa/Cocoa.h>
#import <WebKit/WebKit.h>
#include <cmath>
#include <cstring>
static NSWindow* ownedWindow() {
    for(NSWindow* window in NSApp.windows)if([window.title isEqual:@"RAVET native semantic companion"])return window;
    return nil;
}
static NSView* content() { return ownedWindow().contentView.subviews.firstObject; }
static WKWebView* web() {
    for(NSView* view in content().subviews)if([view isKindOfClass:WKWebView.class])return (WKWebView*)view;
    return nil;
}
extern "C" bool w223_order_own(bool visible) {
    NSWindow* window=ownedWindow();if(!window)return false;
    if(visible)[window orderFront:nil];else[window orderOut:nil];return window.visible==visible;
}
extern "C" bool w223_action(uint64_t nodeID,uint32_t kind) {
    for(id element in [content() accessibilityChildren]) {
        NSString* identifier=[element accessibilityIdentifier];
        NSString* suffix=[NSString stringWithFormat:@"-%llu",(unsigned long long)nodeID];
        if(![identifier hasSuffix:suffix])continue;
        return kind==0?[element accessibilityPerformPress]:(kind==1?[element accessibilityPerformIncrement]:[element accessibilityPerformDecrement]);
    }
    return false;
}
extern "C" double w223_gain() {
    for(id element in [content() accessibilityChildren])if([[element accessibilityLabel] isEqual:@"Gain"])
        return [[element accessibilityValue] doubleValue];
    return NAN;
}
extern "C" bool w223_native_profile(double gain) {
    NSView* root=content();NSArray* children=[root accessibilityChildren];
    if(children.count!=3||![[root accessibilityRole] isEqual:NSAccessibilityGroupRole])return false;
    return [[children[0] accessibilityLabel] isEqual:@"Output status"] &&
        [[children[1] accessibilityValue] doubleValue]==gain &&
        [[children[2] accessibilityLabel] isEqual:@"Reset gain"] &&
        [children[1] accessibilityParent]==root && [children[2] accessibilityParent]==root;
}
extern "C" bool w223_settings_default() {
    for(NSTitlebarAccessoryViewController* controller in ownedWindow().titlebarAccessoryViewControllers)
        if([controller.view isKindOfClass:NSButton.class]&&[((NSButton*)controller.view).title isEqual:@"Audio / MIDI…"])return true;
    return false;
}
extern "C" bool w223_native_key_up() {
    WKWebView* view=web();NSWindow* window=ownedWindow();if(!view||!window)return false;
    NSEvent* event=[NSEvent keyEventWithType:NSEventTypeKeyDown location:NSZeroPoint modifierFlags:0
        timestamp:NSProcessInfo.processInfo.systemUptime windowNumber:window.windowNumber context:nil
        characters:@"\uf700" charactersIgnoringModifiers:@"\uf700" isARepeat:NO keyCode:126];
    [view keyDown:event];return true;
}
static bool lastDone=false,lastPassed=false;
static NSString* lastReport=nil;
extern "C" void w223_report_begin(double gain,uint64_t revision) {
    lastDone=false;lastPassed=false;
    NSString* script=[NSString stringWithFormat:@"(()=>{const r=ravetCompanionReport();return r.gain===%.17g&&r.revision===%llu&&r.painted==='%.2f';})()",gain,(unsigned long long)revision,gain];
    [web() evaluateJavaScript:script completionHandler:^(id result,NSError* error){lastPassed=!error&&[result boolValue];lastDone=true;}];
}
extern "C" int w223_report_done() { return !lastDone?0:(lastPassed?1:-1); }
extern "C" void w223_capture_receipt() {
    lastReport=nil;
    [web() evaluateJavaScript:@"JSON.stringify(ravetCompanionReport())" completionHandler:^(id result,NSError* error){
        if(!error&&[result isKindOfClass:NSString.class]&&[(NSString*)result length]<=8192)lastReport=result;
    }];
}
extern "C" const char* w223_receipt() { return lastReport.UTF8String; }
extern "C" bool w223_frame_profile(const double* rectangles,size_t count) {
    if(!rectangles||count!=3)return false;
    NSView* root=content();NSArray* children=[root accessibilityChildren];if(children.count!=3)return false;
    for(size_t i=0;i<count;++i) {
        NSRect local=NSMakeRect(rectangles[i*4],rectangles[i*4+1],rectangles[i*4+2],rectangles[i*4+3]);
        NSRect independent=[ownedWindow() convertRectToScreen:[root convertRect:local toView:nil]];
        if(!NSEqualRects([children[i] accessibilityFrame],independent))return false;
    }
    return true;
}
