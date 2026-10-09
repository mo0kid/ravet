# 0132: Explicit native note identities

Status: portable event and opt-in hosted CLAP graph route implemented;
synthetic offline evidence only.

Add concrete-channel/key On/Off with product-local u32 identity as NoteWithId,
leaving legacy NoteOn/Off intact. No allocator/uniqueness/voice release policy is
imposed. On velocity zero remains On. Borrowed event output, performance graph
routing and complete worker batches retain identity without owning heap data.
Backend limits are distinct from portable identity range.

Hosted builder identity selection requires an explicitly selected CLAP note port
per direction before activation. Input preserves native nonnegative i32 IDs;
disabled/unsupported/out-of-range input rejects the whole prefix before native
process/flush. Output preserves explicit IDs when selected; default filters them,
while wildcard-ID output retains legacy events. Native raw IDs below -1 reject
within the selected profile because Clack otherwise normalizes all negatives to
wildcard. Unrelated ports filter first. Performance empty-block/event-capacity and
output nonrollback fault policies remain unchanged. Native velocities narrow to
checked finite f32 0..1; no ID aliasing is inferred.

MIDI/MPE/UMP selection cannot drop IDs. Current generated plug-in performance
output uses MIDI, so identified notes reject whole generated delivery before any
host prefix, preserving already rendered audio/DSP. The public sine receiver does
not acquire identity/voice semantics. Choke/end/expression and per-note parameter
mapping remain separate work.

Tests cover full-u32 portable identity/scalar validity, 10000 complete queue
roundtrips, native ID0/i32::MAX and zero On, malformed negative IDs, matching
pre-activation dialect requirements and linked f32/f64 echo/audio timing,
1000-block profiles and valid-prefix failure with native state unchanged. These
are offline contracts, not a third-party instrument, DAW or deadline claim.
