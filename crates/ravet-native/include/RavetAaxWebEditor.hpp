#pragma once
#if !defined(__APPLE__)
#error "RavetAaxWebEditor requires the macOS native editor ABI"
#endif
#include "RavetAaxEditorControls.hpp"
#include "RavetWebEditor.h"
#include "AAX_CEffectGUI.h"
#include "AAX_VAutomationDelegate.h"
#include "AAX_IACFViewContainer.h"
#include "AAX_IACFController.h"
#include "AAX_IACFAutomationDelegate.h"
#include "AAX_UIDs.h"
#include <memory>
#include <pthread.h>

namespace ravet::aax {
// Optional macOS MAIN-only external SDK consumer. Retain originating native
// tables/library and the host parent NSView through final callback completion.
// No native handle or GUI-this pointer escapes into the callback context.
template<size_t MaximumParameters=1024,uint32_t NativeSampleBits=32>
class AaxWebEditor : public AAX_CEffectGUI {
    static_assert(NativeSampleBits==32||NativeSampleBits==64);
public:
    using Controls=AaxEditorControls<MaximumParameters>;
    using Error=typename Controls::Error;
    // UI-only trusted codecs must be nonthrowing and keep HTML library-owned.
    // The exact schema/table origin remains a caller ownership obligation.
    struct Codec {
        const uint8_t*(*html)(size_t*);
        int32_t(*message)(const uint8_t*,size_t,uint32_t*,uint32_t*,double*);
        size_t(*snapshot)(const double*,uint32_t,uint32_t,uint8_t*,size_t);
    };
    // Implementations obey RavetWebEditor.h, including exactly-once callback
    // ownership on create failure and consumed handles on accepted destruction.
    struct NativeApi {
        decltype(&ravet_editor_create_html) create=ravet_editor_create_html;
        decltype(&ravet_editor_attach) attach=ravet_editor_attach;
        decltype(&ravet_editor_set_size) resize=ravet_editor_set_size;
        decltype(&ravet_editor_destroy) destroy=ravet_editor_destroy;
    };
    struct Diagnostics {
        typename Controls::Attempt modelQuery{},controllerQuery{},automationQuery{},viewQuery{},base{};
        typename Controls::Diagnostics controls{};
        std::array<typename Controls::ParameterDiagnostics,MaximumParameters> parameters{};
        uint32_t parameterCount=0;
        Error error=Error::NotReady;
        uint32_t native=0,contextsCreated=0,contextsReleased=0;
        bool nativeAttempted=false,retirementUnresolved=false;
    };
    using Observation=std::shared_ptr<Diagnostics>;
private:
    static bool main(){return pthread_main_np()!=0;}
    struct Context {
        Observation observation;
        NativeAaxParameters<MaximumParameters> schema;
        IACFUnknown* host;
        AAX_IEffectParameters* model;
        AAX_IACFViewContainer* view;
        AAX_VAutomationDelegate automation;
        Controls controls;
        Codec codec;
        bool entered=false;
        Context(Observation d,const RavetNativeCachedParameterEntryV1* cache,IACFUnknown* h,
                AAX_IEffectParameters* m,AAX_IACFViewContainer* v,Codec c):
            observation(std::move(d)),schema(cache,NativeSampleBits),host(h),model(m),view(v),automation(h),
            controls(schema,m,&automation,main),codec(c){host->AddRef();model->AddRef();view->AddRef();++observation->contextsCreated;observation->parameterCount=schema.count();}
        void publish(){
            const auto next=controls.diagnosticsMain();auto& priorControls=observation->controls;
            if(next.initialization.attempted)priorControls.initialization=next.initialization;
            if(next.begin.attempted)priorControls.begin=next.begin;
            if(next.value.attempted)priorControls.value=next.value;
            if(next.end.attempted)priorControls.end=next.end;
            if(next.read.attempted)priorControls.read=next.read;
            priorControls.active=priorControls.unresolved=0;
            for(uint32_t i=0;i<schema.count();++i){
                const auto source=controls.parameterDiagnosticsMain(schema.id(i));auto& prior=observation->parameters[i];
                if(source.begin.attempted)prior.begin=source.begin;
                if(source.value.attempted)prior.value=source.value;
                if(source.end.attempted)prior.end=source.end;
                if(prior.gesture!=Controls::Gesture::Unresolved||source.gesture!=Controls::Gesture::Idle)prior.gesture=source.gesture;
                priorControls.active+=prior.gesture==Controls::Gesture::Active;
                priorControls.unresolved+=prior.gesture==Controls::Gesture::Unresolved;
            }
        }
        ~Context(){
            (void)controls.retireMain();publish();if(observation->controls.unresolved)observation->retirementUnresolved=true;
            ++observation->contextsReleased;view->Release();model->Release();host->Release();
        }
        void close(){(void)controls.closeMain();}
        struct Entry {Context& context;explicit Entry(Context& c):context(c){context.entered=true;}~Entry(){context.entered=false;}};
        static void release(void* raw){delete static_cast<Context*>(raw);}
        static void message(void* raw,const uint8_t* bytes,size_t length) noexcept {
            auto& c=*static_cast<Context*>(raw);if(!main()||c.entered||c.controls.closing()||c.observation->retirementUnresolved)return;
            Entry entry(c);try {
                uint32_t kind=0,id=0;double value=0;
                if(c.codec.message(bytes,length,&kind,&id,&value)!=0){c.observation->error=Error::Invalid;return;}
                const char* key=nullptr;for(uint32_t i=0;i<c.schema.count();++i)if(c.schema.info(c.schema.id(i))->id==id){key=c.schema.id(i);break;}
                Error result=Error::Invalid;
                switch(kind){case 0:result=c.controls.edit(key,value);break;case 1:result=c.controls.begin(key);break;case 2:result=c.controls.set(key,value);break;case 3:result=c.controls.end(key);break;default:break;}
                c.observation->error=result;c.publish();
            }catch(...){c.observation->error=Error::Exception;}
        }
        static size_t snapshot(void* raw,uint8_t* output,size_t capacity) noexcept {
            auto& c=*static_cast<Context*>(raw);if(!main()||c.entered||c.controls.closing())return 0;
            Entry entry(c);try {
                typename Controls::Snapshot state{};const auto result=c.controls.snapshot(state);
                c.publish();if(result!=Error::Ok){c.observation->error=result;return 0;}
                const uint32_t status=c.observation->controls.unresolved||c.observation->retirementUnresolved?2:c.observation->error==Error::Ok?0:1;
                return c.codec.snapshot(state.values.data(),state.count,status,output,capacity);
            }catch(...){c.observation->error=Error::Exception;return 0;}
        }
    };
    const RavetNativeCachedParameterEntryV1* cache_;
    Codec codec_;
    NativeApi native_;
    double width_,height_;
    Observation observation_;
    IACFUnknown* host_=nullptr;
    AAX_IEffectParameters* model_=nullptr;
    RavetWebEditor* editor_=nullptr;
    Context* context_=nullptr; // native handle owns it until release callback
    bool initialized_=false,busy_=false;
    struct Scope{bool& busy;explicit Scope(bool& b):busy(b){busy=true;}~Scope(){busy=false;}};
    AAX_Result local(Error error){observation_->error=error;return error==Error::Ok?AAX_SUCCESS:AAX_ERROR_INVALID_ARGUMENT;}
    template<class F> AAX_Result sdk(typename Controls::Attempt& result,F&& operation){result.attempted=true;result.returned=false;try{result.status=operation();result.returned=true;return result.status;}catch(...){observation_->error=Error::Exception;return AAX_ERROR_INVALID_INTERNAL_DATA;}}
    AAX_Result destroyEditor(){
        if(!editor_)return AAX_SUCCESS;
        context_->close();observation_->nativeAttempted=true;observation_->native=native_.destroy(editor_);
        if(observation_->native==2)return local(Error::WrongThread);
        // Accepted destroy consumes the handle/context even if native teardown
        // reports failure. An entered callback keeps its Context until return.
        editor_=nullptr;context_=nullptr;
        return observation_->native?local(Error::Invalid):observation_->retirementUnresolved?local(Error::Unresolved):AAX_SUCCESS;
    }
protected:
    void CreateViewContents() override {}
    void CreateViewContainer() override {}
    void DeleteViewContainer() override {}
public:
    AaxWebEditor(const RavetNativeCachedParameterEntryV1* cache,Codec codec,double width,double height,
            Observation observation=std::make_shared<Diagnostics>(),NativeApi native={}):cache_(cache),codec_(codec),native_(native),width_(width),height_(height),observation_(observation?std::move(observation):std::make_shared<Diagnostics>()){}
    ~AaxWebEditor() override {if(!main())std::terminate();(void)Uninitialize();}
    // MAIN-only diagnostics and borrowed current public container. Retain an
    // acquired interface explicitly if needed beyond replacement/teardown.
    // AAX_CEffectGUI::GetViewContainer() intentionally remains null: consumers
    // use this owned accessor, not the stock VViewContainer workaround lifetime.
    Observation observationMain()const{return observation_;}
    AAX_IACFViewContainer* ownedViewContainerMain()const{return main()&&context_?context_->view:nullptr;}

