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
int main(int argc,char** argv) {
    @autoreleasepool { try {
        require(argc==2,"listbox fixture path required");[NSApplication sharedApplication];
        void* view=ravet_web_create(argv[1],1100,900,nullptr,nullptr,nullptr);
        require(view,"create listbox WebKit fixture");
        auto* container=(__bridge NSView*)view;
        auto* web=static_cast<WKWebView*>(container.subviews.firstObject);
        __block bool finished=false,passed=false;__block NSString* evidence=nil;
        for(int attempt=0;attempt<200&&!finished;attempt++) {
            __block bool evaluated=false;
            [web evaluateJavaScript:@"window.ravetListResults ? JSON.stringify(window.ravetListResults) : null" completionHandler:^(id result,NSError* error) {
                if(!error&&[result isKindOfClass:[NSString class]]) {
                    evidence=result;NSDictionary* report=[NSJSONSerialization JSONObjectWithData:[evidence dataUsingEncoding:NSUTF8StringEncoding] options:0 error:nil];
                    finished=true;passed=[report[@"pass"] boolValue];
                }
                evaluated=true;
            }];
            for(int wait=0;wait<20&&!evaluated;wait++)pump();pump();
        }
        if(evidence)std::cout<<evidence.UTF8String<<'\n';
        ravet_web_destroy(view);
        require(finished&&passed,"WebKit listbox contracts failed");
        std::cout<<"PASS hidden WebKit listbox contract fixture (no devices)\n";return 0;
    }catch(const std::exception& error){std::cerr<<error.what()<<'\n';return 1;} }
}
