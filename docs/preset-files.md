# Explicit preset-file libraries

The optional `ravet-preset` crate connects observable-tree records to an existing
consumer-selected directory. `PresetLibrary::open` canonicalizes that directory,
validates state limits and retains an explicit eight-byte product identity.
No directory scan, preset selection, default path, watcher, host registration,
thread or network action occurs implicitly. Calls allocate/block on control or
worker threads. The crate reuses `ravet-runtime::files` and its existing dependency
graph; the portable umbrella and `ravet-state` gain no runtime dependency.

`discover` inspects up to 4,096 entries including unrelated files/directories.
It returns name-sorted metadata for regular files with the exact `.ravetpreset`
suffix and a valid name, skipping symlinks/directories. Oversized matching files
or too many entries reject the complete snapshot. Discovery does not decode
records: malformed or foreign-product contents are rejected when selected.
Metadata can become stale after external changes; no directory watch is implied.

Names are 1..128 ASCII letters/digits/spaces/underscore/hyphen, with no leading
or trailing spaces. Windows reserved device stems reject. This explicit portable
file-stem policy performs no sanitization and does not support Unicode display
titles, nested preset categories, aliases, renaming or deletion.

`save(name, store)` encodes and validates the complete product-tagged record
against library limits before replacing a same-named file. It reuses runtime
same-directory temporary-file atomic replacement, content sync and cleanup.
Parent directory sync/power-loss durability and concurrent directory ownership
remain outside that contract. No revisions or overwrite prompt are added.

`with_schema(schema)` sets the product's current preset schema (default 1).
`save` writes it into the record. `load_into(name, store)` bounds the selected
file read and validates product, record format, schema and library limits
before the destination sees any transaction. Newer schemas reject with
`UnsupportedSchema`; older ones reject unless loaded with
`load_into_migrating(name, store, migrate)`, whose hook receives the saved
schema and decoded tree. Files written by RAVET 0.0.1 load as schema 1. The
destination's limits also apply. Successful restore is one undoable complete
edit; malformed/missing/foreign/out-of-bounds records preserve state, revision
and undo/redo. The caller then publishes processor-specific parameter snapshots
through its control/audio bridge. These files do not implement host factory
program indices or CLAP/VST3 preset-discovery extensions.

Filesystem fixtures validate sorting, replacement, complete undoable recall,
failed-load history preservation, destination bounds, pre-write rejection,
schema rejection and migration,
symlink skipping on Unix, name policy and total-entry limits. They use disposable
synthetic files on macOS. Other runtime platforms, DAW preset registration and
product migration beyond the observable codec remain unvalidated. See
[decision 0035](decisions/0035-explicit-preset-files.md).
