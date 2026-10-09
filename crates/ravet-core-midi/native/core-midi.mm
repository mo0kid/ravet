// CoreMIDI object/block ownership only. Packet framing and queues remain Rust.
#include <CoreMIDI/CoreMIDI.h>
#include <CoreFoundation/CoreFoundation.h>
#include <atomic>
#include <chrono>
#include <memory>
#include <thread>
#include <cstring>
using Receive=void(*)(void*,int32_t,uint64_t,uint32_t,const uint32_t*);
using Release=void(*)(void*);
constexpr uint32_t Busy=1,Closed=2;
struct Callback {
    std::atomic<uint32_t> gate{0};
    std::atomic<uint64_t> overlaps{0};
    void* context=nullptr;
    Receive receive=nullptr;
    Release release=nullptr;
    void accept(const MIDIEventList* list) {
        uint32_t expected=0;
        if(!gate.compare_exchange_strong(expected,Busy,std::memory_order_acquire)) {
            if(expected&Busy)overlaps.fetch_add(1,std::memory_order_relaxed);
            return;
        }
        // CoreMIDI supplies valid event-list storage. Bound traversal before Rust.
        bool valid=list && list->numPackets<=1024;
        uint32_t words=0;
        if(valid) {
            const MIDIEventPacket* packet=&list->packet[0];
            for(uint32_t i=0;i<list->numPackets;i++) {
                if(packet->wordCount>4096-words){valid=false;break;}
                words+=packet->wordCount;
                packet=MIDIEventPacketNext(packet);
            }
        }
        if(!valid)receive(context,0,0,0,nullptr);
        else {
            const MIDIEventPacket* packet=&list->packet[0];
            for(uint32_t i=0;i<list->numPackets;i++) {
                receive(context,list->protocol,packet->timeStamp,packet->wordCount,packet->words);
                packet=MIDIEventPacketNext(packet);
            }
        }
        gate.fetch_and(~Busy,std::memory_order_release);
    }
    void close() {
        gate.fetch_or(Closed,std::memory_order_acq_rel);
        while(gate.load(std::memory_order_acquire)&Busy)std::this_thread::sleep_for(std::chrono::microseconds(100));
        // Gate now prevents new Rust entry. Context destruction stays on control.
        if(context){release(context);context=nullptr;}
    }
};
struct Port {
    MIDIClientRef client=0;MIDIPortRef port=0;MIDIEndpointRef destination=0;
    MIDIEndpointRef owned=0;bool source=false;std::shared_ptr<Callback> callback;
    ~Port() {
        if(callback)callback->gate.fetch_or(Closed,std::memory_order_acq_rel);
        if(port)MIDIPortDispose(port);
        if(owned)MIDIEndpointDispose(owned);
        if(callback)callback->close();
        if(client)MIDIClientDispose(client);
    }
};
static CFStringRef label(const char* value){return CFStringCreateWithCString(nullptr,value,kCFStringEncodingUTF8);}
static OSStatus find(int32_t id,bool input,MIDIEndpointRef* endpoint) {
    MIDIObjectRef object=0;MIDIObjectType type;
    OSStatus status=MIDIObjectFindByUniqueID(id,&object,&type);
    if(status)return status;
    bool source=type==kMIDIObjectType_Source||type==kMIDIObjectType_ExternalSource;
    bool dest=type==kMIDIObjectType_Destination||type==kMIDIObjectType_ExternalDestination;
    if((input&&!source)||(!input&&!dest))return -50;
    *endpoint=object;return 0;
}
extern "C" void* ravet_cm_input(const char* client_name,const char* port_name,int32_t protocol,int32_t source_id,int is_virtual,void* context,Receive receive,Release release,int32_t* error) {
    bool transferred=false;
    try {
        auto callback=std::make_shared<Callback>();
        callback->context=context;callback->receive=receive;callback->release=release;
        auto handle=std::make_unique<Port>();handle->callback=callback;transferred=true;
        CFStringRef client_label=label(client_name),port_label=label(port_name);
        OSStatus status=(!client_label||!port_label)?-50:MIDIClientCreate(client_label,nullptr,nullptr,&handle->client);
        if(!status&&is_virtual)status=MIDIDestinationCreateWithProtocol(handle->client,port_label,static_cast<MIDIProtocolID>(protocol),&handle->owned,^(const MIDIEventList* list,void*){callback->accept(list);});
        if(!status&&!is_virtual) {
            MIDIEndpointRef source=0;status=find(source_id,true,&source);
            if(!status)status=MIDIInputPortCreateWithProtocol(handle->client,port_label,static_cast<MIDIProtocolID>(protocol),&handle->port,^(const MIDIEventList* list,void*){callback->accept(list);});
            if(!status)status=MIDIPortConnectSource(handle->port,source,nullptr);
        }
        if(client_label)CFRelease(client_label);if(port_label)CFRelease(port_label);
        *error=status;
        return status?nullptr:handle.release();
    } catch(...) {*error=-108;if(!transferred)release(context);return nullptr;}
}
extern "C" void* ravet_cm_output(const char* name,int32_t destination_id,int32_t* error) {
    try {
        auto handle=std::make_unique<Port>();
        CFStringRef text=label(name);
        OSStatus status=text?find(destination_id,false,&handle->destination):-50;
        if(!status)status=MIDIClientCreate(text,nullptr,nullptr,&handle->client);
        if(!status)status=MIDIOutputPortCreate(handle->client,text,&handle->port);
        if(text)CFRelease(text);*error=status;
        return status?nullptr:handle.release();
    } catch(...) {*error=-108;return nullptr;}
}
extern "C" void* ravet_cm_source(const char* client_name,const char* source_name,int32_t protocol,int32_t* error) {
    try {
        auto handle=std::make_unique<Port>();handle->source=true;
        CFStringRef client_label=label(client_name),source_label=label(source_name);
        OSStatus status=(!client_label||!source_label)?-50:MIDIClientCreate(client_label,nullptr,nullptr,&handle->client);
        if(!status)status=MIDISourceCreateWithProtocol(handle->client,source_label,static_cast<MIDIProtocolID>(protocol),&handle->owned);
        if(client_label)CFRelease(client_label);if(source_label)CFRelease(source_label);
        *error=status;return status?nullptr:handle.release();
    } catch(...) {*error=-108;return nullptr;}
}
extern "C" void ravet_cm_close(void* handle){delete static_cast<Port*>(handle);}
extern "C" int32_t ravet_cm_virtual_id(void* opaque,int32_t* id) {
    auto handle=static_cast<Port*>(opaque);
    return handle->owned?MIDIObjectGetIntegerProperty(handle->owned,kMIDIPropertyUniqueID,id):-50;
}
extern "C" uint64_t ravet_cm_overlaps(void* opaque) {
    auto callback=static_cast<Port*>(opaque)->callback;
    return callback?callback->overlaps.load(std::memory_order_relaxed):0;
}
extern "C" int32_t ravet_cm_send(void* opaque,int32_t protocol,uint64_t timestamp,uint32_t count,const uint32_t* words) {
    auto handle=static_cast<Port*>(opaque);
    if((!handle->destination&&!(handle->source&&handle->owned))||count<1||count>4)return -50;
    alignas(8) unsigned char storage[512]{};
    auto list=reinterpret_cast<MIDIEventList*>(storage);
    auto packet=MIDIEventListInit(list,static_cast<MIDIProtocolID>(protocol));
    if(!MIDIEventListAdd(list,sizeof(storage),packet,timestamp,count,words))return -50;
    return handle->source?MIDIReceivedEventList(handle->owned,list):MIDISendEventList(handle->port,handle->destination,list);
}
extern "C" uint32_t ravet_cm_count(int input){return input?MIDIGetNumberOfSources():MIDIGetNumberOfDestinations();}
extern "C" int32_t ravet_cm_info(int input,uint32_t index,int32_t* id,char* name,uint32_t capacity) {
    auto endpoint=input?MIDIGetSource(index):MIDIGetDestination(index);
    if(!endpoint)return -50;
    auto status=MIDIObjectGetIntegerProperty(endpoint,kMIDIPropertyUniqueID,id);
    CFStringRef value=nullptr;
    if(!status)status=MIDIObjectGetStringProperty(endpoint,kMIDIPropertyDisplayName,&value);
    if(!status&&(!value||!CFStringGetCString(value,name,capacity,kCFStringEncodingUTF8)))status=-50;
    if(value)CFRelease(value);return status;
}
