# ADR0214: bounded readonly native table grid

Status: Accepted

## Context

Fixed list/tree controls provide row navigation but not rectangular cell focus
or explicit column-header associations. A readonly table grid supplies those
contracts while retaining separate selected row models.

## Decision

Use one consumer-supplied exact native table/thead/tr/th(scope=col)/tbody/tr/td
profile, at most256 rows,32 columns,4096 body cells and16384 scanned descendants.
Disallow extra sections/caption, spans, nested tables and interactive/editable/
other-focusable content. Require original live document/frame/ID ownership,
explicit bounded names and fixed rectangular membership before any mutation.

Root virtual focus points to a body cell. Header eligibility explicitly removes
columns from navigation/admission. Cell focus and single/multiple row selection
remain independent; selected unavailable rows survive refresh, while later new
unavailable additions reject. Quiet fallback uses nearest column in current row,
then nearest row/column, with forward tie-breaks. Separate input/selection
callbacks use bounded frozen row IDs/indices and active cell coordinates.
Guard focus/consumer reentry and every owned DOM write, permitting disposal and
exact original attribute cleanup. Committed notification cannot roll back after
consumer mutation/failure. Fixed structural/profile changes retire.

## Consequences

This is readonly cell navigation/row selection, not data sorting, editing,
dynamic membership, virtualization, column resizing/reorder or embedded control
support. No sorting callback is advertised as implemented sorting. Native table
geometry stays consumer-owned. Bounded model/scan costs do not bound browser
layout or global document/ancestor cost. Chromium/WK DOM and trusted browser
delivery evidence do not certify native AX/screen-reader/OS focus/platforms or
real-time deadlines. No new dependency or generated native nodes are required.
