#import <Cocoa/Cocoa.h>
#import <WebKit/WebKit.h>
#include <iostream>
struct Parent { NSWindow* window; };
extern "C" void editor_probe_pump(double seconds) {
    @autoreleasepool {
        NSDate* end=[NSDate dateWithTimeIntervalSinceNow:seconds];
        while(end.timeIntervalSinceNow>0) [[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode beforeDate:end];
    }
}
extern "C" void* editor_probe_create() {
    [NSApplication sharedApplication];
    [NSApp setActivationPolicy:NSApplicationActivationPolicyProhibited];
    NSWindow* window=[[NSWindow alloc] initWithContentRect:NSMakeRect(0,0,480,390) styleMask:NSWindowStyleMaskTitled backing:NSBackingStoreBuffered defer:NO];
    window.releasedWhenClosed=NO;
    return new Parent{window};
}
extern "C" void editor_probe_show(void* parent) { [static_cast<Parent*>(parent)->window orderFront:nil]; }
extern "C" void editor_probe_resize(void* parent,double width,double height) {
    [static_cast<Parent*>(parent)->window setContentSize:NSMakeSize(width,height)];
}
extern "C" void* editor_probe_view(void* parent) { return (__bridge void*)static_cast<Parent*>(parent)->window.contentView; }
extern "C" size_t editor_probe_children(void* parent) { return static_cast<Parent*>(parent)->window.contentView.subviews.count; }
extern "C" bool editor_probe_hidden(void* parent) { return static_cast<Parent*>(parent)->window.contentView.subviews.firstObject.hidden; }
static WKWebView* findWeb(NSView* node,unsigned depth) {
    if(!node || depth>32) return nil;
    if([node isKindOfClass:WKWebView.class]) return (WKWebView*)node;
    for(NSView* child in node.subviews) if(auto* found=findWeb(child,depth+1)) return found;
    return nil;
}
static WKWebView* web(void* parent) {
    return findWeb(static_cast<Parent*>(parent)->window.contentView,0);
}
extern "C" bool editor_probe_eval(void* parent,const char* script) {
    WKWebView* view=web(parent); if(!view) return false;
    __block bool done=false,ok=false;
    [view evaluateJavaScript:[NSString stringWithUTF8String:script] completionHandler:^(id result,NSError* error) { ok=!error&&[result respondsToSelector:@selector(boolValue)]&&[result boolValue];done=true; }];
    for(int i=0;i<60&&!done;++i) editor_probe_pump(.05);
    return done&&ok;
}
extern "C" bool editor_probe_snapshot(void* parent,const char* path) {
    WKWebView* view=web(parent); if(!view) return false;
    NSString* destination=[NSString stringWithUTF8String:path];
    __block bool done=false,ok=false;
    [view takeSnapshotWithConfiguration:nil completionHandler:^(NSImage* image,NSError* error) {
        if(image&&!error) {
            NSBitmapImageRep* bitmap=[[NSBitmapImageRep alloc] initWithData:image.TIFFRepresentation];
            NSData* png=[bitmap representationUsingType:NSBitmapImageFileTypePNG properties:@{}];
            ok=[png writeToFile:destination atomically:YES];
        } else { std::cerr<<"WebKit snapshot failed\n"; }
        done=true;
    }];
    for(int i=0;i<60&&!done;++i) editor_probe_pump(.05);
    return done&&ok;
}
extern "C" void editor_probe_destroy(void* parent) { Parent* p=static_cast<Parent*>(parent); [p->window close];delete p; }
