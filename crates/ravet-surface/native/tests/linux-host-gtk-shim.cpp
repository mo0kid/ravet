#include <gtk/gtk.h>
#include <cstring>
#include <cstdlib>

namespace {
int events=0;
void dispatch(GdkEvent* event,gpointer) {
    if(event->type==GDK_DELETE) { ++events; return; }
    gpointer widget=nullptr;
    if(event->any.window) gdk_window_get_user_data(event->any.window,&widget);
    if(widget&&GTK_IS_WIDGET(widget)) gtk_main_do_event(event);
}
void* extended(const char* key,void*) {
    if(strcmp(key,"LOAD_GTK")||!getenv("RAVET_TEST_HOST_GTK")) return nullptr;
    if(!gtk_init_check(nullptr,nullptr)) return nullptr;
    gdk_event_handler_set(dispatch,nullptr,nullptr);
    return const_cast<char*>("OK");
}
}
// Compatibility shim, not a copy of SWELL. Emulates its GTK loader contract.
extern "C" void* SWELLAPI_GetFunc(const char* key) {
    if(!key) return reinterpret_cast<void*>(0x100);
    return !strcmp(key,"SWELL_ExtendedAPI")?reinterpret_cast<void*>(&extended):nullptr;
}
extern "C" void host_install_events() { gdk_event_handler_set(dispatch,nullptr,nullptr); }
extern "C" int host_event_count() { return events; }