    AAX_Result Initialize(IACFUnknown* host) override {
        if(!main())return AAX_ERROR_INVALID_ARGUMENT;if(busy_||observation_->contextsCreated!=observation_->contextsReleased)return local(Error::Busy);
        if(observation_->retirementUnresolved)return local(Error::Unresolved);
        if(initialized_||host_||!host||!cache_||!codec_.html||!codec_.message||!codec_.snapshot||!native_.create||!native_.attach||!native_.resize||!native_.destroy||
           !native::scalar::finite(width_)||!native::scalar::finite(height_)||width_<=0||height_<=0||width_>16384||height_>16384)return local(Error::Invalid);
        Scope scope(busy_);AAX_IEffectParameters* model=nullptr;
        const auto query=sdk(observation_->modelQuery,[&]{return host->QueryInterface(IID_IAAXEffectParametersV1,reinterpret_cast<void**>(&model));});
        if(query!=AAX_SUCCESS||!model){if(model)model->Release();return query!=AAX_SUCCESS?query:local(Error::NotReady);}
        AAX_IACFController* controller=nullptr;
        const auto controllerQuery=sdk(observation_->controllerQuery,[&]{return host->QueryInterface(IID_IAAXControllerV1,reinterpret_cast<void**>(&controller));});
        if(controller)controller->Release();
        if(controllerQuery!=0||!controller){model->Release();return controllerQuery!=0?controllerQuery:local(Error::NotReady);}
        AAX_IACFAutomationDelegate* automation=nullptr;
        const auto automationQuery=sdk(observation_->automationQuery,[&]{return host->QueryInterface(IID_IAAXAutomationDelegateV1,reinterpret_cast<void**>(&automation));});
        if(automation)automation->Release();
        if(automationQuery!=0||!automation){model->Release();return automationQuery!=0?automationQuery:local(Error::NotReady);}
        host->AddRef();host_=host;model_=model;
        const auto base=sdk(observation_->base,[&]{return AAX_CEffectGUI::Initialize(host);});
        if(base!=AAX_SUCCESS||!GetEffectParameters()){(void)AAX_CEffectGUI::Uninitialize();model_->Release();host_->Release();model_=nullptr;host_=nullptr;return base!=AAX_SUCCESS?base:local(Error::NotReady);}
        initialized_=true;return local(Error::Ok);
    }
    AAX_Result SetViewContainer(IACFUnknown* view) override {
        if(!main())return AAX_ERROR_INVALID_ARGUMENT;if(busy_||(context_&&context_->entered))return local(Error::Busy);
        if(!initialized_&&observation_->contextsCreated!=observation_->contextsReleased)return local(Error::Busy);
        if(!initialized_)return local(Error::NotReady);Scope scope(busy_);
        if(!view)return destroyEditor();
        if(observation_->retirementUnresolved)return local(Error::Unresolved);
        AAX_IACFViewContainer* borrower=nullptr;
        const auto query=sdk(observation_->viewQuery,[&]{return view->QueryInterface(IID_IAAXViewContainerV1,reinterpret_cast<void**>(&borrower));});
        if(query!=0||!borrower){if(borrower)borrower->Release();return query!=0?query:local(Error::Invalid);}
        std::unique_ptr<AAX_IACFViewContainer,void(*)(AAX_IACFViewContainer*)> borrowed(borrower,[](auto* p){p->Release();});
        void* parent=nullptr;try{if(borrower->GetType()!=AAX_eViewContainer_Type_NSView)return local(Error::Invalid);parent=borrower->GetPtr();}catch(...){return local(Error::Exception);}if(!parent)return local(Error::Invalid);
        Context* next=nullptr;try{next=new Context(observation_,cache_,host_,model_,borrower,codec_);}catch(...){return local(Error::Exception);}
        if(!next->controls.valid()){delete next;return local(Error::NotReady);}
        size_t length=0;const auto* html=codec_.html(&length);uint32_t status=0;
        RavetEditorCallbacks callbacks{next,Context::message,Context::snapshot,Context::release};
        observation_->nativeAttempted=true;auto* handle=native_.create(html,length,width_,height_,callbacks,&status);observation_->native=status;
        if(!handle)return local(Error::Invalid); // create consumed next, including failure
        const auto attach=native_.attach(handle,parent);observation_->native=attach;
        if(attach){next->close();(void)native_.destroy(handle);return local(Error::Invalid);}
        const auto previous=destroyEditor();if(previous!=AAX_SUCCESS){next->close();(void)native_.destroy(handle);return previous;}
        editor_=handle;context_=next;return local(Error::Ok);
    }
    AAX_Result Uninitialize() override {
        if(!main())return AAX_ERROR_INVALID_ARGUMENT;if(busy_)return local(Error::Busy);Scope scope(busy_);
        const auto result=destroyEditor();if(editor_)return result;
        // No stock VViewContainer is created: its SDK destructor intentionally
        // detaches queried references. Inherited GetViewContainer stays null;
        // owned view ownership is entirely in the independently retained Context.
        const auto base=sdk(observation_->base,[&]{return AAX_CEffectGUI::Uninitialize();});
        if(model_)model_->Release();if(host_)host_->Release();model_=nullptr;host_=nullptr;initialized_=false;
        return result!=AAX_SUCCESS?result:base;
    }
    AAX_Result GetViewSize(AAX_Point* output)const override {
        if(!main()||!output||!initialized_||!native::scalar::finite(width_)||!native::scalar::finite(height_)||
           width_<=0||height_<=0||width_>16384||height_>16384)return AAX_ERROR_INVALID_ARGUMENT;
        const auto begin=reinterpret_cast<uintptr_t>(output),owner=reinterpret_cast<uintptr_t>(this);
        if(begin%alignof(AAX_Point)||begin>UINTPTR_MAX-sizeof(*output)||
           (begin<owner+sizeof(*this)&&owner<begin+sizeof(*output)))return AAX_ERROR_INVALID_ARGUMENT;
        *output=AAX_Point(float(height_),float(width_));return AAX_SUCCESS;
    }
    // MAIN explicit native child sizing. Host parent geometry/routing remains
    // the host's responsibility; this makes no SetViewSize host request.
    AAX_Result resizeMain(double width,double height) {
        if(!main())return AAX_ERROR_INVALID_ARGUMENT;
        if(busy_||(context_&&context_->entered))return local(Error::Busy);
        if(!initialized_||!editor_)return local(Error::NotReady);
        if(!native::scalar::finite(width)||!native::scalar::finite(height)||width<=0||height<=0||width>16384||height>16384)return local(Error::Invalid);
        Scope scope(busy_);observation_->nativeAttempted=true;observation_->native=native_.resize(editor_,width,height);
        if(observation_->native)return local(Error::Invalid);
        width_=width;height_=height;return local(Error::Ok);
    }
    AAX_Result ParameterUpdated(AAX_CParamID id) override {
        if(!main())return AAX_ERROR_INVALID_ARGUMENT;if(busy_||(context_&&context_->entered))return local(Error::Busy);
        if(!initialized_||!id)return local(Error::NotReady);return AAX_SUCCESS;
    }
};
} // namespace ravet::aax
