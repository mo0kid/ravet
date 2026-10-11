#include "ParameterPoolKeepers.hpp"
#include <iostream>
#include <stdexcept>
#include <cstdlib>
#include <new>
namespace {thread_local bool measured=false;thread_local uint64_t allocations=0,deallocations=0;}
void* operator new(std::size_t n){if(measured)++allocations;if(auto* p=std::malloc(n?n:1))return p;throw std::bad_alloc();}
void* operator new[](std::size_t n){return ::operator new(n);}
void operator delete(void* p)noexcept{if(p&&measured)++deallocations;std::free(p);}
void operator delete[](void* p)noexcept{::operator delete(p);}
void operator delete(void* p,std::size_t)noexcept{::operator delete(p);}
void operator delete[](void* p,std::size_t)noexcept{::operator delete(p);}
using namespace ravet::vst3::parameters_draft;
static void require(bool x){if(!x)throw std::runtime_error("core parameter pool contract");}
int main(){try{
    {
        ParameterPoolKeepers keepers;auto& pool=keepers.attach();require(keepers.count()==1&&keepers.bytes()==sizeof(Pool));
        require(!keepers.externalRange(&pool,sizeof(pool),alignof(Pool))&&!keepers.externalRange(&keepers,1,1));
        int external=0;require(keepers.externalRange(&external,sizeof(external),alignof(int)));
        keepers.reclaim();require(keepers.count()==1);
        Batch* b=nullptr;Point p{3,0,.4};require(pool.prepare(1,&p,1,b)==Admission::Ok);
        bool rejected=false;try{keepers.detach(pool);}catch(const std::logic_error&){rejected=true;}require(rejected);
        require(b->freeze()==0);keepers.detach(pool);keepers.reclaim();require(keepers.count()==0);
    }
    {
        ParameterPoolKeepers keepers;auto& pool=keepers.attach();Batch* b=nullptr;Point p{3,0,.4};
        require(pool.prepare(1,&p,1,b)==Admission::Ok);auto* q=b->getParameterData(0);require(q&&q->addRef()==2);
        require(b->freeze()==0&&pool.observedRetention());keepers.detach(pool);keepers.reclaim();require(keepers.count()==1);
        bool pass=false;std::thread worker([&]{measured=true;allocations=deallocations=0;
            int32_t offset=77;double value=88;require(q->getPoint(0,offset,value)==Steinberg::kInvalidArgument&&offset==77&&value==88);
            require(q->getParameterId()==Steinberg::Vst::kNoParamId&&q->release()==1);measured=false;
            pass=allocations==0&&deallocations==0;
        });worker.join();require(pass&&keepers.observedRetention());
        // Parent must pin before this reclaim: prior retention remains observed
        // even though this worker has already released the last foreign lease.
        keepers.reclaim();require(keepers.count()==0);
    }
    {
        ParameterPoolKeepers keepers;std::array<Pool*,16> pools{};
        for(auto& p:pools)p=&keepers.attach();require(keepers.count()==16&&keepers.bytes()<=ParameterPoolKeepers::MaximumBytes);
        bool rejected=false;try{(void)keepers.attach();}catch(const std::length_error&){rejected=true;}require(rejected&&keepers.count()==16);
        bool wrong=false;std::thread worker([&]{try{(void)keepers.attach();}catch(const std::logic_error&){wrong=true;}});worker.join();require(wrong);
        for(auto* p:pools)keepers.detach(*p);keepers.reclaim();require(keepers.count()==0);
    }
    std::cout<<"PASS core parameter pool keepers:3 groups, CONTROL capacity/detach/reclaim, retained child lifetime, zero worker-release new/delete\n";
}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
