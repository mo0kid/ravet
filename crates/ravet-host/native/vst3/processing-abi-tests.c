#include "RavetVst3Processing.h"
#include <stddef.h>
_Static_assert(sizeof(RavetVst3EventRecord)==112,"event ABI size");
_Static_assert(offsetof(RavetVst3EventRecord,ppq_position)==16,"ppq ABI offset");
_Static_assert(offsetof(RavetVst3EventRecord,integer_value)==64,"integer ABI offset");
_Static_assert(offsetof(RavetVst3EventRecord,payload_offset)==96,"payload ABI offset");
_Static_assert(sizeof(RavetVst3ContextRecord)==120,"context ABI size");
_Static_assert(offsetof(RavetVst3ContextRecord,project_time_samples)==16,"context exact signed samples");
_Static_assert(offsetof(RavetVst3ContextRecord,tempo)==72,"context tempo offset");
_Static_assert(offsetof(RavetVst3ContextRecord,smpte_offset_subframes)==100,"context SMPTE offset");
_Static_assert(offsetof(RavetVst3ContextRecord,samples_to_next_clock)==112,"context signed clock offset");
_Static_assert(sizeof(RavetVst3ProcessingOutputs)==72,"whole output ABI size");
_Static_assert(offsetof(RavetVst3ProcessingOutputs,parameter_points)==56,"output point destination");
_Static_assert(sizeof(RavetVst3ProcessingSelection)==16,"selection ABI");
_Static_assert(sizeof(RavetVst3ProcessingInfo)==32,"copied catalogue ABI");
_Static_assert(sizeof(RavetVst3EventBusRecord)==536,"copied bus ABI");
_Static_assert(sizeof(RavetVst3ProcessingReport)==56,"diagnostic ABI");
_Static_assert(offsetof(RavetVst3ProcessingReport,native_result)==32,"native result ABI");
_Static_assert(sizeof(RavetVst3ProcessingProviderV1)==56,"paired companion ABI");
_Static_assert(offsetof(RavetVst3ProcessingProviderV1,instance_provider)==8,"exact provider pair ABI");
int main(void) { return 0; }
