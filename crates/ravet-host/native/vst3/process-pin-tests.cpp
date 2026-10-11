#include "RavetVst3Provider.h"
#include "public.sdk/source/vst/hosting/module.h"
#include "pluginterfaces/vst/ivsthostapplication.h"
#include <cstring>
#include <filesystem>
#include <iostream>
#include <map>
#include <stdexcept>
#include <string>
#include <unistd.h>
using namespace Steinberg;
namespace {
void checked(bool x,int line){if(!x)throw std::runtime_error("process pin/reopen protocol line "+std::to_string(line));}
#define require(value) checked((value),__LINE__)
struct Observation {
 int mode=0,refs=1,entries=0,exits=0,installed=0,cleared=0,creates=0;
 FUnknown* host=nullptr;RavetVst3Module* parent=nullptr;
 void probe(){require(host);void* out=nullptr;require(host->queryInterface(Vst::IHostApplication::iid,&out)==kResultOk&&out);static_cast<Vst::IHostApplication*>(out)->release();}
};
struct Factory final:IPluginFactory3 {
 Observation& s;explicit Factory(Observation& state):s(state){}
 tresult PLUGIN_API queryInterface(const TUID id,void** out)override{*out=nullptr;
  if(FUnknownPrivate::iidEqual(id,IPluginFactory3::iid)||FUnknownPrivate::iidEqual(id,IPluginFactory2::iid)||FUnknownPrivate::iidEqual(id,IPluginFactory::iid)||FUnknownPrivate::iidEqual(id,FUnknown::iid)){*out=static_cast<IPluginFactory3*>(this);addRef();return kResultOk;}return kNoInterface;}
 uint32 PLUGIN_API addRef()override{return ++s.refs;}
 uint32 PLUGIN_API release()override{return --s.refs;}
 tresult PLUGIN_API getFactoryInfo(PFactoryInfo* out)override{*out={};std::strcpy(out->vendor,"RAVET");out->flags=s.mode?PFactoryInfo::kComponentNonDiscardable:0;return kResultOk;}
 int32 PLUGIN_API countClasses()override{return 1;}
 tresult PLUGIN_API getClassInfo(int32,PClassInfo* out)override{*out={};out->cid[0]=1;out->cardinality=1;std::strcpy(out->category,"Audio Module Class");std::strcpy(out->name,"Pin fixture");return kResultOk;}
 tresult PLUGIN_API getClassInfo2(int32,PClassInfo2*)override{return kNotImplemented;}
 tresult PLUGIN_API getClassInfoUnicode(int32,PClassInfoW* out)override{*out={};out->cid[0]=1;out->cardinality=1;std::strcpy(out->category,"Audio Module Class");out->name[0]='x';return kResultOk;}
 tresult PLUGIN_API createInstance(FIDString,FIDString,void** out)override{
  ++s.creates;*out=nullptr;s.probe();int32_t native=999;
  require(ravet_vst3_provider_v2()->close(s.parent,&native)==RAVET_VST3_LIFECYCLE&&native==999);
  RavetVst3Instance* child=nullptr;require(ravet_vst3_provider_v2()->create(s.parent,0,&child,&native)==RAVET_VST3_LIFECYCLE&&!child&&native==999);
  if(s.mode==2)throw std::runtime_error("first foreign create throws");return 4242;
 }
 tresult PLUGIN_API setHostContext(FUnknown* host)override{
  if(host){require(!s.host);s.host=host;host->addRef();++s.installed;s.probe();}
  else{++s.cleared;s.probe();s.host->release();s.host=nullptr;}return kNotImplemented;
 }
};
std::map<std::string,Factory*> factories;
struct Module final:VST3::Hosting::Module {
 Observation& s;
 explicit Module(Factory& f):s(f.s){++s.entries;f.addRef();factory=VST3::Hosting::PluginFactory(owned<IPluginFactory>(&f));}
 ~Module()noexcept override{factory=VST3::Hosting::PluginFactory(nullptr);++s.exits;}
 bool load(const std::string&,std::string&)override{return true;}
};
}
namespace VST3::Hosting {
Module::Ptr Module::create(const std::string& path,std::string&){const auto key=std::filesystem::canonical(path).string();return std::make_shared<::Module>(*factories.at(key));}
}
int main(){try{
 char dir[]="/tmp/ravet-vst3-process-pin-XXXXXX";require(mkdtemp(dir));auto root=std::filesystem::path(dir);const auto* api=ravet_vst3_provider_v2();
 for(int mode=0;mode<3;++mode){
  // Pinned state deliberately has process lifetime, matching the native pin.
  auto* s=new Observation;s->mode=mode;auto* f=new Factory(*s);
  const auto path=root/(std::to_string(mode)+".vst3"),alias=root/("alias"+std::to_string(mode)+".vst3");std::filesystem::create_directory(path);std::filesystem::create_directory_symlink(path,alias);factories[std::filesystem::canonical(path).string()]=f;
  const auto name=path.string(),other=alias.string();int32_t native=999;
  require(api->open(reinterpret_cast<const uint8_t*>(name.data()),uint32_t(name.size()),&s->parent,&native)==RAVET_VST3_OK);
  RavetVst3Module* duplicate=nullptr;require(api->open(reinterpret_cast<const uint8_t*>(other.data()),uint32_t(other.size()),&duplicate,&native)==RAVET_VST3_CAPACITY&&!duplicate&&s->entries==1&&s->exits==0);
  RavetVst3Instance* child=nullptr;require(api->create(s->parent,0,&child,&native)==RAVET_VST3_NATIVE&&!child&&native==(mode==2?kInternalError:4242));
  require(api->close(s->parent,&native)==(mode?RAVET_VST3_PROCESS_PINNED:RAVET_VST3_OK));s->parent=nullptr;
  if(!mode){require(s->entries==1&&s->exits==1&&s->cleared==1&&!s->host&&s->refs==1);delete f;delete s;}
  else{
   require(s->entries==1&&s->exits==0&&s->cleared==0&&s->installed==1&&s->host&&s->refs==3);s->probe();
   require(ravet_vst3_provider_v1()->open(reinterpret_cast<const uint8_t*>(other.data()),uint32_t(other.size()),&duplicate,&native)==RAVET_VST3_CAPACITY&&!duplicate&&s->entries==1);
   require(api->open(reinterpret_cast<const uint8_t*>(other.data()),uint32_t(other.size()),&s->parent,&native)==RAVET_VST3_OK&&s->entries==1&&s->installed==1);
   require(api->create(s->parent,0,&child,&native)==RAVET_VST3_NATIVE&&!child&&s->creates==2);require(api->close(s->parent,&native)==RAVET_VST3_PROCESS_PINNED);s->parent=nullptr;
   require(s->entries==1&&s->exits==0&&s->cleared==0&&s->host&&s->refs==3);s->probe();
  }
 }
 std::filesystem::remove_all(root);std::cout<<"PASS non-discardable process retention before failed/throwing first create, observable consuming close, canonical alias no repeated entry, v2 same-context reopen, v1 exclusion and CONTROL reentry rejection\n";
}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
