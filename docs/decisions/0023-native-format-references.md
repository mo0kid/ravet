# 0023 — AAX and LV2 reference adapters share Rust gain

Status: accepted for development references, 9 October 2026.

Keep format lifecycle and metadata in native reference adapters and processing
in the existing Rust gain ABI. AAX uses externally supplied Steinberg/Avid SDKs;
LV2 uses externally supplied permissive API headers. No SDK sources, consumer
wrappers or proprietary product policies enter this repository. Artifact scripts
have local target outputs and avoid normal host installation helpers.

AAX exposes native mono/stereo variants, without AudioSuite/MIDI/custom GUI.
Its actual binary load, entry symbols and SDK-version query are verified;
Pro Tools host acceptance needs separately authorized signing or a developer
host. Ad hoc macOS signing does not meet that requirement.

LV2 exposes block-level gain/bypass ports and host-owned control state. A native
loader verifies actual exported descriptors and processing. Sample event, atom,
UI and generic format contracts are separate unfinished capabilities. No hard
real-time or production compatibility declaration follows from offline tests.

See [AAX](../aax.md) and [LV2](../lv2.md) for commands and exact limits.
