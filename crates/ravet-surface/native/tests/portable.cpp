#include "RavetWebView.h"
#include <chrono>
#include <thread>
#include <iostream>
#include <stdexcept>
#include <cstring>
#include <cstdio>
#include <limits>
#ifdef _WIN32
#include <windows.h>
#else
#include <gtk/gtk.h>
#include <gtk/gtkx.h>
#endif
static void require(bool ok,const char* text) { if(!ok) throw std::runtime_error(text); }
static void pump(int milliseconds) {
    auto end=std::chrono::steady_clock::now()+std::chrono::milliseconds(milliseconds);
    while(std::chrono::steady_clock::now()<end) {
#ifdef _WIN32
        MSG msg; while(PeekMessageW(&msg,nullptr,0,0,PM_REMOVE)) { TranslateMessage(&msg); DispatchMessageW(&msg); }
#else
        ravet_web_pump();
#endif
        std::this_thread::sleep_for(std::chrono::milliseconds(2));
    }
}
struct Probe { int snapshots=0,messages=0,updates=0,ready=0,local=0,width=0,height=0; const char* json="{broken"; void* view=nullptr; bool closeInSnapshot=false; };
static void message(void* context,const char* text) {
    auto& p=*static_cast<Probe*>(context); ++p.messages;
    if(strstr(text,"updated")) {
        ++p.updates;
        if(auto* width=strstr(text,"\"width\":")) std::sscanf(width+8,"%d",&p.width);
        if(auto* height=strstr(text,"\"height\":")) std::sscanf(height+9,"%d",&p.height);
    }
    if(strstr(text,"ready")) ++p.ready;
    if(strstr(text,"stillLocal")) ++p.local;
}
static const char* snapshot(void* context) {
    auto& p=*static_cast<Probe*>(context); ++p.snapshots;
    if(p.closeInSnapshot&&p.view) { auto* view=p.view; p.view=nullptr; ravet_web_destroy(view); }
    return p.json;
}
int main(int argc,char** argv) {
    try {
        require(argc==2,"fixture path required");
#ifdef _WIN32
        HWND parent=CreateWindowExW(0,L"STATIC",L"RAVET probe",WS_OVERLAPPEDWINDOW|WS_VISIBLE,0,0,360,180,nullptr,nullptr,GetModuleHandleW(nullptr),nullptr);
        require(parent,"parent HWND"); void* nativeParent=parent;
#else
        require(gtk_init_check(nullptr,nullptr),"GTK X11 display required");
        auto* parent=gtk_window_new(GTK_WINDOW_TOPLEVEL); auto* socket=gtk_socket_new();
        g_signal_connect(socket,"plug-removed",G_CALLBACK(+[](GtkSocket*,gpointer)->gboolean{return TRUE;}),nullptr);
        gtk_container_add(GTK_CONTAINER(parent),socket); gtk_widget_show_all(parent);
        void* nativeParent=reinterpret_cast<void*>(gtk_socket_get_id(GTK_SOCKET(socket)));
#endif
        Probe p;
        require(!ravet_web_create("/missing/ravet.html",320,120,&p,message,snapshot),"missing file accepted");
        require(!ravet_web_create(argv[1],0,120,&p,message,snapshot),"zero width accepted");
        for(int iteration=0;iteration<3;++iteration) {
            p=Probe(); p.view=ravet_web_create(argv[1],320,120,&p,message,snapshot); require(p.view,"editor create");
            require(ravet_web_set_background(p.view,.1,.2,.3),"background");
            require(!ravet_web_set_background(p.view,std::numeric_limits<double>::quiet_NaN(),0,0),"nonfinite colour");
            ravet_web_attach(p.view,nativeParent);
            for(int i=0;i<300&&(!p.ready||!p.local);++i) pump(50);
            require(p.ready==1&&p.local==1,"page loading or navigation restriction failed"); require(p.updates==0,"malformed snapshot delivered");
            p.json="{\"value\":0.5}"; for(int i=0;i<80&&!p.updates;++i) pump(50); require(p.updates==1,"snapshot not delivered"); pump(250); require(p.updates==1,"identical snapshots redelivered");
            require(p.width>=320&&p.height>=120,"web viewport does not fill the requested editor size");
            p.json="{\"value\":0.6}"; for(int i=0;i<80&&p.updates<2;++i) pump(50); require(p.updates==2,"changed snapshot not delivered");
            ravet_web_set_visible(p.view,false); ravet_web_set_visible(p.view,true); ravet_web_detach(p.view); ravet_web_attach(p.view,nativeParent); pump(100);
            if(iteration==2) { p.closeInSnapshot=true; for(int i=0;i<20&&p.view;++i) pump(50); require(!p.view,"callback destruction failed"); }
            else { ravet_web_destroy(p.view); p.view=nullptr; }
            auto snapshots=p.snapshots,messages=p.messages; pump(200); require(p.snapshots==snapshots&&p.messages==messages,"callbacks after destruction");
        }
#ifdef _WIN32
        DestroyWindow(parent);
#else
        gtk_widget_destroy(parent);
#endif
        std::cout<<"Native create/message/snapshot/coalescing/navigation/reattach/destroy checks passed\n"; return 0;
    } catch(const std::exception& e) { std::cerr<<e.what()<<'\n'; return 1; }
}
