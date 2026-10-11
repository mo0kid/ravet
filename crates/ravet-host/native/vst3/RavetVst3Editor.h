#ifndef RAVET_VST3_EDITOR_H
#define RAVET_VST3_EDITOR_H
#include "RavetVst3Provider.h"
#ifdef __cplusplus
extern "C" {
#endif
/* Additive editor companion paired with the exact instance ABI-v2 origin.
   Existing provider/state/parameter/processing/hosted layouts and domains are
   unchanged. macOS NSView only; every entry requires the platform main thread. */
typedef struct RavetVst3Editor RavetVst3Editor;
enum { RAVET_VST3_EDITOR_PLATFORM_NSVIEW = 1 };
enum { RAVET_VST3_EDITOR_MINIMUM_EXTENT = 1, RAVET_VST3_EDITOR_MAXIMUM_EXTENT = 16384 };
/* Companion-only statuses after the shared RAVET_VST3_* domain. */
enum { RAVET_VST3_EDITOR_UNAVAILABLE = 16, RAVET_VST3_EDITOR_UNSUPPORTED = 17,
       RAVET_VST3_EDITOR_REENTRY = 18, RAVET_VST3_EDITOR_EMPTY = 19 };
/* Report stages: 200 admission, 201 createView, 202 isPlatformTypeSupported,
   203 setFrame, 204 getSize, 205 attached, 206 canResize, 207 checkSizeConstraint,
   208 onSize, 209 setContentScaleFactor, 210 removed, 211 setFrame(nullptr),
   212 release, 213 queryInterface. Failures: 0 none, 1 local, 2 returned foreign
   refusal, 3 foreign throw, 4 capacity, 5 allocation, 6 malformed size,
   7 unavailable, 8 unsupported, 9 reentry/lifecycle. native_result is nonzero
   only with RETURNED. STATE marks a report written by a live editor owner on
   MAIN: ATTACHED then states the true attached state after the call, including
   for refusals. Without STATE (provider-level refusals) ATTACHED carries no
   information. DEFERRED: a plug-in resize requested during attached/scale was
   applied in this call. */
enum { RAVET_VST3_EDITOR_RETURNED = 1, RAVET_VST3_EDITOR_THROWN = 2,
       RAVET_VST3_EDITOR_ATTACHED = 4, RAVET_VST3_EDITOR_DEFERRED = 8,
       RAVET_VST3_EDITOR_STATE = 16 };
typedef struct RavetVst3EditorSize {
    uint32_t struct_size, width, height, reserved;
} RavetVst3EditorSize;
typedef struct RavetVst3EditorReport {
    uint32_t struct_size, stage, failure, flags;
    int32_t native_result;
    uint32_t reserved;
} RavetVst3EditorReport;
/* State flags: 1 attached, 2 resizable (last canResize), 4 content-scale
   interface present, 8 consumer request pending, 16 faulted, 32 deferred
   request pending (acknowledged, waiting for the next attach). Counters
   saturate. deferred_discarded counts acknowledged deferred requests that were
   superseded, refused by the plug-in's own onSize, or lost to a fault/teardown.
   size_mismatches counts onSize calls whose re-queried getSize differed or
   failed (the re-queried size is recorded when valid). */
typedef struct RavetVst3EditorState {
    uint32_t struct_size, flags, width, height;
    uint32_t frame_calls, frame_applied, frame_refused, frame_reentry_refused;
    uint32_t deferred_discarded, size_mismatches;
} RavetVst3EditorState;
typedef struct RavetVst3EditorProviderV1 {
    uint32_t struct_size, version;
    const RavetVst3ProviderV2* instance_origin;
    int32_t (*create)(RavetVst3Instance*, uint32_t, RavetVst3Editor**, RavetVst3EditorSize*, RavetVst3EditorReport*);
    int32_t (*attach)(RavetVst3Editor*, void*, RavetVst3EditorSize*, RavetVst3EditorReport*);
    int32_t (*size)(RavetVst3Editor*, RavetVst3EditorSize*, RavetVst3EditorReport*);
    int32_t (*resize)(RavetVst3Editor*, const RavetVst3EditorSize*, RavetVst3EditorSize*, RavetVst3EditorReport*);
    int32_t (*set_scale)(RavetVst3Editor*, float, RavetVst3EditorSize*, RavetVst3EditorReport*);
    int32_t (*detach)(RavetVst3Editor*, RavetVst3EditorReport*);
    int32_t (*state)(const RavetVst3Editor*, RavetVst3EditorState*);
    int32_t (*take_request)(RavetVst3Editor*, RavetVst3EditorSize*);
    int32_t (*destroy)(RavetVst3Editor*, RavetVst3EditorReport*);
} RavetVst3EditorProviderV1;
/* create requires a null output handle; failure leaves it null. One live editor
   per child; instance destroy returns LIFECYCLE while it lives. The caller's
   NSView parent must stay alive until detach succeeds or destroy returns.
   destroy consumes on OK or RETIRED_NATIVE (first foreign failure reported);
   THREAD/INVALID/REENTRY are refusals before teardown starts and retain the
   editor. No view, frame or controller pointer is returned. Idle plug-in frame
   resizes apply onSize on MAIN only while attached and publish a one-slot
   consumer request read by take_request (EMPTY when none); detach, retire and a
   successful host resize clear it. checkSizeConstraint must return kResultTrue
   (== kResultOk); any other result is a returned refusal, with no fallback. */
const RavetVst3EditorProviderV1* ravet_vst3_editor_provider_v1(void);
#ifdef __cplusplus
}
#endif
#endif
