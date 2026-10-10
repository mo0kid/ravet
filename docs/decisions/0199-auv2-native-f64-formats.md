# 0199: Prepared AUv2 native f64 mapping and complete format preflight

Status: prototype; strict normal/fast-math mapping, actual fixed-f64 component
fixtures, the normal public Performance AU package and the existing Gain component
regression pass locally.

The native processor ABI already expresses pointer arrays in the originating
precision, and Apple SDK buffers allocate from ASBD byte sizes. The default SDK
format validator and distributed Steinberg wrapper currently assume f32. An
explicit typed mapper therefore extends reusable integration without broadening
that factory's accepted formats or casting f64 storage into f32 pointers.

The existing f32 names remain aliases to the float implementation, with double
aliases and the same fixed layout/range storage. Precision determines pointer
types, bytes and alignment; complete preflight and prior-map retention apply to
all directions before any publication. Input-only and output-only auxiliary buses
retain native order through compact independent AU element indices. Registration,
routing and full host lifecycle remain consumer-owned.

A separate complete format profile validates structural native table metadata and
precision, exact configuration/directional ASBD count, finite equal rate,
channel shape and native packed planar float encoding before candidate commit.
No mixed rate/precision profile is dispatched. Whole helper rejection retains
accepted configuration/format/map, without promising transactional rollback of
SDK allocation, notifications, backend failures or arbitrary consumer mutations.
Foreign live/aligned/disjoint ranges and exact originating table/library/handle
remain caller responsibilities. Parameter companion handles originate from that
companion's processor_entry(), not presumed address equality with another export.

Apple StreamFormat dispatch copies 36 original bytes into a zeroed ASBD; original
reserved/padding and exact-size rejection are not promised by direct helper tests.
Unchanged active formats can succeed as SDK no-ops. The actual fixture explicitly
separates these facts from stopped changed-format negotiation. A separate fixed64
in-process MusicDevice consumer owns mono/stereo native64 setup/render and complete
Level ClassInfo state, with no precision switching or AU parameter UI. f64 audio
does not change the native Level's f32 schema or persist transient voices/history.

Normal and optimized-fast-math fixtures exercise actual Gain64 precision-preserving
values, Performance64 timed analytic output, late ASBD/map faults and retained
profiles, maximum576outputs and2000ordinary C++heap-free map cycles. Actual component
renders validate correct64buffers, supported late caller fault silence/tails with
no DSP call, recovery, initialized changed-format rejection/unchanged no-op and
stopped native64 complete recall/rate changes. SDK sources remain external.

Public Performance AU remains f32: its inherited wrapper prepares kSample32 and
uses channelBuffers32. Factory migration remains separate work. Offline local
macOS arm64 fixtures establish no main/aux host registration, distributed64factory,
DAW/device/other architecture, hard deadline or all-backend allocation support.
