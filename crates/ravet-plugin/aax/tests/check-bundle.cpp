// Loads a built RAVET .aaxplugin and exercises every registered effect through
// the public AAX SDK interfaces with a synthetic host: exports and SDK version,
// descriptors, Initialize, parameters and text, processing, MIDI nodes, the state
// chunk round trip, and teardown. Works for any product built with the generic
// adapter. It is NOT Avid's validator (DSH/aaxval) and NOT a Pro Tools run.
//
//   ravet_aax_check <path/to/Name.aaxplugin>
#include "FakeHost.hpp"
#include "AAX_CString.h"
#include <cmath>
#include <cstring>
#include <filesystem>
#include <iostream>
#if defined(_WIN32)
#include <windows.h>
#else
#include <dlfcn.h>
#endif

using fake::require;
namespace fs = std::filesystem;

namespace {

struct Module {
#if defined(_WIN32)
    HMODULE handle = nullptr;
    explicit Module(const fs::path &path) : handle(LoadLibraryW(path.c_str())) { require(handle != nullptr, "LoadLibrary failed"); }
    void *symbol(const char *name) const { return reinterpret_cast<void *>(GetProcAddress(handle, name)); }
#else
    void *handle = nullptr;
    explicit Module(const fs::path &path) : handle(dlopen(path.c_str(), RTLD_NOW | RTLD_LOCAL)) {
        if (!handle) throw std::runtime_error(dlerror());
    }
    void *symbol(const char *name) const { return dlsym(handle, name); }
#endif
};

fs::path binary(const fs::path &bundle) {
#if defined(_WIN32)
    return bundle / "Contents" / "x64" / bundle.filename();
#else
    for (const auto &item : fs::directory_iterator(bundle / "Contents" / "MacOS")) return item.path();
    throw std::runtime_error("bundle has no Contents/MacOS binary");
#endif
}

std::string fourcc(uint32_t v) {
    std::string s;
    for (int shift = 24; shift >= 0; shift -= 8) {
        const char c = char((v >> shift) & 0xff);
        s += (c >= 32 && c < 127) ? c : '?';
    }
    return s;
}

void exercise(const std::string &id, fake::Descriptor &descriptor) {
    auto &component = descriptor.component;
    require(descriptor.componentAdded && component.process && descriptor.createParameters, id + ": incomplete descriptor");
    const auto &p = *component.properties;
    const auto input = fake::stem(p.at(AAX_eProperty_InputStemFormat)), output = fake::stem(p.at(AAX_eProperty_OutputStemFormat));
    const uint32_t inputs = input == AAX_eStemFormat_None ? 0 : AAX_STEM_FORMAT_CHANNEL_COUNT(input);
    const uint32_t outputs = AAX_STEM_FORMAT_CHANNEL_COUNT(output);
    require(component.privateField == 0 && component.privateSize == int32_t(sizeof(void *)) && component.frames == 1 && component.audioOut == 2,
            id + ": context fields");
    require((component.audioIn >= 0) == (inputs > 0), id + ": audio input field matches stem");
    std::cout << "effect " << id << ": \"" << descriptor.name << "\" " << inputs << "x" << outputs
              << " manufacturer " << fourcc(uint32_t(p.at(AAX_eProperty_ManufacturerID)))
              << " product " << fourcc(uint32_t(p.at(AAX_eProperty_ProductID)))
              << " type " << fourcc(uint32_t(p.at(AAX_eProperty_PlugInID_Native)))
              << " category 0x" << std::hex << descriptor.category << std::dec
              << " midi-nodes " << component.midi.size() << " editor " << (descriptor.createGui ? "yes" : "no") << "\n";

    TransportControllerHost host;
    host.effect = id;
    host.input = input;
    host.output = output;
    std::unique_ptr<AAX_IEffectParameters> model(descriptor.newParameters());
    require(bool(model), id + ": factory");
    require(model->Initialize(host.unknown()) == AAX_SUCCESS, id + ": Initialize");
    int32_t count = 0;
    require(model->GetNumberOfParameters(&count) == AAX_SUCCESS, id + ": parameter count");
    AAX_CString bypass;
    model->GetMasterBypassParameter(&bypass);
    std::cout << "  parameters " << count << ", master bypass \"" << bypass.Get() << "\", registered with host " << host.registered.size() << "\n";
    for (int32_t i = 0; i < count; ++i) {
        AAX_CString pid, name, text;
        require(model->GetParameterIDFromIndex(i, &pid) == AAX_SUCCESS, id + ": parameter ID");
        double normalized = -1, back = -1;
        int32_t steps = 0;
        require(model->GetParameterNormalizedValue(pid.Get(), &normalized) == AAX_SUCCESS && normalized >= 0 && normalized <= 1, id + ": value");
        model->GetParameterName(pid.Get(), &name);
        model->GetParameterNumberOfSteps(pid.Get(), &steps);
        require(model->GetParameterStringFromValue(pid.Get(), normalized, &text, 32) == AAX_SUCCESS, id + ": text for " + pid.Get());
        if (model->GetParameterValueFromString(pid.Get(), &back, text) == AAX_SUCCESS)
            require(std::fabs(back - normalized) < 1e-3, id + ": text round trip for " + pid.Get() + " (" + text.Get() + ")");
        std::cout << "  " << pid.Get() << " \"" << name.Get() << "\" = \"" << text.Get() << "\" steps " << steps << "\n";
    }

    fake::Context context(component, inputs, outputs);
    require(model->ResetFieldData(0, &context.owner, sizeof(void *)) == AAX_SUCCESS && context.owner, id + ": private owner");
    for (int block = 0; block < 8; ++block) {
        for (auto &channel : context.inputs)
            for (size_t f = 0; f < channel.size(); ++f) channel[f] = 0.25f * float(std::sin(0.01 * double(f + block * 512)));
        if (!context.midiIn.in.empty() || component.fieldIndex(AAX_eMIDINodeType_LocalInput) >= 0) {
            context.midiIn.in.clear();
            AAX_CMidiPacket on{};
            on.mTimestamp = 3; on.mLength = 3; on.mData[0] = 0x90; on.mData[1] = 60; on.mData[2] = 100;
            context.midiIn.in.push_back(on);
        }
        context.run(block == 3 ? 64 : 512);
        for (auto &channel : context.outputs)
            for (int32_t f = 0; f < context.frames; ++f) require(std::isfinite(channel[size_t(f)]), id + ": finite output");
        model->TimerWakeup();
    }
    context.run(0);
    std::cout << "  processed 8 blocks + 1 empty block; MIDI out packets " << context.midiOut.posted.size() << "\n";

    int32_t chunks = 0;
    AAX_CTypeID chunkId = 0;
    uint32_t size = 0;
    require(model->GetNumberOfChunks(&chunks) == AAX_SUCCESS && chunks == 1, id + ": one chunk");
    require(model->GetChunkIDFromIndex(0, &chunkId) == AAX_SUCCESS && model->GetChunkSize(chunkId, &size) == AAX_SUCCESS && size > 0, id + ": chunk size");
    std::vector<uint8_t> storage(sizeof(AAX_SPlugInChunk) + size);
    auto *chunk = reinterpret_cast<AAX_SPlugInChunk *>(storage.data());
    chunk->fChunkID = chunkId;
    require(model->GetChunk(chunkId, chunk) == AAX_SUCCESS && uint32_t(chunk->fSize) == size, id + ": GetChunk");
    require(std::memcmp(chunk->fData, "RAVETSTA", 8) == 0 || std::memcmp(chunk->fData, "MOCKSTAT", 8) == 0, id + ": chunk carries the state envelope");
    AAX_CBoolean equal = false;
    require(model->CompareActiveChunk(chunk, &equal) == AAX_SUCCESS && equal, id + ": chunk compares equal");
    require(model->SetChunk(chunkId, chunk) == AAX_SUCCESS, id + ": SetChunk");
    chunk->fData[0] ^= 0x5a;
    require(model->SetChunk(chunkId, chunk) != AAX_SUCCESS, id + ": corrupt chunk rejected");
    std::cout << "  state chunk " << fourcc(chunkId) << " " << size << " bytes: save/compare/load ok, corrupt rejected\n";

    require(model->Uninitialize() == AAX_SUCCESS, id + ": Uninitialize");
    model.reset();
}

} // namespace

