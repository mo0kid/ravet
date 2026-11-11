#define RAVET_GTK_NO_REDIRECT
#include "RavetGtk.h"
#include <dlfcn.h>

namespace ravet_gtk {
Api api{};
namespace {
// RTLD_LOCAL keeps these libraries out of the host's global scope, as linking
// them did. They are never unloaded: GLib types and WebKit's threads outlive
// any one editor, and GTK cannot be finalized.
void* open(const char* name) { return dlopen(name,RTLD_NOW|RTLD_LOCAL|RTLD_NODELETE); }
// dlsym with a library handle also searches that library's dependencies:
// libgtk-3 reaches GDK, GIO, GObject and GLib; WebKitGTK reaches JavaScriptCore.
template<class F> bool bind(void* library,F& field,const char* name) {
    field=reinterpret_cast<F>(dlsym(library,name));
    return field!=nullptr;
}
#define RAVET_GTK_BIND(name) ok=bind(library,api.name,#name)&&ok;
bool loadGtk() {
    void* library=open("libgtk-3.so.0");
    if(!library) return false;
    bool ok=true;
    RAVET_GTK_FUNCTIONS(RAVET_GTK_BIND)
    return ok;
}
// WebKitGTK 4.1 below 2.40 lacks webkit_web_view_evaluate_javascript and fails here.
bool loadWeb() {
    void* library=open("libwebkit2gtk-4.1.so.0");
    if(!library) return false;
    bool ok=true;
    RAVET_WEBKIT_FUNCTIONS(RAVET_GTK_BIND)
    library=open("libjson-glib-1.0.so.0");
    if(!library) return false;
    RAVET_JSON_FUNCTIONS(RAVET_GTK_BIND)
    return ok;
}
#undef RAVET_GTK_BIND
}
bool load(Part part) noexcept {
    static const bool gtk=loadGtk();
    if(!gtk||part==Part::gtk) return gtk;
    static const bool web=loadWeb();
    return web;
}
}
