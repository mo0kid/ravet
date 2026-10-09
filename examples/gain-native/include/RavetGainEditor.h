#ifndef RAVET_GAIN_EDITOR_H
#define RAVET_GAIN_EDITOR_H
#include <stdint.h>
#include <stddef.h>
#ifdef __cplusplus
extern "C" {
#endif
// Public gain reference UI-only ABI, macOS native-editor feature. No DSP thread
// calls. UTF-8 HTML is static library-owned; do not free or retain beyond unload.
const uint8_t* ravet_gain_editor_html(size_t* length);
// Decode bounded JSON edit/begin/value/end with known stable parameter IDs.
// Writes output only on success. Kind 0=edit,1=begin,2=value,3=end. Values use
// normalized host units; finite physical values clamp/snap to the gain schema.
int32_t ravet_gain_editor_message(const uint8_t* json,size_t length,
    uint32_t* kind,uint32_t* id,double* normalized);
// Encode current finite normalized controller values into caller-owned UTF-8 JSON.
// Returns 0 on rejection. Status 0=ready,1=edit rejected,2=gesture end rejected.
size_t ravet_gain_editor_snapshot(double gain,double bypass,uint32_t status,
    uint8_t* output,size_t capacity);
// UI-only host-delivered meter triples: normalized peak/RMS/diagnostic,
// input L/R then output L/R. Exactly twelve readable doubles, finite 0..1.
// Levels map to 0..4, flags >=0.5. No live DSP read; distinct output range.
// Returns 0 without output mutation on rejection, capacity <=65536.
size_t ravet_gain_editor_meter_snapshot(double gain,double bypass,uint32_t status,
    const double* values,uint8_t* output,size_t capacity);
#ifdef __cplusplus
}
#endif
#endif
