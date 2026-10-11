#include "BoundedStateStream.hpp"
#include <iostream>
#include <string>
using namespace Steinberg;
using namespace ravet::vst3::state_draft;
namespace {
void require(bool value,const char* message){if(!value)throw std::runtime_error(message);}
void captureAndBounds() {
    StateStreamKeepers keepers;
    StateStreamKeepers::Spec spec{16,true,nullptr,0};
    auto* stream=keepers.reserve(&spec,1)[0];
    std::array<uint8_t,4> first{1,2,3,4};int32 count=-88;int64 position=-99;
    require(stream->write(first.data(),4,&count)==kResultOk&&count==4,"first write");
    require(stream->seek(8,IBStream::kIBSeekSet,&position)==kResultOk&&position==8,"seek gap");
    require(stream->write(first.data(),4,&count)==kResultOk&&count==4,"gap write");
    require(stream->seek(-10,IBStream::kIBSeekCur,&position)==kResultOk&&position==2,"relative seek");
    uint8_t byte=9;require(stream->write(&byte,1,nullptr)==kResultOk,"overwrite");
    require(stream->seek(0,IBStream::kIBSeekSet,nullptr)==kResultOk,"seek zero");
    std::array<uint8_t,20> output;output.fill(0xe7);
    require(stream->read(output.data(),20,&count)==kResultOk&&count==12,"partial EOF");
    const std::array<uint8_t,12> expected{1,2,9,4,0,0,0,0,1,2,3,4};
    require(std::equal(expected.begin(),expected.end(),output.begin()),"exact gap bytes");
    for(size_t i=12;i<output.size();++i)require(output[i]==0xe7,"read backing tails");
    require(stream->read(output.data(),20,&count)==kResultOk&&count==0,"EOF zero read");
    require(stream->seek(0,IBStream::kIBSeekEnd,&position)==kResultOk&&position==12,"end logical size");
    for(auto offset:{std::numeric_limits<int64>::min(),std::numeric_limits<int64>::max(),int64(-13),int64(5)}) {
        position=-99;
        require(stream->seek(offset,IBStream::kIBSeekCur,&position)==kInvalidArgument&&position==-99,"atomic seek rejection");
        require(stream->tell(&position)==kResultOk&&position==12,"cursor retained");
    }
    position=-99;require(stream->seek(0,99,&position)==kInvalidArgument&&position==-99,"unknown seek mode");
    count=-88;require(stream->write(first.data(),5,&count)==kInvalidArgument&&count==-88,"whole overflowing write");
    require(stream->read(output.data(),-1,&count)==kInvalidArgument&&count==-88,"negative read count");
    require(stream->failure()!=kResultOk&&stream->failureKind()==1,"sticky first protocol fault");
    stream->freeze();require(stream->size()==12,"frozen capture length");
    require(std::equal(expected.begin(),expected.end(),stream->bytes()),"capture preserved");
    position=-99;count=-88;output.fill(0xe7);
    require(stream->seek(0,IBStream::kIBSeekSet,&position)==kInvalidArgument&&position==-99,"frozen seek");
    require(stream->read(output.data(),4,&count)==kInvalidArgument&&count==-88&&output[0]==0xe7,"frozen read");
    keepers.reclaim();require(keepers.retainedBytes()==0,"capture CONTROL reclamation");
}
void modeThreadAndRetention() {
    StateStreamKeepers keepers;std::array<uint8_t,4> source{10,20,30,40};
    StateStreamKeepers::Spec spec{4,false,source.data(),source.size()};
    auto* stream=keepers.reserve(&spec,1)[0];
    source.fill(0); // stream owns a copy, never the foreign source range
    int32 count=-1;std::array<uint8_t,8> output;output.fill(0xe7);
    require(stream->write(source.data(),4,&count)==kInvalidArgument&&count==-1&&stream->failureKind()==3,"source write refusal");
    require(stream->read(output.data(),8,&count)==kResultOk&&count==4&&output[0]==10&&output[3]==40&&output[4]==0xe7,"owned source bytes");
    void* retained=nullptr;require(stream->queryInterface(IBStream::iid,&retained)==kResultOk&&retained&&stream->references()==2,"owned queried stream");
    require(keepers.hasForeignReferences(),"keeper detects borrowed SDK owner");
    const auto sticky=stream->failureKind();
    std::thread worker([&]{int64 position=-7;require(stream->tell(&position)==kInvalidArgument&&position==-7,"wrong thread leaves output");});worker.join();
    require(stream->failureKind()==sticky,"original first source failure retained");
    stream->freeze();keepers.reclaim();require(keepers.retainedBytes()==4,"retained payload not reclaimed");
    auto* sdkOwner=static_cast<IBStream*>(retained);
    std::thread late([&]{int32 n=-4;require(sdkOwner->read(output.data(),2,&n)==kInvalidArgument&&n==-4,"retained frozen read");require(sdkOwner->release()==1,"AUDIO release leaves keeper reference");});late.join();
    require(!keepers.hasForeignReferences()&&keepers.retainedBytes()==4,"no AUDIO final payload free");
    keepers.reclaim();require(keepers.retainedBytes()==0,"payload freed only on CONTROL");
    StateStreamKeepers::Spec zero{1,false,nullptr,0};auto* empty=keepers.reserve(&zero,1)[0];
    count=-1;require(empty->read(nullptr,0,&count)==kResultOk&&count==0,"zero-byte opaque source");empty->freeze();keepers.reclaim();
}
void capacityAndKeeperBudgets() {
    StateStreamKeepers keepers;StateStreamKeepers::Spec small{4,true,nullptr,0};
    auto* stream=keepers.reserve(&small,1)[0];std::array<uint8_t,5> input{};int32 n=77;
    require(stream->write(input.data(),5,&n)==kInvalidArgument&&n==77&&stream->failureKind()==4,"sticky capacity fault");
    stream->freeze();keepers.reclaim();
    std::array<BoundedStateStream*,16> retained{};
    for(auto& value:retained){value=keepers.reserve(&small,1)[0];value->addRef();value->freeze();}
    bool refused=false;try{(void)keepers.reserve(&small,1);}catch(const std::length_error&){refused=true;}
    require(refused&&keepers.retainedBytes()==64,"sixteen keeper slots bounded");
    for(auto* value:retained)require(value->release()==1,"release retained slot");keepers.reclaim();
    StateStreamKeepers::Spec maximum{BoundedStateStream::MaximumBytes,true,nullptr,0};
    std::array<BoundedStateStream*,8> large{};
    for(auto& value:large){value=keepers.reserve(&maximum,1)[0];value->addRef();value->freeze();}
    refused=false;try{(void)keepers.reserve(&small,1);}catch(const std::length_error&){refused=true;}
    require(refused&&keepers.retainedBytes()==StateStreamKeepers::MaximumBytes,"64MiB keeper bytes bounded");
    for(auto* value:large)value->release();keepers.reclaim();
    require(keepers.retainedBytes()==0,"maximum bytes reclaimed CONTROL");
    StateStreamKeepers::Spec invalid{0,true,nullptr,0};refused=false;
    try{(void)keepers.reserve(&invalid,1);}catch(const std::invalid_argument&){refused=true;}
    require(refused&&keepers.retainedBytes()==0,"invalid batch before publication");
}
void queryInterfaceRanges() {
    StateStreamKeepers keepers;StateStreamKeepers::Spec spec{32,true,nullptr,0};
    auto* stream=keepers.reserve(&spec,1)[0];
    std::array<uint8_t,32> payload{};payload.fill(0x6d);
    require(stream->write(payload.data(),int32(payload.size()))==kResultOk,"QI setup payload");
    std::array<uint8_t,sizeof(BoundedStateStream)> objectBefore{};
    std::memcpy(objectBefore.data(),static_cast<const void*>(stream),objectBefore.size());
    auto* objectOutput=reinterpret_cast<void**>(stream);
    auto* payloadOutput=reinterpret_cast<void**>(const_cast<uint8_t*>(stream->bytes()));
    require(stream->queryInterface(IBStream::iid,objectOutput)==kInvalidArgument,"QI own-object output rejection");
    require(stream->queryInterface(IBStream::iid,payloadOutput)==kInvalidArgument,"QI own-payload output rejection");
    require(std::memcmp(objectBefore.data(),static_cast<const void*>(stream),objectBefore.size())==0,"QI owner unchanged before publication");
    require(std::equal(payload.begin(),payload.end(),stream->bytes()),"QI payload unchanged before publication");
    void* result=reinterpret_cast<void*>(uintptr_t{0x1234});
    require(stream->queryInterface(nullptr,&result)==kInvalidArgument&&result==reinterpret_cast<void*>(uintptr_t{0x1234}),"QI null selector leaves result");
    require(stream->queryInterface(reinterpret_cast<const char*>(stream),&result)==kInvalidArgument&&result==reinterpret_cast<void*>(uintptr_t{0x1234}),"QI own-object selector rejection");
    require(stream->queryInterface(reinterpret_cast<const char*>(stream->bytes()),&result)==kInvalidArgument&&result==reinterpret_cast<void*>(uintptr_t{0x1234}),"QI own-payload selector rejection");
    require(stream->queryInterface(reinterpret_cast<const char*>(UINTPTR_MAX-7),&result)==kInvalidArgument&&result==reinterpret_cast<void*>(uintptr_t{0x1234}),"QI overflowing selector rejection");
    require(stream->queryInterface(IBStream::iid,reinterpret_cast<void**>(UINTPTR_MAX-3))==kInvalidArgument,"QI overflowing destination rejection");
    require(stream->queryInterface(IBStream::iid,nullptr)==kInvalidArgument,"QI null destination rejection");
    alignas(void*) std::array<uint8_t,32> aliased{};
    std::memcpy(aliased.data(),IBStream::iid,sizeof(TUID));const auto aliasedBefore=aliased;
    require(stream->queryInterface(reinterpret_cast<const char*>(aliased.data()),reinterpret_cast<void**>(aliased.data()))==kInvalidArgument&&aliased==aliasedBefore,"QI selector/destination overlap leaves both");
    require(stream->queryInterface(IBStream::iid,reinterpret_cast<void**>(aliased.data()+1))==kInvalidArgument&&aliased==aliasedBefore,"QI unaligned destination leaves bytes");
    TUID unknown{};std::memset(unknown,0x7c,sizeof(unknown));
    require(stream->queryInterface(unknown,&result)==kNoInterface&&!result&&stream->references()==1,"QI unsupported clears external destination only");
    require(stream->failureKind()==0,"QI known-range rejection does not alter stream-phase diagnostic");
    stream->freeze();const auto frozenKind=stream->failureKind();
    std::thread worker([&]{void* owned=nullptr;
        require(stream->queryInterface(IBStream::iid,&owned)==kResultOk&&owned&&stream->references()==2,"QI available on worker after freeze");
        auto* foreign=static_cast<IBStream*>(owned);
        require(foreign->queryInterface(nullptr,&owned)==kInvalidArgument&&owned==foreign,"late QI rejection leaves result");
        require(foreign->release()==1,"QI worker release retains host keeper");
    });worker.join();
    require(stream->failureKind()==frozenKind&&stream->references()==1,"QI completed diagnostic and host ownership retained");
    require(std::equal(payload.begin(),payload.end(),stream->bytes()),"QI frozen payload retained");
    keepers.reclaim();require(keepers.retainedBytes()==0,"QI retained reference CONTROL reclamation");
}
void frozenDiagnosticRace() {
    StateStreamKeepers keepers;StateStreamKeepers::Spec spec{1,true,nullptr,0};
    for(size_t attempt=0;attempt<1000;++attempt) {
        auto* stream=keepers.reserve(&spec,1)[0];stream->addRef();
        std::atomic<bool> go{false};
        std::thread worker([&]{while(!go.load(std::memory_order_acquire))std::this_thread::yield();int64 position=-1;require(stream->tell(&position)==kInvalidArgument&&position==-1,"race wrong-thread output unchanged");stream->release();});
        go.store(true,std::memory_order_release);stream->freeze();
        const auto frozenKind=stream->failureKind();
        require(frozenKind==0||frozenKind==2,"complete packed frozen diagnostic");
        worker.join();require(stream->failureKind()==frozenKind,"frozen diagnostic immutable after retained worker call");
        require(stream->failure()==(frozenKind?kInvalidArgument:kResultOk),"fault code/kind agreement");
        keepers.reclaim();
    }
}
}
int main(){try{captureAndBounds();modeThreadAndRetention();capacityAndKeeperBudgets();queryInterfaceRanges();frozenDiagnosticRace();std::cout<<"PASS bounded actual SDK IBStream ownership/cursor/capacity/retention, atomic frozen diagnostic and CONTROL keeper reclamation\n";return 0;}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
