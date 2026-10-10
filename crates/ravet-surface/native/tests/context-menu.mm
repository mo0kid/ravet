#import <Cocoa/Cocoa.h>
#import <WebKit/WebKit.h>
#include "RavetWebView.h"
#include <iostream>
#include <stdexcept>
static void require(bool ok,const char* message) { if(!ok) throw std::runtime_error(message); }
static void pump() {
    [[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode beforeDate:[NSDate dateWithTimeIntervalSinceNow:.025]];
}
int main(int argc,char** argv) {
    @autoreleasepool { try {
        require(argc==2,"context menu fixture path required"); [NSApplication sharedApplication];
        NSString* html=[NSString stringWithContentsOfFile:[NSString stringWithUTF8String:argv[1]] encoding:NSUTF8StringEncoding error:nil];
        require(html,"read context menu fixture");
        // Both creation paths use the default policy, including fresh/reloaded frames.
        for(int iteration=0;iteration<4;++iteration) {
            const bool inlineHtml=iteration%2;
            void* view=inlineHtml ? ravet_web_create_html(html.UTF8String,320,160,nullptr,nullptr,nullptr)
                : ravet_web_create(argv[1],320,160,nullptr,nullptr,nullptr);
            require(view,"create context menu WebKit fixture");
            auto* container=(__bridge NSView*)view;
            auto* web=static_cast<WKWebView*>(container.subviews.firstObject);
            for(int load=0;load<2;++load) {
                __block bool finished=false,passed=false;
                for(int attempt=0;attempt<200&&!finished;++attempt) {
                    __block bool evaluated=false;
                    [web evaluateJavaScript:@"window.ravetContextMenuResults ? JSON.stringify(window.ravetContextMenuResults) : null" completionHandler:^(id result,NSError* error) {
                        if(!error&&[result isKindOfClass:NSString.class]) {
                            NSDictionary* report=[NSJSONSerialization JSONObjectWithData:[result dataUsingEncoding:NSUTF8StringEncoding] options:0 error:nil];
                            finished=true; passed=[report[@"pass"] boolValue];
                            if(!passed) std::cerr<<[result UTF8String]<<'\n';
                        }
                        evaluated=true;
                    }];
                    for(int wait=0;wait<40&&!evaluated;++wait) pump(); pump();
                }
                require(finished&&passed,"WebKit context menu contracts failed");
                if(load==0) {
                    __block bool cleared=false;
                    [web evaluateJavaScript:@"window.ravetContextMenuResults=null" completionHandler:^(id,NSError* error) {
                        if(error) return;
                        // about:blank reload need not retain an inline body.
                        if(inlineHtml) [web loadHTMLString:html baseURL:nil]; else [web reload];
                        cleared=true;
                    }];
                    for(int wait=0;wait<80&&!cleared;++wait) pump();
                    require(cleared,"clear context menu report before reload");
                }
            }
            ravet_web_destroy(view);
        }
        std::cout<<"PASS WebKit file/inline/reload/frame context-menu suppression and right-click reset propagation (synthetic DOM events)\n";
        return 0;
    } catch(const std::exception& error) { std::cerr<<error.what()<<'\n'; return 1; } }
}
