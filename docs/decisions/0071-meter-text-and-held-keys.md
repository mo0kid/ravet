# 0071: Meter text and held navigation keys

## Context

Visual meter bounds can hide overload levels, and releasing an unrelated key
must not split a held keyboard edit into multiple host gestures.

## Decision

Track recognized navigation keys as a set owned by the slider gesture. End after
the last key release, cancellation or blur, retaining retry semantics if the
transport rejects the end. Preserve native input keyboard behavior.

Provide a separate native-meter wrapper with a required label and visible text.
Clamp only the visual RMS position; expose full finite peak/RMS as dB through
visible text and aria-valuetext. Snapshot/formatter validation precedes mutation,
and text never becomes HTML. Consumers own localization through a formatter,
layout/focus CSS and assistive-technology audits. Meters do not send host edits.

## Consequences and validation

The gain page consumes all five shared controls. Native WebKit fixtures check
multiple held keys, unrelated releases, overload and malformed readings,
literal custom text and disposal over three complete editor lifecycles.
Browser keyboard probing confirms tab order and edit messages. Its document is
unfocused after automation, preventing a visible-focus claim. Native screenshot
inspection confirms readable unclipped meter text; no screen-reader audit or
cross-platform accessibility support is inferred.
