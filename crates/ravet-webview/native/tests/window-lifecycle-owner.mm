#import <Cocoa/Cocoa.h>
#import <WebKit/WebKit.h>
#include <cstring>
@interface RavetFixtureVeto : NSObject<NSWindowDelegate>
@end
@implementation RavetFixtureVeto
-(BOOL)windowShouldClose:(NSWindow*)window { (void)window;return NO; }
@end
// Exact own-window fixture owner. Never linked into the framework library.
struct Fixture {
    NSWindow* window;
    id observer;
    RavetFixtureVeto* veto;
    id<NSWindowDelegate> original;
};
extern "C" void* ravet_lifecycle_fixture_create(const char* title) {
    if(!title||![NSThread isMainThread])return nullptr;
    size_t length=strnlen(title,257);if(!length||length>256)return nullptr;
    NSString* name=[NSString stringWithUTF8String:title];if(!name)return nullptr;
    NSArray<NSWindow*>* windows=NSApp.windows;if(windows.count>8)return nullptr;
    NSWindow* match=nil;
    for(NSWindow* window in windows)if([window.title isEqual:name]){if(match)return nullptr;match=window;}
    return match?new Fixture{match,nil,nil,nil}:nullptr;
}
extern "C" bool ravet_lifecycle_observe(void* raw,void* context,void(*callback)(void*)) {
    if(!raw||!context||!callback||![NSThread isMainThread])return false;
    auto* fixture=static_cast<Fixture*>(raw);if(fixture->observer)return false;
    fixture->observer=[NSNotificationCenter.defaultCenter addObserverForName:NSWindowWillCloseNotification object:fixture->window queue:nil usingBlock:^(NSNotification* note){(void)note;callback(context);}];
    return fixture->observer!=nil;
}
extern "C" void ravet_lifecycle_stop_observing(void* raw) {
    if(!raw)return;auto* fixture=static_cast<Fixture*>(raw);
    if(fixture->observer)[NSNotificationCenter.defaultCenter removeObserver:fixture->observer];
    fixture->observer=nil;
}
extern "C" bool ravet_lifecycle_veto(void* raw,bool enabled) {
    if(!raw||![NSThread isMainThread])return false;
    auto* fixture=static_cast<Fixture*>(raw);
    if(enabled){if(fixture->veto)return false;fixture->original=fixture->window.delegate;fixture->veto=[RavetFixtureVeto new];fixture->window.delegate=fixture->veto;}
    else {if(fixture->veto&&fixture->window.delegate==fixture->veto)fixture->window.delegate=fixture->original;fixture->original=nil;fixture->veto=nil;}
    return true;
}
extern "C" bool ravet_lifecycle_action(void* raw,unsigned action) {
    if(!raw||![NSThread isMainThread])return false;
    auto* fixture=static_cast<Fixture*>(raw);
    if(action==1){[fixture->window performClose:nil];return true;}
    if(action==2){[NSNotificationCenter.defaultCenter postNotificationName:NSWindowWillCloseNotification object:fixture->window];return true;}
    if(action==3){[fixture->window orderOut:nil];return true;}
    if(action==5){fixture->window.contentView=nil;return true;}
    if(action==4){NSArray<NSTitlebarAccessoryViewController*>* accessories=fixture->window.titlebarAccessoryViewControllers;if(accessories.count>8)return false;for(NSTitlebarAccessoryViewController* accessory in accessories)if([accessory.view isKindOfClass:NSButton.class]){[(NSButton*)accessory.view performClick:nil];return true;}}
    return false;
}
extern "C" long long ravet_lifecycle_children(void* raw) {
    if(!raw||![NSThread isMainThread])return -1;
    NSUInteger count=static_cast<Fixture*>(raw)->window.contentView.subviews.count;
    return count<=8?(long long)count:-1;
}
extern "C" int ravet_lifecycle_child_hidden(void* raw) {
    if(!raw||![NSThread isMainThread])return -1;
    NSArray<NSView*>* children=static_cast<Fixture*>(raw)->window.contentView.subviews;
    return children.count==1?(children.firstObject.hidden?1:0):-1;
}
static WKWebView* findWeb(NSView* view,unsigned depth,unsigned& visited) {
    if(!view||depth>32||visited>=256)return nil;
    ++visited;
    if([view isKindOfClass:WKWebView.class])return (WKWebView*)view;
    NSArray<NSView*>* children=view.subviews;if(children.count>8)return nil;
    for(NSView* child in children)if(auto* found=findWeb(child,depth+1,visited))return found;
    return nil;
}
extern "C" bool ravet_lifecycle_query(void* raw,const char* script) {
    if(!raw||!script||![NSThread isMainThread])return false;
    size_t length=strnlen(script,4097);if(!length||length>4096)return false;
    auto* fixture=static_cast<Fixture*>(raw);unsigned visited=0;WKWebView* web=findWeb(fixture->window.contentView,0,visited);if(!web)return false;
    __block bool done=false,ok=false;
    [web evaluateJavaScript:[NSString stringWithUTF8String:script] completionHandler:^(id result,NSError* error){ok=!error&&[result respondsToSelector:@selector(boolValue)]&&[result boolValue];done=true;}];
    for(unsigned i=0;i<60&&!done;i++)[[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode beforeDate:[NSDate dateWithTimeIntervalSinceNow:.05]];
    return done&&ok;
}
extern "C" void ravet_lifecycle_fixture_destroy(void* raw) {
    if(!raw)return;
    ravet_lifecycle_stop_observing(raw);ravet_lifecycle_veto(raw,false);
    delete static_cast<Fixture*>(raw);
}
