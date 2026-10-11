# Optional skrifa outline backend

`postscript-outlines` adds exactly pinned skrifa 0.48.0 with default features off
and `std` on. Its selected direct runtime dependency is the already pinned
read-fonts 0.45.0; it does not replace the existing text reader or introduce a
second font instance. The package is dual licensed MIT OR Apache-2.0. See its
[primary package manifest](https://docs.rs/crate/skrifa/0.48.0/source/Cargo.toml),
[MIT licence](https://docs.rs/crate/skrifa/0.48.0/source/LICENSE-MIT), and
[Apache licence](https://docs.rs/crate/skrifa/0.48.0/source/LICENSE-APACHE).

The current dependency proof adds only skrifa and preserves all existing package
versions/checksums. Workspace feature unification is a separate adoption check.
The backend's CharString parsing/temporary memory and execution are distinct from
RAVET's own source DICT and whole-output bounds; no real-time guarantee is made.
