#pragma once
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
// All operations are control/worker-thread only. Status: 0=ok, 1=invalid,
// 2=file I/O, 3=queue full, 4=queue stopped, 5=timeout, 6=transport, 7=too large.
// Paths/URLs/user agents are valid terminated UTF-8. Byte inputs may be null only
// at length zero. Outputs are writable and cleared on failure. Free returned
// bytes exactly once, using the unmodified returned record (never a copy).
typedef struct RavetRuntimeBytes { uint8_t* data; size_t len; } RavetRuntimeBytes;
typedef void (*RavetRuntimeCallback)(void*);
void ravet_runtime_bytes_free(RavetRuntimeBytes);
uint32_t ravet_runtime_clock_now(int64_t* unix_milliseconds);
// Location kind: 0=home, 1=application data, 2=module containing address.
uint32_t ravet_runtime_location(uint32_t kind, const void* address, RavetRuntimeBytes*);
// Raw hardware kind: 0=macOS UUID, 1=Windows SMBIOS. No product hashing.
uint32_t ravet_runtime_hardware(uint32_t kind, RavetRuntimeBytes*);
uint32_t ravet_runtime_read(const char* path, size_t maximum, RavetRuntimeBytes*);
uint32_t ravet_runtime_write(const char* path, const uint8_t*, size_t);
uint32_t ravet_runtime_post_json(const char* url, const uint8_t*, size_t,
    uint64_t timeout_ms, size_t maximum, const char* user_agent, RavetRuntimeBytes*, uint16_t* status);
void* ravet_runtime_executor_create(size_t pending_capacity);
void ravet_runtime_executor_destroy(void*);
// Submission and timer creation consume context even on failure. Destruction
// callback runs exactly once, after the final invocation or rejected submission.
// Callbacks never throw/unwind; context is safe to use on the worker thread.
// Do not destroy a handle concurrently with another call on that handle.
uint32_t ravet_runtime_executor_submit(void*, RavetRuntimeCallback call, RavetRuntimeCallback destroy, void* context);
void* ravet_runtime_timer_create(uint64_t interval_ms, RavetRuntimeCallback call, RavetRuntimeCallback destroy, void* context);
void ravet_runtime_timer_destroy(void*);
#ifdef __cplusplus
}
#endif
