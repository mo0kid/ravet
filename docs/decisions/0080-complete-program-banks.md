# 0080: Complete immutable program banks and staged MIDI requests

Status: implemented portable mechanisms; format/consumer integration separate.

## Context

Stable parameter state and preset files do not provide a prepared in-memory
program catalogue or MIDI bank/program staging. Program indices can change when
a catalogue changes, so they should not replace permanent identity. Bank Select
and Program Change are distinct in the [MIDI Association overview](https://midi.org/about-midi-part-3midi-messages).

## Decision

Borrow explicit complete program entries using the existing schema. Reject the
entire catalogue on invalid metadata, duplicate stable IDs/MIDI addresses,
noncanonical physical values or capacity. Limits are 128 entries, 1024 parameters
and 65536 total values. No hidden allocation/default filling or partial load.
Exact lookups propose copied schema-bound states without a mutable current index.
Consumer publication/voice/click/persistence policies remain explicit.

A separate fixed sixteen-channel decoder stages CC0/32 and emits every Program
Change, including duplicates/unavailable requests. Pending bank edits do not
rewrite the last request. Its CC121/120/123 preserve bank/request; explicit and
System Reset clear. Master routing and sample scheduling remain outside the
wire decoder. Neither mechanism performs file, network, native host or UI work.

## Validation

Tests cover permanent/index/address independence, metadata/value/duplicate/total
capacity rejection, signed-zero/state-copy behavior, either bank-half order,
all channels, reset/repeat/unavailable requests and 10000 heap-free complete
selections. One metadata fixture moved a still-borrowed string; retaining that
string and constructing the oversized name separately fixes the test lifetime.
This does not establish General MIDI sounds, MIDI2 programs, MPE or native host
program support. The reference consumer/export is separate integration work.
