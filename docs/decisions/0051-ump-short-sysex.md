# 0051: UMP short messages and caller-buffer SysEx assembly

Keep MIDI 1.0 short-message representation in MT=1/2 separate from MIDI protocol
translation. Defined fields retain their resolution; reserved receiver bits are
ignored and sender padding is zero. F0/F7 have no short-message UMP encoding.

SysEx7 and SysEx8 chunk codecs retain group, form, payload and optional stream
ID using thirteen fixed bytes. SysEx7 validates seven-bit data; SysEx8 supports
full bytes and explicit abort. The byte-count field for SysEx8 includes the stream
ID, while the public payload excludes it. Empty nonterminal chunks remain active.

Assembly borrows a caller buffer, accepts one explicit address at a time, and
checks ordering/capacity before every copy. Errors preserve accepted state; reset
and matching abort discard it. Complete payload borrows expire before subsequent
operations. No locks, I/O or allocation occur. Whole-stream interspersing rules,
concurrent-stream routing, manufacturer interpretation and negotiated stream
counts remain consumer responsibilities. Native delivery remains separate.

Tests cover independent wire words, every form/count/group, SysEx8 stream extrema,
reserved/malformed packets, canonical padding, explicit abort, wrong sequence,
capacity retry, empty storage and repeated caller-buffer reassembly.
