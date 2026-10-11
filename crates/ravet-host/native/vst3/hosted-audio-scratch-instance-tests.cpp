#include "processing-instance-fixture.hpp"
using ravet::vst3::hosted_audio::ScratchBudget;
namespace {
struct Owned {
    Observation state;Factory factory{state};ControllerHandlerKeepers handlers;
    std::shared_ptr<int> core=std::make_shared<int>(0);
    std::unique_ptr<Instance> owner;
    explicit Owned(ScratchBudget& budget) {
        owner=Instance::allocate(metadata(),0,core,handlers,&budget);
        owner->initialize(factory,*Steinberg::owned(new HostApplication),metadata(),0);
    }
    ~Owned(){if(owner){owner->retire();owner.reset();}handlers.reclaim();}
};
void fullCurrentPendingOwnershipDuringForeignCallback() {
    ScratchBudget budget;Owned f(budget);f.owner->prepare({48000,16,2,2});require(budget.bytes()==256);
    const auto calls=f.state.calls.size();bool localRefused=false;
    try{f.owner->prepare({0,8,1,1});}catch(const InstanceFailure& error){localRefused=error.stage==Stage::Profile;}
    require(localRefused&&budget.bytes()==256&&f.state.calls.size()==calls);
    unsigned observed=0;
    f.state.controlCall=[&](const char* phase){if(std::strcmp(phase,"setup"))return;
        require(budget.bytes()==320);unsigned current=0,pending=0;size_t ranges=0;
        require(f.owner->appendOwnedRanges([&](const void* p,size_t bytes,size_t alignment){
            ++ranges;if(!bytes)return true;
            if(alignment==alignof(float)&&(bytes==128||bytes==32)) {
                require(!f.owner->stateExternal(p,bytes,alignment));
                require(!f.owner->stateExternal(static_cast<const uint8_t*>(p)+bytes-1,1,1));
                bytes==128?++current:++pending;
            }return true;
        }));
        require(current==2&&pending==2&&ranges<=Instance::MaximumOwnedRanges);++observed;
    };
    f.owner->prepare({48000,8,1,1});require(observed==1&&budget.bytes()==64);
    f.state.controlCall=nullptr;f.owner->retire();f.owner.reset();f.handlers.reclaim();require(budget.bytes()==0);
}
void failedForeignSetupReleasesOnlyPendingAndKeepsCoreAccounting() {
    ScratchBudget budget;
    {Owned f(budget);f.owner->prepare({48000,16,2,2});f.state.fail="setup";
        bool returned=false;try{f.owner->prepare({48000,8,1,1});}catch(const InstanceFailure& error){returned=error.stage==Stage::Setup&&error.returned&&error.native==4242;}
        require(returned&&budget.bytes()==256);bool refused=false;try{f.owner->start();}catch(const InstanceFailure& error){refused=error.stage==Stage::Lifecycle;}require(refused);
    }require(budget.bytes()==0);
    {Owned first(budget),second(budget);first.owner->prepare({48000,16,2,2});second.owner->prepare({48000,8,1,1});require(budget.bytes()==320);}
    require(budget.bytes()==0);
}
}
int main(){try{fullCurrentPendingOwnershipDuringForeignCallback();failedForeignSetupReleasesOnlyPendingAndKeepsCoreAccounting();std::cout<<"2 hosted scratch instance groups passed\n";}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
