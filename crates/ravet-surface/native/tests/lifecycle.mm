#import <Cocoa/Cocoa.h>
#import <WebKit/WebKit.h>
#include "RavetWebView.h"
#include <iostream>
#include <stdexcept>
#include <limits>
struct Probe { int snapshots=0,messages=0; };
static void require(bool ok,const char* text) { if(!ok) throw std::runtime_error(text); }
static void pump(double seconds) {
    NSDate* deadline=[NSDate dateWithTimeIntervalSinceNow:seconds];
    while(deadline.timeIntervalSinceNow>0) [[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode beforeDate:deadline];
}
int main(int argc,char** argv) {
    @autoreleasepool { try {
        require(argc==2,"fixture path required"); [NSApplication sharedApplication];
        auto* parent=[[NSView alloc] initWithFrame:NSMakeRect(0,0,320,120)]; Probe probe;
        require(!ravet_web_create("/missing/ravet-editor.html",320,120,&probe,nullptr,nullptr),"missing page accepted");
        for(int iteration=0;iteration<3;++iteration) {
            void* view=ravet_web_create(argv[1],320,120,&probe,
                [](void* context,const char*) { ++static_cast<Probe*>(context)->messages; },
                [](void* context) { ++static_cast<Probe*>(context)->snapshots; return "{\"value\":0.5}"; });
            require(view,"create"); ravet_web_attach(view,(__bridge void*)parent);
            auto* container=(__bridge NSView*)view; require(container.superview==parent,"attach");
            auto* web=static_cast<WKWebView*>(container.subviews.firstObject);
            require(!ravet_web_set_size(view,std::numeric_limits<double>::quiet_NaN(),120),"nonfinite resize accepted");
            require(!ravet_web_set_size(view,0,120) && !ravet_web_set_size(view,16385,120),"invalid resize accepted");
            require(ravet_web_set_size(view,340,140),"native resize rejected");
            require(container.frame.size.width==340 && web.frame.size.width==340 && web.frame.size.height==140,"native child did not resize");
            require(ravet_web_set_size(view,320,120),"native resize restore rejected");
            require(container.isOpaque && container.wantsLayer && web.wantsLayer,"opaque layer backing missing");
            require(ravet_web_set_background(view,.1,.2,.3),"background colour rejected");
            require(!ravet_web_set_background(view,std::numeric_limits<double>::quiet_NaN(),0,0),"nonfinite background accepted");
            require(CGColorGetAlpha(container.layer.backgroundColor)==1,"background not opaque");
            const int before=probe.messages;
            for(int tries=0;tries<80&&probe.messages<before+2;++tries) pump(.05);
            if(probe.messages<before+2) std::cerr<<"messages="<<probe.messages<<" before="<<before<<" snapshots="<<probe.snapshots<<" url="<<(web.URL.absoluteString.UTF8String?:"nil")<<'\n';
            require(probe.messages>=before+2,"page did not receive its snapshot");
            require(probe.snapshots>0,"snapshots not polled");
            __block bool updated=false;
            [web evaluateJavaScript:@"window.lastValue===0.5" completionHandler:^(id result,NSError* error) { updated=!error&&[result boolValue]; }];
            for(int tries=0;tries<20&&!updated;++tries) pump(.05); require(updated,"snapshot not rendered");
            pump(.2); __block bool coalesced=false;
            [web evaluateJavaScript:@"window.updateCount===1" completionHandler:^(id result,NSError* error) { coalesced=!error&&[result boolValue]; }];
            for(int tries=0;tries<20&&!coalesced;++tries) pump(.05); require(coalesced,"unchanged snapshots redelivered");
            [web evaluateJavaScript:@"location.href='https://example.invalid/'" completionHandler:nil]; pump(.1);
            require(web.URL.isFileURL,"external navigation allowed");
            ravet_web_detach(view); require(!container.superview,"detach"); ravet_web_attach(view,(__bridge void*)parent);
            ravet_web_destroy(view); const int snapshots=probe.snapshots,messages=probe.messages; pump(.15);
            require(probe.snapshots==snapshots&&probe.messages==messages,"callback after destruction");
            require(parent.subviews.count==0,"child retained after destruction");
        }
        std::cout<<"WKWebView create/message/snapshot/navigation/detach/destroy checks passed\n"; return 0;
    } catch(const std::exception& error) { std::cerr<<error.what()<<'\n'; return 1; } }
}
