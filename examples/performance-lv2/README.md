# Public native MIDI LV2 performance reference

Mono/stereo f32 LV2 instrument factories use the public 16-voice
`ravet-performance-native` archive, the reusable C++17 native LV2 profile and a
bounded Atom-sequence MIDI1 short-note adapter and exact block-start
`time:Position` collector. SDK headers stay external.

The build and actual-module contract commands are in
[the LV2 guide](../../docs/lv2.md#performance-instrument). The explicit `--plugin performance` bundling command creates
`target/lv2/ravet_performance.lv2` with public metadata/module/licence notices and
checks that actual packaged module. It is a local native prototype with no global
installation. No DAW discovery, MIDI device, callback
deadline or production platform support is established by the synthetic probe.

Ports are audio outputs, Level control, then the Notes and transport Atom input; there are no
audio inputs. The profile requires `urid:map`, supports complete short MIDI1
messages and maps NoteOn/NoteOff only. Other defined short messages are ignored;
SysEx, long payloads, running status and undefined status values reject the whole
block. Native expression/identity events and completion output are not exported
through this profile. State persists Level, rather than active voices/history.


Transport opts into the helper's fourth template argument. Exact Long sample and
Double beat coordinates, Float tempo/speed/signature numerator and Int signature
denominator receive whole-sequence preflight before state/DSP. Only offset-0
Position supplies context; later Positions validate then remain omitted without
extrapolation. Missing values remain unknown; speed other than 0/1 rejects rather
than inventing stopped state. Complete native signature carries DETAILS with
unknown/default recording/loop/pre-roll flags, which are not host-known false.
The DSP currently ignores transport. See the
[transport contract](../../docs/lv2.md#prepared-lv2-block-start-transport) for exact
encoding, storage/property bounds, availability and fault limits.
