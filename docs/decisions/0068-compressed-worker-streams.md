# 0068: Lazy compressed reads and exact replay seeks

## Context

Complete compressed clips require storing decoded audio before playback. Worker
read-ahead needs bounded reads and exact decoded-frame seeks, while codec-native
timestamp/granule/preroll behavior differs across the supported formats.

## Decision

Provide a lazy worker reader over one immutable owned encoded byte buffer, using
the existing pinned/default-disabled Symphonia feature. Preserve channel/rate,
gapless and metadata/packet/sample limits. Reads retain pending packet suffixes;
errors clear the supplied output and poison reads, acknowledging consumed progress.
EOF alone checks declared duration/integrity.

Exact seeks construct/replay a fresh decoder to the target decoded-frame index,
then swap only on success. Sharing encoded bytes avoids another full source copy;
failed replay preserves old state. This consciously trades O(target) worker decode
for exact reproducible positioning without claiming native indexed codec seeking.

## Consequences and validation

Encoded bytes remain memory-backed and bounded. Backend allocations/deadlines
are not guaranteed. No file-backed source, automatic buffering/clock scheduler,
resampling, speaker remapping or callback-safe seek is added here.

Tests compare all three actual synthetic encoded formats against whole decoding
for partitioned reads and exact forward/backward/boundary/end seeks; corruption,
poison/recovery, output tails and failed-seek preservation are covered separately.
EOF verification must still occur after reaching a nominal end position.
