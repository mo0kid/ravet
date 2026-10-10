# 0142: Generic prepared native planar adaptation

Status: reusable f32/f64 adapter and migrated public Gain ABI implemented and
validated with scoped Rust tests and rebuilt native bundles. No generic SDK
registration or additional DAW support claim.

Place native pointer processing in optional ravet-native, outside the portable
umbrella and independent of SDK/OS/product dependencies. Retain processor ownership
in consumers through exclusive borrowing. Prepare copied-input/output planes and
bounded generated events once; validate frames/events before byte arithmetic and
limit owned payload to64 MiB. Deactivate before preparation and reject invalid
configuration without callback-time storage work. Control panic containment stays
in foreign wrappers.

Use the prepared main/auxiliary order and raw C-compatible bus counts/pointers.
Validate shape, pointer presence/alignment and output range disjointness. Copy
every input across every bus before host output writes; build safe private planar
views and validate the entire portable event stream before invoking DSP. Supply
exact borrowed transport, without format conversion, extrapolation or implied
reset. Return bounded typed output events for consumer native delivery.

Silence supported declared raw outputs on faults, retaining tails and discarding
event delivery without rolling back DSP history. Processing/reset panic poisons
the adapter and requires fresh stopped preparation; reset does not conceal that
exception. Normal paths avoid heap operations, while exceptional panic handlers
carry no deadline promise. Foreign range validity and serialized ownership remain
unsafe caller obligations.

Migrate both public Gain precision paths to the reusable core, retaining existing
C symbols/product state and metadata. Separate input/output scratch increases
maximum Gain audio payload to1 MiB f32/2 MiB f64. Gain keeps bounded native event
decoding and result values in its product wrapper. Reusable schemas/event/state
marshalling/SDK registration are independent implementation packages.
