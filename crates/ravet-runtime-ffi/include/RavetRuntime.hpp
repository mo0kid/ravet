#pragma once
#include "RavetRuntime.h"
#include <filesystem>
#include <functional>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>
// Thin RAII facade only: all I/O, networking, scheduling and teardown run in Rust.
namespace ravet::runtime {
inline int64_t wallClockMilliseconds() {
    int64_t result=0;
    ravet_runtime_clock_now(&result);
    return result;
}
inline std::filesystem::path utf8Path(const std::string& text) {
#if defined(__cpp_char8_t)
    return std::filesystem::path(std::u8string(reinterpret_cast<const char8_t*>(text.data()),text.size()));
#else
    return std::filesystem::u8path(text);
#endif
}
struct OwnedBytes {
    RavetRuntimeBytes value{};
    ~OwnedBytes() { ravet_runtime_bytes_free(value); }
    OwnedBytes() = default;
    OwnedBytes(const OwnedBytes&) = delete;
    OwnedBytes& operator=(const OwnedBytes&) = delete;
    std::string text() const { return value.data ? std::string(reinterpret_cast<const char*>(value.data),value.len) : std::string(); }
};
inline std::filesystem::path location(uint32_t kind,const void* address=nullptr) {
    OwnedBytes bytes;
    return ravet_runtime_location(kind,address,&bytes.value) ? std::filesystem::path() : utf8Path(bytes.text());
}
inline std::filesystem::path homeDirectory() { return location(0); }
inline std::filesystem::path applicationDataDirectory() { return location(1); }
inline std::filesystem::path modulePath(const void* address) { return location(2,address); }
inline std::string hardwareUuid() {
    OwnedBytes bytes;
    return ravet_runtime_hardware(0,&bytes.value) ? std::string() : bytes.text();
}
inline std::vector<uint8_t> smbiosFirmware() {
    OwnedBytes bytes;
    if(ravet_runtime_hardware(1,&bytes.value)) return {};
    return {bytes.value.data,bytes.value.data+bytes.value.len};
}
inline std::string readFile(const std::filesystem::path& path, size_t maximum) {
    OwnedBytes bytes;
    auto utf8=path.u8string();
    if (ravet_runtime_read(reinterpret_cast<const char*>(utf8.c_str()), maximum, &bytes.value)) return {};
    return bytes.text();
}
inline bool atomicWrite(const std::filesystem::path& path, const uint8_t* bytes, size_t size) {
    auto utf8=path.u8string();
    return ravet_runtime_write(reinterpret_cast<const char*>(utf8.c_str()), bytes, size) == 0;
}
struct HttpResponse { uint32_t error = 0; uint16_t status = 0; std::string body; };
inline HttpResponse postJSON(const std::string& url, const std::string& body,
    const std::string& userAgent, uint64_t timeoutMs = 15000, size_t maximum = 65536) {
    OwnedBytes bytes;
    HttpResponse result;
    result.error = ravet_runtime_post_json(url.c_str(), reinterpret_cast<const uint8_t*>(body.data()),
        body.size(), timeoutMs, maximum, userAgent.c_str(), &bytes.value, &result.status);
    result.body=bytes.text();
    return result;
}
using Task = std::function<void()>;
namespace detail {
inline void invoke(void* context) noexcept { try { (*static_cast<Task*>(context))(); } catch (...) {} }
inline void destroy(void* context) noexcept { delete static_cast<Task*>(context); }
}
class Executor {
public:
    virtual ~Executor() = default;
    virtual bool submit(Task) = 0;
};
class SerialExecutor final : public Executor {
    void* handle;
public:
    explicit SerialExecutor(size_t capacity = 32) : handle(ravet_runtime_executor_create(capacity)) {
        if (!handle) throw std::runtime_error("RAVET worker creation failed");
    }
    ~SerialExecutor() override { ravet_runtime_executor_destroy(handle); }
    SerialExecutor(const SerialExecutor&) = delete;
    SerialExecutor& operator=(const SerialExecutor&) = delete;
    bool submit(Task task) override {
        return ravet_runtime_executor_submit(handle, detail::invoke, detail::destroy, new Task(std::move(task))) == 0;
    }
};
class PeriodicTask {
    void* handle = nullptr;
public:
    ~PeriodicTask() { stop(); }
    PeriodicTask() = default;
    PeriodicTask(const PeriodicTask&) = delete;
    PeriodicTask& operator=(const PeriodicTask&) = delete;
    void start(uint64_t intervalMs, Task task) {
        stop();
        handle = ravet_runtime_timer_create(intervalMs, detail::invoke, detail::destroy, new Task(std::move(task)));
        if (!handle) throw std::runtime_error("RAVET timer creation failed");
    }
    void stop() { if (handle) { auto* old = handle; handle = nullptr; ravet_runtime_timer_destroy(old); } }
};
}
