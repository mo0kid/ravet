# 0264: Prepared calibrated loudness and loudness range

Status: accepted scoped implementation; synthetic numerical and offline Graph
evidence only, recorded in work status separately.

## Decision

Provide a reusable immutable K-weighting design and bounded prepared one-programme
M/S/I/LRA owner. Support every integer 8000..768000-Hz rate, explicit 1..64 roles,
native-f64 measurement and exact widening of native-f32 source. Reuse existing
strictly admitted biquad recurrence unchanged, adding only readonly delay-state
inspection. Use independently expressed bilinear calibration facts, disclose actual
coefficients/response and retain native platform rounding/rate deviations.

Use exact rational 10-Hz endpoints floor(k*Fs/10) with bounded integer remainders.
Four-grid windows have visible sample quantization and thirty-grid windows exactly
3Fs frames. Positive scaled chunk energies and log-domain records avoid preventable
squaring/physical-power overflow and sliding subtraction. Source-frame means retain
actual counts. Complete I/LRA records preserve retrospective gates rather than an
approximate histogram. Explicit strict I/inclusive LRA gates and nearest zero-based
10th/95th indices are part of this profile, without a normative odd-rate convention
or standards certification claim.

Independent I/LRA capacities each 1..1,000,000 and prepared LRA scratch pass a
defensive 32-MiB guard on requested and on actual allocated capacity. Current caps
request at most 24,000,000 array bytes plus the owner, so the pre-reserve refusal is
not reachable today; `ResourceLimit` is kept for a larger cap, and actual capacity
may exceed a request, so the post-allocation check is not called unreachable. Whole
calls preflight every shape/prefix, maximum, lifecycle, source headroom and exact
upcoming record count. Raw audio remains unchanged. NaN/Inf are measured silence
with flag; actual finite-input filter output/next-state overflow latches original
first coordinate and invalidates readings and reports until reset, while accepted
counts continue. The readonly record slices stay available afterwards as raw
diagnostic history (pre-fault records, partial windows, later `-inf` windows), not
as valid reports. No false rollback is implied.

Programme pause retains complete records but discards partial programme windows and
both rings; K filtering of weighted (Front/Surround) channels and live M/S continue,
while Excluded channels run no filter and keep no K history. Resume starts fresh full
programme windows. Reset clears all history while retaining running/profile/capacities. Exclusive
CONTROL analysis scans/log-sums/sorts prepared storage; no concurrent report/UI
channel or automatic audio analysis is added.

Explicit stopped finish either discards incomplete windows or advances exactly
ceil(3Fs/2) zero-input frames for LRA only. Original/live/I counts stay separate.
Exact tail record admission can require sixteen new observations at odd rates;
capacity refusal changes nothing and a caller can explicitly retry without tail.
Success is idempotent and the first selected policy wins; with no admitted programme
frame the tail request succeeds without a tail. The artificial tail is this crate's
own policy, not a standards requirement, and `None` is preferred when only the
original duration is analysed. No implicit Drop/deactivate processing and no finite
IIR tail-drain claim. Source admission reserves full optional-tail counter headroom.

## Evidence and limits

A Decimal bilinear/complex-response oracle (independent arithmetic over the SAME
calibration facts, so it does not certify them; the CSV holds exactly 170
coefficient and 85 response identities), an independent 48 kHz numeric anchor from
ITU-R BS.1770-5 Annex 1 Tables 1/2 (numerical calibration only, not conformity),
direct-form I absolute-source windows, all 760001 integer-rate admissions/all ten
remainders, strict/inclusive boundary distributions, exact ranks, power weights,
independent64-channel histories, 40-s5/10/20-LU programmes/repetition/silence with
independent gates, discriminating reset/pause/live-oracle/Excluded/f32/fault-injection
tests, failures/headroom/heap and actual native32/64 Foundation→Graph→FIR ownership
tests (twenty-four ordinary combinations plus reset and poison follow-ups over the
same matrix) qualify the selected contract. An optional caller-selected external
FFmpeg 6.1.1 comparison preserves reference histogram/rank/grid differences and
printed precision, and its 0.233355 LU LRA mismatch stays visible. Against EBU Tech
3342 V4 only the rank equivalence, inclusive gates and 10-Hz hop were compared.
Rate calibration variation remains explicit as selected-transfer facts at named
frequencies (for example +0.215 dB at 37 Hz for 8 kHz versus 48 kHz), not a wideband,
profile-accuracy or conformity bound. The isolated W251 R4 draft and the staged
workspace adoption executed these tests; the FFmpeg comparison remains an optional
recorded receipt. No dependency, codec, published coefficient table, external
programme fixture, automatic spatial metadata or hardware support is introduced.

This resolves the prepared calibrated M/S/I/LRA numerical family; native SIMD,
processor-wide denormal state, concurrent live report/UI delivery, advanced spatial
metadata, normative/certified metering and physical timing remain independent.

See the [signal-processing guide](../signal-processing.md#prepared-calibrated-loudness-and-loudness-range).
