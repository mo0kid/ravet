#include "RavetAaxParameters.hpp"
#include "RavetGain.h"
#include <cstdlib>
#include <iostream>
#include <new>
#include <stdexcept>
namespace {
int failAllocation = -1;
uint64_t allocations = 0;
void *tracked = nullptr;
uint32_t trackedFrees = 0, liveCaches = 0;
}
void *operator new(size_t n) {
    ++allocations;
    if (failAllocation > 0 && --failAllocation == 0) { failAllocation = -1; throw std::bad_alloc(); }
    if (auto *p = std::malloc(n ? n : 1)) return p;
    throw std::bad_alloc();
}
void *operator new[](size_t n) { return ::operator new(n); }
void operator delete(void *p) noexcept { if (p == tracked && p) ++trackedFrees; std::free(p); }
void operator delete[](void *p) noexcept { ::operator delete(p); }
void operator delete(void *p, size_t) noexcept { ::operator delete(p); }
void operator delete[](void *p, size_t) noexcept { ::operator delete(p); }
namespace {
void require(bool condition, const char *text) { if (!condition) throw std::runtime_error(text); }
const RavetNativeCachedParameterEntryV1 *origin;
void *createCache() { auto *value = origin->create(); if (value) ++liveCaches; return value; }
void destroyCache(void *value) { --liveCaches; origin->destroy(value); }
using Helper = ravet::aax::NativeAaxParameters<2>;
RavetNativeParameterEntryV1 syntheticParameters{};
int schemaFault = 0;
const RavetNativeParameterEntryV1 *syntheticOrigin() { return &syntheticParameters; }
int32_t syntheticInfo(const void *h, uint32_t i, RavetNativeParameterInfoV1 *output) {
    if (origin->info(h,i,output)) return 1;
    if (schemaFault == 20) {
        output->id = i ? 0xABCDu : UINT32_MAX;
        if (!i) { output->flags = RAVET_PARAMETER_AUTOMATABLE | RAVET_PARAMETER_READ_ONLY; std::memset(output->name,0,sizeof(output->name)); output->name[0]=0xC3;output->name[1]=0xA9;output->name_length=2; }
    }
    if (i != 1) return 0;
    switch(schemaFault) {
    case 1: output->reserved=1; break;
    case 2: output->flags=8; break;
    case 3: output->id=1; break;
    case 4: output->flags|=RAVET_PARAMETER_READ_ONLY; break;
    case 5: output->maximum=2.f; break;
    case 6: output->name[0]=0xC0; break;
    case 7: output->unit[63]=1; break;
    case 8: { const uint32_t bits=0x7FC00001;std::memcpy(&output->default_value,&bits,4);break; }
    case 9: output->name[1]=0; break;
    case 10: output->intervals=65536; break;
    }
    return 0;
}
uint32_t syntheticId(uint32_t id) { return schemaFault==20 ? (id==UINT32_MAX?1u:id==0xABCDu?2u:0u) : id; }
int32_t syntheticNormalize(const void *h,uint32_t id,float physical,float *output) { return origin->normalize(h,syntheticId(id),physical,output); }
int32_t syntheticDenormalize(const void *h,uint32_t id,float normalized,float *output) { return origin->denormalize(h,syntheticId(id),normalized,output); }
void syntheticContracts() {
    origin=ravet_native_cached_parameters_v1(32); syntheticParameters=*origin->parameters_entry();
    auto entry=*origin;entry.parameters_entry=syntheticOrigin;entry.info=syntheticInfo;entry.normalize=syntheticNormalize;entry.denormalize=syntheticDenormalize;
    for(schemaFault=1;schemaFault<=10;++schemaFault) { Helper invalid(&entry);require(!invalid.valid()&&!invalid.count(),"late malformed native metadata leaves schema invalid"); }
    schemaFault=20;Helper wide(&entry);require(wide.valid()&&!std::strcmp(wide.id(0),"pFFFFFFFF")&&!std::strcmp(wide.id(1),"pABCD"),"generic full native ID and uppercase canonical hexadecimal mapping");
    double output=77.;require(!wide.control("pFFFFFFFF",.5,output)&&output==77.&&wide.value("pFFFFFFFF",.5,output)&&output==.5,"read-only control rejects while pure metadata value remains representable");
    require(!wide.value("pabcd",.5,output)&&output==.5,"lowercase canonical alias rejected");
    Helper::Prepared staged{};require(wide.prepare(staged)&&!staged[0]->Automatable()&&staged[0]->Name().Length()==2,"UTF8 native name and read-only automation suppression in SDK object");
    schemaFault=0;
    Helper default64(ravet_native_cached_parameters_v1(64));Helper explicit64(ravet_native_cached_parameters_v1(64),64);
    require(!default64.valid()&&explicit64.valid(),"default32 versus explicit64 schema precision is independent of AAX audio claim");
    auto invalidEntry=entry;invalidEntry.sample_bits=64;Helper mixed(&invalidEntry,64);require(!mixed.valid(),"mixed cached/native schema precision rejects");
    invalidEntry=entry;invalidEntry.destroy=nullptr;Helper missingDestroy(&invalidEntry);require(!missingDestroy.valid(),"missing cache retirement rejects before creation");
}

void contracts() {
    origin = ravet_native_cached_parameters_v1(32);
    auto counted = *origin; counted.create = createCache; counted.destroy = destroyCache;
    {
        Helper helper(&counted);
        require(helper.valid() && helper.count() == 2 && !std::strcmp(helper.id(0), "p1") && !std::strcmp(helper.masterBypass(), "p2"), "exact Gain native/AAX stable IDs and native master bypass");
        double output = 77.;
        for (const char *id : {"p01", "p1tail", "P1", "p00000001", "p-1", "Byp", "pFFFFFFFF", "p100000001", ""})
            require(!helper.control(id, .25, output) && output == 77., "canonical identity rejects aliases and reserved bypass without output mutation");
        for (uint64_t pattern : {UINT64_C(0x7ff8000000000001), UINT64_C(0x7ff0000000000000), UINT64_C(0xfff0000000000000)}) {
            volatile uint64_t foreign = pattern; const uint64_t bits = foreign; double value; std::memcpy(&value, &bits, 8);
            require(!helper.control("p1", value, output) && output == 77., "foreign nonfinite scalar rejection under fast math");
        }
        require(helper.control("p1", .123456789012345, output) && output == double(float(.123456789012345)), "explicit normalized f32 narrowing");
        require(helper.control("p2", .5, output) && output == 1., "native bypass tie snaps up");
        const auto before = allocations;
        for (uint32_t iteration = 0; iteration < 100000; ++iteration) {
            require(helper.control("p1", .125, output) && output == .125, "repeated prepared valid scalar preflight");
            require(!helper.control("p1tail", .75, output) && output == .125, "repeated prepared malformed identity preserves scalar");
        }
        require(allocations == before, "prepared scalar preflight makes no C++ heap allocation");
        AAX_CString text("retained");
        require(!helper.format("p1", .125, 2, text) && text == "retained", "short text capacity retains output");
        require(helper.format("p1", .125, 3, text) && text == "0.5", "actual physical Gain f32 format");
        require(helper.parse("p1", AAX_CString(" +0.5 \n"), output) && output == .125, "complete physical Gain parse");
        for (const char *bad : {"0.5junk", "+-0.5", "nan", "inf", "4.1", ""}) { output = 77.; require(!helper.parse("p1", AAX_CString(bad), output) && output == 77., "complete parse rejection preserves output"); }
        Helper::Prepared staged{}; require(helper.prepare(staged), "real SDK object staging");
        require(staged[0]->Name() == "Gain" && staged[0]->Automatable() && staged[0]->GetType() == AAX_eParameterType_Continuous && staged[0]->GetNumberOfSteps() == 128, "SDK Gain object metadata");
        require(staged[1]->GetType() == AAX_eParameterType_Discrete && staged[1]->GetNumberOfSteps() == 2, "SDK bypass object metadata");
        auto *oldFirst = staged[0].get(), *oldLast = staged[1].get();
        failAllocation = 1; bool stageFailed = false;
        try { (void)helper.prepare(staged); } catch (const std::bad_alloc &) { stageFailed = true; }
        failAllocation = -1;
        require(stageFailed && staged[0].get() == oldFirst && staged[1].get() == oldLast, "failed SDK object staging preserves previous complete staged set");
        auto last = std::move(staged[1]); AAX_CParameterManager manager;
        require(!helper.registerEmpty(manager, staged) && !manager.NumParameters() && staged[0], "known late missing object preflight before any SDK adoption");
        staged[1] = std::move(last); require(helper.registerEmpty(manager, staged) && manager.NumParameters() == 2, "complete real SDK parameter registration");
    }
    require(!liveCaches, "ordinary SDK/cache retirement");
    // Establish the precise installed SDK failure stages without guessing maps.
    for (int failure : {1, 2}) {
        auto helper = std::make_unique<Helper>(&counted); Helper::Prepared staged{}; require(helper->prepare(staged), "stage direct failure probe");
        auto *raw = staged[0].release(); tracked = raw; trackedFrees = 0; AAX_CParameterManager manager;
        failAllocation = failure; bool failed = false;
        try { manager.AddParameter(raw); } catch (const std::bad_alloc &) { failed = true; }
        failAllocation = -1; require(failed, "installed SDK vector/map allocation fault reached");
        if (failure == 1) { require(!manager.NumParameters(), "vector failure before adoption"); delete raw; }
        else { require(manager.NumParameters() == 1 && manager.GetParameter(0) == raw && !manager.GetParameterByID("p1"), "map failure after vector adoption before lookup publication"); manager.RemoveParameter(raw); require(!manager.NumParameters(), "public vector-based removal safely retires map-failed adopted pointer"); }
        require(trackedFrees == 1, "exact one SDK object retirement"); tracked = nullptr;
    }
    require(!liveCaches, "direct insertion-fault cache retirement");
    for (int failure = 1; failure <= 4; ++failure) {
        auto helper = std::make_unique<Helper>(&counted); Helper::Prepared staged{}; require(helper->prepare(staged), "stage helper registration fault");
        tracked = staged[0].get(); trackedFrees = 0; AAX_CParameterManager manager; failAllocation = failure;
        require(!helper->registerEmpty(manager, staged), "whole helper SDK insertion fault reports failure");
        failAllocation = -1; require(!manager.NumParameters() && trackedFrees == 1, "failed helper registration retires accepted prefix without dangling vector/map pointer"); tracked = nullptr;
    }
    require(!liveCaches, "helper insertion-fault cache retirement");
    // Borrowers retain the native cache independently of helper member order.
    {
        auto helper = std::make_unique<Helper>(&counted); Helper::Prepared staged{}; require(helper->prepare(staged), "stage independently owned metadata borrowers");
        AAX_CParameterManager manager; require(helper->registerEmpty(manager, staged), "register shared-cache objects");
        helper.reset(); require(liveCaches == 1, "cloned SDK display adapters retain cache after facade retirement");
        AAX_CString value; require(manager.GetParameterByID("p1")->GetValueString(&value) && value == "1", "retained SDK object can format actual native default after helper retirement");
        manager.RemoveAllParameters(); require(!liveCaches, "last SDK display owner retires native cache once");
    }
}
}
int main() { try { contracts(); syntheticContracts(); std::cout << "PASS real SDK AAX schema/control/objects, exact identities and vector/map allocation-fault ownership contracts\n"; return 0; } catch (const std::exception &error) { failAllocation = -1; std::cerr << error.what() << '\n'; return 1; } }
