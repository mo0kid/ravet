#import <Cocoa/Cocoa.h>
#import <WebKit/WebKit.h>
#include <algorithm>
#include <cmath>
// This owned native class admits close before superclass notification effects.
// No raw host pointer, global subscriber or application delegate is installed.
@interface RavetStandaloneWindow : NSWindow {
    bool _ravetClosed;
    bool _ravetClosing;
}
-(BOOL)ravetIsClosed;
-(BOOL)ravetIsClosing;
@end
@implementation RavetStandaloneWindow
-(BOOL)ravetIsClosed { return _ravetClosed; }
-(BOOL)ravetIsClosing { return _ravetClosing; }
-(void)close {
    if(_ravetClosed||_ravetClosing)return;
    _ravetClosed=true;_ravetClosing=true;
    @try { [super close]; } @finally { _ravetClosing=false; }
}
@end
static WKWebView* findWeb(NSView* node,unsigned depth);
@interface RavetSettingsAction : NSObject
@property(nonatomic,weak) NSView* content;
@property(nonatomic,weak) RavetStandaloneWindow* window;
-(void)open:(id)sender;
@end
@implementation RavetSettingsAction
-(void)open:(id)sender {
    if(!self.window||self.window.ravetIsClosed)return;
    WKWebView* web=findWeb(self.content,0);
    [web evaluateJavaScript:@"typeof window.ravetOpenSettings==='function'&&window.ravetOpenSettings()" completionHandler:nil];
}
@end
@interface RavetSettingsStore : NSObject<WKScriptMessageHandler>
@property(nonatomic,strong) NSUserDefaults* defaults;
@property(nonatomic,copy) NSString* key;
@property(nonatomic,copy) NSString* saved;
@property(nonatomic,strong) NSMutableDictionary* memory;
@end
@implementation RavetSettingsStore
-(void)userContentController:(WKUserContentController*)controller didReceiveScriptMessage:(WKScriptMessage*)message {
    if(![message.body isKindOfClass:NSDictionary.class]||![message.body[@"version"] isEqual:@1])return;
    NSData* data=[NSJSONSerialization dataWithJSONObject:message.body options:0 error:nil];
    if(!data||data.length>8192)return;
    self.saved=[[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
    if(self.defaults)[self.defaults setObject:self.saved forKey:self.key];
    else if(self.memory)self.memory[self.key]=self.saved;
}
@end
struct Window { RavetStandaloneWindow* window; NSButton* settings; RavetSettingsAction* action; RavetSettingsStore* store; NSSize aspect; NSSize originalMin; NSSize originalMax; };
extern "C" void* ravet_standalone_create(bool hidden,bool settings,const char* title,double width,double height,bool lockAspect) {
    [NSApplication sharedApplication];
    [NSApp setActivationPolicy:hidden?NSApplicationActivationPolicyProhibited:NSApplicationActivationPolicyRegular];
    RavetStandaloneWindow* window=[[RavetStandaloneWindow alloc] initWithContentRect:NSMakeRect(0,0,width,height)
        styleMask:NSWindowStyleMaskTitled|NSWindowStyleMaskClosable|NSWindowStyleMaskMiniaturizable|NSWindowStyleMaskResizable
        backing:NSBackingStoreBuffered defer:NO];
    window.releasedWhenClosed=NO; window.title=[NSString stringWithUTF8String:title];
    window.minSize=NSMakeSize(640,560); [window center];
    if(lockAspect){
        window.contentAspectRatio=NSMakeSize(width,height);
        double minimum=std::max(640./width,540./height);
        window.contentMinSize=NSMakeSize(width*minimum,height*minimum);
    }
    NSButton* button=nil;RavetSettingsAction* action=nil;
    if(settings) {
        action=[[RavetSettingsAction alloc] init];action.content=window.contentView;action.window=window;
        button=[NSButton buttonWithTitle:@"Audio / MIDI…" target:action action:@selector(open:)];
        button.bezelStyle=NSBezelStyleRounded;button.font=[NSFont systemFontOfSize:11];
        button.frame=NSMakeRect(0,0,112,24);
        NSTitlebarAccessoryViewController* accessory=[[NSTitlebarAccessoryViewController alloc] init];
        accessory.view=button;accessory.layoutAttribute=NSLayoutAttributeRight;
        [window addTitlebarAccessoryViewController:accessory];
    }
    if(!hidden) {
        NSMenu* menu=[[NSMenu alloc] init]; NSMenuItem* app=[[NSMenuItem alloc] init]; [menu addItem:app];
        NSMenu* submenu=[[NSMenu alloc] init];
        NSMenuItem* quit=[[NSMenuItem alloc] initWithTitle:@"Quit RAVET Examples" action:@selector(performClose:) keyEquivalent:@"q"];
        quit.target=window; [submenu addItem:quit]; app.submenu=submenu; NSApp.mainMenu=menu;
        [NSApp finishLaunching]; [window makeKeyAndOrderFront:nil]; [NSApp activateIgnoringOtherApps:YES];
    }
    RavetSettingsStore* store=nil;
    if(settings){
        store=[RavetSettingsStore new];store.key=window.title;
        if(hidden){
            static NSMutableDictionary* memory=[NSMutableDictionary new];
            store.memory=memory;store.saved=memory[store.key];
        }
        if(!hidden){
            store.defaults=[[NSUserDefaults alloc] initWithSuiteName:@"org.ravet.standalone.settings"];
            NSString* saved=[store.defaults stringForKey:store.key];
            NSData* data=[saved dataUsingEncoding:NSUTF8StringEncoding];
            id value=data&&data.length<=8192?[NSJSONSerialization JSONObjectWithData:data options:0 error:nil]:nil;
            if([value isKindOfClass:NSDictionary.class]&&[value[@"version"] isEqual:@1])store.saved=saved;
        }
    }
    return new Window{window,button,action,store,lockAspect?NSMakeSize(width,height):NSZeroSize,window.contentMinSize,window.contentMaxSize};
}
extern "C" void* ravet_standalone_view(void* handle) {
    if(!handle||![NSThread isMainThread])return nullptr;
    auto* host=static_cast<Window*>(handle);if(host->window.ravetIsClosed)return nullptr;
    return (__bridge void*)host->window.contentView;
}
extern "C" bool ravet_standalone_pump(void* handle,double seconds) {
    @autoreleasepool {
        NSDate* end=[NSDate dateWithTimeIntervalSinceNow:seconds];
        do {
            NSEvent* event;
            while((event=[NSApp nextEventMatchingMask:NSEventMaskAny untilDate:[NSDate distantPast] inMode:NSDefaultRunLoopMode dequeue:YES])) [NSApp sendEvent:event];
            [[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode beforeDate:end];
        } while(end.timeIntervalSinceNow>0);
        auto* host=static_cast<Window*>(handle);return !host->window.ravetIsClosed&&host->window.isVisible;
    }
}
static WKWebView* findWeb(NSView* node,unsigned depth) {
    if(!node||depth>32)return nil;
    if([node isKindOfClass:WKWebView.class])return (WKWebView*)node;
    for(NSView* child in node.subviews)if(auto* found=findWeb(child,depth+1))return found;
    return nil;
}
extern "C" void ravet_standalone_fit_editor(void* handle) {
    auto* host=static_cast<Window*>(handle);
    if(host->window.ravetIsClosed)return;
    NSView* content=host->window.contentView;
    for(NSView* child in content.subviews)if(findWeb(child,0)) {
        child.autoresizingMask=NSViewWidthSizable|NSViewHeightSizable;
        if(host->window.ravetIsClosed)return;
        child.frame=content.bounds;
        break;
    }
    if(host->window.ravetIsClosed)return;
    if(host->store){
        WKWebView* web=findWeb(content,0);
        [web.configuration.userContentController addScriptMessageHandler:host->store name:@"ravetSettings"];
        NSString* seed=[NSString stringWithFormat:@"window.ravetSavedSettings=%@;if(typeof window.ravetRestoreSettings==='function')window.ravetRestoreSettings(window.ravetSavedSettings);",host->store.saved?:@"null"];
        [web.configuration.userContentController addUserScript:[[WKUserScript alloc] initWithSource:seed injectionTime:WKUserScriptInjectionTimeAtDocumentStart forMainFrameOnly:YES]];
        [web evaluateJavaScript:seed completionHandler:nil];
    }
}
extern "C" bool ravet_standalone_eval(void* handle,const char* script) {
    if(!handle||!script||![NSThread isMainThread])return false;
    auto* host=static_cast<Window*>(handle);if(host->window.ravetIsClosed)return false;
    WKWebView* web=findWeb(host->window.contentView,0);if(!web)return false;
    __block bool done=false,ok=false;
    [web evaluateJavaScript:[NSString stringWithUTF8String:script] completionHandler:^(id result,NSError* error){ok=!error&&[result respondsToSelector:@selector(boolValue)]&&[result boolValue];done=true;}];
    for(int i=0;i<60&&!done&&!host->window.ravetIsClosed;++i)ravet_standalone_pump(handle,.05);return !host->window.ravetIsClosed&&done&&ok;
}
extern "C" bool ravet_standalone_snapshot(void* handle,const char* path) {
    if(!handle||!path||![NSThread isMainThread])return false;
    auto* host=static_cast<Window*>(handle);RavetStandaloneWindow* window=host->window;
    if(window.ravetIsClosed)return false;
    WKWebView* web=findWeb(window.contentView,0);if(!web)return false;
    NSString* destination=[NSString stringWithUTF8String:path]; __block bool done=false,ok=false;
    [web takeSnapshotWithConfiguration:nil completionHandler:^(NSImage* image,NSError* error){
        if(image&&!error&&!window.ravetIsClosed){NSBitmapImageRep* bitmap=[[NSBitmapImageRep alloc] initWithData:image.TIFFRepresentation];
            NSData* png=[bitmap representationUsingType:NSBitmapImageFileTypePNG properties:@{}];ok=!window.ravetIsClosed&&[png writeToFile:destination atomically:YES];}done=true;}];
    for(int i=0;i<60&&!done&&!window.ravetIsClosed;++i)ravet_standalone_pump(handle,.05);return !window.ravetIsClosed&&done&&ok;
}
extern "C" void ravet_standalone_destroy(void* handle) {auto* host=static_cast<Window*>(handle);[host->window close];delete host;}

extern "C" bool ravet_standalone_open_settings(void* handle) {
    if(!handle||![NSThread isMainThread])return false;
    auto* host=static_cast<Window*>(handle);if(host->window.ravetIsClosed||!host->settings)return false;
    [host->settings performClick:nil];return true;
}

extern "C" bool ravet_standalone_set_size(void* handle,double width,double height) {
    if(!handle||![NSThread isMainThread])return false;
    auto* host=static_cast<Window*>(handle);if(host->window.ravetIsClosed)return false;
    if(host->aspect.width>0){
        double scale=std::min(width/host->aspect.width,height/host->aspect.height);
        width=host->aspect.width*scale;height=host->aspect.height*scale;
    }
    [host->window setContentSize:NSMakeSize(width,height)];return !host->window.ravetIsClosed;
}

// Read only this owned main-thread window; publish both outputs atomically.
extern "C" bool ravet_standalone_content_size(void* handle,double* width,double* height) {
    if(!handle||!width||!height||![NSThread isMainThread])return false;
    auto* host=static_cast<Window*>(handle);if(host->window.ravetIsClosed)return false;
    NSView* content=host->window.contentView;
    if(!content)return false;
    NSSize size=content.bounds.size;
    if(!std::isfinite(size.width)||!std::isfinite(size.height)||size.width<=0||size.height<=0)return false;
    *width=size.width;*height=size.height;return true;
}

// Scalar preflight is complete before any native property or content mutation.
extern "C" bool ravet_standalone_content_constraints(void* handle,bool enabled,
    double minWidth,double minHeight,double maxWidth,double maxHeight,double width,double height) {
    if(!handle||![NSThread isMainThread])return false;
    auto* host=static_cast<Window*>(handle);if(host->window.ravetIsClosed)return false;
    NSSize minimum=host->originalMin,maximum=host->originalMax;
    if(enabled) {
        if(!std::isfinite(minWidth)||!std::isfinite(minHeight)||!std::isfinite(maxWidth)||!std::isfinite(maxHeight)
            ||minWidth<320||maxWidth>4096||minHeight<240||maxHeight>4096||minWidth>maxWidth||minHeight>maxHeight
            ||!std::isfinite(width)||!std::isfinite(height)||width<minWidth||width>maxWidth||height<minHeight||height>maxHeight)return false;
        if(host->aspect.width>0) {
            double low=std::max(minWidth/host->aspect.width,minHeight/host->aspect.height);
            double high=std::min(maxWidth/host->aspect.width,maxHeight/host->aspect.height);
            if(!std::isfinite(low)||!std::isfinite(high)||low>high)return false;
        }
        minimum=NSMakeSize(minWidth,minHeight);maximum=NSMakeSize(maxWidth,maxHeight);
    }
    // Widen first so transitions never install a temporary min>max pair.
    NSSize previousMaximum=host->window.contentMaxSize;
    host->window.contentMaxSize=NSMakeSize(std::max(previousMaximum.width,maximum.width),std::max(previousMaximum.height,maximum.height));
    if(host->window.ravetIsClosed)return false;
    host->window.contentMinSize=minimum;
    if(host->window.ravetIsClosed)return false;
    host->window.contentMaxSize=maximum;
    if(host->window.ravetIsClosed)return false;
    if(enabled)[host->window setContentSize:NSMakeSize(width,height)];
    return !host->window.ravetIsClosed;
}

extern "C" bool ravet_standalone_state(void* handle,bool* closed,bool* visible,bool* key,bool* miniaturized) {
    if(!handle||!closed||!visible||!key||!miniaturized||![NSThread isMainThread])return false;
    RavetStandaloneWindow* window=static_cast<Window*>(handle)->window;
    if(!window)return false;
    bool c=window.ravetIsClosed,v=window.isVisible,k=window.isKeyWindow,m=window.isMiniaturized;
    *closed=c;*visible=v;*key=k;*miniaturized=m;return true;
}
extern "C" bool ravet_standalone_request_close(void* handle) {
    if(!handle||![NSThread isMainThread])return false;
    RavetStandaloneWindow* window=static_cast<Window*>(handle)->window;
    if(!window||window.ravetIsClosing)return false;
    if(window.ravetIsClosed)return true;
    [window performClose:nil];return window.ravetIsClosed&&!window.ravetIsClosing;
}
