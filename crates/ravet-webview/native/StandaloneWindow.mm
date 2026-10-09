#import <Cocoa/Cocoa.h>
#import <WebKit/WebKit.h>
#include <algorithm>
static WKWebView* findWeb(NSView* node,unsigned depth);
@interface RavetSettingsAction : NSObject
@property(nonatomic,weak) NSView* content;
-(void)open:(id)sender;
@end
@implementation RavetSettingsAction
-(void)open:(id)sender {
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
struct Window { NSWindow* window; NSButton* settings; RavetSettingsAction* action; RavetSettingsStore* store; NSSize aspect; };
extern "C" void* ravet_standalone_create(bool hidden,bool settings,const char* title,double width,double height,bool lockAspect) {
    [NSApplication sharedApplication];
    [NSApp setActivationPolicy:hidden?NSApplicationActivationPolicyProhibited:NSApplicationActivationPolicyRegular];
    NSWindow* window=[[NSWindow alloc] initWithContentRect:NSMakeRect(0,0,width,height)
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
        action=[[RavetSettingsAction alloc] init];action.content=window.contentView;
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
    return new Window{window,button,action,store,lockAspect?NSMakeSize(width,height):NSZeroSize};
}
extern "C" void* ravet_standalone_view(void* handle) {return (__bridge void*)static_cast<Window*>(handle)->window.contentView;}
extern "C" bool ravet_standalone_pump(void* handle,double seconds) {
    @autoreleasepool {
        NSDate* end=[NSDate dateWithTimeIntervalSinceNow:seconds];
        do {
            NSEvent* event;
            while((event=[NSApp nextEventMatchingMask:NSEventMaskAny untilDate:[NSDate distantPast] inMode:NSDefaultRunLoopMode dequeue:YES])) [NSApp sendEvent:event];
            [[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode beforeDate:end];
        } while(end.timeIntervalSinceNow>0);
        return static_cast<Window*>(handle)->window.isVisible;
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
    NSView* content=host->window.contentView;
    for(NSView* child in content.subviews)if(findWeb(child,0)) {
        child.autoresizingMask=NSViewWidthSizable|NSViewHeightSizable;
        child.frame=content.bounds;
        break;
    }
    if(host->store){
        WKWebView* web=findWeb(content,0);
        [web.configuration.userContentController addScriptMessageHandler:host->store name:@"ravetSettings"];
        NSString* seed=[NSString stringWithFormat:@"window.ravetSavedSettings=%@;if(typeof window.ravetRestoreSettings==='function')window.ravetRestoreSettings(window.ravetSavedSettings);",host->store.saved?:@"null"];
        [web.configuration.userContentController addUserScript:[[WKUserScript alloc] initWithSource:seed injectionTime:WKUserScriptInjectionTimeAtDocumentStart forMainFrameOnly:YES]];
        [web evaluateJavaScript:seed completionHandler:nil];
    }
}
extern "C" bool ravet_standalone_eval(void* handle,const char* script) {
    WKWebView* web=findWeb(static_cast<Window*>(handle)->window.contentView,0);if(!web)return false;
    __block bool done=false,ok=false;
    [web evaluateJavaScript:[NSString stringWithUTF8String:script] completionHandler:^(id result,NSError* error){ok=!error&&[result respondsToSelector:@selector(boolValue)]&&[result boolValue];done=true;}];
    for(int i=0;i<60&&!done;++i)ravet_standalone_pump(handle,.05);return done&&ok;
}
extern "C" bool ravet_standalone_snapshot(void* handle,const char* path) {
    WKWebView* web=findWeb(static_cast<Window*>(handle)->window.contentView,0);if(!web)return false;
    NSString* destination=[NSString stringWithUTF8String:path]; __block bool done=false,ok=false;
    [web takeSnapshotWithConfiguration:nil completionHandler:^(NSImage* image,NSError* error){
        if(image&&!error){NSBitmapImageRep* bitmap=[[NSBitmapImageRep alloc] initWithData:image.TIFFRepresentation];
            NSData* png=[bitmap representationUsingType:NSBitmapImageFileTypePNG properties:@{}];ok=[png writeToFile:destination atomically:YES];}done=true;}];
    for(int i=0;i<60&&!done;++i)ravet_standalone_pump(handle,.05);return done&&ok;
}
extern "C" void ravet_standalone_destroy(void* handle) {auto* host=static_cast<Window*>(handle);[host->window close];delete host;}

extern "C" bool ravet_standalone_open_settings(void* handle) {
    auto* host=static_cast<Window*>(handle);if(!host->settings)return false;
    [host->settings performClick:nil];return true;
}

extern "C" void ravet_standalone_set_size(void* handle,double width,double height) {
    auto* host=static_cast<Window*>(handle);
    if(host->aspect.width>0){
        double scale=std::min(width/host->aspect.width,height/host->aspect.height);
        width=host->aspect.width*scale;height=host->aspect.height*scale;
    }
    [host->window setContentSize:NSMakeSize(width,height)];
}
