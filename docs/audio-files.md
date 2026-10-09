# WAV/AIFF streams and audio handoff buffers

The explicit `ravet-audio-file` crate supplies bounded seekable WAV/AIFF readers and
writers plus a prepared interleaved SPSC audio ring. File methods allocate or
perform blocking I/O on control/worker threads. Only prepared ring push/pop
belongs in audio callbacks; endpoint destruction follows callback shutdown.

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
no I/O. File size must fit the 32-bit FORM limit. WAV read-ahead/recorder workers
currently remain WAV-specific; AIFF consumers explicitly drive these readers and
writers on control/worker threads and can use the shared prepared audio ring.

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
Compressed encoders, indexed/file-backed random access and automatic clocked
transport remain separate work; the lazy worker reader below provides exact
replay-based seeking.

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
queued and pending old-generation samples. Live streamed seek/loops, transport clock/UI and automatic worker ownership
remain pending. Prepared memory playback is available separately below.

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

`CompressedReader` uses the same optional/pinned FLAC/MP3/Vorbis backend with
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
This deliberately provides O(target) bounded worker seeking, without assuming
codec-native timestamp/preroll behavior. It is not efficient indexed random access,
file-backed streaming, a sample-clock scheduler or callback-safe seeking.

Synthetic encoded fixtures for all three formats match complete decode exactly
under variable short reads and forward/backward/packet-boundary/end seeks. Tests
also cover output tails, malformed/oversized inputs, failed-seek preservation,
deferred corruption/poison and successful recovery. See
[decision 0068](decisions/0068-compressed-worker-streams.md).

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
users. It first completes exact replay, then clears queued/pending old-generation
audio and poison. Foreign rings or failed replay preserve queued/pending state.

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
Vorbis/MP3 encoding, physical recording, file-backed seeking and automatic
clocked playback remain
separate work. See [decision 0070](decisions/0070-worker-flac-encoding.md).

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
