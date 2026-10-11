#import <Cocoa/Cocoa.h>
#include <cstring>
// Informational/notification probe scoped to one explicitly named own NSWindow.
static NSWindow* ownWindow(const char* title) {
    if(!title||![NSThread isMainThread])return nil;
    size_t length=strnlen(title,257);if(length==0||length>256)return nil;
    NSString* name=[NSString stringWithUTF8String:title];if(!name)return nil;
    NSWindow* match=nil;
    NSArray<NSWindow*>* windows=NSApp.windows;if(windows.count>8)return nil;
    for(NSWindow* window in windows)if([window.title isEqual:name]){if(match)return nil;match=window;}
    return match;
}
extern "C" void* ravet_constraints_observe(const char* title,void* context,void(*callback)(void*)) {
    NSWindow* window=ownWindow(title);if(!window||!context||!callback)return nullptr;
    id token=[NSNotificationCenter.defaultCenter addObserverForName:NSWindowDidResizeNotification object:window queue:nil usingBlock:^(NSNotification* note){(void)note;callback(context);}];
    return (__bridge_retained void*)token;
}
extern "C" void ravet_constraints_stop_observing(void* raw) {
    if(!raw)return;id token=CFBridgingRelease(raw);[NSNotificationCenter.defaultCenter removeObserver:token];
}
extern "C" bool ravet_constraints_user_properties(const char* title,double* minw,double* minh,double* maxw,double* maxh) {
    NSWindow* window=ownWindow(title);if(!window||!minw||!minh||!maxw||!maxh)return false;
    NSSize minimum=window.contentMinSize,maximum=window.contentMaxSize;
    *minw=minimum.width;*minh=minimum.height;*maxw=maximum.width;*maxh=maximum.height;return true;
}
