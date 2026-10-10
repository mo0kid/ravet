# Public AUv2 native performance instrument

The public MusicDevice component embeds the public native performance sine VST3
reference. RAVET owns bounded note input, opaque identity scheduling, complete
Level state validation and rejected-render cleanup; Apple/Steinberg SDK sources
remain external.
Its parameter schema/control/text and complete-state preflight consume the
[reusable native AU helpers](../../docs/native-au.md).
Prepared output buffer validation also uses their explicit planar bus mapper.
Their AU transport collector preflights supported host callbacks before DSP,
retains independent native availability and explicitly omits partial transport
from its VST3 projection.

Use `scripts/bundle-au.py --plugin performance` with the two explicit SDK paths.
It builds and tests the actual self-contained component under
`target/au-performance/release/AU/ravet_performance_au.component`, without global
installation. [The guide](../../docs/performance-au.md) describes note/state/build
contracts and actual local validation limits. The Gain default is unchanged.
