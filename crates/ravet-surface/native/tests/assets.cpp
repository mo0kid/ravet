// Embedded asset origin (ravet_web_create_assets) on every backend, and on
// Linux an editor attached to a plain X11 window that is not an XEmbed socket.
// Compiled as Objective-C++ on macOS.
#include "RavetWebView.h"
#include <chrono>
#include <cstdio>
#include <cstring>
#include <iostream>
#include <stdexcept>
#include <string>
#include <thread>
#include <vector>
#if defined(__APPLE__)
#import <Cocoa/Cocoa.h>
#elif defined(_WIN32)
#include <windows.h>
#else
#include <gtk/gtk.h>
#include <gdk/gdkx.h>
#include <X11/Xlib.h>
#endif

static void require(bool ok,const char* text) { if(!ok) throw std::runtime_error(text); }
static void pump(int milliseconds) {
#if defined(__APPLE__)
    NSDate* deadline=[NSDate dateWithTimeIntervalSinceNow:milliseconds/1000.0];
    while(deadline.timeIntervalSinceNow>0) [[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode beforeDate:deadline];
#else
    auto end=std::chrono::steady_clock::now()+std::chrono::milliseconds(milliseconds);
    while(std::chrono::steady_clock::now()<end) {
#if defined(_WIN32)
        MSG msg; while(PeekMessageW(&msg,nullptr,0,0,PM_REMOVE)) { TranslateMessage(&msg); DispatchMessageW(&msg); }
#else
        ravet_web_pump();
#endif
        std::this_thread::sleep_for(std::chrono::milliseconds(2));
    }
#endif
}

struct Probe { std::vector<std::string> messages; std::string snapshot; };
static void message(void* context,const char* json) { static_cast<Probe*>(context)->messages.emplace_back(json); }
static size_t snapshot(void* context,char* output,size_t capacity) {
    auto& p=*static_cast<Probe*>(context); if(p.snapshot.empty()||p.snapshot.size()>=capacity) return 0;
    memcpy(output,p.snapshot.data(),p.snapshot.size()); return p.snapshot.size();
}
static bool seen(const Probe& p,const char* text) { for(auto& m:p.messages) if(m.find(text)!=std::string::npos) return true; return false; }
static void waitFor(Probe& p,const char* text,const char* failure) {
    for(int i=0;i<300&&!seen(p,text);++i) pump(50);
    if(!seen(p,text)) { for(auto& m:p.messages) std::cerr<<"message: "<<m<<'\n'; }
    require(seen(p,text),failure);
}

// A page whose script, stylesheet and image come from assets. The script is
// larger than the 64 KiB message/snapshot limit (and 1 MiB inline pages).
static const char* page=
    "<!doctype html><html><head><meta charset='utf-8'><link rel='stylesheet' href='css/style.css'>"
    "<script src='app.js'></script></head><body><img id='dot' src='img/dot.svg' width='8' height='8'></body></html>";
static const char* svg="<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'><rect width='8' height='8' fill='red'/></svg>";
static const char* css="body{margin:0;background:rgb(1,2,3)}";

int main() {
    try {
#if defined(__APPLE__)
        [NSApplication sharedApplication];
        NSView* parentView=[[NSView alloc] initWithFrame:NSMakeRect(0,0,320,180)]; void* parent=(__bridge void*)parentView;
#elif defined(_WIN32)
        HWND window=CreateWindowExW(0,L"STATIC",L"RAVET assets",WS_OVERLAPPEDWINDOW|WS_VISIBLE,0,0,360,240,nullptr,nullptr,GetModuleHandleW(nullptr),nullptr);
        require(window,"parent HWND"); void* parent=window;
#else
        gdk_set_allowed_backends("x11");
        require(gtk_init_check(nullptr,nullptr)&&GDK_IS_X11_DISPLAY(gdk_display_get_default()),"GTK X11 display required");
        // A plain X11 child window, not an XEmbed socket: nothing will map the
        // plug or send it XEmbed messages, as in hosts such as REAPER.
        auto* top=gtk_window_new(GTK_WINDOW_TOPLEVEL); gtk_window_set_default_size(GTK_WINDOW(top),360,240);
        gtk_widget_show_all(top); pump(200);
        Display* display=gdk_x11_display_get_xdisplay(gdk_display_get_default());
        Window frame=XCreateSimpleWindow(display,gdk_x11_window_get_xid(gtk_widget_get_window(top)),0,0,320,180,0,0,0);
        XMapWindow(display,frame); XSync(display,False); void* parent=reinterpret_cast<void*>(uintptr_t(frame));
#endif
        // Invalid asset sets are refused before any view exists.
        Probe p;
        const char tiny[]="x";
        RavetWebAsset bad[]={{"../escape.js","text/javascript",tiny,1}};
        require(!ravet_web_create_assets(page,bad,1,320,180,&p,message,snapshot),"parent-relative path accepted");
        RavetWebAsset reserved[]={{"index.html","text/html",tiny,1}};
        require(!ravet_web_create_assets(page,reserved,1,320,180,&p,message,snapshot),"reserved path accepted");
        RavetWebAsset twice[]={{"a.js","text/javascript",tiny,1},{"a.js","text/javascript",tiny,1}};
        require(!ravet_web_create_assets(page,twice,2,320,180,&p,message,snapshot),"duplicate path accepted");

        // 1.5 MiB of script, delivered without chunking.
        std::string script="const blob='";
        script.append(1536*1024,'a');
        script+="';\n"
            "const post=m=>window.webkit.messageHandlers.ravet.postMessage(m);\n"
            "window.ravetUpdate=x=>post({type:'update',value:x.value});\n"
            "addEventListener('DOMContentLoaded',()=>{\n"
            " post({type:'loaded',bytes:blob.length,origin:location.origin,\n"
            "   background:getComputedStyle(document.body).backgroundColor});\n"
            " const img=document.getElementById('dot');\n"
            " const done=()=>post({type:'image',width:img.naturalWidth});\n"
            " const check=()=>img.naturalWidth?done():setTimeout(check,20); check();\n"
            " fetch('missing.txt').then(r=>post({type:'missing',status:r.status})).catch(()=>post({type:'missing',status:'error'}));\n"
            " fetch('img/dot.svg').then(r=>r.text()).then(t=>post({type:'fetched',length:t.length}));\n"
            "});\n";
        RavetWebAsset assets[]={
            {"app.js","text/javascript; charset=utf-8",script.data(),script.size()},
            {"css/style.css","text/css",css,strlen(css)},
            {"img/dot.svg","image/svg+xml",svg,strlen(svg)},
        };
        for(int iteration=0;iteration<2;++iteration) {
            p=Probe();
            void* view=ravet_web_create_assets(page,assets,3,320,180,&p,message,snapshot);
            require(view,"create with assets");
            ravet_web_attach(view,parent);
            waitFor(p,"\"bytes\":1572864","large script asset not loaded");
            require(seen(p,"rgb(1, 2, 3)"),"stylesheet asset not applied");
            waitFor(p,"\"width\":8","image asset not loaded");
            waitFor(p,"\"status\":404","unknown asset not answered with 404");
            char length[64]; snprintf(length,sizeof length,"\"length\":%zu",strlen(svg));
            waitFor(p,length,"fetch of an asset failed");
            p.snapshot="{\"value\":0.25}";
            waitFor(p,"\"value\":0.25","snapshot not delivered to an asset page");
#if !defined(__APPLE__) && !defined(_WIN32)
            // The plug is mapped (viewable) inside the plain X11 parent.
            Window root=0,up=0,*children=nullptr; unsigned count=0;
            require(XQueryTree(display,frame,&root,&up,&children,&count)&&count==1,"plug not reparented into the X11 parent");
            Window plug=children[0]; XFree(children);
            XWindowAttributes attributes{}; XGetWindowAttributes(display,plug,&attributes);
            require(attributes.map_state==IsViewable,"plug not mapped inside a non-XEmbed parent");
            require(attributes.width==320&&attributes.height==180,"plug does not have the editor size");
            require(ravet_web_set_size(view,400,200),"resize"); pump(300);
            XGetWindowAttributes(display,plug,&attributes);
            require(attributes.width==400&&attributes.height==200,"plug did not follow an editor resize");
#endif
            for(auto& m:p.messages) if(m.find("\"loaded\"")!=std::string::npos) std::cout<<"loaded: "<<m.substr(0,160)<<'\n';
            ravet_web_destroy(view); auto before=p.messages.size(); pump(200);
            require(p.messages.size()==before,"callback after destruction");
        }
#if defined(_WIN32)
        DestroyWindow(window);
#elif !defined(__APPLE__)
        XDestroyWindow(display,frame); gtk_widget_destroy(top);
#endif
        std::cout<<"Asset origin: large script/style/image/fetch/404/snapshot checks passed"
#if !defined(__APPLE__) && !defined(_WIN32)
            " (plain X11 parent: plug mapped)"
#endif
            "\n";
        return 0;
    } catch(const std::exception& e) { std::cerr<<e.what()<<'\n'; return 1; }
}
