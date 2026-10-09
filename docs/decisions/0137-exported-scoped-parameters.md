# 0137: Exported endpoint-scoped parameter values and modulation

Status: portable events, exported CLAP translation and explicit metadata flags
implemented; offline adapter/metadata evidence only. Hosted metadata validation
and instrument voice application are independent work.

Keep saved global f32 parameter events intact. Add distinct finite f64 scoped
value/modulation events with stable parameter ID and NoteTarget wildcards within
a routed note input endpoint. All-wildcard scope still addresses that port, not
global state. Receivers own parameter ranges, voice matching/precedence, base
state, clipping/smoothing and lifetime. Parameter graph filters include scoped
events and performance filters exclude them; complete worker queues retain bits.

Exported adapters opt into exact input-port scoped translation/delivery separately
from note/MIDI/output selectors and global modulation. Default scoped input filters;
missing generated scope routes reject whole delivery before host prefix. Native
IDs are nonnegative i32; raw scope validates before Clack normalization, and only
four exact -1 fields count as global. Invalid selected fields/values reject whole
input pre-DSP; unrelated/wildcard ports filter. Scoped parameters retain process
timing and use offset zero in parameter flush. Generated cookies stay empty.

Add independent explicit per-note-ID/key/channel/port automation/modulation flags
to registration options, all false by default with NONE/ALL const presets. Flags
do not enable DSP or translation; global flags remain independently selected.
Gain explicitly keeps NONE. Native CLAP consumers must implement the protocol's
polyphonic override semantics; adapters only retain the event information.

Tests cover all 256 flag combinations, portable full-f64 ranges/scope/invalid
prefixes, graph filter classification, 10000 complete queues/translation workloads,
native main f32/f64 and auxiliary f64 1000-block process/flush precision and
pre-DSP rejection. Whole generated missing-route/ID-width rejection retains audio.
These do not establish third-party voice, DAW, physical device or deadline support.