int main(int argc, char **argv) {
    if (argc != 2) {
        std::cerr << "usage: ravet_aax_check <Name.aaxplugin>\n";
        return 2;
    }
    try {
        const fs::path bundle = fs::absolute(argv[1]);
        Module module(binary(bundle));
        for (const char *name : {"ACFRegisterPlugin", "ACFRegisterComponent", "ACFGetClassFactory", "ACFCanUnloadNow",
                                 "ACFStartup", "ACFShutdown", "ACFGetSDKVersion"})
            require(module.symbol(name) != nullptr, std::string("missing export ") + name);
        using Version = int32_t (*)(uint64_t *);
        uint64_t version = 0;
        require(reinterpret_cast<Version>(module.symbol("ACFGetSDKVersion"))(&version) == 0 && version, "ACFGetSDKVersion");
#if defined(_WIN32)
        void *describeSymbol = module.symbol("?GetEffectDescriptions@@YAHPEAVAAX_ICollection@@@Z");
#else
        void *describeSymbol = module.symbol("_Z21GetEffectDescriptionsP15AAX_ICollection");
#endif
        require(describeSymbol != nullptr, "GetEffectDescriptions export");
        std::cout << "loaded " << bundle.filename().string() << "; ACF exports present; SDK version 0x" << std::hex << version << std::dec << "\n";
        fake::Collection collection;
        using Describe = AAX_Result (*)(AAX_ICollection *);
        require(reinterpret_cast<Describe>(describeSymbol)(&collection) == AAX_SUCCESS && !collection.effects.empty(), "GetEffectDescriptions");
        std::cout << "package \"" << collection.package << "\" by \"" << collection.manufacturer << "\" version 0x" << std::hex
                  << collection.version << std::dec << ", " << collection.effects.size() << " effect(s)\n";
        for (auto &effect : collection.effects) exercise(effect.first, *effect.second);
        std::cout << "PASS\n";
        return 0;
    } catch (const std::exception &error) {
        std::cerr << "FAIL: " << error.what() << "\n";
        return 1;
    }
}
