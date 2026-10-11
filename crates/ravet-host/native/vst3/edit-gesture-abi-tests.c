#include "RavetVst3EditGestures.h"
#include <stddef.h>
/* Measured 64-bit macOS profile; Rust layout tests pin the same values. */
_Static_assert(sizeof(RavetVst3EditGesture)==32,"gesture record");
_Static_assert(offsetof(RavetVst3EditGesture,value)==16,"value offset");
_Static_assert(offsetof(RavetVst3EditGesture,sequence)==24,"sequence offset");
_Static_assert(sizeof(RavetVst3EditGestureState)==104,"gesture state");
_Static_assert(offsetof(RavetVst3EditGestureState,accepted)==24,"first counter offset");
_Static_assert(offsetof(RavetVst3EditGestureState,unpaired)==96,"last counter offset");
_Static_assert(sizeof(RavetVst3EditGestureProviderV1)==40,"gesture table");
_Static_assert(offsetof(RavetVst3EditGestureProviderV1,instance_origin)==8,"origin offset");
_Static_assert(offsetof(RavetVst3EditGestureProviderV1,enable)==16,"enable offset");
_Static_assert(offsetof(RavetVst3EditGestureProviderV1,state)==32,"state offset");
_Static_assert(RAVET_VST3_EDIT_GESTURE_CAPACITY==256&&RAVET_VST3_EDIT_OPEN_GESTURES==64,"ring capacity");
_Static_assert(RAVET_VST3_EDIT_RESTART_SUPPORTED==28&&RAVET_VST3_EDIT_SYNTHESIZED==1,"restart and synthesized flags");
_Static_assert(RAVET_VST3_EDIT_BEGIN==1&&RAVET_VST3_EDIT_RESTART_REFUSED==5,"gesture kinds");
int main(void){return 0;}
