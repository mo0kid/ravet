# WAV/AIFF streams and audio handoff buffers

The explicit `ravet-audio-file` crate supplies bounded seekable WAV/AIFF readers and
writers plus a prepared interleaved SPSC audio ring. File methods allocate or
perform blocking I/O on control/worker threads. Prepared ring push/pop and transport renders belong in audio callbacks; endpoint
destruction follows callback shutdown.

## WAV decoding and recording

Supported encodings are little-endian RIFF PCM16/24/32 and IEEE Float32, with
1..64 interleaved channels and sample rates 1..768000 Hz. Scratch capacity is
1..65536 frames. There is no implicit resampling, channel reorder or speaker
mask interpretation. Extensible WAV, RF64/W64, big-endian WAVE, PCM8 and AIFF-C
remain separate work. Optional FLAC writing is implemented below; other
compressed writing remains separate work. Optional FLAC, Ogg/Vorbis and MP3 reading
is described below.

Readers inspect bounded RIFF chunks from offset zero, validating file extent,
fmt/data uniqueness, byte rate, frame alignment and chunk padding. Unknown
chunks are skipped; at most 4096 chunks and 4096 fmt bytes are inspected. Bytes
outside declared RIFF are ignored. Read returns frame count and preserves the
remaining destination. Frame seeks are validated against length. A read I/O
failure preserves destination and logical position; retry seeks back before
reading again. Float samples preserve bits, including nonfinite values.

Writers require an initially empty seekable destination. Blocks must contain
whole frames within scratch capacity. PCM finite values clip to [-1,1], scale
by 2^(bits-1), round and saturate to the signed integer range. The positive
endpoint differs from unity by one integer LSB (f32 decoding may round it).
Nonfinite PCM input rejects before write. Float32 stores exact bit patterns and
includes an extended fmt/fact chunk. Explicit `finish` patches RIFF/data/fact
sizes, adds odd-data padding and flushes. RIFF size limits reject before writing.
Drop does no file I/O. Partial write failure poisons the writer; discard that
partial file rather than treating it as a finalized recording.

## AIFF decoding and writing

