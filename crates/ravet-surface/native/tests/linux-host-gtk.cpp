#include "RavetWebView.h"
#include <gdk/gdk.h>
#include <dlfcn.h>
#include <cstdlib>
#include <iostream>
#include <stdexcept>

static void require(bool ok,const char* message) { if(!ok) throw std::runtime_error(message); }
int main(int argc,char** argv) {
    try {
        require(argc==3,"host shim path and supported/unsupported mode required");
        const bool supported=std::string(argv[2])=="supported";
        if(supported) setenv("RAVET_TEST_HOST_GTK","1",1);
        else unsetenv("RAVET_TEST_HOST_GTK");
        require(gdk_init_check(nullptr,nullptr),"GDK display required");
        // Load locally: host APIs are not necessarily in RTLD_DEFAULT.
        void* shim=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); require(shim,"load host shim");
        auto install=reinterpret_cast<void(*)()>(dlsym(shim,"host_install_events"));
        auto count=reinterpret_cast<int(*)()>(dlsym(shim,"host_event_count"));
        require(install&&count,"host shim exports"); install();
        GdkWindowAttr attrs{}; attrs.width=100; attrs.height=100;
        attrs.window_type=GDK_WINDOW_TOPLEVEL; attrs.wclass=GDK_INPUT_OUTPUT;
        auto* window=gdk_window_new(nullptr,&attrs,0); require(window,"host GDK window");
        auto dispatch=[&] {
            auto* event=gdk_event_new(GDK_DELETE);
            event->any.window=GDK_WINDOW(g_object_ref(window));
            gdk_event_put(event); gdk_event_free(event);
            ravet_web_pump();
        };
        dispatch(); require(count()==1,"initial host event routing");
        void* view=ravet_web_create_html("<html><body>test</body></html>",100,100,nullptr,nullptr,nullptr);
        require((view!=nullptr)==supported,"host GTK capability must govern creation");
        dispatch(); require(count()==2,"GTK initialization replaced host event routing");
        if(view) ravet_web_destroy(view);
        dispatch(); require(count()==3,"host event routing changed after destruction");
        gdk_window_destroy(window); g_object_unref(window);
        // The GDK handler is process-owned; keep its provider loaded until exit.
        std::cout<<"Host event routing preserved; "<<(supported?"cooperative GTK creation":"unsupported GTK creation rejected")<<"\n";
        return 0;
    } catch(const std::exception& e) { std::cerr<<e.what()<<'\n'; return 1; }
}
