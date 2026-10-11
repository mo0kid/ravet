# 0221. Select exact integer-rate scheduling for bounded-memory long file streams

- Status: Implemented prototype; scoped offline validation.
- Work package: W208.

The existing floating-distance sinc profile has a finite duration proof and must
keep its accepted sample behavior. Add an explicit integer constructor and file
conversion policy rather than silently switching legacy fixed/live modes or merely
raising their source ceiling.

Retain a signed relative numerator D initially O. Input subtracts O only when input
is required; output adds I only when D<=0. After processing begins, -O<D<=I, so
full-u32 positive rates fit signed 64-bit arithmetic independently of stream duration.
The exact identity D=O+J*I-C*O yields output J after 1+ceil(J*I/O) consumed inputs.
Only bounded fractional coefficient interpolation uses f64. Reuse existing quality,
history, delay, prefix and explicit padding mechanics; no implicit live ppm or
spectral/deadline promise is introduced. Reset restores exact initial phase.

Add PlaybackConversion::IntegerSinc. Downstream exhaustive enum matches must handle
this additive policy. Existing Copy/Sinc defaults, output behavior and source
ceilings remain unchanged. Complete-file memory limits stay bounded separately.
Long integer read-ahead uses caller usize source bounds, checked u128 rational duration
and finite u64 output coordinates, capped at `u64::MAX-65536` for largest automatic callback
terminal render headroom. Validate determinable bounds before reader/factory I/O;
small batch limits precede usize conversions. Preserve reader/container limits,
committed-prefix/error ownership and exact successful-candidate generation adoption.

Long stopped seek remains bounded-memory replay O(target), not an efficient index
or live callback operation. Failed candidates preserve old queue/phase/suffix/cursor;
factory side effects cannot be rolled back. Native output ownership is explicit and
reclaimed only after callback retirement. This work adds no dependency and makes no
native timing, broad physical-device, clock/drift, RF64 or long-compressed guarantee.

Acceptance requires independent rational scheduling/partition/reset/heap evidence,
unchanged floating/live regressions, actual conversion/replay beyond the old source
limit, terminal impulse/tail and verified EOF counts, late candidate error retention,
near-coordinate-limit failure-owned preflight and actual prepared output lifecycle.


Scoped acceptance includes 38 signal tests (including the independent absolute
integer oracle, partition/reset/heap tests, floating/live regressions and recurrence
past 16,777,216 inputs), 25 file tests and eight actual prepared-output groups.
Seven device groups now run both explicit Sinc policies. The long fixtures process
16,777,249 original frames, including a real WaveReader over generated RIFF bytes;
no whole source PCM allocation, physical-file or native-device claim follows.
The actual WAV stream reaches verified EOF with 1,048,579 retained output frames.
A late failed fresh-reader replay preserves the old queued/pending prefix, counts
and generation; successful replay past the former ceiling retains the exact final
impulse and finite tail. Near-coordinate preflight accepts exactly the retained
headroom boundary and rejects boundary+1 with unchanged owned reader and no I/O.

Warnings-denied all-target Clippy, nine doctests and warnings-denied Rustdoc pass
for the three affected packages. The scoped 64-tap 977 Hz comparison against the
existing floating schedule measured whole-stream maximum differences 4.40e-14
(44.1→48 kHz), 3.34e-7 (48→44.1 kHz), and zero (48→24 and 96→96 kHz).
Interior comparison excludes only the first 100 output samples and measured
263.90/143.22 dB SNR for those first two profiles. Independent passband RMS,
18 kHz downsample rejection and 31.5-sample input delay checks pass. These fixtures
do not establish arbitrary spectral quality or native clock/physical support.
