#include "RavetVst3Editor.h"
#include <stddef.h>
/* Measured 64-bit macOS profile; Rust layout tests pin the same values. */
_Static_assert(sizeof(RavetVst3EditorSize)==16,"size record");
_Static_assert(sizeof(RavetVst3EditorReport)==24,"report");
_Static_assert(offsetof(RavetVst3EditorReport,native_result)==16,"native result offset");
_Static_assert(sizeof(RavetVst3EditorState)==40,"state");
_Static_assert(offsetof(RavetVst3EditorState,frame_reentry_refused)==28,"reentry counter offset");
_Static_assert(offsetof(RavetVst3EditorState,size_mismatches)==36,"mismatch counter offset");
_Static_assert(RAVET_VST3_EDITOR_STATE==16,"authoritative state flag");
_Static_assert(sizeof(RavetVst3EditorProviderV1)==88,"editor table");
_Static_assert(offsetof(RavetVst3EditorProviderV1,instance_origin)==8,"origin offset");
_Static_assert(offsetof(RavetVst3EditorProviderV1,create)==16,"create offset");
_Static_assert(offsetof(RavetVst3EditorProviderV1,destroy)==80,"destroy offset");
_Static_assert(RAVET_VST3_EDITOR_UNAVAILABLE==16&&RAVET_VST3_EDITOR_EMPTY==19,"companion statuses");
_Static_assert(RAVET_VST3_LIFECYCLE<RAVET_VST3_EDITOR_UNAVAILABLE,"shared domain unchanged");
int main(void){return 0;}
