# 0054: Bounded hosted parameters and inactive edits

Own bounded parameter metadata while exclusively borrowing the live CLAP instance.
Retain stable IDs, plain f64 ranges/defaults, flags and raw display bytes; do not
cache opaque cookies or infer normalized mappings. Validate the whole catalogue
before replacing it. Explicit refresh leaves rescan scheduling with consumers.

Plain reads validate returned values. Inactive edits reject unknown/read-only IDs,
nonfinite/out-of-range values and nonintegral steps before invoking plugin code.
Clack's inactive handle prevents dispatch on active instances. Output queues are
caller-owned; dispatch is not acknowledgement. Active automation stays with audio
processing and persistence stays with the plugin state extension.

Linked Gain tests verify edits and active rejection; a synthetic parameter
provider exercises invalid metadata, capacity, failed-refresh preservation and
read-only/invalid returned values. The trusted offline host prints actual plain
values. No automatic rescan or host parameter extension is advertised.
