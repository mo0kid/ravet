#include "RavetVst3CoreRegistry.hpp"
#include <iostream>
#include <thread>
#include <atomic>
using namespace ravet::vst3::draft;
namespace {
void require(bool value) { if (!value) throw std::runtime_error("registry contract"); }
struct Core { int* retired; std::string key; const std::string& identityKey()const{return key;} ~Core() { ++*retired; } };
}
int main() {
    try {
        using Registry = CoreRegistry<Core,2>;
        using R = Registry::Result;
        int destroyed = 0, firstIdentity = 0, secondIdentity = 0, thirdIdentity = 0;
        Registry registry;
        auto first = std::make_shared<Core>(); first->retired = &destroyed; first->key="canonical-first";
        auto second = std::make_shared<Core>(); second->retired = &destroyed;
        auto third = std::make_shared<Core>(); third->retired = &destroyed;
        require(registry.reserve(&firstIdentity,first) == R::Accepted);
        std::shared_ptr<Core> keyOutput;
        require(registry.reopenKey("canonical-first",true,keyOutput)==R::Busy&&!keyOutput);
        require(registry.reopenKey("unowned",true,keyOutput)==R::NotFound&&!keyOutput);
        require(registry.reserve(&firstIdentity,second) == R::Busy);
        require(registry.pin(&firstIdentity,second) == R::Invalid);
        require(registry.pin(&firstIdentity,first) == R::Accepted);
        bool pinned = false;
        require(registry.beginRetire(&firstIdentity,first,pinned) == R::Accepted && pinned);
        require(registry.reserve(&firstIdentity,second) == R::Busy);
        require(registry.pin(&firstIdentity,first) == R::Invalid);
        require(registry.beginRetire(&firstIdentity,first,pinned) == R::Invalid);
        require(registry.finishRetire(&firstIdentity,first) == R::Accepted);
        first.reset(); require(destroyed == 0);
        require(registry.reopenKey("canonical-first",false,keyOutput)==R::Busy&&!keyOutput);
        require(registry.reopenKey("canonical-first",true,keyOutput)==R::Accepted&&bool(keyOutput));
        std::shared_ptr<Core> keyRejected;
        require(registry.reopenKey("canonical-first",true,keyRejected)==R::Busy&&!keyRejected);
        require(registry.beginRetire(&firstIdentity,keyOutput,pinned)==R::Accepted&&pinned);
        require(registry.finishRetire(&firstIdentity,keyOutput)==R::Accepted);
        keyOutput.reset();require(destroyed==0);
        // Old ABI cannot overwrite a pinned factory context, even between facades.
        require(registry.reserve(&firstIdentity,third) == R::Busy);
        std::shared_ptr<Core> reopened;
        require(registry.reopen(&firstIdentity,reopened) == R::Accepted);
        std::shared_ptr<Core> rejected;
        require(registry.reopen(&firstIdentity,rejected) == R::Busy && !rejected);
        require(registry.reopen(&firstIdentity,reopened) == R::Invalid);
        require(registry.reserve(&secondIdentity,second) == R::Accepted);
        require(registry.reserve(&thirdIdentity,third) == R::CapacityFull);
        bool unchanged = true;
        require(registry.beginRetire(&secondIdentity,third,unchanged) == R::Invalid && unchanged);
        require(registry.finishRetire(&secondIdentity,second) == R::Invalid);
        require(registry.beginRetire(&secondIdentity,second,pinned) == R::Accepted && !pinned);
        // Mock native teardown may reenter bookkeeping; identity stays busy.
        require(registry.reserve(&secondIdentity,third) == R::Busy);
        require(registry.finishRetire(&secondIdentity,third) == R::Invalid);
        require(registry.finishRetire(&secondIdentity,second) == R::Accepted);
        second.reset(); require(destroyed == 1);
        require(registry.reserve(&thirdIdentity,third) == R::Accepted);
        require(registry.beginRetire(&thirdIdentity,third,pinned) == R::Accepted && !pinned);
        require(registry.finishRetire(&thirdIdentity,third) == R::Accepted);
        third.reset(); require(destroyed == 2);
        require(registry.beginRetire(&firstIdentity,reopened,pinned) == R::Accepted && pinned);
        require(registry.finishRetire(&firstIdentity,reopened) == R::Accepted);
        reopened.reset(); require(destroyed == 2);
        // Simultaneous bookkeeping callers can admit only one live facade.
        std::atomic<int> admitted{0};
        std::shared_ptr<Core> left,right;
        std::thread a([&]{ if(registry.reopen(&firstIdentity,left)==R::Accepted)++admitted; });
        std::thread b([&]{ if(registry.reopen(&firstIdentity,right)==R::Accepted)++admitted; });
        a.join();b.join();require(admitted == 1 && bool(left) != bool(right));
        auto& winner = left ? left : right;
        require(registry.beginRetire(&firstIdentity,winner,pinned)==R::Accepted && pinned);
        require(registry.finishRetire(&firstIdentity,winner)==R::Accepted);
        winner.reset();require(destroyed == 2);
        std::cout << "PASS bounded registry admission, exact retirement, pinned reopen and old-ABI exclusion\n";
        // Draft test registry teardown checks ownership only. Native process pins
        // require separate process-exit fixtures, not this stack destructor.
    } catch(const std::exception& error) { std::cerr<<error.what()<<'\n';return 1; }
}
