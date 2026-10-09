#include "RavetWebView.h"
#include <windows.h>
#include <shlobj.h>
#include <shlwapi.h>
#include <WebView2.h>
#include <algorithm>
#include <array>
#include <atomic>
#include <cmath>
#include <cstring>
#include <filesystem>
#include <functional>
#include <memory>
#include <string>
#include <thread>

namespace {
DWORD uiThread=0;
std::wstring wide(const std::string& text) {
    int n=MultiByteToWideChar(CP_UTF8,MB_ERR_INVALID_CHARS,text.data(),int(text.size()),nullptr,0);
    std::wstring out(n,0); if(n) MultiByteToWideChar(CP_UTF8,MB_ERR_INVALID_CHARS,text.data(),int(text.size()),out.data(),n); return out;
}
std::string utf8(const wchar_t* text) {
    if(!text) return {}; int n=WideCharToMultiByte(CP_UTF8,WC_ERR_INVALID_CHARS,text,-1,nullptr,0,nullptr,nullptr); if(!n) return {};
    std::string out(n,0); WideCharToMultiByte(CP_UTF8,WC_ERR_INVALID_CHARS,text,-1,out.data(),n,nullptr,nullptr); out.pop_back(); return out;
}
GUID guid(const wchar_t* text) { GUID id{}; IIDFromString(text,&id); return id; }
template<class Interface,class... Args> class Handler final : public Interface {
    std::atomic<ULONG> refs{1}; GUID id; std::function<HRESULT(Args...)> function;
public:
    Handler(const wchar_t* iid,std::function<HRESULT(Args...)> fn):id(guid(iid)),function(std::move(fn)) {}
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid,void** object) override {
        if(!object) return E_POINTER; *object=nullptr;
        if(IsEqualIID(iid,IID_IUnknown)||IsEqualIID(iid,id)) { *object=static_cast<Interface*>(this); AddRef(); return S_OK; } return E_NOINTERFACE;
    }
    ULONG STDMETHODCALLTYPE AddRef() override { return ++refs; }
    ULONG STDMETHODCALLTYPE Release() override { auto n=--refs; if(!n) delete this; return n; }
    HRESULT STDMETHODCALLTYPE Invoke(Args... args) override { return function(args...); }
};
struct State {
    HWND window=nullptr;
    HMODULE loader=nullptr;
    ICoreWebView2Controller* controller=nullptr;
    ICoreWebView2* web=nullptr;
    void* context=nullptr;
    RavetWebMessage message=nullptr;
    RavetWebSnapshot snapshot=nullptr;
    RavetWebSnapshotBuffer bufferSnapshot=nullptr;
    bool closed=false,evaluating=false,com=false,ready=false;
    std::wstring path,root,html;
    std::string delivered;
    COLORREF background=RGB(255,255,255);
    EventRegistrationToken messageToken{},navigationToken{},loadToken{},popupToken{};
    ~State() { if(web) web->Release(); if(controller) controller->Release(); if(loader) FreeLibrary(loader); if(com) CoUninitialize(); }
};
using Owner=std::shared_ptr<State>;
using Weak=std::weak_ptr<State>;
std::wstring quote(const char* text) {
    std::string out="\""; const char* hex="0123456789abcdef";
    for(const unsigned char* p=reinterpret_cast<const unsigned char*>(text);*p;++p) {
        if(*p=='"'||*p=='\\') { out+='\\'; out+=char(*p); }
        else if(*p<32) { out+="\\u00"; out+=hex[*p>>4]; out+=hex[*p&15]; }
        else out+=char(*p);
    }
    out+='"'; return wide(out);
}
void colour(State& s) {
    if(!s.controller) return; ICoreWebView2Controller2* c=nullptr;
    if(SUCCEEDED(s.controller->QueryInterface(guid(L"{c979903e-d4ca-4228-92eb-47ee3fa96eab}"),reinterpret_cast<void**>(&c)))) {
        c->put_DefaultBackgroundColor(COREWEBVIEW2_COLOR{255,GetRValue(s.background),GetGValue(s.background),GetBValue(s.background)}); c->Release();
    }
}
void poll(Owner owner) {
    auto& s=*owner; if(s.closed||!s.web||!s.ready||s.evaluating||(!s.snapshot&&!s.bufferSnapshot)) return;
    std::array<char,65537> buffer; const char* text=nullptr;
    if(s.bufferSnapshot) { auto n=s.bufferSnapshot(s.context,buffer.data(),buffer.size()); if(!n||n>=buffer.size()) return; buffer[n]=0; text=buffer.data(); }
    else text=s.snapshot(s.context);
    if(s.closed||!text||strnlen(text,65537)>65536||s.delivered==text) return;
    std::string next=text; auto encoded=quote(text); if(encoded.empty()) return;
    // JSON is encoded as a string before parsing; malformed snapshots cannot inject script.
    auto script=L"(()=>{try{const x=JSON.parse("+encoded+L");if(!x||typeof x!=='object'||typeof window.ravetUpdate!=='function')return false;window.ravetUpdate(x);return true}catch(e){return false}})()";
    s.evaluating=true; Weak weak=owner;
    auto* done=new Handler<ICoreWebView2ExecuteScriptCompletedHandler,HRESULT,LPCWSTR>(L"{49511172-cc67-4bca-9923-137112f4c4cc}",[weak,next](HRESULT error,LPCWSTR result) {
        if(auto s=weak.lock();s&&!s->closed) { s->evaluating=false; if(SUCCEEDED(error)&&result&&!wcscmp(result,L"true")) s->delivered=next; } return S_OK;
    });
    if(FAILED(s.web->ExecuteScript(script.c_str(),done))) s.evaluating=false; done->Release();
}
LRESULT CALLBACK windowProc(HWND hwnd,UINT message,WPARAM wp,LPARAM lp) {
    auto* handle=reinterpret_cast<Owner*>(GetWindowLongPtrW(hwnd,GWLP_USERDATA));
    if(handle) {
        auto owner=*handle; auto& s=*owner;
        if(message==WM_TIMER) { poll(owner); return 0; }
        if(message==WM_SIZE&&s.controller) { RECT rect; GetClientRect(hwnd,&rect); s.controller->put_Bounds(rect); return 0; }
        if(message==WM_MOVE&&s.controller) s.controller->NotifyParentWindowPositionChanged();
        if(message==WM_ERASEBKGND) { RECT rect; GetClientRect(hwnd,&rect); auto brush=CreateSolidBrush(s.background); FillRect(reinterpret_cast<HDC>(wp),&rect,brush); DeleteObject(brush); return 1; }
    }
    return DefWindowProcW(hwnd,message,wp,lp);
}
void initialize(Owner owner) {
    Weak weak=owner;
    auto* environmentHandler=new Handler<ICoreWebView2CreateCoreWebView2EnvironmentCompletedHandler,HRESULT,ICoreWebView2Environment*>(L"{4e8a3389-c9d8-4bd2-b6b5-124fee6cc14d}",[weak](HRESULT error,ICoreWebView2Environment* environment) {
        auto s=weak.lock(); if(!s||s->closed||FAILED(error)||!environment) return S_OK;
        auto* handler=new Handler<ICoreWebView2CreateCoreWebView2ControllerCompletedHandler,HRESULT,ICoreWebView2Controller*>(L"{6c4819f3-c9b7-4260-8127-c9f5bde7f68c}",[weak](HRESULT error,ICoreWebView2Controller* controller) {
            auto s=weak.lock(); if(!s||s->closed||FAILED(error)||!controller) return S_OK;
            controller->AddRef(); s->controller=controller; if(FAILED(controller->get_CoreWebView2(&s->web))) return S_OK;
            RECT rect; GetClientRect(s->window,&rect); controller->put_Bounds(rect); controller->put_IsVisible(TRUE); colour(*s);
            auto* messages=new Handler<ICoreWebView2WebMessageReceivedEventHandler,ICoreWebView2*,ICoreWebView2WebMessageReceivedEventArgs*>(L"{57213f19-00e6-49fa-8e07-898ea01ecbd2}",[weak](ICoreWebView2*,ICoreWebView2WebMessageReceivedEventArgs* args) {
                auto s=weak.lock(); if(!s||s->closed||!s->message) return S_OK; LPWSTR json=nullptr;
                if(SUCCEEDED(args->get_WebMessageAsJson(&json))) { auto text=utf8(json); if(!text.empty()&&text.size()<=65536&&text.front()=='{') s->message(s->context,text.c_str()); CoTaskMemFree(json); } return S_OK;
            }); s->web->add_WebMessageReceived(messages,&s->messageToken); messages->Release();
            auto* navigation=new Handler<ICoreWebView2NavigationStartingEventHandler,ICoreWebView2*,ICoreWebView2NavigationStartingEventArgs*>(L"{9adbe429-f36d-432b-9ddc-f8881fbd76e3}",[weak](ICoreWebView2*,ICoreWebView2NavigationStartingEventArgs* args) {
                auto s=weak.lock(); if(!s||s->closed) { args->put_Cancel(TRUE); return S_OK; }
                LPWSTR uri=nullptr; args->get_Uri(&uri); bool allowed=uri&&!wcscmp(uri,L"about:blank");
                wchar_t path[32768]; DWORD length=32768;
                if(uri&&SUCCEEDED(PathCreateFromUrlW(uri,path,&length,0))&&!s->root.empty()) { std::error_code error; auto file=std::filesystem::canonical(path,error).wstring(); allowed=!error&&file.rfind(s->root+L"\\",0)==0; }
                if(!allowed) args->put_Cancel(TRUE); CoTaskMemFree(uri); return S_OK;
            }); s->web->add_NavigationStarting(navigation,&s->navigationToken); navigation->Release();
            auto* popup=new Handler<ICoreWebView2NewWindowRequestedEventHandler,ICoreWebView2*,ICoreWebView2NewWindowRequestedEventArgs*>(L"{d4c185fe-c81c-4989-97af-2d3fa7ab5651}",[](ICoreWebView2*,ICoreWebView2NewWindowRequestedEventArgs* args) { args->put_Handled(TRUE); return S_OK; });
            s->web->add_NewWindowRequested(popup,&s->popupToken); popup->Release();
            auto* loaded=new Handler<ICoreWebView2NavigationCompletedEventHandler,ICoreWebView2*,ICoreWebView2NavigationCompletedEventArgs*>(L"{d33a35bf-1c49-4f98-93ab-006e0533fe1c}",[weak](ICoreWebView2*,ICoreWebView2NavigationCompletedEventArgs* args) {
                if(auto s=weak.lock();s&&!s->closed) { BOOL success=FALSE; args->get_IsSuccess(&success); s->ready=success; s->delivered.clear(); } return S_OK;
            }); s->web->add_NavigationCompleted(loaded,&s->loadToken); loaded->Release();
            auto* script=new Handler<ICoreWebView2AddScriptToExecuteOnDocumentCreatedCompletedHandler,HRESULT,LPCWSTR>(L"{b99369f3-9b11-47b5-bc6f-8e7895fcea17}",[weak](HRESULT error,LPCWSTR) {
                if(auto s=weak.lock();s&&!s->closed&&SUCCEEDED(error)) {
                    if(!s->path.empty()) { wchar_t uri[32768]; DWORD length=32768; if(SUCCEEDED(UrlCreateFromPathW(s->path.c_str(),uri,&length,0))) s->web->Navigate(uri); }
                    else s->web->NavigateToString(s->html.c_str());
                } return S_OK;
            });
            s->web->AddScriptToExecuteOnDocumentCreated(L"window.webkit={messageHandlers:{ravet:{postMessage:x=>window.chrome.webview.postMessage(x)}}};",script); script->Release();
            return S_OK;
        }); environment->CreateCoreWebView2Controller(s->window,handler); handler->Release(); return S_OK;
    });
    using Create=HRESULT(STDAPICALLTYPE*)(PCWSTR,PCWSTR,ICoreWebView2EnvironmentOptions*,ICoreWebView2CreateCoreWebView2EnvironmentCompletedHandler*);
    auto create=reinterpret_cast<Create>(GetProcAddress(owner->loader,"CreateCoreWebView2EnvironmentWithOptions"));
    wchar_t local[MAX_PATH]{}; SHGetFolderPathW(nullptr,CSIDL_LOCAL_APPDATA,nullptr,SHGFP_TYPE_CURRENT,local);
    auto profile=(std::filesystem::path(local)/L"RAVET/WebView2").wstring();
    if(create) create(nullptr,profile.c_str(),nullptr,environmentHandler); environmentHandler->Release();
}
Owner create(double w,double h,void* context,RavetWebMessage message,RavetWebSnapshot snapshot,RavetWebSnapshotBuffer buffer) {
    if(!ravet_web_is_main_thread()||!std::isfinite(w)||!std::isfinite(h)||w<=0||h<=0||w>32767||h>32767) return {};
    uiThread=GetCurrentThreadId();
    auto s=std::make_shared<State>(); auto hr=CoInitializeEx(nullptr,COINIT_APARTMENTTHREADED); if(FAILED(hr)) return {}; s->com=true;
    HMODULE module=nullptr; GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS|GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,reinterpret_cast<LPCWSTR>(&ravet_web_create),&module);
    wchar_t path[32768]{}; if(!GetModuleFileNameW(module,path,32768)) return {};
    auto loader=std::filesystem::path(path).parent_path()/L"WebView2Loader.dll";
    s->loader=LoadLibraryExW(loader.c_str(),nullptr,LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR|LOAD_LIBRARY_SEARCH_DEFAULT_DIRS); if(!s->loader) return {};
    using Version=HRESULT(STDAPICALLTYPE*)(PCWSTR,LPWSTR*);
    auto version=reinterpret_cast<Version>(GetProcAddress(s->loader,"GetAvailableCoreWebView2BrowserVersionString"));
    LPWSTR installed=nullptr; auto available=version?version(nullptr,&installed):E_FAIL; CoTaskMemFree(installed); if(FAILED(available)) return {};
    WNDCLASSW cls{}; cls.lpfnWndProc=windowProc; cls.hInstance=module; cls.lpszClassName=L"RavetWebEditor"; cls.hCursor=LoadCursorW(nullptr,MAKEINTRESOURCEW(32512));
    RegisterClassW(&cls);
    s->window=CreateWindowExW(0,cls.lpszClassName,L"",WS_POPUP,0,0,int(w),int(h),nullptr,nullptr,module,nullptr); if(!s->window) return {};
    s->context=context; s->message=message; s->snapshot=snapshot; s->bufferSnapshot=buffer; return s;
}
void* finish(Owner s) { auto* handle=new Owner(s); SetWindowLongPtrW(s->window,GWLP_USERDATA,reinterpret_cast<LONG_PTR>(handle)); SetTimer(s->window,1,50,nullptr); initialize(s); return handle; }
}
extern "C" bool ravet_web_is_main_thread() { return !uiThread||GetCurrentThreadId()==uiThread; }
extern "C" void ravet_web_pump() {}
extern "C" void* ravet_web_create(const char* path,double w,double h,void* context,RavetWebMessage message,RavetWebSnapshot snapshot) {
    if(!path) return nullptr; std::error_code error; auto file=std::filesystem::canonical(std::filesystem::u8path(path),error); if(error||!std::filesystem::is_regular_file(file,error)) return nullptr;
    auto s=create(w,h,context,message,snapshot,nullptr); if(!s) return nullptr; s->path=file.wstring(); s->root=file.parent_path().wstring(); return finish(s);
}
extern "C" void* ravet_web_create_html(const char* html,double w,double h,void* context,RavetWebMessage message,RavetWebSnapshotBuffer snapshot) {
    if(!html||strnlen(html,1048577)>1048576) return nullptr; auto page=wide(html); if(page.empty()) return nullptr;
    auto s=create(w,h,context,message,nullptr,snapshot); if(!s) return nullptr; s->html=page; return finish(s);
}
extern "C" bool ravet_web_set_background(void* view,double r,double g,double b) {
    if(!view||!std::isfinite(r)||!std::isfinite(g)||!std::isfinite(b)) return false;
    auto& s=**static_cast<Owner*>(view); s.background=RGB(int(std::clamp(r,0.0,1.0)*255),int(std::clamp(g,0.0,1.0)*255),int(std::clamp(b,0.0,1.0)*255)); colour(s); InvalidateRect(s.window,nullptr,TRUE); return true;
}
extern "C" void ravet_web_attach(void* view,void* parent) {
    if(!view||!parent) return; auto& s=**static_cast<Owner*>(view); SetParent(s.window,static_cast<HWND>(parent)); SetWindowLongPtrW(s.window,GWL_STYLE,WS_CHILD|WS_VISIBLE);
    SetWindowPos(s.window,nullptr,0,0,0,0,SWP_NOSIZE|SWP_NOZORDER|SWP_NOACTIVATE|SWP_FRAMECHANGED); ShowWindow(s.window,SW_SHOW);
}
extern "C" void ravet_web_detach(void* view) { if(view) { auto& s=**static_cast<Owner*>(view); ShowWindow(s.window,SW_HIDE); SetWindowLongPtrW(s.window,GWL_STYLE,WS_POPUP); SetParent(s.window,nullptr); } }
extern "C" void ravet_web_set_visible(void* view,bool visible) { if(view) ShowWindow((**static_cast<Owner*>(view)).window,visible?SW_SHOW:SW_HIDE); }
extern "C" void ravet_web_destroy(void* view) {
    if(!view) return; auto s=*static_cast<Owner*>(view); s->closed=true; s->context=nullptr; s->message=nullptr; s->snapshot=nullptr; s->bufferSnapshot=nullptr; KillTimer(s->window,1);
    if(s->web) { s->web->remove_WebMessageReceived(s->messageToken); s->web->remove_NavigationStarting(s->navigationToken); s->web->remove_NavigationCompleted(s->loadToken); s->web->remove_NewWindowRequested(s->popupToken); s->web->Stop(); }
    if(s->controller) s->controller->Close(); SetWindowLongPtrW(s->window,GWLP_USERDATA,0); DestroyWindow(s->window);
    HMODULE module=nullptr; GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS|GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,reinterpret_cast<LPCWSTR>(&ravet_web_create),&module);
    UnregisterClassW(L"RavetWebEditor",module); delete static_cast<Owner*>(view);
}
