#include "RavetAaxFactoryRegistration.hpp"
#include "AAX_IComponentDescriptor.h"
#include "AAX_IPropertyMap.h"
#include "public.sdk/source/vst/aaxwrapper/aaxwrapper_parameters.h"
#include "public.sdk/source/vst/aaxwrapper/aaxwrapper.h"
#include "AAX_CString.h"
#include <array>
#include <cmath>
#include <dlfcn.h>
#include <iostream>
#include <map>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>
namespace {
void require(bool yes, const char *message) { if (!yes) throw std::runtime_error(message); }
struct Properties final : AAX_IPropertyMap {
 std::map<AAX_EProperty,AAX_CPropertyValue> values; std::map<AAX_EProperty,const void*> pointers;
 AAX_CBoolean GetProperty(AAX_EProperty p,AAX_CPropertyValue *v) const override { const auto i=values.find(p); if(i==values.end()||!v)return false; *v=i->second;return true; }
 AAX_CBoolean GetPointerProperty(AAX_EProperty p,const void **v) const override { const auto i=pointers.find(p);if(i==pointers.end()||!v)return false;*v=i->second;return true; }
 AAX_Result AddProperty(AAX_EProperty p,AAX_CPropertyValue v) override {values[p]=v;return AAX_SUCCESS;}
 AAX_Result AddPointerProperty(AAX_EProperty p,const void *v) override {pointers[p]=v;return AAX_SUCCESS;}
 AAX_Result AddPointerProperty(AAX_EProperty p,const char *v) override {return AddPointerProperty(p,static_cast<const void*>(v));}
 AAX_Result RemoveProperty(AAX_EProperty p) override {values.erase(p);pointers.erase(p);return AAX_SUCCESS;}
 AAX_Result AddPropertyWithIDArray(AAX_EProperty,const AAX_SPlugInIdentifierTriad*,uint32_t) override {return AAX_ERROR_UNIMPLEMENTED;}
 AAX_CBoolean GetPropertyWithIDArray(AAX_EProperty,const AAX_SPlugInIdentifierTriad**,uint32_t*)const override{return false;}
 IACFUnknown *GetIUnknown()override{return nullptr;}
};
struct Component final : AAX_IComponentDescriptor {
 mutable Properties properties; AAX_CProcessProc process=nullptr; AAX_CInstanceInitProc init=nullptr;
 int audioIn=-1,audioOut=-1,frames=-1,privateField=-1,privateSize=0;
 AAX_Result Clear()override{return AAX_SUCCESS;}
 AAX_Result AddAudioIn(AAX_CFieldIndex p)override{audioIn=p;return AAX_SUCCESS;}
 AAX_Result AddAudioOut(AAX_CFieldIndex p)override{audioOut=p;return AAX_SUCCESS;}
 AAX_Result AddAudioBufferLength(AAX_CFieldIndex p)override{frames=p;return AAX_SUCCESS;}
 AAX_Result AddSampleRate(AAX_CFieldIndex)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddClock(AAX_CFieldIndex)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddSideChainIn(AAX_CFieldIndex)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddDataInPort(AAX_CFieldIndex,uint32_t,AAX_EDataInPortType)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddAuxOutputStem(AAX_CFieldIndex,int32_t,const char[])override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddPrivateData(AAX_CFieldIndex p,int32_t n,uint32_t)override{privateField=p;privateSize=n;return AAX_SUCCESS;}
 AAX_Result AddTemporaryData(AAX_CFieldIndex,uint32_t)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddDmaInstance(AAX_CFieldIndex,AAX_IDma::EMode)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddMeters(AAX_CFieldIndex,const AAX_CTypeID*,uint32_t)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddMIDINode(AAX_CFieldIndex,AAX_EMIDINodeType,const char[],uint32_t)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddReservedField(AAX_CFieldIndex,uint32_t)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_IPropertyMap *NewPropertyMap()const override{return &properties;}
 AAX_IPropertyMap *DuplicatePropertyMap(AAX_IPropertyMap*)const override{return nullptr;}
 AAX_Result AddProcessProc_Native(AAX_CProcessProc p,AAX_IPropertyMap*,AAX_CInstanceInitProc i,AAX_CBackgroundProc,AAX_CSelector*)override{process=p;init=i;return AAX_SUCCESS;}
 AAX_Result AddProcessProc_TI(const char[],const char[],AAX_IPropertyMap*,const char[],const char[],AAX_CSelector*)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result AddProcessProc(AAX_IPropertyMap*,AAX_CSelector*,int32_t)override{return AAX_ERROR_UNIMPLEMENTED;}
};
struct Descriptor final : AAX_IEffectDescriptor {
 Component component; Properties properties; void *create=nullptr; int createCalls=0; AAX_Result createStatus=AAX_SUCCESS;
 AAX_IComponentDescriptor *NewComponentDescriptor()override{return &component;}
 AAX_Result AddComponent(AAX_IComponentDescriptor *p)override{return p==&component?AAX_SUCCESS:AAX_ERROR_INVALID_ARGUMENT;}
 AAX_Result AddName(const char*)override{return AAX_SUCCESS;}
 AAX_Result AddCategory(uint32_t)override{return AAX_SUCCESS;}
 AAX_Result AddCategoryBypassParameter(uint32_t,AAX_CParamID)override{return AAX_SUCCESS;}
 AAX_Result AddProcPtr(void *p,AAX_CProcPtrID kind)override{if(kind==kAAX_ProcPtrID_Create_EffectParameters){++createCalls;if(createStatus)return createStatus;create=p;}return AAX_SUCCESS;}
 AAX_IPropertyMap *NewPropertyMap()override{return &properties;}
 AAX_Result SetProperties(AAX_IPropertyMap*)override{return AAX_SUCCESS;}
 AAX_Result AddResourceInfo(AAX_EResourceType,const char*)override{return AAX_SUCCESS;}
 AAX_Result AddMeterDescription(AAX_CTypeID,const char*,AAX_IPropertyMap*)override{return AAX_SUCCESS;}
 AAX_Result AddControlMIDINode(AAX_CTypeID,AAX_EMIDINodeType,const char[],uint32_t)override{return AAX_SUCCESS;}
};
struct Collection final : AAX_ICollection {
 std::array<Descriptor,4> descriptors;size_t used=0;std::vector<std::pair<std::string,AAX_IEffectDescriptor*>> effects;Properties properties; AAX_Result effectStatus=AAX_SUCCESS;
 AAX_IEffectDescriptor *NewDescriptor()override{return used<descriptors.size()?&descriptors[used++]:nullptr;}
 AAX_Result AddEffect(const char *id,AAX_IEffectDescriptor *p)override{effects.emplace_back(id,p);return effectStatus;}
 AAX_Result SetManufacturerName(const char*)override{return AAX_SUCCESS;}
 AAX_Result AddPackageName(const char*)override{return AAX_SUCCESS;}
 AAX_Result SetPackageVersion(uint32_t)override{return AAX_SUCCESS;}
 AAX_IPropertyMap *NewPropertyMap()override{return &properties;}
 AAX_Result SetProperties(AAX_IPropertyMap*)override{return AAX_SUCCESS;}
 AAX_Result GetHostVersion(uint32_t *p)const override{if(p)*p=0;return AAX_SUCCESS;}
 AAX_IDescriptionHost *DescriptionHost()override{return nullptr;}
 const AAX_IDescriptionHost *DescriptionHost()const override{return nullptr;}
 IACFDefinition *HostDefinition()const override{return nullptr;}
};
void registrationContracts() {
 int first=1,second=2,original=3;
 const ravet::aax::NativeAaxParameterFactory profiles[]={{"mono",&first},{"stereo",&second}};
 Collection host; ravet::aax::NativeAaxFactoryRegistration<2> owner(host,profiles,2);
 auto *a=owner.NewDescriptor(),*b=owner.NewDescriptor(); require(a&&b,"proxy descriptors");
 require(a->AddProcPtr(&original,kAAX_ProcPtrID_Create_EffectParameters)==AAX_SUCCESS&&b->AddProcPtr(&original,kAAX_ProcPtrID_Create_EffectParameters)==AAX_SUCCESS,"defer callbacks");
 require(!host.descriptors[0].createCalls&&!host.descriptors[1].createCalls,"no callback host mutation before exact identity");
 require(owner.AddEffect("stereo",a)==AAX_SUCCESS&&owner.AddEffect("mono",b)==AAX_SUCCESS,"reverse descriptor identity registration");
 require(host.descriptors[0].create==&second&&host.descriptors[1].create==&first&&host.effects[0].second==&host.descriptors[0],"exact effect ID substitutes callback and unwraps borrowed descriptor");
 auto *c=owner.NewDescriptor(); require(c->AddProcPtr(&original,kAAX_ProcPtrID_Create_EffectParameters)==AAX_SUCCESS&&owner.AddEffect("foreign",c)==AAX_SUCCESS&&host.descriptors[2].create==&original,"unknown identity original callback unchanged");
 auto *d=owner.NewDescriptor(); require(d->AddProcPtr(&original,kAAX_ProcPtrID_Create_EffectParameters)==AAX_SUCCESS,"stage host refusal");host.descriptors[3].createStatus=AAX_ERROR_UNIMPLEMENTED;
 require(owner.AddEffect("mono",d)==AAX_ERROR_UNIMPLEMENTED&&host.effects.size()==3,"exact original host refusal before AddEffect preserves earlier effects");
 require(owner.AddEffect("mono",d)==AAX_ERROR_INVALID_ARGUMENT&&host.descriptors[3].createCalls==1,"refusal consumes readiness and cannot retry");
}
void componentContracts(const char *path) {
 void *module=dlopen(path,RTLD_NOW|RTLD_LOCAL); if(!module)throw std::runtime_error(dlerror());
 using Describe=AAX_Result(*)(AAX_ICollection*);auto describe=reinterpret_cast<Describe>(dlsym(module,"_Z21GetEffectDescriptionsP15AAX_ICollection"));require(describe,"actual owned registered SDK entry exported");
 Collection firstRefusal;firstRefusal.descriptors[0].createStatus=AAX_ERROR_UNIMPLEMENTED;
 require(describe(&firstRefusal)==AAX_SUCCESS&&firstRefusal.effects.size()==1&&firstRefusal.effects[0].first=="org.ravet.gain.stereo","retained SDK aggregate last-variant status may mask earlier refused registration");
 Collection lastRefusal;lastRefusal.descriptors[1].createStatus=AAX_ERROR_UNIMPLEMENTED;
 require(describe(&lastRefusal)==AAX_ERROR_UNIMPLEMENTED&&lastRefusal.effects.size()==1&&lastRefusal.effects[0].first=="org.ravet.gain.mono","actual final host refusal status preserved and earlier registered effect retained");
 Collection host;require(describe(&host)==AAX_SUCCESS&&host.effects.size()==2,"actual module SDK descriptor registration");
 for(size_t variant=0;variant<2;++variant){
  auto &descriptor=host.descriptors[variant];auto &component=descriptor.component;const int channels=int(variant+1);
  require(host.effects[variant].first==(variant?"org.ravet.gain.stereo":"org.ravet.gain.mono")&&descriptor.create&&descriptor.createCalls==1,"public exact mono/stereo factory registration");
  require(component.audioIn==2&&component.audioOut==3&&component.frames==1&&component.privateField==0&&component.privateSize==sizeof(void*)&&component.process,"actual registered public Gain context services and algorithm");
  using Create=AAX_IEffectParameters*(AAX_CALLBACK*)();std::unique_ptr<AAX_IEffectParameters> parameters(reinterpret_cast<Create>(descriptor.create)());require(bool(parameters),"actual registered factory instance");
  void *wrapper=nullptr;
  std::array<std::array<float,1024>,2> input{},output{};
  float *ins[2]={input[0].data(),input[1].data()},*outs[2]={output[0].data(),output[1].data()};
  for(int ch=0;ch<channels;++ch)input[ch].fill(float(ch+1));
  int32_t frames=1024;void *context[4]={&wrapper,&frames,ins,outs};void *contexts[]={context};
  // The owned factory's public inheritance is AAXWrapper_Parameters. No private
  // layout is inspected; the public virtual EffectInit and ResetFieldData run.
  auto *model=static_cast<AAXWrapper_Parameters*>(parameters.get());require(model->EffectInit()==AAX_SUCCESS,"actual native consumer EffectInit");

  require(parameters->ResetFieldData(0,&wrapper,sizeof(wrapper))==AAX_SUCCESS&&wrapper,"ordinary realtime public context initialization");
  for(int cycle=0;cycle<16;++cycle)component.process(contexts,contexts+1);
  for(int ch=0;ch<channels;++ch)require(output[ch][1023]==(variant?float(ch+1):0.f),"stock ordinary realtime initialization: stereo ready, mono rejected before later setup");
  // This is an explicitly retained stock lifecycle gap: initial setup precedes
  // descriptor mono bus arrangement. A public host offline notification forces
  // new preparation. No private layout or protected setup shortcut is used.
  require(parameters->NotificationReceived(AAX_eNotificationEvent_EnteringOfflineMode,nullptr,0)==AAX_SUCCESS,"synthetic host public offline setup after descriptor bus selection");
  require(model->EffectInit()==AAX_ERROR_INVALID_ARGUMENT,"repeated known EffectInit rejects without SDK manager mutation");
  int32_t count=-1;require(parameters->GetNumberOfParameters(&count)==AAX_SUCCESS&&count==2,"owned schema installed into actual data model");
  AAX_CString text;require(parameters->GetMasterBypassParameter(&text)==AAX_SUCCESS&&text=="p2","actual native master bypass identity");
  AAX_EParameterType type=AAX_eParameterType_Continuous;require(parameters->GetParameterType("p2",&type)==AAX_SUCCESS&&type==AAX_eParameterType_Discrete,"actual native SDK bypass object adoption");
  require(parameters->GetParameterStringFromValue("p1",.125,&text,64)==AAX_SUCCESS&&text=="0.5","actual callback physical native format differs from stock normalized title decorator");
  double previous=77.;require(parameters->GetParameterNormalizedValue("p01",&previous)==AAX_ERROR_INVALID_PARAMETER_ID&&previous==77.,"actual strict identity preflight");
  require(parameters->UpdateParameterNormalizedValue("p1",.125,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS,"actual accepted Gain control queue");
  for(const char *id:{"p01","p1tail","Byp"})require(parameters->UpdateParameterNormalizedValue(id,.75,AAX_eUpdateSource_Unspecified)==AAX_ERROR_INVALID_PARAMETER_ID,"actual malformed control identity rejection before queue");
  uint64_t nanbits=UINT64_C(0x7ff8000000000001);double nan;std::memcpy(&nan,&nanbits,8);require(parameters->UpdateParameterNormalizedValue("p1",nan,AAX_eUpdateSource_Unspecified)==AAX_ERROR_INVALID_ARGUMENT,"actual nonfinite control rejection before queue");
  for(int cycle=0;cycle<16;++cycle)component.process(contexts,contexts+1);
  for(int ch=0;ch<channels;++ch)require(std::abs(output[ch][1023]-.5f*float(ch+1))<1e-6f,"actual registered algorithm processes native Gain control with mono/stereo identity");
  require(parameters->UpdateParameterNormalizedValue("p2",1.,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS,"actual accepted native bypass");component.process(contexts,contexts+1);for(int ch=0;ch<channels;++ch)require(output[ch][1023]==float(ch+1),"actual registered native bypass processing");
  require(parameters->NotificationReceived(AAX_eNotificationEvent_ExitingOfflineMode,nullptr,0)==AAX_SUCCESS,"public host notification prepares realtime from accepted offline profile");
  require(parameters->UpdateParameterNormalizedValue("p2",0.,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS,"realtime native bypass release");
  for(int cycle=0;cycle<16;++cycle)component.process(contexts,contexts+1);
  for(int ch=0;ch<channels;++ch)require(std::abs(output[ch][1023]-.5f*float(ch+1))<1e-6f,"mono/stereo positive realtime after public notification preparation");
  require(parameters->Uninitialize()==AAX_SUCCESS,"actual SDK manager uninitialization");
  int32_t steps=77;require(parameters->GetParameterNumberOfSteps("p1",&steps)==AAX_ERROR_INVALID_PARAMETER_ID&&steps==77,"retired SDK manager invalidates borrowed parameter callbacks before dereference");
  require(model->EffectInit()==AAX_SUCCESS&&parameters->GetNumberOfParameters(&count)==AAX_SUCCESS&&count==2,"uninitialized native parameter objects can be adopted afresh");
  require(parameters->Uninitialize()==AAX_SUCCESS,"readopted SDK manager cleanup");
 }
 require(dlclose(module)==0,"module retirement after borrowed callbacks and data models");
}
}
int main(int argc,char **argv){try{require(argc==2,"module binary required");registrationContracts();componentContracts(argv[1]);std::cout<<"PASS actual AAX registered native parameter factories, schema/control preflight and mono/stereo Gain algorithm processing contracts\n";return 0;}catch(const std::exception &error){std::cerr<<error.what()<<'\n';return 1;}}
