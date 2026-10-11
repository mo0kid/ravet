#include "RavetVst3Provider.h"
#include <array>
#include <cstring>
#include <iostream>
#include <stdexcept>
#include <string>
#include <thread>
static void require(bool condition) { if (!condition) throw std::runtime_error("provider contract failed"); }
int main(int argc, char** argv) {
    try {
        require(argc >= 2);
        const auto* provider = ravet_vst3_provider_v1();
        require(provider && provider->version == 1 && provider->struct_size == sizeof(*provider));
        for (int index = 1; index < argc; ++index) {
            const std::string path = argv[index];
            for (int repetition = 0; repetition < 3; ++repetition) {
                RavetVst3Module* module = nullptr; int32_t native = 123;
                const auto result = provider->open(reinterpret_cast<const uint8_t*>(path.data()), static_cast<uint32_t>(path.size()), &module, &native);
                if (result != RAVET_VST3_OK) std::cerr << "open status " << result << " native " << native << '\n';
                require(result == RAVET_VST3_OK && module && native == 0);
                RavetVst3Module* duplicate = nullptr;
                require(provider->open(reinterpret_cast<const uint8_t*>(path.data()), path.size(), &duplicate, &native) == RAVET_VST3_CAPACITY);
                require(!duplicate);
                RavetVst3FactoryRecord factory{}; factory.struct_size = sizeof(factory);
                require(provider->factory(module, &factory) == RAVET_VST3_OK);
                require(factory.class_count >= 2 && factory.class_count <= 256 && std::string(factory.vendor) == "RAVET");
                std::array<std::uint8_t, 16> previous{};
                for (uint32_t item = 0; item < factory.class_count; ++item) {
                    RavetVst3ClassRecord record{}; record.struct_size = sizeof(record);
                    require(provider->class_info(module, item, &record) == RAVET_VST3_OK);
                    require(record.name[0] && record.category[0] && record.vendor[0] && record.cardinality > 0);
                    require(std::memcmp(record.id, previous.data(), previous.size()) != 0);
                    std::memcpy(previous.data(), record.id, previous.size());
                    if (!repetition) std::cout << item << ' ' << record.category << ' ' << record.name << ' ' << record.version << '\n';
                }
                RavetVst3ClassRecord unchanged{}; unchanged.struct_size = sizeof(unchanged); unchanged.name[0] = 'x';
                const auto original = unchanged;
                require(provider->class_info(module, factory.class_count, &unchanged) == RAVET_VST3_INVALID);
                require(std::memcmp(&unchanged, &original, sizeof(original)) == 0);
                unchanged.struct_size = 0; const auto badSize = unchanged;
                require(provider->class_info(module, 0, &unchanged) == RAVET_VST3_INVALID);
                require(std::memcmp(&unchanged, &badSize, sizeof(badSize)) == 0);
                int32_t threadFactory = -1, threadClose = -1;
                std::thread foreign([&] {
                    RavetVst3FactoryRecord attempt{}; attempt.struct_size = sizeof(attempt);
                    threadFactory = provider->factory(module, &attempt);
                    int32_t ignored = 0; threadClose = provider->close(module, &ignored);
                }); foreign.join();
                require(threadFactory == RAVET_VST3_THREAD && threadClose == RAVET_VST3_THREAD);
                require(provider->factory(module, &factory) == RAVET_VST3_OK);
                require(provider->close(module, &native) == RAVET_VST3_OK && (native == 0 || native == 3));
            }
        }
        RavetVst3Module* module = nullptr; int32_t native = 7;
        const std::array<uint8_t, 3> invalid{ 'a', 0, 'b' };
        require(provider->open(invalid.data(), invalid.size(), &module, &native) == RAVET_VST3_INVALID && !module && native == 7);
        const std::string missing = "/nonexistent/ravet-w197-selected.vst3";
        require(provider->open(reinterpret_cast<const uint8_t*>(missing.data()), missing.size(), &module, &native) == RAVET_VST3_NATIVE && !module);
        std::cout << "PASS actual selected modules, cached records, retirement/thread/capacity contracts\n";
    } catch (const std::exception& error) { std::cerr << error.what() << '\n'; return 1; }
}
