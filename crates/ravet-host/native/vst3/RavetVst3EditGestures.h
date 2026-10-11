#ifndef RAVET_VST3_EDIT_GESTURES_H
#define RAVET_VST3_EDIT_GESTURES_H
#include "RavetVst3Provider.h"
#ifdef __cplusplus
extern "C" {
#endif
/* Additive edit-gesture companion paired with the exact instance ABI-v2 origin.
   Existing provider/state/parameter/processing/hosted/editor layouts and
   domains are unchanged. Children that never call enable keep the original
   component handler behaviour: beginEdit/performEdit/endEdit return
   kNotImplemented and restartComponent coalesces 0..0xfff flags.

   After enable, the child's IComponentHandler accepts edit gestures on the
   platform main thread only, into a fixed per-child ring of
   RAVET_VST3_EDIT_GESTURE_CAPACITY records:
   - beginEdit/endEdit(id) and performEdit(id, value) need an id that was a
     writable (not read-only, not hidden) parameter in the refreshed catalogue
     when enable ran, and performEdit a finite value in 0..1. Otherwise
     kInvalidArgument (counted invalid or unknown).
   - Gesture pairing: at most RAVET_VST3_EDIT_OPEN_GESTURES ids may be open
     (begun, not ended). Each open gesture reserves one ring slot, so the
     endEdit of an accepted beginEdit is never refused for capacity. A begin
     needs two unreserved slots (itself and its end). A second begin of an
     open id, and an end without an open begin, return kResultFalse (counted
     unpaired) and are not recorded, so drained records are paired, with these
     exceptions: destroying the child's editor records a synthesised END
     (flags RAVET_VST3_EDIT_SYNTHESIZED) for every open gesture, after which a
     late endEdit for that id is refused as unpaired; gestures still open when
     the child retires are not closed; and records lost by the consumer (a
     malformed batch or a panicking callback on the Rust side) cannot be
     re-sent. While a gesture is open, the next endEdit closes it; at most 64
     reserved slots are held. kNoParamId (0xFFFFFFFF) is refused as unknown.
     A 64-bit sequence exhaustion (unreachable) refuses further records.
   - Otherwise a full ring returns kResultFalse and counts overflowed. Nothing
     accepted is overwritten or dropped; the record waits for drain.
   - Off-main calls return kResultFalse (counted off_thread) and calls after
     child retirement return kResultFalse (counted retired).
   - restartComponent accepts only kParamValuesChanged (4), kLatencyChanged (8)
     and kParamTitlesChanged (16), in any combination. A call carrying any other
     defined flag (0..0xfff) is refused whole with kResultFalse. Zero or
     undefined bits return kInvalidArgument (counted invalid); after child
     retirement it returns kInvalidArgument as before (counted retired). Off-main
     restarts return kResultFalse (counted off_thread). Accepted and refused restarts are
     both recorded (kinds 4 and 5) when the ring has room; an accepted restart
     with a full ring is refused as overflow. Accepted flags still coalesce
     into restart_requests.
   The handler never calls foreign or Rust code, so plug-in reentry from view,
   parameter or processing callbacks is safe; it is a leaf on MAIN. */
enum { RAVET_VST3_EDIT_GESTURE_CAPACITY = 256, RAVET_VST3_EDIT_OPEN_GESTURES = 64 };
enum { RAVET_VST3_EDIT_BEGIN = 1, RAVET_VST3_EDIT_PERFORM = 2, RAVET_VST3_EDIT_END = 3,
       RAVET_VST3_EDIT_RESTART = 4, RAVET_VST3_EDIT_RESTART_REFUSED = 5 };
enum { RAVET_VST3_EDIT_RESTART_SUPPORTED = 4 | 8 | 16 };
/* flags bit on an END record synthesised when the editor closed with that
   gesture open (see pairing below). */
enum { RAVET_VST3_EDIT_SYNTHESIZED = 1 };
/* One captured gesture. id is 0 for restarts; flags carries restart flags;
   value is the normalized performEdit value, 0 otherwise. sequence counts
   every recorded gesture of the child from 0 without gaps or wrapping. */
typedef struct RavetVst3EditGesture {
    uint32_t struct_size, kind, id, flags;
    double value;
    uint64_t sequence;
} RavetVst3EditGesture;
/* flags: 1 enabled, 2 handler retired. pending is the number of undrained
   records; open the number of open gestures (each reserving one end slot).
   unpaired counts refused begins of an already open id and ends without an
   open begin. Counters saturate at UINT64_MAX. */
typedef struct RavetVst3EditGestureState {
    uint32_t struct_size, flags, capacity, pending, open, reserved;
    uint64_t accepted, drained, overflowed, invalid, unknown, off_thread, retired;
    uint64_t restart_accepted, restart_refused, unpaired;
} RavetVst3EditGestureState;
typedef struct RavetVst3EditGestureProviderV1 {
    uint32_t struct_size, version;
    const RavetVst3ProviderV2* instance_origin;
    int32_t (*enable)(RavetVst3Instance*);
    int32_t (*drain)(RavetVst3Instance*, RavetVst3EditGesture*, uint32_t, uint32_t*);
    int32_t (*state)(const RavetVst3Instance*, RavetVst3EditGestureState*);
} RavetVst3EditGestureProviderV1;
/* enable is CONTROL work on MAIN: the child must have a controller and a
   refreshed parameter catalogue, and no AUDIO capability of the module may be
   live. It snapshots the writable parameter ids (LIFECYCLE otherwise; CAPACITY
   is impossible within the 1024-parameter catalogue bound). Re-enabling
   refreshes the snapshot and keeps undrained records.
   drain and state run on MAIN, take the module's busy flag (LIFECYCLE while
   another provider entry runs) but not the live-core CONTROL admission, so they
   work while the bank's AUDIO leases are live. drain copies up to capacity of
   the oldest records (capacity 1..RAVET_VST3_EDIT_GESTURE_CAPACITY), writes the
   copied count and removes them; output records must not overlap the
   child/module/core owners or the count. A child that was never enabled
   reports LIFECYCLE from drain and flags 0 from state. */
const RavetVst3EditGestureProviderV1* ravet_vst3_edit_gesture_provider_v1(void);
#ifdef __cplusplus
}
#endif
#endif
