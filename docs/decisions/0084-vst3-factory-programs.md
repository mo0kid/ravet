# 0084: Optional VST3 factory program mapping

Status: prototype, actual-bundle SDK/control/audio validation; DAW mapping pending.

## Context and decision

Consume Rust complete gain programs/metadata from 0083 in an explicit macOS
`--factory-programs` build. The default gain/bypass wrapper and other reference
formats remain unchanged. SDK EditControllerEx1 owns one root unit/list and an
ID-3 list/program selector with three discrete normalized values, nonautomatable.
Metadata names remain the fixed English reference catalogue. Control state,
rather than selected programme identity, remains the persisted record.

Controller requests update complete gain/bypass values and notify optional host
`kParamValuesChanged`, leaving actual processor delivery to host queues. Failure
rolls back local selector/control values; reentrant writes reject during callback.
No handler means local values only. This cannot roll back external host side
effects. Processor selector queues expand each point to two ordinary control
events at its frame, with shared stable insertion and existing DSP smoothing/
bypass. Both raw and expanded streams cap at 1024; complete validation precedes
mutation. Programme events alone max 512. No MIDI input or automatic AU/AAX
programme build is added.

## Validation

Actual module loading/factory interfaces, root/list/selector names/flags, rejected
notification rollback/reentrant writes, complete host control delivery, f32/f64
timed and zero-frame state/audio and expanded 512/513-point bounds pass 36 blocks
in six activations. SDK validator passes 47; optional native editor passes three
cycles. First compile lacked the SDK UString header. Validator then caught empty
names because UString128 copied an output array into private storage; borrowed
UString writes the actual array and fixes all names. Default flag-off regression
is separate. No DAW/other-format/architecture/deadline claim follows.
