#include "RavetWebView.h"
#include "RavetWebAssets.h"
#include <gtk/gtk.h>
#include <gtk/gtkx.h>
#include <gdk/gdkx.h>
#include <webkit2/webkit2.h>
#include <json-glib/json-glib.h>
#include <algorithm>
#include <array>
#include <atomic>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <filesystem>
#include <memory>
#include <string>
#include <thread>
#include <dlfcn.h>
#include <link.h>

namespace {
// Adopted by the first creation; read by every entry point.
std::atomic<std::thread::id> uiThread{};
struct State {
    GtkWidget* box=nullptr;
    GtkWidget* plug=nullptr;
    WebKitWebView* web=nullptr;
    WebKitUserContentManager* manager=nullptr;
    void* context=nullptr;
    RavetWebMessage message=nullptr;
    RavetWebSnapshot snapshot=nullptr;
    RavetWebSnapshotBuffer bufferSnapshot=nullptr;
    guint timer=0;
    gulong messageHandler=0;
    bool closed=false,evaluating=false;
    std::string root,delivered;
    // ravet_web_create_assets: the page and assets served at assetOrigin().
    bool assets=false;
    std::string page;
    ravet_web_assets::Table table;
};
using Owner=std::shared_ptr<State>;
bool initializeGtk() {
    // SWELL owns GDK's process-wide event handler. Its GTK loader initializes
    // GTK and restores a dispatcher that handles both host and GTK windows.
    // Calling gtk_init_check first would silently replace the host dispatcher.
    using GetFunc=void* (*)(const char*);
    using Extended=void* (*)(const char*,void*);
    auto getFunc=reinterpret_cast<GetFunc>(dlsym(RTLD_DEFAULT,"SWELLAPI_GetFunc"));
    void* library=nullptr;
    if(!getFunc) {
        dl_iterate_phdr(+[](dl_phdr_info* info,size_t,void* data)->int {
            const char* name=info->dlpi_name;
            const char* base=strrchr(name,'/'); base=base?base+1:name;
            if(strcmp(base,"libSwell.so")) return 0;
            *static_cast<void**>(data)=dlopen(name,RTLD_NOW|RTLD_NOLOAD);
            return *static_cast<void**>(data)?1:0;
        },&library);
        if(library) getFunc=reinterpret_cast<GetFunc>(dlsym(library,"SWELLAPI_GetFunc"));
    }
    bool ready=true;
    if(getFunc) {
        auto extended=getFunc(nullptr)==reinterpret_cast<void*>(0x100)
            ?reinterpret_cast<Extended>(getFunc("SWELL_ExtendedAPI")):nullptr;
        ready=extended&&extended("LOAD_GTK",nullptr);
    } else if(library) ready=false;
    if(library) dlclose(library);
    if(!ready) return false;
    return gtk_init_check(nullptr,nullptr);
}
bool jsonContainer(const char* text) {
    JsonParser* parser=json_parser_new();
    bool valid=json_parser_load_from_data(parser,text,-1,nullptr);
    if(valid) { auto* root=json_parser_get_root(parser); valid=JSON_NODE_HOLDS_OBJECT(root)||JSON_NODE_HOLDS_ARRAY(root); }
    g_object_unref(parser); return valid;
}
gboolean poll(gpointer data) noexcept {
    auto owner=*static_cast<Owner*>(data); auto& s=*owner;
    if(s.closed) return G_SOURCE_REMOVE;
    if(s.evaluating||(!s.snapshot&&!s.bufferSnapshot)) return G_SOURCE_CONTINUE;
    std::array<char,65537> buffer;
    const char* text=nullptr;
    if(s.bufferSnapshot) {
        auto size=s.bufferSnapshot(s.context,buffer.data(),buffer.size());
        if(!size||size>=buffer.size()) return G_SOURCE_CONTINUE;
        buffer[size]=0; text=buffer.data();
    } else text=s.snapshot(s.context);
    if(s.closed||!text||strnlen(text,65537)>65536||!jsonContainer(text)||s.delivered==text) return G_SOURCE_CONTINUE;
    std::string next=text;
    std::string script="typeof window.ravetUpdate==='function'&&(window.ravetUpdate("+next+"),true)";
    s.evaluating=true;
    auto* pending=new std::pair<Owner,std::string>(owner,next);
    webkit_web_view_evaluate_javascript(s.web,script.c_str(),-1,nullptr,nullptr,nullptr,
        [](GObject* web,GAsyncResult* result,gpointer data) noexcept {
            std::unique_ptr<std::pair<Owner,std::string>> pending(static_cast<std::pair<Owner,std::string>*>(data));
            GError* error=nullptr;
            auto* value=webkit_web_view_evaluate_javascript_finish(WEBKIT_WEB_VIEW(web),result,&error);
            auto& s=*pending->first;
            if(!s.closed) { s.evaluating=false; if(!error&&value&&jsc_value_is_boolean(value)&&jsc_value_to_boolean(value)) s.delivered=pending->second; }
            if(value) g_object_unref(value); if(error) g_error_free(error);
        },pending);
    return G_SOURCE_CONTINUE;
}
// GObject's type registry is process-wide. A host built on another GDK (GTK 2,
// Ardour's YTK fork, GTK 4) has already registered GdkDisplay and friends, and
// GTK 3 cannot register them again, so the first GDK 3 call corrupts the host.
// Decline before any GTK 3 call; hosts then fall back to generic controls.
bool foreignGdkLoaded() {
    return dl_iterate_phdr(+[](dl_phdr_info* info,size_t,void*)->int {
        const char* name=info->dlpi_name;
        const char* base=strrchr(name,'/'); base=base?base+1:name;
        for(const char* prefix:{"libgdk-x11-2.0.so","libydk.so","libgtk-4.so"})
            if(!strncmp(base,prefix,strlen(prefix))) return 1;
        return 0;
    },nullptr)!=0;
}
// WebKitGTK URI schemes are registered on the process-wide default context, so
// every loaded binary (two RAVET products in one host) gets its own scheme,
// named after an address in this module. Requests find their view through
// object data on the web view.
const std::string& assetScheme() {
    static const std::string scheme=[] { char text[40]; snprintf(text,sizeof text,"ravet-%zx",size_t(reinterpret_cast<uintptr_t>(&uiThread))); return std::string(text); }();
    return scheme;
}
std::string assetOrigin() { return assetScheme()+"://editor/"; }
constexpr const char* stateKey="ravet-web-state";
void serveAsset(WebKitURISchemeRequest* request,gpointer) {
    auto* web=webkit_uri_scheme_request_get_web_view(request);
    auto* s=web?static_cast<State*>(g_object_get_data(G_OBJECT(web),stateKey)):nullptr;
    const char* uri=webkit_uri_scheme_request_get_uri(request);
    std::string path=ravet_web_assets::pathOf(uri?uri:"",assetOrigin());
    if(path.empty()) path="index.html";
    const char* method=webkit_uri_scheme_request_get_http_method(request);
    const void* data=nullptr; size_t size=0; const char* mime=nullptr;
    if(s&&!s->closed&&s->assets&&(!method||!strcmp(method,"GET"))) {
        if(path=="index.html") { data=s->page.data(); size=s->page.size(); mime="text/html"; }
        else if(auto found=s->table.find(path);found!=s->table.end()) { data=found->second.data; size=found->second.size; mime=found->second.mime.c_str(); }
    }
    // Copy: WebKit may read the stream after the view, or the module, is gone.
    GBytes* bytes=mime?g_bytes_new(data,size):g_bytes_new_static("",0);
    GInputStream* stream=g_memory_input_stream_new_from_bytes(bytes);
    auto* response=webkit_uri_scheme_response_new(stream,mime?gint64(size):0);
    webkit_uri_scheme_response_set_status(response,mime?200:404,mime?nullptr:"Not Found");
    webkit_uri_scheme_response_set_content_type(response,mime?mime:"text/plain");
    webkit_uri_scheme_request_finish_with_response(request,response);
    g_object_unref(response); g_object_unref(stream); g_bytes_unref(bytes);
}
void registerAssetScheme() {
    static bool registered=false; if(registered) return; registered=true;
    auto* context=webkit_web_context_get_default();
    webkit_web_context_register_uri_scheme(context,assetScheme().c_str(),serveAsset,nullptr,nullptr);
    auto* security=webkit_web_context_get_security_manager(context);
    webkit_security_manager_register_uri_scheme_as_secure(security,assetScheme().c_str());
    webkit_security_manager_register_uri_scheme_as_cors_enabled(security,assetScheme().c_str());
}
// GLib and WebKit keep callbacks into this module (signal destroy notifies,
// script completions, scheme handlers) that can run after the last view is
// destroyed. A host that unloads the plug-in library then (REAPER does for
// CLAP) would jump into unmapped code, so the module stays loaded once it has
// created a view.
void pinModule() {
    static bool pinned=false; if(pinned) return;
    Dl_info info{};
    if(dladdr(reinterpret_cast<void*>(&ravet_web_pump),&info)&&info.dli_fname) pinned=dlopen(info.dli_fname,RTLD_NOW|RTLD_NOLOAD|RTLD_NODELETE)!=nullptr;
}
Owner create(double width,double height,void* context,RavetWebMessage message,RavetWebSnapshot snapshot,RavetWebSnapshotBuffer buffer) {
    if(!ravet_web_is_main_thread()||!std::isfinite(width)||!std::isfinite(height)||width<=0||height<=0||width>32767||height>32767) return {};
    if(foreignGdkLoaded()) return {};
    // VST3 Linux parents are X11 windows, including those supplied by XWayland.
    // Select X11 when we initialize GTK; an already initialized Wayland GTK
    // context cannot be repurposed, so fail rather than attach to the wrong display.
    if(!gdk_display_get_default()) gdk_set_allowed_backends("x11");
    else if(!GDK_IS_X11_DISPLAY(gdk_display_get_default())) return {};
    if(!initializeGtk()||!GDK_IS_X11_DISPLAY(gdk_display_get_default())) return {};
    uiThread=std::this_thread::get_id();
    pinModule();
    auto s=std::make_shared<State>(); s->context=context; s->message=message; s->snapshot=snapshot; s->bufferSnapshot=buffer;
    s->box=gtk_box_new(GTK_ORIENTATION_VERTICAL,0); g_object_ref_sink(s->box);
    gtk_widget_set_size_request(s->box,int(width),int(height));
    s->manager=webkit_user_content_manager_new();
    // Cancel only the browser menu; controls still receive right-click events.
    auto* contextMenuScript=webkit_user_script_new(
        "window.addEventListener('contextmenu',event=>event.preventDefault(),{capture:true,passive:false});",
        WEBKIT_USER_CONTENT_INJECT_ALL_FRAMES,WEBKIT_USER_SCRIPT_INJECT_AT_DOCUMENT_START,nullptr,nullptr);
    webkit_user_content_manager_add_script(s->manager,contextMenuScript);
    webkit_user_script_unref(contextMenuScript);
    webkit_user_content_manager_register_script_message_handler(s->manager,"ravet");
    s->web=WEBKIT_WEB_VIEW(webkit_web_view_new_with_user_content_manager(s->manager));
#if RAVET_WEB_LINUX_SOFTWARE_RENDERING
    webkit_settings_set_hardware_acceleration_policy(webkit_web_view_get_settings(s->web),WEBKIT_HARDWARE_ACCELERATION_POLICY_NEVER);
#endif
    // GtkBox's generic add operation does not expand children vertically.
    // The web view must receive the editor's full allocation, not its 1px
    // minimum height (which also produces unusable compositor buffers).
    gtk_box_pack_start(GTK_BOX(s->box),GTK_WIDGET(s->web),TRUE,TRUE,0);
    s->messageHandler=g_signal_connect_data(s->manager,"script-message-received::ravet",G_CALLBACK(+[](WebKitUserContentManager*,WebKitJavascriptResult* result,gpointer data) noexcept {
        auto s=*static_cast<Owner*>(data); if(s->closed||!s->message) return;
        auto* value=webkit_javascript_result_get_js_value(result);
        char* json=jsc_value_to_json(value,0);
        if(json&&strlen(json)<=65536&&jsonContainer(json)) s->message(s->context,json);
        g_free(json);
    }),new Owner(s),+[](gpointer p,GClosure*){delete static_cast<Owner*>(p);},GConnectFlags(0));
    g_signal_connect(s->web,"load-changed",G_CALLBACK(+[](WebKitWebView*,WebKitLoadEvent event,gpointer data) {
        if(event==WEBKIT_LOAD_FINISHED) static_cast<State*>(data)->delivered.clear();
    }),s.get());
    g_signal_connect(s->web,"decide-policy",G_CALLBACK(+[](WebKitWebView*,WebKitPolicyDecision* decision,WebKitPolicyDecisionType type,gpointer data) noexcept->gboolean {
        if(type!=WEBKIT_POLICY_DECISION_TYPE_NAVIGATION_ACTION&&type!=WEBKIT_POLICY_DECISION_TYPE_NEW_WINDOW_ACTION) return FALSE;
        auto& s=*static_cast<State*>(data);
        auto* request=webkit_navigation_policy_decision_get_request(WEBKIT_NAVIGATION_POLICY_DECISION(decision));
        const char* uri=webkit_uri_request_get_uri(request);
        char* path=g_filename_from_uri(uri,nullptr,nullptr);
        bool local=s.assets&&!strncmp(uri,assetOrigin().c_str(),assetOrigin().size());
        if(path&&!s.root.empty()) { std::error_code error; auto resolved=std::filesystem::canonical(path,error).string(); local=!error&&resolved.rfind(s.root+"/",0)==0; }
        g_free(path);
        if(type==WEBKIT_POLICY_DECISION_TYPE_NEW_WINDOW_ACTION||(!local&&strcmp(uri,"about:blank"))) { webkit_policy_decision_ignore(decision); return TRUE; }
        return FALSE;
    }),s.get());
    s->timer=g_timeout_add_full(G_PRIORITY_DEFAULT,50,poll,new Owner(s),+[](gpointer p){delete static_cast<Owner*>(p);});
    return s;
}
}
namespace {
// Hosts may destroy the X11 parent before detaching the editor (REAPER does when
// it closes a floating FX window); the plug's windows then die with it and the
// unmap/destroy requests below fail with BadWindow, which GDK treats as fatal.
struct XErrorTrap {
    GdkDisplay* display=gdk_display_get_default();
    XErrorTrap() { if(display&&GDK_IS_X11_DISPLAY(display)) gdk_x11_display_error_trap_push(display); else display=nullptr; }
    ~XErrorTrap() { if(display) gdk_x11_display_error_trap_pop_ignored(display); }
};
}
extern "C" bool ravet_web_is_main_thread() { auto ui=uiThread.load(); return ui==std::thread::id()||std::this_thread::get_id()==ui; }
extern "C" void ravet_web_pump() { for(int i=0;i<32&&g_main_context_pending(nullptr);++i) g_main_context_iteration(nullptr,FALSE); }
extern "C" void* ravet_web_create(const char* path,double w,double h,void* context,RavetWebMessage message,RavetWebSnapshot snapshot) noexcept {
    if(!path) return nullptr; std::error_code error; auto file=std::filesystem::canonical(path,error); if(error||!std::filesystem::is_regular_file(file,error)) return nullptr;
    auto s=create(w,h,context,message,snapshot,nullptr); if(!s) return nullptr;
    s->root=file.parent_path().string(); char* uri=g_filename_to_uri(file.c_str(),nullptr,nullptr);
    webkit_web_view_load_uri(s->web,uri); g_free(uri); return new Owner(s);
}
extern "C" void* ravet_web_create_html(const char* html,double w,double h,void* context,RavetWebMessage message,RavetWebSnapshotBuffer snapshot) noexcept {
    if(!html||strnlen(html,1048577)>1048576) return nullptr;
    auto s=create(w,h,context,message,nullptr,snapshot); if(!s) return nullptr;
    webkit_web_view_load_html(s->web,html,nullptr); return new Owner(s);
}
extern "C" void* ravet_web_create_assets(const char* html,const RavetWebAsset* assets,size_t count,double w,double h,void* context,RavetWebMessage message,RavetWebSnapshotBuffer snapshot) noexcept {
    if(!html||strnlen(html,1048577)>1048576) return nullptr;
    ravet_web_assets::Table table; if(!ravet_web_assets::build(assets,count,table)) return nullptr;
    auto s=create(w,h,context,message,nullptr,snapshot); if(!s) return nullptr;
    registerAssetScheme();
    s->assets=true; s->page=html; s->table=std::move(table);
    g_object_set_data(G_OBJECT(s->web),stateKey,s.get());
    webkit_web_view_load_uri(s->web,(assetOrigin()+"index.html").c_str()); return new Owner(s);
}
extern "C" bool ravet_web_set_background(void* view,double r,double g,double b) {
    if(!view||!std::isfinite(r)||!std::isfinite(g)||!std::isfinite(b)) return false;
    GdkRGBA color{std::clamp(r,0.0,1.0),std::clamp(g,0.0,1.0),std::clamp(b,0.0,1.0),1};
    auto& s=**static_cast<Owner*>(view); webkit_web_view_set_background_color(s.web,&color); return true;
}
extern "C" void ravet_web_detach(void* view) {
    if(!view) return; auto& s=**static_cast<Owner*>(view); XErrorTrap trap;
    if(s.plug) { gtk_container_remove(GTK_CONTAINER(s.plug),s.box); gtk_widget_destroy(s.plug); s.plug=nullptr; }
}
extern "C" void ravet_web_attach(void* view,void* parent) {
    if(!view||!parent) return; ravet_web_detach(view); auto& s=**static_cast<Owner*>(view); XErrorTrap trap;
    s.plug=gtk_plug_new(reinterpret_cast<uintptr_t>(parent)); gtk_container_add(GTK_CONTAINER(s.plug),s.box); gtk_widget_show_all(s.plug);
    // XEmbed leaves mapping the client to the embedder. Parents that are plain
    // X11 windows (LV2 UIs, CLAP/VST3 hosts without XEmbed) never map it.
    if(auto* window=gtk_widget_get_window(s.plug)) gdk_window_show(window);
}
extern "C" void ravet_web_set_visible(void* view,bool visible) {
    if(view) { auto& s=**static_cast<Owner*>(view); XErrorTrap trap; if(s.plug) { if(visible) { gtk_widget_show_all(s.plug); if(auto* window=gtk_widget_get_window(s.plug)) gdk_window_show(window); } else gtk_widget_hide(s.plug); } }
}
extern "C" void ravet_web_destroy(void* view) {
    if(!view) return; auto s=*static_cast<Owner*>(view); XErrorTrap trap; s->closed=true; s->context=nullptr; s->message=nullptr; s->snapshot=nullptr; s->bufferSnapshot=nullptr;
    g_source_remove(s->timer); s->timer=0;
    g_signal_handler_disconnect(s->manager,s->messageHandler);
    g_signal_handlers_disconnect_by_data(s->web,s.get());
    g_object_set_data(G_OBJECT(s->web),stateKey,nullptr);
    webkit_user_content_manager_unregister_script_message_handler(s->manager,"ravet");
    webkit_web_view_stop_loading(s->web); ravet_web_detach(view); gtk_widget_destroy(s->box); g_object_unref(s->box); g_object_unref(s->manager);
    delete static_cast<Owner*>(view);
}
extern "C" bool ravet_web_set_size(void* view,double w,double h) {
    if(!view||!std::isfinite(w)||!std::isfinite(h)||w<=0||h<=0||w>16384||h>16384) return false;
    auto& s=**static_cast<Owner*>(view); if(s.closed) return false;
    int width=int(std::lround(w)),height=int(std::lround(h));
    gtk_widget_set_size_request(s.box,width,height);
    if(s.plug) gtk_window_resize(GTK_WINDOW(s.plug),width,height);
    return true;
}
extern "C" bool ravet_web_eval(void* view,const char* script,void* context,RavetWebEvalDone done) noexcept {
    if(!view||!script||strnlen(script,1048577)>1048576) return false;
    auto owner=*static_cast<Owner*>(view); if(owner->closed) return false;
    struct Pending { std::weak_ptr<State> state; void* context; RavetWebEvalDone done; };
    webkit_web_view_evaluate_javascript(owner->web,script,-1,nullptr,nullptr,nullptr,
        [](GObject* web,GAsyncResult* result,gpointer data) noexcept {
            std::unique_ptr<Pending> pending(static_cast<Pending*>(data));
            GError* error=nullptr;
            auto* value=webkit_web_view_evaluate_javascript_finish(WEBKIT_WEB_VIEW(web),result,&error);
            bool truthy=!error&&value&&jsc_value_to_boolean(value);
            if(value) g_object_unref(value); if(error) g_error_free(error);
            auto s=pending->state.lock(); if(s&&!s->closed&&pending->done) pending->done(pending->context,truthy);
        },new Pending{owner,context,done});
    return true;
}