`AiffReader` and `AiffWriter` supply uncompressed big-endian PCM16/24/32 with
1..64 interleaved channels, integer rates 1..768000 Hz and 1..65536 scratch frames.
The extended80 sample rate is validated exactly; fractional, negative, denormal,
infinite or out-of-range rates reject rather than rounding. Readers validate FORM
extent, unique COMM/SSND, exact declared frame bytes and odd chunk padding within
at most 4096 chunks. Unknown chunks and bytes outside FORM are ignored. SSND
offset bytes are skipped; nonzero blockSize is unsupported. Zero-frame files may
omit SSND. AIFF-C, other sample widths, metadata/loop markers and compression are
not implemented. The layout follows the original [Apple AIFF specification](https://ich.music.mcgill.ca/classes/synth/AudioIFF1.2.1/AudioIFF1.2.1.html).

Reads and frame seeks have the same destination/position preservation and retry
policy as WAV. Writers require an empty destination, reject invalid complete
blocks before I/O, clip/round finite PCM and explicitly finalize FORM/COMM/SSND
lengths and padding. Partial writes poison further writing/finalization; Drop does
no I/O. File size must fit the 32-bit FORM limit. AIFF and WAV also have explicit
read-ahead/recorder adapters sharing the same prepared handoff mechanics below.

Create a new synthetic interoperability file (existing files reject):

```sh
cargo run -p ravet-audio-file --example aiff --locked -- target/test.aiff
```

The local macOS Core Audio `afinfo`/`afconvert` inspection decoded the generated
stereo PCM24 fixture exactly. This is file interoperability evidence, not physical
recording or DAW/device validation.

## Optional compressed clip decoding

Enable `ravet-audio-file`'s `compressed` Cargo feature for `decode_compressed` and
`DecodedClip`. Pinned Symphonia 0.5.5 supplies FLAC, MP3 and Ogg/Vorbis decoding;
its other codecs/containers, SIMD and default feature set are disabled. The feature
is off by default and stays outside the portable umbrella dependency graph.
Symphonia's source is [MPL-2.0](https://github.com/pdeljanov/Symphonia/blob/v0.5.5/symphonia/Cargo.toml);
retain its notices and separately audit distribution/source obligations for the
selected consumer edition. It is not relicensed by RAVET's commercial agreement.

Supply encoded bytes explicitly on a worker/control thread. Input copy is limited
to 64 MiB, output to caller frames and 16,777,216 f32 samples (64 MiB), decoded
packet capacity to 65536 frames and demux progress to 1,048,576 packets. Rates are
1..768000 Hz and channel count 1..64. Backend metadata/visual limits are 64 KiB/0.
These bound RAVET's accepted input/output; dependency-internal allocations and
execution time remain backend-owned, with no total-memory or deadline guarantee.
No filesystem/network/clock access occurs inside decoding. Encoded readers,
Other compressed encoders, file-backed random access and automatic clocked
transport remain separate work. The lazy worker reader below supplies exact replay
seeks; its explicit native-FLAC indexed mode avoids replaying earlier audio.

The selected default track retains encoded rate/channel order. Configuration
changes/chained resets, decode errors, nonfinite samples, declared-length mismatch
and backend integrity failure reject the whole result. There is no partial clip,
resampling, normalization or speaker mapping. Backend end-of-stream behavior is
used when no declared length exists; this does not guarantee every truncated or
corrupt stream can be detected. Gapless trim is enabled and the three synthetic
fixtures return the declared 4096 frames. FLAC matches source PCM exactly; lossy
fixtures pass sample-error/channel-amplitude checks. Independent FFmpeg agreement
is exact for FLAC and near floating-point precision for MP3. For this Vorbis
fixture FFmpeg returns 3968 frames versus the declared 4096 returned by Symphonia;
the 3968 overlapping frames agree near floating-point precision. Broader gapless/
corruption/channel-layout/encoder conformance needs further fixtures.

Export an explicitly selected clip into a new Float32 WAV for inspection:

```sh
cargo run -p ravet-audio-file --features compressed --example decode --locked -- input.flac target/decoded.wav
cargo test -p ravet-audio-file --features compressed --all-targets --locked
```

Tests embed only public generated tones; [fixture provenance](../crates/ravet-audio-file/tests/fixtures/README.md)
and the regeneration script document the local encoder. FFmpeg is not a runtime,
normal build or test dependency. A decoded clip can initialize `MemoryPlayback`
explicitly; its ownership/real-time limits still apply.

## Worker/audio handoff

`audio_ring` prepares 1..64 channels and a power-of-two capacity 1..65536 frames,
returning unique producer and consumer endpoints. Atomics publish whole frames;
no unsafe memory, locks, I/O or allocation occurs during push/pop. The producer
returns the prefix that fits. The consumer returns the prefix available and leaves
its output suffix untouched. Consumers explicitly choose retry/drop/silence policy;
overflow and underflow are not hidden. Power-of-two indexing preserves wraparound.

`capacity_frames` reports prepared capacity; consumer `queued_frames` is a
snapshot that a producer can extend later. Producer `clear` exclusively borrows
both matching endpoints and rejects foreign pairs before mutation. Stop and
reclaim endpoint users before clearing a playback generation.

## Explicit read-ahead and recording pumps

`WaveReadAhead` owns a reader, producer and prepared f32 scratch no larger than
the reader's capacity. Worker `pump` performs at most one bounded read and
publishes the whole-frame prefix that fits. Unqueued scratch remains until later
pumps publish it, preventing skipped data under backpressure. Progress separates
read/queued/pending frame counts. EOF means the reader is exhausted while audio
may remain pending/queued. Exhausted pumps perform no I/O; read position includes
read-ahead and differs from audible position. Failed reads remain retryable.

`seek_stopped` requires the matching consumer exclusively borrowed after endpoint
users stop. Ownership and frame bounds validate before mutation; success clears
queued and pending old-generation samples. Live streamed seek/loops and transport
clock/UI remain separate work. Automatic read-ahead ownership, prepared memory
transport and explicit output-rate file preparation are documented separately below.

`WaveRecorder` owns a consumer and prepared WAV writer. Worker `pump` drains at
most scratch capacity; an empty queue does no I/O. Any recording error poisons
the sink because dequeued frames cannot roll back, including nonfinite PCM
input. Capture drop counters remain producer diagnostics. After stopping/joining
the producer, explicit `finish` drains at most one ring capacity and patches/
flushes WAV lengths. Continuing publication violates lifecycle ownership and
may cause rejection or late-frame loss. Drop does no implicit I/O. No thread,
file destination, capture permission or physical device is selected.

The consumer from `ravet_device::open_input` or `PreparedCapture` connects to a
recorder while callbacks only publish samples. Synthetic prepared-capture tests
record and decode exact frame order. Other fixtures check retained suffixes,
EOF, stopped/foreign-ring seeks, bounded drains and poisoned partial-write/PCM
errors. No physical recording or callback deadline is claimed. See
[decision 0033](decisions/0033-buffered-wave-workers.md).

Tests compare an independent unknown-chunk fixture, all truncated file prefixes,
malformed fields, PCM/float roundtrips, seeks/tails, odd padding, injected partial
reads/writes, ring backpressure and 100000 two-thread ordered stereo frames.
10000 prepared ring exchanges report no Rust heap operations. This evidence uses
memory streams and synthetic audio; no device recording or DAW support is claimed.
See [decision 0026](decisions/0026-audio-streams.md).

## Prepared memory transport

`MemoryPlayback` copies an explicitly supplied finite interleaved f32 clip during
preparation. It accepts 1..64 channels, 1..768,000 Hz metadata, at most 16,777,216
samples (64 MiB), and 1..65,536 frames per render. Decode files on the worker side
before constructing it; empty/nonfinite/partial-frame clips reject.

Exclusive audio-thread commands provide play, pause, stop, frame seek and a
half-open loop range. Pause retains position; stop rewinds to the loop start or
zero. Seek allows EOF; with a loop, positions outside it normalize to its start.
Loop boundaries wrap exactly, including after a block's final copied frame.
Render fills all samples, zeroes paused/end suffixes, counts source frames/wraps
and stops at EOF. Invalid output shapes/capacities silence the buffer and preserve
transport state. Zero-frame render preserves play/pause. Invalid seeks/loops
preserve configuration and position.

After preparation these operations use no heap, blocking or file I/O. Drop frees
clip storage outside audio. Caller serializes commands and matches the output
sample rate; there is no resampling, crossfade, command mailbox, streaming seek,
UI or device-clock alignment. Loop/seek boundaries can click. The synthetic WAV
example decodes first, then verifies stereo looping and silent pause:

```sh
CARGO_BUILD_JOBS=2 cargo run -p ravet-audio-file --example transport --locked
```

Fixtures validate variable block/loop boundaries, EOF/paused silence, malformed
commands, and 10,000 prepared operations with no Rust heap activity. See
[decision 0038](decisions/0038-memory-playback-transport.md).

## Optional lazy compressed reader

`CompressedReader::new` uses the same optional/pinned FLAC/MP3/Vorbis backend with
one owned copy of explicitly supplied encoded bytes (1..64 MiB). It decodes lazily
into pending packets instead of building a complete decoded clip. Construction
probes/decodes the first nonempty packet; rate/channel order stay fixed. Caller
maximum duration is 1..16,777,216 decoded frames, with at most 16,777,216 total
interleaved samples. Decoder packet capacity is at most 65536 frames; metadata/
visual/packet bounds match clip decoding. Backend internal allocations are not a
total-memory guarantee. All methods and destruction belong on control/workers.

`read_frames` accepts up to 65536 whole interleaved frames, advances the decoded
cursor and leaves a successful short-read output tail untouched. Zero output
makes no progress. Invalid output shapes reject before mutation. Decode faults
clear the supplied output and poison further reads; some cursor progress may
already have occurred. Already delivered audio cannot be rolled back. EOF checks
declared duration and available backend integrity; reaching a nominal end cursor
alone does not establish that verification. Changing/chained configuration rejects.

`seek_frame` constructs a fresh decoder sharing immutable encoded storage, replays
from the beginning to the exact decoded-frame target and replaces state only on
success. Past-end or failed replay preserves the old cursor/decoder/poison state;
a successful replay can recover a poisoned reader. End positions are valid.
This default mode provides O(target) bounded worker seeking without assuming
codec-native timestamp/preroll behavior. Native FLAC also has the explicit indexed
mode below. Neither mode supplies file-backed streaming, a sample-clock scheduler
or callback-safe seeking.

Synthetic encoded fixtures for all three formats match complete decode exactly
under variable short reads and forward/backward/packet-boundary/end seeks. Tests
also cover output tails, malformed/oversized inputs, failed-seek preservation,
deferred corruption/poison and successful recovery. See
[decision 0068](decisions/0068-compressed-worker-streams.md).

## Indexed native-FLAC seeks

`CompressedReader::new_indexed_flac(encoded, maximum_frames)` supplies the same
reader/read-ahead API with a worker-built compressed-packet index. Construction
requires native `fLaC` bytes with a nonzero declared duration, decodes every packet,
checks contiguous frame timestamps and durations, fixed rate/channel layout,
finite samples, declared total and the backend's available whole-stream MD5.
It retains compressed packets and their timestamps; it retains neither the full
PCM clip nor the original container copy. `indexed_packets()` reports the retained
packet count, or `None` for the default replay reader.

The input copy is temporarily at most 64 MiB. Retained packet payload is separately
capped at 64 MiB and packet/index entries at 65,536. Caller duration and total
16,777,216-sample bounds remain in force; each decoded packet is at most 65,536
frames. Backend working memory, metadata and temporary candidate storage are
additional; these are bounded framework capacities, not a total process-memory
or real-time deadline guarantee. All construction, reads, seeks and destruction
stay on workers. MP3, Vorbis, Ogg-FLAC and streams without declared duration reject
this constructor; they can retain the existing replay reader where supported.

An indexed seek binary-searches the immutable timestamp index, creates a fresh
FLAC decoder and decodes only the containing packet. It trims the packet prefix
to the exact requested frame without decoding preceding packets. EOF seeks decode
none and report verified EOF immediately because construction already checked the
whole stream. Other reads report EOF once they exhaust their pending/indexed
packets. Whole-stream MD5 verification is disabled for fresh suffix decoders;
the immutable packets were already verified during construction. Failed or
past-end candidates preserve the live reader and cursor.

```rust,ignore
let reader = ravet_audio_file::CompressedReader::new_indexed_flac(&encoded, maximum_frames)?;
let mut pump = ravet_audio_file::CompressedReadAhead::new(reader, producer, 4096)?;
// Reclaim matching endpoints after their users stop, then discard the old generation.
pump.seek_stopped(&mut consumer, selected_frame)?;
```

The tests compare exact indexed audio to independently generated integer PCM for
mono/stereo/eight-channel PCM16/24 streams, including every packet edge, first/last
frame and EOF. Instrumented backend decode calls establish that arbitrary seeks
in a 33-packet stream decode one packet, and EOF/invalid seeks decode none. Tests
also cover truncated input, bad MD5/counts, unsupported profiles, short-read tails,
failed-seek preservation and stopped read-ahead generations with allocation-free
consumer transfer. See [decision 0168](decisions/0168-indexed-flac-worker-seeks.md).

## Compressed worker read-ahead

`CompressedReadAhead::new(reader, producer, maximum_frames)` prepares 1..65536
scratch frames for a matching-channel audio ring. The worker calls `pump`, which
performs at most one bounded compressed stream read and retains every unqueued
scratch suffix under ring backpressure. Further reads wait until that suffix is
published. The consumer's prepared `pop` never decodes, seeks or allocates.
Sample rate stays explicit; rings do not carry or convert rates. No hidden worker,
device, sample clock, automatic underrun recovery or playback scheduler is created.

`ReadAheadProgress` reports newly decoded, queued and pending frames. EOF means
the decoder has completed its duration/integrity checks; queued/pending audio can
still require playback. A decode fault poisons the pump and publishes none of
that failed read. Earlier queued audio remains; consumed data cannot be rolled
back. `seek_stopped` requires matching endpoints reclaimed after stopping/joining
users. It first completes the reader's exact replay or indexed seek, then clears
queued/pending old-generation audio and poison. Foreign rings or failed seeks
preserve queued/pending state.

All three actual compressed fixtures pass bounded backpressure/seek/end delivery
and worker-to-consumer sample-order checks. Consumer handoffs measure zero heap
operations; worker decoding intentionally allocates. Deferred malformed streams
verify pump poison, failed seek preservation and successful stopped recovery.
See [decision 0069](decisions/0069-compressed-read-ahead.md).

## Optional lossless FLAC encoding

Enable `flac-encode` independently of decoding for `encode_flac`, `FlacFormat` and
`FlacPcmEncoding::{Pcm16,Pcm24}`. The pinned Apache-2.0 `flacenc` 0.5.1 backend
has default features disabled; configuration explicitly disables parallel mode.
Encoding stays on the calling worker with fixed 4096-frame blocks. No hidden
threads, network/file output, float quantization, clipping or speaker remapping
is performed by this complete-clip API. Incremental output is described below.

Inputs are nonempty whole-frame interleaved i32 samples, channels 1..8, integer
rates 1..96000 and at most 4,194,304 samples (16 MiB of source storage). PCM16
requires -32768..32767; PCM24 requires -8388608..8388607. Validation completes
before backend block copying/encoding. Source remains unchanged. The returned FLAC
record is limited to 64 MiB, with bit-count preflight before serialization.
Backend internal allocation/storage is not a total-memory or deadline guarantee.
Encoding, copying and destruction belong outside audio callbacks.

STREAMINFO min/max block sizes retain the planned 4096 size, including one-frame
clips and final blocks below 16 frames. The last block is excluded from the
minimum-size rule; samples are not padded or dropped. This follows
[RFC 9639 section 8.2](https://www.rfc-editor.org/rfc/rfc9639.html#section-8.2).
MD5/sample counts remain backend-generated for the original signed PCM.
Prediction frames larger than the raw-PCM-sized bound use a constant/verbatim
fallback before serialization; samples and timing remain unchanged. Complete
encoding uses the same incremental block path internally.

Generate explicit public synthetic PCM/FLAC files in a new directory:

```sh
cargo run -p ravet-audio-file --features flac-encode --example encode-flac --locked -- target/flac-fixtures
```

The example creates matching raw little-endian PCM16/24 records with complete
and incremental FLAC for 1/4097/4113 stereo frames at 48 kHz, plus a high-dynamic-
range five-channel PCM24 fallback fixture; it never replaces an existing directory.
All eight channel counts, both precisions, frame boundaries/extrema and rate edges
pass exact Symphonia roundtrips. Independent FFmpeg CRC-checked decoding matches
all seven incremental PCM records byte for byte, including the fallback profile.
Optional Vorbis encoding and automatic prepared capture are documented below.
MP3 encoding is documented below. Physical recording, compressed file-backed seeking and automatic
clocked playback remain separate work. See [decision 0070](decisions/0070-worker-flac-encoding.md).

## Incremental lossless FLAC output

Optional `FlacWriter<W: Write + Seek>` accepts an explicitly empty destination,
format, total frame limit 1..2^36-1 (fitting usize) and encoded byte budget at
least 42. It retains one 4096-frame interleaved block and one backend frame buffer;
serialization is capped at 256 KiB per frame. Backend working allocations and
CPU time remain its contract. It does not accumulate earlier audio frames.

`write_frames` accepts at most 65536 whole interleaved signed PCM16/24 frames per
call. Empty writes do nothing. Invalid shapes/ranges/input capacity preserve
pending samples, output and digest. Full blocks emit immediately; only `finish`
emits a short final block and rewrites STREAMINFO count/MD5/block/frame bounds,
then flushes and returns the sink positioned at its end. Empty recordings reject.
Drop does not finalize. Consumers own file creation, worker scheduling and errors.

Encoding, output budget or I/O errors after acceptance poison the writer; a file
may retain a complete or partial prefix. Further writes/finalization reject.
`accepted_frames` includes the retained/failed-call accepted prefix;
`written_bytes` excludes a potentially partial failed write. Finishing consumes
the writer and cannot retry a header/seek/flush failure. No float quantization,
implicit device/ring integration or physical recording support is implied.

Acceptance compares differently partitioned all-channel/both-precision streams,
short/full/partial blocks and extrema with exact decode; it covers rejection
preservation, total/output budgets, partial-write poison and final seek/flush
failures. Independent FFmpeg checks actual generated files. See
[decision 0074](decisions/0074-incremental-flac-writer.md).

## Capture-ring FLAC recording

`FlacRecorder` validates matching ring channels and 1..65536 worker scratch frames
before creating an incremental writer. The caller supplies the empty sink, format,
total frame/byte budgets, prepared audio consumer and `FlacCapturePolicy`.
`RejectOverload` rejects finite values outside -1..1; `ClipOverload` explicitly
clips them. Both reject NaN/infinity. PCM16/24 quantization rounds nearest with
ties away from zero, maps -1/+1 to signed minimum/maximum and adds no dither or
level scaling. Channel order and configured file rate stay caller policy.

`pump` drains at most its prepared batch, validates the whole dequeued input,
then quantizes/encodes/writes on the worker. Empty pumps perform no file I/O.
Any failure after dequeue poisons the recorder: dequeued samples cannot roll back,
and writer failure can leave an accepted/output prefix. `frames` reports encoder
accepted frames, including pending data; rejected batches are excluded. Audio
producer publication remains prepared and heap-free; ring backpressure/drops are
explicit producer policy. Worker scratch is bounded, while encoder working
allocations/time remain backend policy.

Stop/join the producer before `finish`, which drains at most one ring capacity
and finalizes the nonempty file. Drop never finalizes. Continued late publication
can reject or lose late data; no hidden scheduler or clock conversion is added.

```sh
cargo run -p ravet-audio-file --features flac-encode --example capture-flac --locked -- target/flac-capture
```

This creates a new directory with two public synthetic stereo PCM16/24 records
and actual threaded capture-to-FLAC files. Independent FFmpeg checks reproduce
both 8193-frame records byte for byte. Four acceptance groups cover endpoints/
ties/overload policy, stopped tail draining, malformed/capacity and partial output
poison, and exact threaded order with measured heap-free producer handoff. This
is synthetic capture, with no physical device/real-time deadline claim. See
[decision 0075](decisions/0075-flac-capture-workers.md).

## Automatic read-ahead playback ownership

`ReadAheadWorker::new(pump, consumer, maximum_render_frames)` prepares a control
owner and paused `BufferedPlayback` endpoint for a matching `WaveReadAhead`,
`AiffReadAhead` or `CompressedReadAhead`. Preparation requires no queued frames or pending worker
scratch; a reader already positioned by a stopped seek is accepted. The source
rate remains explicit, and maximum render size is 1..65,536 frames. Call
`worker.start()` on control to start a named automatic worker, then call
`playback.play()` on its audio owner when playback should begin.

The worker repeatedly performs the pump's bounded read/publication operation,
retains every unqueued suffix under backpressure and parks with a 2 ms timeout after
an attempt publishes no frames. Pump file I/O and decoder allocations stay on the
worker; callback rendering never decodes, reads a file, joins a thread or locks.
The transfer mutex is used only during control ownership/startup/exit, outside
pumping and callback rendering. `ReadAheadPump` is an explicit extension contract
for other worker sources, including matching endpoint ownership, bounded pumps
and failed-seek preservation.

`BufferedPlayback::render` fills all output with available source frames and a
zero suffix. Source-frame position advances only by consumed source frames; a
running-worker underrun stalls that position and reports `underflow_frames`.
Pause retains the position and does not consume queued frames. Invalid or
oversized whole-frame output shapes or a conservative u64 cursor-addition overflow
silence the complete supplied buffer while preserving position, play/pause and
queue; zero frames preserve play/pause.
Playback uses the source rate and channel order with no conversion or amplitude
change; WAV floating-point values retain their bits, including nonfinite samples.

Worker EOF means its retained suffix has been completely published. Audible
`ended` is true only when that generation's ring also drains. End stops playback;
its zero suffix is distinct from running-worker underflow. A pump fault reports
`Failed`, retains any earlier queued audio for once-only consumption, and stops
playback once that prefix drains. It cannot report successful EOF. Status snapshots
can change concurrently; they are diagnostics, not a sample-clock synchronization
contract.

`seek_stopped` requires the matching callback endpoint reclaimed after its user
stops. It joins the worker before seeking, then restarts automatic read-ahead.
Foreign endpoints reject before stopping the worker. A source seek rejection
preserves cursor, queued/pending generation, callback source position and
play/pause; the old generation resumes. Successful positioning clears old audio,
updates source position and retains play/pause. `stop_stopped` seeks to zero and
pauses while allowing the new generation to prefetch. These are control operations;
there is no live callback seek or asynchronous command mailbox.

Worker thread creation/restart failure retains the pump in its owner, reports
`Failed` and returns an I/O error; callers may retry `start` or reclaim through
`shutdown`. Already completed source-seek generation changes are not rolled back
if restart fails. `shutdown` joins and returns the owned pump plus its original
pump error. Drop also joins, so both belong outside callbacks. Stop is cooperative
between bounded pump calls and does not cancel active file I/O or guarantee a join
deadline. A worker panic reports `Failed`; its unwound source is lost and control
reclamation returns `Poisoned`.

Local tests exercise concurrent retained backpressure, pause/resume, exact EOF
drain and indexed-FLAC recall; gated running underruns and failed/foreign seeks
establish cursor/generation behavior. Callback heap measurements cover repeated
paused renders and active consumption. Pump error/panic tests establish prefix
retention, source reclamation and failure distinct from EOF. No physical recording,
clock drift, device/DAW deadline, streamed loop, resampling or crossfade claim follows
from these tests. See [decision 0171](decisions/0171-automatic-read-ahead-playback.md).

## AIFF PCM read-ahead and recording

`AiffReadAhead::new(reader, producer, maximum_frames)` prepares matching channels
and scratch within the reader's 1..65,536-frame capacity. It shares WAV's bounded
handoff implementation: at most one read per pump, complete-frame ring publication,
retained unqueued scratch under backpressure and EOF distinct from pending/queued
playback. Failed file reads preserve logical position and publication and may be
retried explicitly. Matching `seek_stopped` first validates the source target, then
clears all queued/pending old-generation audio; failed/foreign seeks preserve it.
The adapter also implements `ReadAheadPump` for the automatic owner above.

`AiffRecorder::new(destination, format, maximum_frames, consumer)` validates matching
channels before writing a header to the caller's empty seekable destination.
`pump` drains at most 1..65,536 prepared scratch frames. Empty pumps perform no file
I/O. PCM conversion/FORM/count limits remain those of `AiffWriter`; PCM16/24/32
finite clipping/rounding and big-endian bytes are unchanged. Once audio is dequeued,
any validation/write failure poisons further pumping/finalization because the
consumed input cannot be restored. An I/O failure can leave a partial output file.

Stop/join the producer before `finish`. It drains at most one ring capacity,
patches FORM/COMM/SSND lengths, adds odd-payload padding and flushes. Continuing
publication can reject finalization or lose late frames. Drop performs no implicit
drain, finalize or file I/O. No capture device, sample clock, hidden recording
worker or path is chosen; only prepared ring push/pop/transport rendering belongs
in callbacks. Physical recording/routing and resampling remain consumer/backend
work.

Tests cover exact PCM16/24/32 at 1/2/8 channels, retained suffixes, stopped-generation
replacement, read failure/retry, callback heap counts, bounded final recorder drains,
odd PCM24 payload padding, nonfinite/partial-write poison and automatic playback
with stopped recall. Existing WAV and automatic/compressed worker regressions
exercise the shared handoff extraction. See
[decision 0173](decisions/0173-aiff-pcm-worker-adapters.md).

## Automatic recording worker ownership

`RecordingWorker::new(recorder, &producer)` prepares a control owner for a matching
`WaveRecorder`, `AiffRecorder` or optional `FlacRecorder`. The recorder's explicit
format, scratch and output/count budgets remain unchanged; its preparation may
already have written the selected sink's header. Call `start` to begin a named
automatic worker. The capture callback keeps its unique `AudioProducer` and only
calls its prepared `push`. Full-ring input returns an accepted whole-frame prefix;
the caller must retain/retry or explicitly count/drop the suffix. The worker adds
no implicit callback blocking, allocation, file I/O or device selection.

Automatic pumping repeatedly drains one source scratch batch and uses a 2 ms park
timeout when no frames are available. The common private worker owner also serves
read-ahead playback, retaining the source through OS thread-creation failure and
keeping ownership-transfer locks outside pumping/callbacks. `stop` cooperatively
joins without draining/finalizing and leaves the recorder available to restart.
Stop/join/Drop belong on control and cannot cancel active backend I/O or establish
a join deadline. Drop joins but never patches headers, flushes or finalizes a file.

Reclaim the unique producer after its capture user stops/joins, then call
`finish_stopped(&mut producer)`. A foreign queue rejects before the worker stops.
The owner joins and invokes the recorder's existing bounded final ring drain and
consuming file/codec finalization on control.

For capture adapters whose producer stays inside a callback, prepare with
`RecordingWorker::from_recorder(recorder)`. Stop/drop the device stream or capture
owner, then use `finish_after_producer_retired()`. The unique producer's Drop
publishes permanent atomic retirement; it cannot be resurrected. An acquire proof
synchronizes the final publication, then joining the recording worker permits its
bounded remaining drain. Live or leaked producers reject before stopping the
worker. `AudioConsumer::producer_retired()` exposes the same diagnostic; retirement
does not discard queued audio or establish a finalized recording. This matches
`ravet-device::PreparedCapture` and `open_input`'s hidden-producer ownership; it does
not implicitly stop a device, and offline capture tests do not establish backend
stream-destruction timing or physical capture support.

`Finalized` means that finalization succeeded and returned the complete sink;
further start/finish reject. Concurrent
late publication violates stopped-producer ownership and may reject or lose audio.
The API does not implicitly stop a physical input device or callback thread.

Pump failures report `Failed` and retain the poisoned recorder and original error;
earlier accepted/written prefixes cannot be rolled back. `shutdown` returns the
owned recorder plus that error without finalizing. If either finish method returns a
saved pump error first, it returns that error once and keeps the recorder available
to `shutdown`; reclamation then has no duplicate error. A failure during consuming
finalization loses the sink, cannot be retried, and cannot advertise a complete
file. A worker panic reports `Failed` and `Poisoned`; its unwound recorder is lost.
WAV/AIFF frame counts represent successful writes, while FLAC counts accepted
encoder frames including its retained block; neither means finalized capture.

Tests exercise concurrent exact synthetic capture, accepted-prefix overload,
callback heap counts, stop/restart and final drain, matching queue ownership,
original write/PCM errors, owned-source reclamation, terminal final flush failure,
panic distinct from successful finalization, permanent producer retirement,
actual hidden-producer PreparedCapture integration and AIFF odd PCM24 padding.
Optional FLAC tests cover exact explicit clipping, overload/nonfinite rejection and total
capacity poison. No physical capture, long-session drift, device deadline, rate
conversion or hidden clock/path policy is established. See
[decision 0177](decisions/0177-automatic-recording-workers.md).

## Optional bounded Ogg Vorbis worker encoding

Enable `ravet-audio-file`'s default-disabled `vorbis-encode` feature for
`encode_vorbis`, `VorbisWriter` and `VorbisFormat`. The backend is pinned safe
vorbis_rs 0.5.6 with default features disabled, over vendored BSD libogg and
aoTuV/libvorbis. Ordinary builds require a C compiler through cc and supplied
bindings; they do not require a system codec library, bindgen/libclang or random
serial generation. Preserve [the backend notices](../crates/ravet-audio-file/third-party/vorbis.md).

Callers select mono/stereo Vorbis I channel order, encoded rate (mono 8000..96000 Hz, stereo 16000..96000 Hz),
finite perceptual quality -0.2..1 and an explicit i32 Ogg stream serial. Quality
controls the backend's lossy encoding; no exact PCM or subjective quality promise
is made. Finite interleaved samples must be within [-1,1]; overload/nonfinite
input rejects instead of clipping. No implicit resampling, channel mapping, path,
clock, device, thread or serial policy is selected. Logical streams chained or
multiplexed by consumers need distinct serials; that container policy is separate.

Stereo rates below 16000 Hz reject before sink output. At maximum quality, the
selected backend's 8000 Hz synthetic stereo fixture showed sustained second-channel
loss: whole-channel SNR 7.26 dB and interior SNR 6.98 dB, compared with first-channel
46.25 dB. Direct backend and reframed native decoding agreed; lag/gain fitting did
not repair the loss. The public wrapper excludes that profile instead of changing
input rate or silently choosing another quality/coupling policy. Its safe builder
does not expose channel-coupling control. Mono 8000 Hz remains supported; selected
16000/22050/48000 Hz stereo fixtures retained more than 25 dB whole-stream SNR.
These measured synthetic figures describe the reviewed backend on macOS arm64,
without a perceptual or cross-platform guarantee.

`encode_vorbis(samples, format)` produces a complete nonempty owned byte stream
from at most 4,194,304 f32 samples with output capped at 64 MiB.
`VorbisWriter::new(destination, format, maximum_frames, maximum_bytes)` prepares
an incremental writer, with total frames 1..16,777,216 and output bytes
1..67,108,864. It writes the initial headers at the sink's current position; the
caller supplies the intended destination/offset and owns truncation policy.
Header-output failure can consume the sink and leave an irreversible prefix.

Whole calls to `write_frames` preflight shape, samples and per-call/total counts
before accepting input. Each call accepts at most 65536 frames and is divided
into prepared 4096-frame planar batches. Empty input is a no-op; it does not
signal EOS. Wrapper scratch remains fixed, including one physical Ogg page at most 65,307
bytes; backend working allocations remain its own contract. The encoder output
is framed with a separate zero-granule priming packet page, preserving every
packet/lacing/body/serial and exact final granule while rewriting sequence/CRC.
Header packets and the priming packet must terminate within a physical page;
continued headers, mixed header/audio pages and spanning priming packets reject.
This boundary is tested for the selected mono/stereo backend profiles and is not
an arbitrary Ogg repackaging API. `accepted_frames` includes submitted batches even on a later
encoding failure; `written_bytes` counts bytes actually accepted by the sink,
including failed partial writes. Neither count proves a complete file.

Preflight failures preserve a usable writer. Codec, I/O or output-budget failures
after acceptance poison it; earlier output cannot be rolled back. Explicit
nonempty `finish` writes EOS and flushes once before returning the sink. A failed
consuming finish loses the sink and has no complete-file or retry guarantee.
The private sink gate latches every parser/partial-page/sink/budget failure and disables writes
before Drop, so the backend's automatic-drop EOS does not write or flush the
caller sink. User-supplied sink destructors retain their own contract.

Create, use, finish and drop an incremental writer on one worker thread: the safe
backend's raw-pointer ownership is not Send/Sync. A consumer may move the input
format/sink into a worker and construct the writer there. Every codec method and
teardown belongs outside callbacks. The dedicated `VorbisRecordingWorker` below
creates the codec locally for automatic prepared capture; it does not implement
the movable `RecordingPump` contract or establish an audio deadline.

Tests cover mono/stereo rates and short/final tails through independent Symphonia
decoding, declared page CRC/serial/BOS/EOS/exact final granules, lossy synthetic
PCM alignment and SNR, partitioned writes/preflight preservation, sink/output
budget poison, original errors and write/flush counters across ordinary/failed
Drop. An explicitly invoked ignored test exercises the independent FFmpeg native
Vorbis decoder using generated synthetic streams:

```sh
RAVET_TEST_FFMPEG=/path/to/ffmpeg cargo test -p ravet-audio-file --all-features --test vorbis --locked -- --ignored ffmpeg_native_decoder_independently_agrees_on_pcm_timing_and_channel_order
```

This synthetic codec evidence does not validate physical capture, subjective
listening quality, host devices, deadlines or another target toolchain. See
[decision 0181](decisions/0181-bounded-vorbis-worker-encoding.md).

### Automatic Vorbis capture ownership

With `vorbis-encode`, `VorbisRecordingWorker` prepares a selected Send sink and
exact capture-ring consumer, then constructs, uses and destroys the !Send Vorbis
codec on the named worker. `new` validates format/queue/budgets and prepares
1..4096-frame source scratch without codec/header/thread creation. `start`
acknowledges OS spawn only; inspect status and the eventual outcome for startup
header errors. OS spawn failure retains preparation for retry or abort.

Capture callbacks keep prepared `AudioProducer::push` or `PreparedCapture`
ownership. A full ring accepts a prefix; the caller explicitly retries or drops
the suffix. Pause waits for worker acknowledgement after active sink I/O; after
success it leaves queued frames untouched. Resume acknowledges the same running
codec. These control methods and join can block indefinitely on selected sink
I/O; they do not cancel I/O or provide a real-time deadline.

Stop and reclaim the exact producer before `finish_stopped`, or drop the hidden
callback producer before `finish_after_producer_retired`. Foreign/live/leaked
producers reject before changing worker state. Finish drains at most the stopped
fixed-ring tail, emits EOS, validates complete pages and flushes on the codec's
owning worker. Even a never-started controller creates the codec there. Its
`Ok` result means ownership was recovered: inspect the private-constructed
`VorbisRecordingCompletion` through `completion()`.

- `Finalized` proves EOS and successful flush for the returned sink.
- `Aborted` returns an incomplete sink without final drain/EOS/flush.
- `Failed(error)` returns an incomplete irreversible prefix with the original
  startup/pump/drain/EOS/flush error.

The exit also returns the owned consumer and accepted encoder-frame,
dequeued-ring-frame and actual sink-byte counts. A rejected dequeued batch can
make source/dequeue counts differ; failed workers stop further ring consumption
until explicit finish/abort. `abort` recovers partial ownership without encoding
or flushing more input. Drop requests gated abort and joins but discards the
returned ownership; sink destructor behavior remains caller-owned. Panic loses
the unwound worker-local sink/source and reports Failed/Poisoned rather than
pretending they can be reclaimed. Existing `VorbisWriter::new`/`finish` retain
their consuming-failure signatures.

The default-disabled device feature `vorbis-recording` forwards the audio-file
encoder for `PreparedCapture` integration. Synthetic callback tests cover hidden
producer retirement, allocation-free publication and automatic bounded final
ring drain; physical CPAL shutdown timing and recording deadlines remain
unvalidated. Rates/quality/PCM/output/framing and backend-memory limits remain
those of the Vorbis writer above.

See [decision 0186](decisions/0186-worker-created-vorbis-recording.md).

```sh
cargo test -p ravet-audio-file --all-features --test vorbis_recording --locked
cargo test -p ravet-device --features vorbis-recording --test vorbis_recording --locked
```

## Optional local MP3 encoding

Default-disabled `mp3-encode` pins `mp3lame-encoder` 0.2.5 and `mp3lame-sys`
0.1.11 with defaults disabled, including the unmodified LAME 3.100 encoder.
Unix builds require a working C compiler, configure and make. The encoder builds
statically; no system LAME library or executable is selected implicitly. Only
this macOS arm64 build and offline fixtures have been validated. Codec/target
support elsewhere is not established by the upstream build script alone.

`encode_mp3` handles complete clips and `Mp3Writer<W: Write + Seek>` handles
incremental worker-thread output. Callers explicitly choose `Mp3Bitrate` 192,
256 or 320 kbps, mono/stereo and 32, 44.1 or 48 kHz. Encoding uses the pinned
backend's highest quality CBR profile. No ID3 metadata, VBR, hidden resampling,
channel remapping or physical device support is added.
LAME's 192 kbps preset scales input by 0.97; its 256/320 presets use 1.0. RAVET
adds no gain compensation. Lossy output can have codec filtering/transients;
measured raw and gain-fitted signal agreement are separate evidence.

Source calls require finite normalized [-1, 1] interleaved samples. Shape,
value, complete-call source count and format preflight precede codec mutation.
Invalid preflight calls are retryable. Prepared encode scratch handles at most
4,096 source frames and 12,320 output bytes per batch; the tag buffer is 4 KiB.
Calls accept at most 65,536 frames, total source at most 16,777,216 frames,
complete clips at most 4,194,304 samples and actual sink writes at most 64 MiB,
with tighter caller budgets. Codec-owned working allocations remain outside
these adapter bounds; no RSS ceiling or real-time deadline is claimed.

Construction requires current sink position at EOF and saves that fresh region's
origin. Existing caller prefix bytes can remain outside the encoded region;
callers own truncation, reading and concurrent-access policy. A non-EOF rejection
restores the original position after successful seek; an actual seek error is
returned immediately with its original error and sink, with no hidden restoration
retry. A failed selected seek may already have changed its own cursor.

Explicit `finish` uses standalone codec flush, validates the emitted MPEG frame,
Info/Xing identity, LAME version, tag CRC and signaled delay/padding/source-frame
relation, then replaces the reserved first metadata frame, restores the file end
and flushes. Metadata frame bit rate need not match data-frame bit rate. Actual
write budgets/counts include the rewritten metadata bytes, whereas `file_bytes`
measures the encoded region. Successful finalization supplies exact source-frame
metadata. Gapless-aware completed-file decoders honor it; stream players may
retain padding. RAVET never trims arbitrary decoded PCM to force agreement.

`Mp3WriteError::Rejected` owns a retryable preflight error;
`Mp3WriteError::Failed` borrows the retained original operational error. Failed
encoding/write/seek/flush or output-capacity checks poison the writer and block
further encoding and sink I/O. Consuming `finish` returns `Mp3Failure<W>`
with the recovered partial sink, original error and accepted/file/write counts.
An accepted source batch can remain irreversible even when its output write
fails. `abort` recovers the incomplete sink and intentionally discards stored
error/counts; inspect getters first, or finish a poisoned writer to recover its
error without further I/O. Ordinary and poisoned Drop never write, seek, flush
or finalize; downstream sink destructor behavior belongs to its caller.

The optional binding crates declare LGPL 3.0, core LAME source grants Library
GPL 2-or-later and ReplayGain analysis grants LGPL 2.1-or-later. Complete notices,
source provenance and attribution are in [the backend notices](../crates/ravet-audio-file/third-party/mp3.md).
RAVET's commercial agreement does not relicense these components; consumers must
review their own combined/static-linking, source/relinking and distribution
obligations. No commercial compliance or patent-clearance certification is made.

See [decision 0193](decisions/0193-bounded-mp3-worker-encoding.md).

```sh
cargo test -p ravet-audio-file --all-features --test mp3 --test mp3_faults --locked
RAVET_TEST_FFMPEG=/path/to/ffmpeg cargo test -p ravet-audio-file --all-features --test mp3 --locked -- --ignored ffmpeg_native_file_decoder_agrees_on_all_bitrates_timing_pcm_and_gain_policy
```

## Automatic MP3 capture ownership

With `mp3-encode`, `Mp3RecordingWorker<W: Write + Seek + Send + 'static>`
prepares a consumer, transferable destination and 1..4,096-frame scratch on
control. Preparation validates writer configuration, ring channels and bounds;
it neither constructs the encoder nor seeks/writes the sink. The encoder is
created, pumped, explicitly finalized and dropped on one named worker.
Vorbis and MP3 share the private controller's ownership, proof, bounded drain
and acknowledged pause/resume mechanics; no `Send` bound is added to their local
codec sessions and the public Vorbis APIs retain their signatures.

`start` acknowledges OS thread creation; consult status and eventual completion
for startup errors. A failed OS spawn preserves preparation for retry or abort.
`pause` waits for the active batch and sink I/O to finish, then acknowledges no
further dequeue until resume or finalization. These control operations, join
and Drop may wait indefinitely on caller I/O; no cancellation or deadline is
provided. Prepared-ring publication retains its existing callback heap/lock/I/O
bounds and full-ring accepted-prefix/drop policy.

After stopping capture, call `finish_stopped` with the matching mutable producer,
or drop its unique owner and call `finish_after_producer_retired`. Retirement
also works with the hidden producer in `PreparedCapture`/`open_input`. Foreign
or live/leaked producer proofs reject before changing worker state. The stopped
final tail is bounded by queue capacity. The controller's outer `Ok` proves
recovered ownership only: inspect the private-constructed completion enum
`Finalized`, `Aborted` or `Failed(original error)`. Only `Finalized` proves the
codec finish, validated metadata rewrite, restored file end and successful flush.

Failures stop later dequeue and return the original sink/source/error, accepted
codec frames, dequeued source frames, encoded `file_bytes` and actual
`written_bytes` including metadata rewrites and partial failed writes. A rejected
source batch can make dequeued and accepted counts differ. A failing encoded
batch can have irrevocably accepted its source before output failure. The shared
adapter preserves the writer's original owned operational error even though the
public writer exposes a borrowed error. Startup seek faults trigger no automatic
restoration retry; partial destinations remain caller-owned.

`abort` recovers incomplete ownership without draining, rewriting or flushing.
Prepared abort touches no sink. Drop requests abort and joins, discarding recovered
ownership; downstream sink destructors retain their own contract. Worker panic
loses the unwound destination/source and reports Failed/Poisoned rather than
claiming recoverable ownership or a complete file. MP3 profiles, normalized PCM,
physical-output bounds, gain policy, gapless-file requirements and independent
LGPL obligations remain those of the writer above.

The default-disabled `ravet-device` feature `mp3-recording` forwards the encoder
for actual `PreparedCapture` integration. Synthetic tests cover hidden-producer
retirement, bounded automatic drain, counted overload and allocation-free input
publication. Physical CPAL shutdown/capture timing and recording deadlines remain
unvalidated. See [decision 0198](decisions/0198-worker-created-mp3-recording.md).

```sh
cargo test -p ravet-audio-file --all-features --test mp3_recording --test vorbis_recording --locked
cargo test -p ravet-device --features mp3-recording,vorbis-recording --test mp3_recording --test vorbis_recording --locked
```

## Prepared file playback at a selected output rate

Enable `ravet-audio-file/resampled-playback` explicitly to prepare a complete
bounded file clip off the audio thread. `prepare_file_playback` accepts existing
WAV and AIFF readers, optional `compressed` FLAC/MP3/Vorbis readers, or a
`PlaybackReader` implementing the documented stable metadata/committed cursor
contract. Preparation starts at source frame zero and consumes validated EOF.
The resulting `PreparedFilePlayback` contains the existing `MemoryPlayback`
transport at the caller's selected output rate, without a callback reader or
converter.

Select `PlaybackConversion::Copy` for an exact same-rate sample copy; differing
rates reject before reads. Select `Sinc { taps }` for explicit causal filtering,
including when input and output rates match. It uses `ravet-signal`'s existing
1025-phase Blackman sinc profile with even 16..256 taps and fixed rate ratio
1/16..16. It retains `(taps - 1) / 2` source-sample filter delay, including its
half-sample fraction, and the finite filter tail. There is no hidden bypass,
gain change, resampling of a differently labelled rate, delay compensation or
additional compressed-codec trimming.

For `N` validated source frames and `T` sinc taps, metadata reports nominal output
duration `ceil(N * output_rate / source_rate)` and retained clip length
`floor((N + T - 2) * output_rate / source_rate) + 1`, computed with exact integer
arithmetic. Source frame counts preserve the reader's gapless decoder semantics.
Padding submitted to the converter is a separate count, never source duration;
at most `T` explicit zeros include one numerical boundary guard. Playback cursors,
seeks and half-open loops use the complete output clip's frame coordinates,
including its leading causal delay and tail. Loop/seek boundaries can click.

All options and budgets are explicit: 1..64 channels, integer source/target rates
1..768000 Hz, at most 16,777,216 source and output interleaved samples, 1..4096
source read batches, and 1..65536 render frames. Output storage is at most 64MiB;
converter coefficients/history/planar and source scratch are separately bounded
to 32MiB before reads. The reader's own encoded-data/file/codec allocations retain
its documented limits. Known source duration and required output capacity reject
before any new source read. Unknown duration conservatively preflights/reserves
for the caller's maximum source duration. No full second source PCM clip or second
output copy is retained. Allocator/RSS recovery and callback deadlines are not
promised.

Failure returns `FilePlaybackFailure` owning the original reader/error, the
incomplete interleaved output prefix and distinct committed reader, submitted
source, padding and prepared-output counts. A source read error can advance its
logical frame or underlying byte cursor before returning. A decoder EOF-integrity
error can advance through a read whose output is discarded. There is no automatic
seek rollback or retry, and the prefix is not a complete prepared clip. Inspect
`into_parts` to decide whether to recover, reopen or discard the source.

Enable `ravet-device/resampled-playback` to use `FilePlaybackOutput`. It owns the
prepared clip and implements the existing output-only `Processor` path accepted by
actual `PreparedOutput` and `open_output`. Prepare rejects mismatched sample rate,
channel layout, events/auxiliary buses or render bounds before rendering, then
allocates interleaved scratch. Valid callbacks only render the memory transport and
copy to planar output; invalid/unprepared blocks silence output without advancing.
Configure playback through its exclusive `playback_mut` before transferring the
processor into output ownership. Reset pauses at frame zero/configured loop start;
deactivation retires processing and preserves the clip/cursor. There is no new
cross-thread command queue. Stop callbacks before dropping/recovering clip storage.

This is complete-file preparation. Live buffered conversion/drift, clocked transport
mapping, physical output, long-running callback deadlines and physical seek/recovery
remain separate validation work.

### Impulse-response files

With `resampled-playback`, `prepare_impulse_response_file(&mut reader,
ImpulseResponseFileOptions, &ImpulseResponseRequest)` reads a complete response through
`PlaybackReader` (WAV, AIFF or optional compressed readers). It prepares the response with
`ravet-signal`'s explicit `prepare_impulse_response` at the file's source rate. Before any
read, it checks that the reader is at frame zero and that it has 1..=64 channels and a
1..=768,000 Hz rate. It also checks source bounds (1..=4,194,304 frames, batch 1..=65,536)
and the request plan. Planar f64 source plus scratch plus declared preparation bytes must
fit the request's working budget. Decoded f32 samples are widened exactly. Reader errors,
contract violations, nonfinite samples, declared-length mismatches and preparation
refusals return `ImpulseResponseFileError` with the committed frames and no partial
response. There is no reader rollback. This is worker-only work.

### Bounded streaming playback at an explicit output rate

With `resampled-playback`, `ConvertedReadAhead` converts original file frames into
an explicitly selected queue/output rate without retaining a whole PCM clip. Supply
a frame-zero `PlaybackReader`, a caller-selected factory that reproduces the same
source/version from frame zero, and matching empty stopped ring endpoints.
`ConvertedReadAheadOptions` selects equal-rate bit-exact `Copy`, floating `Sinc`,
or exact-scheduled `IntegerSinc` filtering, source/output ceilings and
source/conversion batch bounds. Preparation
returns original reader/factory/producer ownership on failure and performs no
source read or factory call; reader construction has its own I/O contract.

The supported profile is 1..64 channels, rates 1..768,000Hz, ratio 1/16..16, even
Sinc taps 16..256 and batches 1..4096. `Copy` and floating `Sinc` cap original
duration at 16,777,216 source frames. Explicit `IntegerSinc` lifts only this stream
source ceiling, subject to the checked finite duration/headroom constraints below.
Compressed decoders' interleaved-sample limits remain separate. Explicit
source/converter/output arrays are at most 32MiB; reader/backend and ring allocations
are separately bounded. Unknown duration conservatively preflights the caller
source maximum, then rejects actual over-limit reads. Replay candidates temporarily
coexist with the old bounded state. These limits do not promise arbitrary-u64
terminal transport, RF64 or unlimited container/decoder durations.

Each worker pump performs at most one source read, one converter step per channel
and one publication. Retained output suffixes survive partial queues. A full queue
performs no fresh read or input consumption. A downsampling pump can consume source
without generating an output frame; the bounded pure `ReadAheadPump::worker_progressed`
hook observes that progress without changing native pumps' publication-based default.
Use `ReadAheadWorker` and `BufferedPlayback` for automatic ownership, pause/resume,
underrun position stalls and queue drain. Their cursors and generic progress use
queue-source frames at the explicitly selected output rate.

Typed converted progress distinguishes original input reads/submission, explicit
zero padding, generated target-rate output and generation-specific publication.
Sinc retains causal `(taps-1)/2` source-sample delay, including the half sample.
Retained output count is `floor((N+taps-2)*output_rate/input_rate)+1`; nominal duration
is distinct. Exact EOF requires decoder integrity, retained filter tail and suffix
publication, followed by callback queue drain. No implicit trim, gain change,
resampling choice or delay compensation occurs. The floating recurrence profile
retains its accepted ceiling/impulse evidence; integer scheduling has separate
long-stream evidence and finite coordinate limits below.

Stopped seek uses absolute converted output frames. Matching endpoint ownership,
generation increment and determinable bounds are checked before factory I/O. Replay
an independent candidate from zero; only successful adoption clears the old queue.
Failed factory/decode/metadata/EOF candidates preserve original reader position,
phase, pending suffixes, queue and generation. The factory must reproduce the same
content: matching metadata is not content authentication and external factory side
effects cannot roll back. Replay is O(target) control work without cancellation or
a deadline guarantee. First operational pump error belongs to its Result or worker
exit; subsequent pumping is poisoned. `into_parts` recovers the advanced reader,
factory, producer and exact counts without hidden read/seek/drain.

## Integer-rate conversion for long streams

`PlaybackConversion::IntegerSinc { taps }` explicitly selects the integer-rate
schedule for complete-file preparation and `ConvertedReadAhead`. This adds an enum
variant: downstream exhaustive matches must handle it. Existing `Copy` and `Sinc`
retain their sample behavior and limits. At equal rates, IntegerSinc filters; it
does not silently copy. Delay remains `(taps-1)/2` original input samples; retained
finite output for a nonempty source is `floor((N+taps-2)*output_rate/input_rate)+1`. Explicit padding and
initial zero history remain in the result; no hidden gapless trimming is added.

Integer converted read-ahead accepts caller-selected original source bounds up to
`usize::MAX`, with independently enforced reader/container/interleaved sample
limits, source/converted batches 1..4096 and 32 MiB for source, converter and
output arrays (including coefficient tables). Reader, queue and allocator overhead
remain separate. Checked rational duration
must fit the configured output budget and remain at most `u64::MAX-65536` frames,
leaving the largest automatic callback render bound as terminal cursor headroom.
This constraint checks computed retained duration; an oversized unused output
budget does not by itself reject. This does not claim arbitrary-u64 terminal
playback. Preparation rejects determinable duration/configuration limits before
source reads or fresh-reader factory calls. Unknown readers remain subject to
their caller bounds and verified EOF/integrity checks. Complete-file preparation
keeps its existing PCM/clip memory ceilings; the long-stream mode does not allocate
whole source or output audio.

Absolute stopped output seeks still replay a fresh same-version reader from the
beginning using bounded memory and exact scheduling. Replay costs O(target), has
no cancellation/deadline guarantee and is not an indexed codec/container seek.
Candidate errors retain their original error and preserve old generation, queue,
phase, retained suffix and source cursor; factory side effects are not rolled back.
The unique worker/playback/BufferedPlaybackOutput/reclaimable output ownership
contracts still apply. Live seek mailboxes, clock mapping/drift, RF64, arbitrary
long compressed readers, broad physical-device support and callback deadlines are
separate capabilities. No native support claim follows from offline long fixtures.

## Indexed integer PCM read-ahead

With `resampled-playback`, `IndexedConvertedReadAhead` provides a separate
known-duration `IntegerSinc` pump. It implements `ReadAheadPump`, so the existing
worker and unique-consumer `BufferedPlaybackOutput` bridge can render it. Its
`IndexedPlaybackReader` contract is implemented by `WaveReader<Read + Seek>` and
`AiffReader<Read + Seek>`; their logical seeks defer physical repositioning to
the next read. Copy, floating Sinc, unknown-duration and compressed readers have
no indexed adapter here. Existing `ConvertedReadAhead` retains its stopped replay
behavior and counters.

`new` prepares frame zero without reads, seeks or factory calls and returns owned
inputs on failure. `seek_indexed_stopped` requires the matching, exclusively
stopped consumer. It preflights output bounds and generation before calling the
fresh-reader factory, validates identical rate/channel/declared duration before
seeking, and reads at most the taps-sized original source window needed for the
requested output. It supplies explicit zero history before frame zero and after
the declared source end, primes the exact integer phase, and commits one new
generation only after validation succeeds. A failed candidate returns its original
error and optional reader through `IndexedSeekFailure::into_parts`; its actual
cursor may have advanced. The old reader, queue, retained suffix, phase, origin
and generation remain intact. Factory side effects are not rolled back.

The retained endpoint is a separate operation: seeking to the exact output total
validates the last min(taps, source length) original frames and a zero EOF read,
then installs an empty completed stream. Its receipt has
`filter_input_coordinate = None` and `verified_end_point = true`; no unemitted
next output or fictitious padding is primed. The new indexed pump accepts a known
empty PCM source as zero retained output after an EOF check. The existing replay
and complete-file preparation APIs still reject empty clips. Positive streams
keep the existing causal delay and retained-tail policy.

`IndexedSeekReceipt` reports the validated original window and its actual ending
reader position separately from the u128 filter coordinate and leading/trailing
zero history. `IndexedReadAheadCounts` reports original cursor movement and
source/padding submitted after priming, absolute generated output, generated
output since priming and queue publication. Cursor movement includes commits
before reader errors and is not labelled validated decoded PCM; it is `None`
if a contract-violating reader regresses. Skipped prefixes are never reported as
read or submitted source. Generic worker `seek_stopped` preserves the original
error but releases the failed candidate on control; use the richer direct
method when the candidate reader must be recovered.

A successful candidate reads at most min(taps, source length) original frames,
with at most taps positive read calls when a custom reader supplies short reads,
plus one bounded EOF probe when the window reaches EOF. The built-in full-read
WAV/AIFF implementations use at most ceil(window frames / read batch) payload
reads, plus that probe. Instrumented container tests measure payload bytes and
physical read/seek calls after the fresh reader's metadata constructor; factory
parsing, custom reader internals and filesystem latency remain separate.
Temporary history scratch is at most 66 KiB in addition to each candidate's
existing 32 MiB array ceiling; old and candidate states coexist until commit.
Reader, ring and allocator memory are separate. Existing rate/channel/batch,
reader/container and configured duration bounds remain, including retained output
no greater than u64::MAX−65536.

The factory must supply the same source version. Window/EOF validation does not
authenticate skipped regions or detect every earlier malformed/changed byte.
This adds no compressed indexing, RF64, clock/drift control, callback seek/wait,
filesystem deadline or physical-device support claim. Stopped renderer reclamation
and explicit preparation/reopening remain necessary for device composition.

## File transport through clocked captured-input processing

The optional device `BufferedFileDuplexProcessor` sends unique buffered file PCM
and already output-clock-converted captured input to a caller's downstream f32
processor. Its file-first/capture-second input grouping is explicit. The existing
`PreparedClockedDuplex` supplies live-input drift correction; file worker queue
occupancy is never treated as drift. File reads/conversion remain on their worker.

Consumed file PCM and paused/underrun/EOF zero-file PCM form at most two child
blocks. Capture processing continues while the file cursor stalls. Child transport
positions are queue output frames, distinct from original decoded frames and the
parent logical device-render frame count. Failed-source prefixes drain without
claiming EOF; original source errors remain recoverable from worker shutdown.
Downstream faults retain their original diagnostic, silence the entire output and
stop further source consumption until explicit reset/reprepare, with already
consumed file/child history reported rather than rolled back.

Direct stopped extraction returns original file/DSP owners after once-only
deactivation, permitting matching worker failed/successful indexed seek and
explicit reprepare. A native clocked closure is still inaccessible; no live seek,
state migration, physical clock alignment or callback deadline follows. See
[audio devices](audio-devices.md) for the profile and [decision 0228](decisions/0228-output-clock-file-capture-processing.md) for the contract.

## Stopped file ownership after paired clocked device retirement

The device `open_reclaimable_clocked_duplex` route can reclaim a W215 buffered
file/capture processor only after all four native callback guards have retired and
the control worker owns their shared cell exclusively. A paused stream or pending
retirement does not expose a file consumer. The stopped result can be consumed
into the original processor and both matching raw capture endpoints; consuming the
buffered wrapper then returns its original file/DSP owners after once-only
deactivation. Control may perform the existing stopped worker indexed seek and
explicitly prepare/open a fresh clocked pair. Failed seeks retain queue/cursor/
generation as specified by the file worker; success adopts its exact suffix.

Converted/prefetched capture histories are disposed on control, with no rollback,
file state restoration or physical clock alignment inferred. The explicit recovery
coordinator waits old callback proof before invoking the caller's restoration
policy. No native live seek mailbox, automatic file-state migration or new device
validation claim follows. See [audio devices](audio-devices.md) and
[decision 0231](decisions/0231-reclaimable-clocked-duplex-ownership.md).
