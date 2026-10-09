#pragma once
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
/* Immutable public gain reference catalogue. No handle/current identity or
   host/DSP mutation. Indices 0..2, stable IDs 1..3: Unity [1,0], Silence [0,0],
   Bypass [1,1]. Complete physical gain/bypass; MIDI bank 0/numbers 0..2.
   Names are complete UTF-8, NUL-terminated and zero-padded to 128 bytes.
   Caller exclusively owns a live writable aligned output struct. All fields
   are untouched on failure. Returns 0 success, 1 rejected, 2 contained panic.
   Fixed validation/copy has no normal allocation, locks or I/O. Consumers own
   host metadata conversion, selection publication/smoothing/history and state.
   This is a reference catalogue, not a generic SDK ABI or persisted selection. */
typedef struct {
  uint32_t id;
  float gain;
  uint32_t bypass;
  uint16_t midi_bank;
  uint8_t midi_number, reserved;
  char name[128];
} RavetGainProgramInfo;
uint32_t ravet_gain_program_count(void);
int32_t ravet_gain_program_info(uint32_t index, RavetGainProgramInfo *output);
#ifdef __cplusplus
}
#endif
