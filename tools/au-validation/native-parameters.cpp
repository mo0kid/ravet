#include "RavetAuParameters.hpp"
#include "RavetAuState.hpp"
#include "RavetGain.h"
#include "RavetPerformance.h"
#include <iostream>
#include <stdexcept>
namespace {
void require(bool value, const char *message) { if (!value) throw std::runtime_error(message); }
void releaseInfo(AudioUnitParameterInfo &info) {
    if (info.cfNameString) CFRelease(info.cfNameString);
    if (info.unitName) CFRelease(info.unitName);
}
std::array<RavetNativeParameterInfoV1, 5> synthetic{};
uint32_t creates = 0, destroys = 0;
bool mappingFault = false;
RavetNativeParameterEntryV1 syntheticParameters{};
const RavetNativeParameterEntryV1 *parameters() { return &syntheticParameters; }
void *create() { ++creates; return synthetic.data(); }
void destroy(void *) { ++destroys; }
int32_t info(const void *, uint32_t i, RavetNativeParameterInfoV1 *out) {
    if (i >= synthetic.size()) return 1;
    *out = synthetic[i]; return 0;
}
const RavetNativeParameterInfoV1 *find(uint32_t id) {
    for (const auto &entry : synthetic) if (entry.id == id) return &entry;
    return nullptr;
}
float canonical(const RavetNativeParameterInfoV1 &entry, float physical) {
    auto value = std::clamp(physical, entry.minimum, entry.maximum);
    if (entry.intervals) {
        const auto step = double(entry.maximum - entry.minimum) / entry.intervals;
        value = float(entry.minimum + std::floor((value - entry.minimum) / step + .5) * step);
    }
    return value;
}
int32_t normalize(const void *, uint32_t id, float input, float *output) {
    const auto entry = find(id); if (!entry || !std::isfinite(input) || mappingFault) return 1;
    const auto value = canonical(*entry, input);
    *output = entry->scale ? float(std::log(double(value) / entry->minimum) / std::log(double(entry->maximum) / entry->minimum)) :
                            (value - entry->minimum) / (entry->maximum - entry->minimum);
    return 0;
}
int32_t denormalize(const void *, uint32_t id, float input, float *output) {
    const auto entry = find(id); if (!entry || !std::isfinite(input) || mappingFault) return 1;
    const auto value = std::clamp(input, 0.f, 1.f);
    const auto physical = entry->scale ? float(entry->minimum * std::exp(double(value) * std::log(double(entry->maximum) / entry->minimum))) :
                                        entry->minimum + value * (entry->maximum - entry->minimum);
    *output = canonical(*entry, physical); return 0;
}
RavetNativeCachedParameterEntryV1 table() {
    return {1, sizeof(RavetNativeCachedParameterEntryV1), 32, 5, parameters, create, destroy, info, normalize, denormalize, nullptr};
}
void populate() {
    syntheticParameters.version = 1; syntheticParameters.byte_size = sizeof(syntheticParameters);
    syntheticParameters.sample_bits = 32; syntheticParameters.parameter_count = 5;
    uint32_t i = 0;
    for (const uint32_t id : {UINT32_MAX, 2u, 7u, 9u, 4u}) {
        auto &entry = synthetic[i++]; entry = {};
        entry.id = id; entry.flags = RAVET_PARAMETER_AUTOMATABLE; entry.maximum = 1.f; entry.default_value = .25f;
        entry.name_length = 5; std::memcpy(entry.name, "Value", 5);
    }
    auto &longName = synthetic[0];
    longName.name_length = 254;
    for (uint32_t n = 0; n < 63; ++n) std::memcpy(longName.name + n * 4, "\xf0\x9f\x8e\xb5", 4);
    longName.name[252] = 'A'; longName.name[253] = 'B';
    synthetic[1].scale = 1; synthetic[1].minimum = 10.f; synthetic[1].maximum = 1000.f; synthetic[1].default_value = 100.f;
    synthetic[1].unit_length = 2; std::memcpy(synthetic[1].unit, "Hz", 2);
    synthetic[2].intervals = 4;
    synthetic[3].flags = RAVET_PARAMETER_READ_ONLY;
    synthetic[4].flags |= RAVET_PARAMETER_BYPASS; synthetic[4].intervals = 1; synthetic[4].default_value = 0.f;
}
void syntheticCases() {
    populate(); auto cache = table(); AudioUnitParameterInfo retained{};
    {
        ravet::au::NativeAuParameters<5> helper(&cache); require(helper.valid(), "synthetic complete AU schema");
        std::array<AudioUnitParameterID, 6> ids{77,77,77,77,77,77}; uint32_t count = 99;
        require(!helper.list(ids.data(), 4, count) && ids[0] == 77 && count == 99, "list capacity failure is atomic");
        require(helper.list(ids.data(), 5, count) && count == 5 && ids[0] == UINT32_MAX && ids[5] == 77, "full UInt32 IDs and untouched list tails");
        AudioUnitParameterInfo metadata{}; require(helper.info(UINT32_MAX, metadata), "full-width metadata ID");
        require(CFStringGetLength(metadata.cfNameString) == 128, "complete supplementary Unicode metadata beyond String128"); releaseInfo(metadata);
        require(helper.info(UINT32_MAX, retained), "caller-owned CF metadata");
        require(helper.info(2, metadata) && metadata.defaultValue == .5f && metadata.unit == kAudioUnitParameterUnit_CustomUnit && CFEqual(metadata.unitName, CFSTR("Hz")), "log mapping and custom unit metadata"); releaseInfo(metadata);
        require(helper.info(9, metadata) && !(metadata.flags & kAudioUnitParameterFlag_IsWritable), "readonly metadata"); releaseInfo(metadata);
        metadata={};metadata.minValue=77.f;metadata.cfNameString=CFSTR("sentinel");require(!helper.info(999,metadata)&&metadata.minValue==77.f&&CFEqual(metadata.cfNameString,CFSTR("sentinel")),"unknown metadata leaves output unchanged");
        float value = 77.f; require(helper.control(7, .375f, 65535, value) && value == .5f, "stepped ties-up canonical control");
        for (const auto id : {9u, 999u}) { value = 77.f; require(!helper.control(id,.5f,0,value) && value == 77.f, "readonly/unknown control atomic"); }
        value = 77.f; require(!helper.control(7,.5f,65536,value) && value == 77.f, "offset preflight");
        for (const float bad : {-1.f,2.f,std::numeric_limits<float>::quiet_NaN(),std::numeric_limits<float>::infinity()})
            require(!helper.control(7,bad,0,value) && value == 77.f, "normalized scalar rejection");
        CFStringRef text = nullptr; require(helper.format(2,.5f,text) && CFEqual(text,CFSTR("100")), "physical log text"); CFRelease(text);
        require(helper.parse(2,CFSTR(" +100 \n"),value) && value == .5f, "complete physical parse");
        require(helper.parse(7,CFSTR("0.375"),value) && value == .5f, "physical stepped parse");
        require(helper.format(4,1.f,text) && CFEqual(text,CFSTR("On")), "bypass format"); CFRelease(text);
        require(helper.parse(4,CFSTR("Off"),value) && value == 0.f, "bypass parse");
        for (const auto bad : {CFSTR(""),CFSTR("100Hz"),CFSTR("nan"),CFSTR("inf"),CFSTR("1001"),CFSTR("+-100"),CFSTR("100 0"),CFSTR("\u00e9")}) {
            value = 77.f; require(!helper.parse(2,bad,value) && value == 77.f, "malformed physical text atomic");
        }
        const UniChar embedded[] = {'1',0,'0'}; auto embeddedText = CFStringCreateWithCharacters(nullptr, embedded, 3);
        require(!helper.parse(2,embeddedText,value) && value == 77.f, "embedded NUL rejects"); CFRelease(embeddedText);
        std::array<UniChar,129> longInput{};longInput.fill('1');auto oversized = CFStringCreateWithCharacters(nullptr,longInput.data(),longInput.size());
        require(!helper.parse(2,oversized,value) && value == 77.f, "oversized text rejects without truncation");CFRelease(oversized);
        mappingFault = true; require(!helper.control(7,.5f,0,value) && value == 77.f, "cached mapping failure atomic"); mappingFault = false;
        text=CFSTR("sentinel");require(!helper.format(999,.5f,text)&&CFEqual(text,CFSTR("sentinel")),"unknown text format preserves output");
    }
    require(CFStringGetLength(retained.cfNameString)==128,"CF metadata survives helper retirement");releaseInfo(retained);
    require(creates == destroys, "cache stopped retirement");
    for (uint32_t malformed = 0; malformed < 8; ++malformed) {
        populate(); cache = table();
        if (malformed == 0) cache.destroy = nullptr;
        if (malformed == 1) cache.sample_bits = 64;
        if (malformed == 2) synthetic[1].id = synthetic[0].id;
        if (malformed == 3) synthetic[0].name[0] = 0xff;
        if (malformed == 4) synthetic[0].name[255] = 1;
        if (malformed == 5) cache.parameters_entry=[]()->const RavetNativeParameterEntryV1 *{return nullptr;};
        if (malformed == 6) cache.create=[]()->void *{return nullptr;};
        if (malformed == 7) cache.parameter_count=6;
        ravet::au::NativeAuParameters<5> rejected(&cache); require(!rejected.valid(), "malformed table/schema profile rejected");
        uint32_t count = 77; require(!rejected.list(nullptr,0,count) && count == 77, "invalid helper callback safe and atomic");
    }
    require(creates == destroys, "invalid cache retirement");
}
void actualCases() {
    const auto cache = ravet_native_cached_parameters_v1(32);
    ravet::au::NativeAuParameters<2> gain(cache); require(gain.valid(), "actual Gain cache adoption");
    CFStringRef text = nullptr; float value = 77.f;
    require(gain.format(1,.25f,text) && CFEqual(text,CFSTR("1")), "actual Gain physical text"); CFRelease(text);
    require(gain.parse(1,CFSTR("1"),value) && value == .25f, "actual Gain native range parse");
    ravet::au::NativeAuState gainState(ravet_native_state_v1(32),gain.parametersEntry());require(gainState.valid(),"actual Gain state companion");
    std::array<uint8_t,20> legacy{};std::memcpy(legacy.data(),"RAVETGN\0",8);legacy[8]=1;legacy[12]=1;float legacyGain=2.f;std::memcpy(legacy.data()+16,&legacyGain,4);
    auto legacyData=CFDataCreate(nullptr,legacy.data(),legacy.size());require(gainState.validateData(legacyData),"explicit actual Gain legacy migration accepted");CFRelease(legacyData);
    ravet::au::NativeAuParameters<1> performance(ravet_performance_cached_parameters_v1(32)); require(performance.valid(), "actual Performance cache adoption");
    const auto state = ravet_performance_state_v1(32);
    ravet::au::NativeAuState validator(state, performance.parametersEntry()); require(validator.valid(), "actual exact native state companion");
    auto publication = state->create(nullptr,0); std::array<uint8_t,8208> bytes{}; RavetNativeStateRevisionV1 revision{};
    require(publication && !state->save_state(publication,bytes.data(),performance.parametersEntry()->state_bytes,&revision), "native coherent default state");
    state->destroy(publication);
    auto data = CFDataCreate(nullptr,bytes.data(),performance.parametersEntry()->state_bytes);
    auto dictionary = CFDictionaryCreateMutable(nullptr,0,&kCFTypeDictionaryKeyCallBacks,&kCFTypeDictionaryValueCallBacks);
    CFDictionarySetValue(dictionary,CFSTR("Processor State"),data); CFDictionarySetValue(dictionary,CFSTR("Controller State"),data);
    AudioComponentDescription description{'aumu','RvPS','Ravt',0,0};
    const SInt32 identity[] = {0,SInt32(description.componentSubType),SInt32(description.componentManufacturer)}; uint32_t i = 0;
    for (const auto key : {CFSTR("version"),CFSTR("subtype"),CFSTR("manufacturer")}) {
        auto number = CFNumberCreate(nullptr,kCFNumberSInt32Type,&identity[i++]); CFDictionarySetValue(dictionary,key,number); CFRelease(number);
    }
    require(validator.validate(dictionary,description), "complete native dictionary accepted");
    CFDictionarySetValue(dictionary,CFSTR("part"),kCFBooleanTrue);require(!validator.validate(dictionary,description),"partial AU preset rejected");CFDictionaryRemoveValue(dictionary,CFSTR("part"));
    CFDictionarySetValue(dictionary,CFSTR("Controller State"),CFSTR("invalid"));
    require(!validator.validate(dictionary,description), "wrong state type rejected");
    require(validator.cohere(dictionary) && CFEqual(CFDictionaryGetValue(dictionary,CFSTR("Controller State")),data), "processor authoritative coherence");
    bytes[0] ^= 1; auto bad = CFDataCreate(nullptr,bytes.data(),24); CFDictionarySetValue(dictionary,CFSTR("Processor State"),bad);
    require(!validator.cohere(dictionary) && CFEqual(CFDictionaryGetValue(dictionary,CFSTR("Controller State")),data), "malformed save leaves dictionary intact");
    auto invalid = *state; invalid.destroy = nullptr; ravet::au::NativeAuState missingDestroy(&invalid,performance.parametersEntry()); require(!missingDestroy.valid(), "state destructor table preflight");
    auto copiedParameters = *performance.parametersEntry(); ravet::au::NativeAuState mismatch(state,&copiedParameters); require(!mismatch.valid(), "exact state origin required");
    invalid=*state;invalid.create=[](const uint8_t *,uint32_t)->void *{return nullptr;};ravet::au::NativeAuState failedCreate(&invalid,performance.parametersEntry());require(!failedCreate.valid()&&!failedCreate.validateData(data),"failed state creation callbacks reject safely");
    CFRelease(bad); CFRelease(data); CFRelease(dictionary);
}
}
int main() {
    try { syntheticCases(); actualCases(); std::cout << "Reusable AU native schema/text/control/state preflight and actual native companions passed\n"; }
    catch (const std::exception &error) { std::cerr << error.what() << '\n'; return 1; }
}
