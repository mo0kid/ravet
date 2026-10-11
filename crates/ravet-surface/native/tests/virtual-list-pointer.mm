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
    if(failure)std::cerr<<failure.localizedDescription.UTF8String<<" script "<<script.UTF8String<<'\n';require(done&&!failure,"pointer probe JavaScript failed");
    return [value isKindOfClass:[NSString class]]?value:nil;
}
static void mouse(NSWindow* window,NSEventType type,NSPoint point,NSEventModifierFlags flags=0) {
    [NSApp sendEvent:[NSEvent mouseEventWithType:type location:point modifierFlags:flags
        timestamp:NSProcessInfo.processInfo.systemUptime windowNumber:window.windowNumber
        context:nil eventNumber:1 clickCount:1 pressure:type==NSEventTypeLeftMouseUp?0:1]];
    for(int i=0;i<5;i++)pump();
}
int main(int argc,char** argv) {
 @autoreleasepool { try {
 require(argc==2,"pointer fixture path required");[NSApplication sharedApplication];
 void* view=ravet_web_create(argv[1],1100,900,nullptr,nullptr,nullptr);require(view,"create pointer fixture");
 auto* container=(__bridge NSView*)view;auto* web=static_cast<WKWebView*>(container.subviews.firstObject);
 bool ready=false;for(int i=0;i<200&&!ready;i++){ready=[evaluate(web,@"window.ravetVirtualTrusted ? 'ready' : null") isEqualToString:@"ready"];if(!ready)pump();}require(ready,"virtual pointer fixture ready");
 [NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];[NSApp finishLaunching];[NSApp activateIgnoringOtherApps:YES];
 auto* window=[[NSWindow alloc] initWithContentRect:NSMakeRect(200,200,1100,900) styleMask:NSWindowStyleMaskTitled backing:NSBackingStoreBuffered defer:NO];
 window.title=@"RAVET virtual-list pointer proof";ravet_web_attach(view,(__bridge void*)window.contentView);[window makeKeyAndOrderFront:nil];for(int i=0;i<10;i++)pump();
 evaluate(web,@"JSON.stringify(ravetVirtualTrusted.setup())");
 NSString* setup=evaluate(web,@"(()=>{const row=ravetVirtualTrusted.fixture.control.rows.get(1),b=row.getBoundingClientRect();return JSON.stringify({x:b.x+b.width/2,y:b.y+b.height/2});})()");require(setup,"pointer setup");
 NSDictionary* point=[NSJSONSerialization JSONObjectWithData:[setup dataUsingEncoding:NSUTF8StringEncoding] options:0 error:nil];
 NSPoint position=NSMakePoint([point[@"x"] doubleValue],900-[point[@"y"] doubleValue]);
 mouse(window,NSEventTypeLeftMouseDown,position);
 NSString* pressed=evaluate(web,@"JSON.stringify({index:ravetVirtualTrusted.fixture.control.pressed?.index,up:ravetVirtualTrusted.fixture.control.pressed?.up})");std::cout<<"pressed "<<pressed.UTF8String<<'\n';
 require([pressed containsString:@"\"index\":1"],"trusted native pressed ownership");
 evaluate(web,@"(()=>{const f=ravetVirtualTrusted.fixture;f.root.scrollTop=12000;return JSON.stringify({ready:f.control.refresh(),pin:f.control.rows.has(1)});})()");
 mouse(window,NSEventTypeLeftMouseUp,position);
 NSString* cancelled=evaluate(web,@"JSON.stringify(ravetVirtualTrusted.report())");std::cout<<"cancelled "<<cancelled.UTF8String<<'\n';
 NSDictionary* cancelledReport=[NSJSONSerialization JSONObjectWithData:[cancelled dataUsingEncoding:NSUTF8StringEncoding] options:0 error:nil];require([cancelledReport[@"calls"] intValue]==0,"recycled native click cannot select");
 NSString* next=evaluate(web,@"(()=>{const f=ravetVirtualTrusted.fixture,row=f.control.rows.get(501),b=row.getBoundingClientRect();return JSON.stringify({x:b.x+b.width/2,y:b.y+b.height/2});})()");
 NSDictionary* nextPoint=[NSJSONSerialization JSONObjectWithData:[next dataUsingEncoding:NSUTF8StringEncoding] options:0 error:nil];position=NSMakePoint([nextPoint[@"x"] doubleValue],900-[nextPoint[@"y"] doubleValue]);
 mouse(window,NSEventTypeLeftMouseDown,position);mouse(window,NSEventTypeLeftMouseUp,position);
 NSString* result=evaluate(web,@"JSON.stringify(ravetVirtualTrusted.report())");std::cout<<"accepted "<<result.UTF8String<<'\n';
 NSDictionary* report=[NSJSONSerialization JSONObjectWithData:[result dataUsingEncoding:NSUTF8StringEncoding] options:0 error:nil];
 require([report[@"calls"] intValue]==1&&[report[@"active"] isEqual:@"trusted"]&&[report[@"selected"][@"selectedIndices"] isEqual:@[@501]],"trusted native matching click selects only501 once");
 for(NSString* mode in @[@"prevented",@"alt"]){
 evaluate(web,@"JSON.stringify(ravetVirtualTrusted.setup())");
 evaluate(web,@"(()=>{const f=ravetVirtualTrusted.fixture;f.root.addEventListener('click',e=>e.preventDefault(),{capture:true,once:true});return 'ready';})()");
 NSString* rawPoint=evaluate(web,@"(()=>{const b=ravetVirtualTrusted.fixture.control.rows.get(1).getBoundingClientRect();return JSON.stringify({x:b.x+b.width/2,y:b.y+b.height/2});})()");
 NSDictionary* pair=[NSJSONSerialization JSONObjectWithData:[rawPoint dataUsingEncoding:NSUTF8StringEncoding] options:0 error:nil];NSPoint p=NSMakePoint([pair[@"x"] doubleValue],900-[pair[@"y"] doubleValue]);
 mouse(window,NSEventTypeLeftMouseDown,p);mouse(window,NSEventTypeLeftMouseUp,p);
 NSString* old=evaluate(web,@"JSON.stringify({calls:ravetVirtualTrusted.calls(),index:ravetVirtualTrusted.fixture.control.pressed?.index,up:ravetVirtualTrusted.fixture.control.pressed?.up})");std::cout<<mode.UTF8String<<" old "<<old.UTF8String<<'\n';
 require([old containsString:@"\"index\":1"]&&[old containsString:@"\"calls\":0"],"native abandoned prior press retained");
 if([mode isEqual:@"prevented"])evaluate(web,@"(()=>{ravetVirtualTrusted.fixture.root.addEventListener('pointerdown',e=>e.preventDefault(),{capture:true,once:true});return 'ready';})()");
 mouse(window,NSEventTypeLeftMouseDown,p,[mode isEqual:@"alt"]?NSEventModifierFlagOption:0);mouse(window,NSEventTypeLeftMouseUp,p,0);
 NSString* fresh=evaluate(web,@"JSON.stringify({report:ravetVirtualTrusted.report(),pressed:ravetVirtualTrusted.fixture.control.pressed!==null})");std::cout<<mode.UTF8String<<" fresh "<<fresh.UTF8String<<'\n';
 NSDictionary* receipt=[NSJSONSerialization JSONObjectWithData:[fresh dataUsingEncoding:NSUTF8StringEncoding] options:0 error:nil];require([receipt[@"report"][@"calls"] intValue]==0&&![receipt[@"pressed"] boolValue]&&[receipt[@"report"][@"selected"][@"selectedIndices"] count]==0,"new rejected native press cannot reuse old token");if([mode isEqual:@"alt"]){NSArray* events=receipt[@"report"][@"events"];require([events[events.count-3][@"alt"] boolValue]&&![events[events.count-2][@"alt"] boolValue]&&![events.lastObject[@"alt"] boolValue],"actual native Alt down then released modifier up/click");}
 }
 evaluate(web,@"JSON.stringify(ravetVirtualTrusted.setup())");
 evaluate(web,@"(()=>{const f=ravetVirtualTrusted.fixture;f.control.setSelected(Array.from({length:256},(_,i)=>i));f.root.scrollTop=12000;f.control.refresh();window.capacityBefore={selected:JSON.stringify(f.control.selection()),active:f.control.activeIndex(),anchor:f.control.anchor};window.nativeCapacityToken=null;f.root.addEventListener('pointerdown',()=>window.nativeCapacityToken=f.control.pressed);return 'ready';})()");
 NSString* capacityPoint=evaluate(web,@"(()=>{const b=ravetVirtualTrusted.fixture.control.rows.get(501).getBoundingClientRect();return JSON.stringify({x:b.x+b.width/2,y:b.y+b.height/2});})()");
 NSDictionary* capacityPair=[NSJSONSerialization JSONObjectWithData:[capacityPoint dataUsingEncoding:NSUTF8StringEncoding] options:0 error:nil];position=NSMakePoint([capacityPair[@"x"] doubleValue],900-[capacityPair[@"y"] doubleValue]);
 mouse(window,NSEventTypeLeftMouseDown,position,NSEventModifierFlagCommand);mouse(window,NSEventTypeLeftMouseUp,position,NSEventModifierFlagCommand);
 NSString* capacity=evaluate(web,@"(()=>{const f=ravetVirtualTrusted.fixture,c=f.control;return JSON.stringify({report:ravetVirtualTrusted.report(),same:JSON.stringify(c.selection())===capacityBefore.selected&&c.activeIndex()===capacityBefore.active&&c.anchor===capacityBefore.anchor,sameToken:c.pressed===nativeCapacityToken,token:c.pressed&&{index:c.pressed.index,up:c.pressed.up,consumed:c.pressed.consumed,pinned:c.rows.get(c.pressed.index)===c.pressed.row}});})()");std::cout<<"capacity "<<capacity.UTF8String<<'\n';
 NSDictionary* capacityReceipt=[NSJSONSerialization JSONObjectWithData:[capacity dataUsingEncoding:NSUTF8StringEncoding] options:0 error:nil];require([capacityReceipt[@"same"] boolValue]&&[capacityReceipt[@"sameToken"] boolValue]&&[capacityReceipt[@"token"][@"consumed"] boolValue]&&[capacityReceipt[@"token"][@"pinned"] boolValue]&&[capacityReceipt[@"report"][@"calls"] intValue]==0&&[capacityReceipt[@"report"][@"error"] isEqual:@"Capacity"],"native Capacity consumes click but retains model and identity pin");
 evaluate(web,@"JSON.stringify(ravetVirtualTrusted.fixture.drop())");
 ravet_web_destroy(view);[window orderOut:nil];
 return 0;
 }catch(const std::exception& error){std::cerr<<error.what()<<'\n';return 1;} }
}
