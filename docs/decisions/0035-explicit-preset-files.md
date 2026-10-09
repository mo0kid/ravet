# 0035 — Explicit preset-file libraries

Status: implemented within the documented control-thread filesystem scope.

Keep file discovery outside the portable tree/state model. The optional preset
crate uses state codecs plus the existing Rust runtime bounded-read/atomic-write
mechanics. Reusing runtime dependencies avoids a second persistence backend;
it does not add network activity or a runtime dependency to portable state.

An explicit existing directory, product tag and decoding limits determine the
library. Discovery is sorted metadata bounded to 4,096 directory entries; files
are decoded only on explicit selection. Portable ASCII stems cap at 128 bytes,
skip categories/aliases and reject reserved names rather than silently rewrite.
Complete loads validate both library and destination limits before one undoable
restore; failures preserve state/history. Saves validate before replacement.

Ownership, concurrent external directory changes and release/user path policy
remain consumer concerns. No watcher, worker, host program registration, rename,
delete, automatic DSP application or power-loss guarantee is supplied. Tests use
disposable synthetic files and prove local contracts, not DAW preset integration.
