#ifndef RAVET_AU_HOST_H
#define RAVET_AU_HOST_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
/* Authored optional Apple AUv3 host ABI. All CONTROL calls use platform main;
   provider code is process-lifetime. Genuine live opaque handles and complete
   aligned nonaliased output storage are caller obligations. Only process borrows
   joined exclusive AUDIO; no raw Objective-C interface escapes. Enumeration
   reads explicit system registry filters, never installs or loads components.
   Loading executes selected trusted native code with OS-managed IPC when observed.
   No sandbox/crash/deadline proof follows. */
enum RavetAuHostStatusV1 { RAVET_AUH_OK=0,RAVET_AUH_PENDING=1,RAVET_AUH_INVALID=2,
 RAVET_AUH_BUSY=3,RAVET_AUH_CAPACITY=4,RAVET_AUH_WRONG_THREAD=5,
 RAVET_AUH_FOREIGN=6,RAVET_AUH_EXCEPTION=7,RAVET_AUH_FAULTED=8,RAVET_AUH_MODE=9 };
enum RavetAuHostLoadV1 { RAVET_AUH_AUTOMATIC=0,RAVET_AUH_IN_PROCESS=1,RAVET_AUH_OUT_OF_PROCESS=2 };
typedef struct RavetAuHostFilterV1 { uint32_t byte_size,type,subtype,manufacturer,maximum_components,reserved; } RavetAuHostFilterV1;
typedef struct RavetAuHostComponentV1 {
 uint32_t byte_size,type,subtype,manufacturer,flags,version,name_length,reserved;
 uint8_t name[4096];
} RavetAuHostComponentV1;
/* Independent domains: status is this host ABI, os_status is actual catalogue,
   format/resource/render OSStatus, error_code is original signed NSError code.
   Attempted/returned flags distinguish foreign exceptions and never-returned
   results; numeric zero alone does not prove a native call returned. */
typedef struct RavetAuHostDiagnosticV1 {
 uint32_t byte_size,attempted,returned,exception,has_os_status,has_error,domain_length,consumed;
 int32_t os_status;uint32_t reserved2;int64_t error_code;uint8_t domain[256];
} RavetAuHostDiagnosticV1;
typedef struct RavetAuHostInstanceInfoV1 {
 uint32_t byte_size,type,subtype,manufacturer,requested_mode,observed_in_process,reserved[2];
} RavetAuHostInstanceInfoV1;
typedef struct RavetAuHostProfileV1 {
 uint32_t byte_size,maximum_frames,input_channels,output_channels;double sample_rate;
} RavetAuHostProfileV1;
typedef struct RavetAuHostInputV1 {const float* samples;uint32_t capacity,reserved;} RavetAuHostInputV1;
typedef struct RavetAuHostOutputV1 {float* samples;uint32_t capacity,reserved;} RavetAuHostOutputV1;
typedef struct RavetAuHostBlockV1 {
 uint32_t byte_size,frames,input_channels,output_channels;
 const RavetAuHostInputV1* inputs;const RavetAuHostOutputV1* outputs;
 int64_t sample_position;
} RavetAuHostBlockV1;
typedef struct RavetAuHostEntryV1 {
 uint32_t byte_size,version;
 int32_t (*catalogue)(const RavetAuHostFilterV1*,void**,uint32_t*,RavetAuHostDiagnosticV1*);
 int32_t (*component)(const void*,uint32_t,RavetAuHostComponentV1*,RavetAuHostDiagnosticV1*);
 /* Facade retirement can be consumed while another AUDIO child is active.
    Protected identities/storage and foreign ownership remain held until final
    joined MAIN end; this is not permission for geometry/SDK controls. */
 int32_t (*close_catalogue)(void*,RavetAuHostDiagnosticV1*);
 int32_t (*create_async)(void*,uint32_t,uint32_t,void**,RavetAuHostDiagnosticV1*);
 /* Pending and preflight/thread/Busy rejection leave request owned. A completed
    poll reports consumed=1 and returns one instance or a terminal failure. Cancel consumes facade immediately, but
    keeps the process pending reservation until actual completion/MAIN cleanup. */
 int32_t (*poll)(void*,void**,RavetAuHostInstanceInfoV1*,RavetAuHostDiagnosticV1*);
 int32_t (*cancel)(void*,RavetAuHostDiagnosticV1*);
 int32_t (*prepare)(void*,const RavetAuHostProfileV1*,RavetAuHostDiagnosticV1*);
 int32_t (*begin_audio)(void*,void**,RavetAuHostDiagnosticV1*);
 int32_t (*process)(void*,const RavetAuHostBlockV1*,RavetAuHostDiagnosticV1*);
 int32_t (*end_audio)(void*,RavetAuHostDiagnosticV1*);
 int32_t (*stop)(void*,RavetAuHostDiagnosticV1*);
 int32_t (*destroy)(void*,RavetAuHostDiagnosticV1*);
} RavetAuHostEntryV1;
const RavetAuHostEntryV1* ravet_au_host_v1(void);
#ifdef __cplusplus
}
#endif
#endif
