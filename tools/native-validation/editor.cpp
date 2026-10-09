// Actual public C ABI checks in a disposable synthetic Cocoa parent.
#include "RavetWebEditor.h"
#include <cassert>
#include <cmath>
#include <cstring>
#include <iostream>
#include <thread>
extern "C" void* editor_probe_create();
extern "C" void* editor_probe_view(void*);
extern "C" size_t editor_probe_children(void*);
extern "C" bool editor_probe_hidden(void*);
extern "C" bool editor_probe_eval(void*,const char*);
extern "C" void editor_probe_pump(double);
extern "C" void editor_probe_destroy(void*);
struct Context {
    RavetWebEditor* editor=nullptr;
    int messages=0,snapshots=0,releases=0;
    bool destroyOnMessage=false,destroyOnSnapshot=false,releaseDuringCallback=false;
};
void release(void* raw) {++static_cast<Context*>(raw)->releases;}
void message(void* raw,const uint8_t* json,size_t size) {
    auto& context=*static_cast<Context*>(raw);
    ++context.messages;
    assert(size>0 && size<=65536);
    if(context.destroyOnMessage) {
        assert(ravet_editor_destroy(context.editor)==0);
        context.editor=nullptr;
        context.releaseDuringCallback=context.releases!=0;
    }
}
size_t snapshot(void* raw,uint8_t* output,size_t capacity) {
    auto& context=*static_cast<Context*>(raw);
    ++context.snapshots;
    const char* text="{\"value\":7}";
    assert(capacity>=std::strlen(text));
    std::memcpy(output,text,std::strlen(text));
    if(context.destroyOnSnapshot && context.editor) {
        assert(ravet_editor_destroy(context.editor)==0);
        context.editor=nullptr;
        context.releaseDuringCallback=context.releases!=0;
    }
    return std::strlen(text);
}
RavetEditorCallbacks callbacks(Context& c) {return {&c,message,snapshot,release};}
const char* html=R"HTML(<!doctype html><meta charset="utf-8"><title>RAVET public ABI probe</title><body><button id="edit" onclick='window.webkit.messageHandlers.ravet.postMessage({type:"edit",value:2})'>Edit</button><output id="reading">waiting</output><script>window.ravetUpdate=s=>document.getElementById("reading").textContent=String(s.value);</script>)HTML";
RavetWebEditor* create(Context& c,uint32_t& status) {
    return ravet_editor_create_html(reinterpret_cast<const uint8_t*>(html),std::strlen(html),480,390,callbacks(c),&status);
}
int main() {
    uint32_t status=99;
    Context rejected;
    assert(!ravet_editor_create_html(nullptr,0,480,390,callbacks(rejected),&status));
    assert(status==1 && rejected.releases==1);
    Context noStatus;
    assert(!ravet_editor_create_html(reinterpret_cast<const uint8_t*>(html),std::strlen(html),480,390,callbacks(noStatus),nullptr));
    assert(noStatus.releases==1);
    void* parent=editor_probe_create();
    for(int cycle=0;cycle<3;++cycle) {
        Context context;
        context.editor=create(context,status);
        assert(context.editor && status==0);
        assert(ravet_editor_attach(context.editor,nullptr)==1);
        assert(ravet_editor_attach(context.editor,editor_probe_view(parent))==0);
        assert(editor_probe_children(parent)==1);
        assert(ravet_editor_set_size(context.editor,640,480)==0);
        assert(ravet_editor_set_size(context.editor,NAN,480)==1);
        assert(ravet_editor_set_size(context.editor,16385,480)==1);
        assert(ravet_editor_set_background(context.editor,.1,.2,.3)==0);
        assert(ravet_editor_set_background(context.editor,NAN,.2,.3)==1);
        assert(ravet_editor_set_visible(context.editor,2)==1);
        assert(ravet_editor_set_visible(context.editor,0)==0);
        assert(editor_probe_hidden(parent));
        assert(ravet_editor_set_visible(context.editor,1)==0);
        for(int i=0;i<60&&!context.snapshots;++i)editor_probe_pump(.05);
        assert(context.snapshots>0);
        bool ready=false;
        for(int i=0;i<60&&!ready;++i) {editor_probe_pump(.05);ready=editor_probe_eval(parent,"document.getElementById('reading')?.textContent==='7'");}
        assert(ready);
        assert(editor_probe_eval(parent,"document.getElementById('edit').click();true"));
        editor_probe_pump(.1);
        assert(context.messages==1 && context.releases==0);
        Context wrong;
        std::thread worker([&] {
            uint32_t code=99;
            assert(!create(wrong,code));assert(code==2 && wrong.releases==1);
            assert(ravet_editor_set_visible(context.editor,0)==2);
            assert(ravet_editor_destroy(context.editor)==2);
        });worker.join();
        assert(context.releases==0 && editor_probe_children(parent)==1);
        assert(ravet_editor_detach(context.editor)==0);
        assert(editor_probe_children(parent)==0);
        assert(ravet_editor_attach(context.editor,editor_probe_view(parent))==0);
        assert(ravet_editor_destroy(context.editor)==0);
        context.editor=nullptr;
        assert(context.releases==1 && editor_probe_children(parent)==0);
        int messages=context.messages,snapshots=context.snapshots;
        editor_probe_pump(.15);
        assert(context.messages==messages && context.snapshots==snapshots);
    }
    for(bool fromSnapshot:{false,true}) {
        Context context;context.destroyOnSnapshot=fromSnapshot;context.destroyOnMessage=!fromSnapshot;
        context.editor=create(context,status);assert(context.editor && status==0);
        assert(ravet_editor_attach(context.editor,editor_probe_view(parent))==0);
        if(!fromSnapshot) {
            editor_probe_pump(.4);
            // Click initiates an asynchronous native message and destroys owner.
            // Completion may be cancelled by teardown; callback counters are proof.
            editor_probe_eval(parent,"document.getElementById('edit').click();true");
        }
        for(int i=0;i<60&&context.editor;++i)editor_probe_pump(.05);
        assert(!context.editor && context.releases==1 && !context.releaseDuringCallback);
        assert(editor_probe_children(parent)==0);
        const int before=context.snapshots;
        editor_probe_pump(.15);assert(context.snapshots==before);
    }
    assert(ravet_editor_destroy(nullptr)==0);
    editor_probe_destroy(parent);
    std::cout<<"PASS: native Rust-owned editor C ABI, three lifecycle cycles, wrong-thread retention and two callback self-destruction paths\n";
}
