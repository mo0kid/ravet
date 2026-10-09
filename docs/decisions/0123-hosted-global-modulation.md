# 0123: Hosted native global parameter modulation

Status: prototype with linked and loaded public synthetic CLAP fixtures.

## Decision

Add independent input/output modulation opt-ins to the preparation builder,
requiring the corresponding explicit parameter mapping before activation. Cache
native `IS_MODULATABLE` metadata on main, alongside writable input identities and
output metadata. Global input amounts must be finite and mapped to writable,
modulatable native parameters; unsupported input rejects the whole block before
native mutation. Plain portable f32 amounts widen to native f64 with global targets
and null cookies. Native scalar base range/step validation remains independent;
modulation amounts do not undergo base range/step quantization.

Enabled output maps only explicitly selected writable native modulatable global
parameters. Disabled/unmapped/scoped output filters. Invalid selected amounts,
finite-f32 narrowing failure, ordering/offset or shared-capacity overflow clear
host audio/events with nonrollback backend failure. Zero-frame parameter flush
supports modulation without advancing render time. Native plug-in DSP owns
combination/clipping/smoothing and base persistence.

Expose a default-false `ClapParameterOptions::global_modulation` flag for reusable
registration. It advertises native capability without enabling bridge translation
or implementing DSP. Consumers explicitly align their metadata and processing.

## Evidence and limits

Linked f32/f64 fixtures cover twelve input/output profiles, 1000 blocks per profile,
zero-frame flush, shared capacity, sign/fractional amounts on a stepped base,
metadata mismatch, filtering and native base value preservation. Loaded public
fixture probes render 1000 blocks per precision with measured heap-free audio and
owner stop/main retirement. This does not establish a DAW/device/deadline guarantee,
modulation matrix, per-note handling, rescan/recovery or other-format hosting.
Graph limits remain 1024 events per direction; standalone native node profiles
retain their separate 4096 shared-slot limit. Trusted native code is not isolated.
