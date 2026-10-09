# 0118: Bounded input acknowledgement for graph output workers

Status: implemented within the explicit buffered worker prototype.

## Decision

Permit 0..1024 configured input events and pass an owner-borrowed stream to each
fresh fixed render. Report input_events_consumed only for successful new renders;
while draining retained audio, neither new input nor its transport is consumed.
Caller retains a staged batch/context until a later render. Preserve the existing
empty-input pump, fixed block/queue semantics and zero output-event capacity.

Let the graph validate input before nodes execute. Preserve nonrollback graph
fault semantics rather than claiming delivery acknowledgement on failure or
retrying potentially applied notes/parameters. Do not infer event timestamps,
late-event retiming, device clocks or asynchronous command scheduling.

## Evidence and limits

Portable tests check exact offsets, once-only parameter mutation despite repeated
pending pumps, rejected outer input without publication/mutation, retained frame
order and heap-free pump/consume. Loaded public Sine's offline-only note mode
checks native MIDI graph input, delayed onset/finite stereo sound/release silence
through the prepared queue. Physical runs remain silent; offline note evidence
does not establish audible hardware, physical MIDI or host/deadline support.
Cross-thread event queues, output events, live graph edits, GUI/recovery/clock
integration remain separate work.
