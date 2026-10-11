# Prepared signal processing

These primitives are independent of devices, formats and editors. Prepare designs
and storage on control threads; processing/reset use existing storage. APIs are
provisional. Audio values are not sanitized: nonfinite input and numerical
overflow may propagate. Users remain responsible for channel instances, smoothing,
denormal policy, lifecycle, headroom and actual callback deadlines.

## Waveshapers

`Waveshaper::new(function)` stores a caller-owned transfer function inline.
Custom closures accept and return the same sample type, including native f32 or
f64 without conversion. `process_sample` applies it once; `process_in_place`
and `process_block` visit samples in order. Separate input/output lengths must
match exactly: a mismatch preserves output and never invokes the function.
Empty blocks are no-ops. The wrapper owns no sample history, sample-rate setting,
latency or tail, and needs no reset. Custom captured state/resources remain
caller-owned; cloning/destruction follows their ownership.

Three f64 constructors provide explicit curves:

| Constructor | Transfer function | Output endpoints |
|---|---|---|
| `hard_clip()` | clamp x to [-1, 1] | +/-1 |
| `tanh()` | tanh(x) | asymptotic +/-1 |
| `cubic_soft_clip()` | x - x^3/3 inside [-1, 1], constant beyond | +/-2/3 |

All have unity slope at zero. Cubic clipping has continuous zero slope at the
knees and is not peak-normalized to unity. Clamping before cubic arithmetic
avoids overflow for extreme input. Built-ins propagate NaN and bound infinite
input; custom functions receive samples unchanged and define their own behavior.
The tanh profile uses a transcendental per sample.

```rust
use ravet_signal::{Oversampler, OversamplingFactor, Waveshaper};

let shaper = Waveshaper::new(|x: f32| (2.0 * x).tanh());
let mut oversampler = Oversampler::new(OversamplingFactor::Four);
let wet = oversampler.process_sample(0.25, |x| shaper.process_sample(x));
assert!(wet.is_finite());
```

Drive/output gain, smoothing, DC removal and oversampling are explicit consumer
choices. Nonlinear transfer functions can alias. The existing f32 oversampler
can call a custom f32 waveshaper as shown; f64 curves do not imply an f64
oversampler. Wrapper construction/processing allocate nothing, but a custom
function must meet its own audio-thread obligations. Tests cover curve landmarks,
symmetry/monotonicity/extremes, native precision, ordered calls, block partitions,
atomic length rejection, explicit oversampling composition and 10000 workloads
without counted heap actions. This is offline evidence, without an alias-free or
native callback-deadline claim. See
[decision 0149](decisions/0149-sample-wise-waveshapers.md).

## Biquad filters

`BiquadCoefficients::design` supplies lowpass, highpass, unity-center bandpass,
notch, allpass and peaking filters. Sample rate must be finite/positive, frequency
strictly between DC and Nyquist, Q in (0, 1,000,000], and peaking gain between
-120 and 120 dB. The equations follow the
[Audio EQ Cookbook](https://www.w3.org/TR/audio-eq-cookbook/).
`new` also accepts normalized numerator/denominator coefficients, checking finite
values and strict second-order Jury stability. Numerically unrepresentable
stability margins reject instead of producing a nominally stable filter.
`magnitude_at` evaluates linear amplitude at normalized frequencies 0..=0.5.

`Biquad` uses one channel of transposed direct-form II f64 state. Replacement
preserves delay state; reset clears it. Neither operation smooths transitions.
Even-order Butterworth designs and explicit cascades are described below;
automatic modulation remains pending. Tests check
response landmarks and an independent direct-form impulse recurrence across
block partitions, plus invalid/unstable designs.

## Complex FFT

`FftPlan` allocates twiddles for a power-of-two size from 1 through 1,048,576.
Forward/inverse operate on caller-owned exact-length `Complex` storage, rejecting
length mismatches before mutation. Forward uses negative complex phase; inverse
uses positive phase and divides by N. The radix-two implementation is f64 and
in-place after bit reversal. It has no real-only packing, window selection or
SIMD backend. Tests compare against a direct complex DFT for multiple sizes and
check inverse reconstruction and atomic rejection.

## FIR convolution

`FirConvolver` copies 1..=65,536 finite impulse taps and allocates a history ring
during construction. Its f64 direct convolution has zero algorithmic block
latency and an impulse-length-minus-one tail. Processing cost is O(taps) per
sample: this is suitable for short responses and does not establish a deadline
for the maximum size. Response replacement requires re-preparation; no live
crossfade is supplied. Tests compare an independent
finite convolution sum and verify reset/partition continuity.

`PartitionedConvolver` supplies a separate uniform overlap-add FFT engine for
1..=262,144 finite f64 taps. A power-of-two partition of 16..=4,096 samples sets
the exact algorithmic latency; caller block sizes can vary freely. Each boundary
transforms the new input, sums delayed input spectra against response partitions,
and combines inverse-transform output with the previous overlap. Feed zeros to
flush the declared tail of latency plus taps minus one. Reset clears every
history buffer while retaining the prepared response. Preparation owns the
response spectra, input spectrum ring and scratch; processing/reset allocate
nothing. Work is concentrated at partition boundaries, with no deadline claim,
zero-latency head, nonuniform partitioning, crossfade or automatic worker setup.
Tests compare against independent direct sums, exercise arbitrary caller blocks,
maximum-length delayed impulses, reset and heap instrumentation. See
[decision 0031](decisions/0031-partitioned-convolution.md).

## Native-precision convolution schedules and banks

`Convolver32` and `Convolver64` own one prepared finite response of 1..=262,144
taps and run every operation (direct head, FFT butterflies, spectral products and
sums, overlap and output) in the named native sample precision. FFT twiddles are
computed in f64 during preparation and, for f32, explicitly rounded to `Complex32`.
`ConvolutionPartitioning` fixes the schedule at preparation:

- `UniformDelayed { partition }`: one overlap-add stage; algorithmic latency is
  exactly `partition` frames (16..=4,096, power of two). For f64 this matches
  `PartitionedConvolver` sample-for-sample.
- `ZeroLatency { head }`: taps `0..head` run as a direct FIR, taps from `head` in a
  `head`-partitioned stage whose intrinsic delay equals their original offset. Zero
  algorithmic latency. Responses no longer than `head` have no FFT stage.
- `NonUniform { head, tail }`: direct head, a `head`-partitioned middle stage for
  taps `head..tail` and a `tail`-partitioned late stage (`tail` power of two,
  `head..=16,384`). Zero latency; empty intervals are omitted. This is a fixed
  two-stage schedule, not an optimal arbitrary partitioning.

`info()` reports original taps, direct taps, each stage's offset, taps, partition
count and FFT size, the exact latency, the tail (`taps - 1 + latency`) and declared
and actual numeric bytes. `stage_work()` reports completed FFT blocks per stage
(saturating counters, no timing). Every processed source frame produces one output
frame; supply explicit zeros to drain the tail. Reset clears all history and keeps
the response and schedule.

Preparation checks the schedule, response finiteness, a maximum block of
1..=65,536 frames and a caller numeric byte budget of 1 byte..=256 MiB before any
allocation, reserves storage fallibly, and rechecks actual capacities. Allocator
overhead is excluded from the budget. Processing and reset do not allocate or
deallocate. Calls whose frame count exceeds the prepared maximum, or whose slices
are shorter than the requested prefix, are refused before any history or output
changes; unused slice suffixes are untouched.

`ConvolutionBank32`/`ConvolutionBank64` own 1..=64 independent diagonal channels
sharing one schedule, each with its own response. The aggregate budget and every
response are checked before any channel is built. Planar and in-place calls are
admitted for the whole bank (channel count, maximum, all prefixes) before any
channel runs; `reset_channel` affects one channel only. There is no stereo
crossfeed, true-stereo matrix or linked normalization.

`ravet-signal` does not implement the Foundation `Processor` trait for these
types. A processor wrapper reports `info().latency_samples` and the largest
channel `tail_samples` so Graph latency compensation uses the actual algorithmic
delay; `engine_mut` gives per-channel access for frame-interleaved processing. The
crate tests include such a wrapper in an actual Graph.

Nonfinite and overflowing values propagate until reset; there is no denormal,
gain, trim or sample-rate policy. Direct-head cost is O(head) per sample and FFT
work is concentrated at partition boundaries; no real-time deadline is claimed.
The engines do not load, convert, trim or normalize responses, and do not replace
them live. Explicit preparation and live transitions are described below.

Tests compare against independent f64 direct sums and an analytic frequency
response, check bit-identical output across arbitrary block sizes including 1 and
odd sizes, single taps at every stage boundary, maximum-length responses, empty
stage intervals, zero and subnormal responses, bank refusals and channel isolation,
allocation-free processing and reset, and an actual Graph with a downstream FIR and
a latency-compensated dry path. See [decision 0270](decisions/0270-native-convolution-schedules.md).

### Native f32 FFT

`FftPlan32::try_new(size, maximum_twiddle_bytes)` prepares a 1..=1,048,576-point
radix-two complex f32 FFT with the same sign and `1/N` inverse convention as
`FftPlan`. `FftPlan::try_new` is an additive bounded constructor for the f64 plan;
`FftPlan::new` and all f64 transform results are unchanged. Both return
`FftPreparationError::{InvalidSize, Capacity, Allocation}`. At the maximum size, the
f32 plan shows about 3.2e-11 coherent leakage on an exact quarter-cycle tone because
the f64 value of cos(pi/2) is not exactly zero before rounding; this is recorded,
not corrected.

## Source-rate impulse-response preparation

`prepare_impulse_response(source_rate, &[&[f64]], &ImpulseResponseRequest)` prepares
planar responses at their source rate for a target engine rate. The request has no
defaults and its stages always run in the order map, convert, trim, normalize:

- `ResponseRateConversion::RequireEqualRates` makes an exact copy and refuses unequal
  rates. `Sinc { taps, gain, delay }` uses `SincResampler::new_integer_rates` (even taps
  16..=256, ratio 1/16..=16), also at equal rates. It produces
  `floor((N + taps - 2) * target / source) + 1` frames. `ResponseRateGain::PreserveResponseGain`
  scales by `source / target`, preserving the frequency response;
  `PreserveSampleAmplitude` keeps gain 1. `ResponseDelay::Retain` keeps the
  `(taps - 1) / 2` source-sample filter delay. `RemoveWholeTargetFrames` drops its whole
  target frames and reports the fractional remainder.
- `ResponseChannelMap::Identity` or `Select(&[source index])` (1..=64, duplicates and
  reorders allowed, no mixing).
- `ResponseTrim` sets optional leading and trailing `TrimThreshold::Absolute` or
  `RelativeToPeak` thresholds. Trimming is linked across channels and the removed frame
  counts are reported.
- `ResponseNormalization::None`, `Peak` or `Energy` applies one linked gain, which is
  reported. Energy sums squares after scaling by an exact power of two near
  `1 / peak`. Huge finite responses are therefore not zeroed, and subnormal-scale
  responses are not refused as silent. The gain is bitwise the direct formula when
  nothing over- or underflows. An overflowing gain is `NonFinite` and an underflowing
  one is `UnrepresentableGain`. An all-zero response is accepted only without
  normalization (it then convolves to silence).

`ImpulseResponseRequest::plan` validates every policy and computes exact frame counts and
declared bytes (result, staging and sinc table, budget 1..=256 MiB) before any allocation.
Prepared length is at most `maximum_taps` (1..=262,144). Refusals return no partial
response. The `ImpulseResponseReport` records rates, frame counts, rate gain, removed and
retained delay, trim offsets and thresholds, normalization gain, peaks and energy.
`PreparedImpulseResponse::{bank64, bank32, engine64, engine32}` build engines; f32 uses
explicit round-to-nearest conversion and fallibly reserved builder scratch. This is
CONTROL or worker work that allocates; the existing sinc table uses its own infallible
allocation, counted in the budget. The onset shift caused by preparation is
`removed_delay_frames + leading_trimmed_frames` whole frames, with
`retained_delay_target_frames` still present.
`ravet-audio-file`'s `prepare_impulse_response_file` reads WAV, AIFF or compressed files
through the same path (see [audio files](audio-files.md)).

Tests compare resampled Gaussian-burst responses against the analytic continuous
response and its Fourier transform for six rate pairs. The maximum error relative to peak
is below 1.6e-5 with 64 taps and 1.1e-6 with 256 taps. The tests also cover length
formulas, bitwise gain and delay relations, exact mapping, trim offsets and normalization
gains, energy normalization at 1e200, `f64::MAX` and subnormal scale, and allocation-free
policy refusals. Live replacement is in
[audio graphs](audio-graphs.md). See [decision 0272](decisions/0272-impulse-response-preparation-and-live-convolution-transitions.md).

## Configurable oversampling

`Oversampler` selects `OversamplingFactor::{One,Two,Four,Eight}`. It cascades the
existing fixed f32 `HalfBand2x` profile with three state objects and stack
scratch. `process_sample` invokes a caller's DSP closure exactly factor times
in temporal order before decimating. Prepare that DSP at host rate times factor.
Factor One is an exact bypass around the filters. Reset clears all stages;
changing factor requires a new instance, with consumer-owned transition policy.
Wet-path phase delay remains frequency dependent; no dry alignment or constant
integer host latency is claimed. An arbitrary-ratio sinc converter is provided separately; there is no oversampling
filter coefficient designer. Tests check DC, call counts, reset and partition invariance.

## ADSR envelopes

`AdsrConfig` validates finite positive sample rate, nonnegative durations in
seconds, sustain amplitude 0..=1 and rounded sample counts up to u32::MAX.
`Adsr` uses linear attack/decay/release from its current level, with exact final
sample targets. Retrigger preserves the current level. Zero-duration stages
transition immediately; note-off begins release from the current level. A final
attack sample retains its peak even when decay duration is zero; the next sample
is sustain. Reset returns to idle/zero. Configuration replacement requires a new
instance; consumer voices own note timing and sustain-pedal policy. Tests check
ramps, interruptions, retrigger, zero durations, partitioning and count bounds.

A shared allocation-counter fixture runs 1,000 prepared FFT/filter/convolution/
8x-oversampling/envelope workloads, including resets, with zero counted Rust allocation
or deallocation. Native host behavior and callback deadlines are outside that
evidence. Additional dynamics/synthesis profiles remain pending.
See [decision 0019](decisions/0019-signal-kernels.md).

## Synthesis oscillators and dynamics

`Oscillator` generates f64 sine, saw, square and triangle with phase-continuous
frequency changes and waveform switching. `set_waveform` retains phase and
frequency; changes are immediate without a crossfade. Rate is finite/positive and frequency is from zero to strictly
below Nyquist. Sine uses per-sample trig; saw/square correct edges with polyBLEP.
Triangle is a naive waveform and may alias. These profiles are not an alias-free
or deadline claim. Reset clears phase. Invalid retuning preserves frequency/phase.

`EnvelopeFollower` rectifies samples and applies one-pole attack/release time
constants (63.2% settling); times are 0..60 seconds and zero is instantaneous.
`CompressorConfig` prepares threshold -120..24 dB, ratio 1..100, knee 0..48 dB and
makeup -60..60 dB. `Compressor` uses an explicit sidechain and no lookahead. It
provides no hard brickwall limiting, linked channels or automatic makeup; it
uses log/pow per sample. Nonfinite audio can poison history until reset. Tests
compare independent exponential steps, static ratio/knee landmarks, sidechain
behavior, waveform phase/edge corrections and heap counts.

## Arbitrary-ratio streaming conversion

`SincResampler` prepares 1025 normalized phases of a Blackman-windowed sinc kernel
with an even 16..256 tap count. Input/output rate ratio is 1/16..16; finite positive
rates use the same units. The fixed cutoff is 95% of the smaller Nyquist. One f64
channel uses a prepared history ring and interpolated phase coefficients. Cost
is O(taps) per output. The causal filter delays by (taps-1)/2 input samples.
Initial history is zero; callers supply trailing zeros to flush a finite stream.

`process` returns exact consumed/produced prefixes. Retry only the remaining
input, with fresh output capacity. Zero output capacity consumes nothing; empty
input can drain pending upsample output. Unproduced output remains untouched.
The fixed constructor keeps its rate. Reset clears history without allocation
and retains any variable-profile adjustment described below.
Tests cover 1/16..16 ratios and 44.1/48 kHz, output backpressure, partition
identity, counts, settled DC, a passband/stopband fixture and reset/heap paths.
Those fixtures do not establish a general spectral rejection specification,
long-duration clock accuracy, SIMD optimization or callback deadlines.
See [decision 0025](decisions/0025-synthesis-dynamics-resampling.md).

## Downward-expansion noise gate

`NoiseGateConfig` prepares a -120..24 dB threshold, expansion ratio 1..100,
0..48 dB soft knee, -120..0 dB attenuation floor and 0..60 second detector
times. Below a hard-knee threshold, output dB changes by the expansion ratio;
a quadratic soft knee joins the above-threshold unity region continuously.
The attenuation floor is finite; ratio one is unity even with a silent detector.

`NoiseGate` applies the existing rectified attack/release detector to an explicit
f64 sidechain. These times smooth the detector, rather than a separate gain
stage. Reset clears detector history and preserves configuration. Preparation
validates all configuration; sample/reset paths allocate nothing and have no
lookahead. Log/pow run per sample. Hold, hysteresis, linked channels, hard mute,
nonfinite-input sanitization and host/device deadline validation are outside
this profile. Tests compare independent dB/knee landmarks, exponential detector
steps/releases, silent floor/reset, invalid configurations and 10000 heap-free
sample/reset iterations. See [decision 0086](decisions/0086-downward-noise-gate.md).

## Low/high shelving filters

`BiquadCoefficients::design_shelf(ShelfKind::Low/High, rate, midpoint, gain_db,
slope)` adds low/high shelves to the existing prepared one-channel processing.
Gain is -120..120 dB; midpoint is strictly between DC and Nyquist. Shelf slope
is in (0,1], distinct from Q: one gives the steepest monotonic cookbook profile.
The midpoint response is half the requested gain in dB. The selected endpoint
gets full gain while the opposite endpoint remains unity. Equal boost/cut designs
are reciprocal. Formula reference: [Audio EQ Cookbook](https://www.w3.org/TR/audio-eq-cookbook/).

Existing finite coefficient/Jury stability checks still apply; extreme designs
with unrepresentable margins reject. Smoothing, headroom and
nonfinite/denormal policies remain caller concerns. Three groups check endpoint/
midpoint and reciprocal responses, monotonic grids over both shelves/gains/slopes,
boost/cut impulse cancellation, block partition identity, reset, invalid settings
and heap-free processing. These fixtures establish the tested portable profiles,
rather than every numerical corner or a native deadline. See
[decision 0087](decisions/0087-shelving-filters.md).

## Fixed cascades and Butterworth designs

`CascadeCoefficients<N>` validates 1..16 existing second-order sections in fixed
storage. `BiquadCascade<N>` processes one f64 channel through those stages.
Sample/block/reset and coefficient replacement allocate nothing. Replacement
preserves every stage's history without smoothing; reset clears all histories.
Arbitrary stage gains can overflow the response product or audio, so consumers
own gain/headroom and nonfinite/denormal policies.

`CascadeCoefficients::butterworth` prepares lowpass/highpass orders 2..32 (order
is twice N), ordered from low to high pole-pair Q. Rate is finite/positive and
cutoff is strictly between DC/Nyquist. Existing stable biquad validation rejects
unrepresentable designs. The cutoff magnitude is 1/sqrt(2), with bilinear-warped
maximally flat passband. [SciPy's Butterworth reference](https://docs.scipy.org/doc/scipy/reference/generated/scipy.signal.butter.html)
and [prototype poles](https://github.com/scipy/scipy/blob/main/scipy/signal/_filter_design.py)
provide design reference; SciPy is not a runtime dependency and no implementation
source is copied. Odd orders, bandpass/bandstop, order selection and other filter
families remain separate work.

Tests compare all sixteen even orders/both passbands against independent warped
analytic response grids and cutoff/endpoints, rendered cutoff RMS, block
partitioning, reset, state-preserving changed coefficients, explicit stages,
invalid capacities/settings and prepared heap counts. This is portable fixture
evidence; it does not establish extreme numerical accuracy or host deadlines.
See [decision 0088](decisions/0088-fixed-filter-cascades.md).

## Sample-peak limiter

`PeakLimiter::new(rate, ceiling_db, release_seconds)` prepares one f64 channel
with ceiling -120..24 dBFS and release 0..60 seconds. The rectified detector
attacks instantaneously and releases exponentially. Gain is the smaller of
unity and ceiling/detector level; a final symmetric hard clamp protects rounding
at the sample ceiling. Zero release is memoryless limiting. `gain` and
`detector_level` expose the latest scalar values, without creating meter windows.
Reset clears history and restores unity gain while retaining the ceiling.

Processing/block/reset allocate nothing and add no lookahead. Independent
channels are not linked. This limits sampled amplitudes, not reconstructed
intersample/true peaks. Clipping can distort; no oversampling or separate gain
smoothing is implied. Nonfinite input is not validated: NaN propagates and can
poison detector history until reset; infinite input passes the hard clamp but
can also poison history. Tests cover both polarities, extreme finite values,
pass-through, memoryless reference, independent release recurrence, attacks,
partition identity, reset, invalid settings and 100000 heap-free samples.
No native deadline or true-peak claim follows. See
[decision 0089](decisions/0089-sample-peak-limiter.md).

## Linear state-variable filter

`StateVariableCoefficients::new(rate, cutoff, q)` prepares a one-channel f64
trapezoidal SVF. Rate is positive/finite, cutoff/rate is 1e-6..=0.499999 and
Q is 0.1..=100; design rejects outside these numerical limits. Create a
`StateVariableFilter`, then call `process_sample` once to receive simultaneous
`low_pass`, `band_pass` and `high_pass`. Raw band-pass gain at cutoff is Q.

Processing/reset use fixed two-integrator state and allocate nothing. Replacing
prepared coefficients retains histories; control smoothing is the caller's
choice. Nonfinite samples can poison state until reset and extreme amplitudes
can overflow. There is no nonlinear saturation/self-oscillation, denormal policy,
arbitrary-modulation stability or native callback-deadline guarantee. See
[decision 0095](decisions/0095-linear-state-variable-filter.md).

## Prepared comb filters

`CombFilter::new(CombKind, delay_samples, coefficient)` prepares a one-channel
f64 comb with a fixed delay D of 1..=1048576 whole samples. `Feedforward` computes
`y[n] = x[n] + g*x[n-D]`, a finite input echo; `Feedback` computes
`y[n] = x[n] + g*y[n-D]`, a recursive echo train. Both include current input at
unity gain. Positive and negative coefficients change the comb's reinforcement
and cancellation frequencies. Feedforward accepts any finite g (including +/-1
for complete notches); feedback requires finite `|g| < 1` for fixed-coefficient
stability. Delay is measured in samples, so consumers convert their own time/rate.

```rust
use ravet_signal::{CombFilter, CombKind};

let mut comb = CombFilter::new(CombKind::Feedback, 2, 0.5).unwrap();
let mut impulse = [1.0, 0.0, 0.0, 0.0, 0.0];
comb.process_in_place(&mut impulse);
assert_eq!(impulse, [1.0, 0.0, 0.5, 0.0, 0.25]);
```

Preparation validates before allocating exactly D f64 history values (at most
8 MiB, excluding overhead). `set_coefficient` validates before mutation and
retains history without smoothing. A zero coefficient returns input exactly
while advancing history. `process_sample`, `process_in_place` and `reset` use
existing storage; empty blocks do nothing and arbitrary partitions match.
Reset retains the prepared kind, delay and latest coefficient. Type/delay changes
require a new instance; instantiate separately for channels.

This standalone profile supplies no damping, fractional delay or normalization.
Feedback near unity has substantial gain and a long tail; consumers own headroom,
smoothing, nonfinite/denormal policy and deadlines. The reverb below has its own
private lowpass-feedback comb network. Tests cover exact signed impulse timing,
independent full-history recurrence with live edits, analytic complex frequency
response, partition/reset/zero history, atomic invalid edits and bounded maximum
storage with 10000 heap-instrumented prepared workloads. See
[decision 0150](decisions/0150-prepared-comb-filters.md).

## Prepared synthetic mono reverb

`SchroederReverb::new(ReverbConfig)` allocates four parallel damped feedback
combs and two serial canonical allpasses. Defaults: 48000 Hz, room scale 1,
nominal comb DC decay 1.5 seconds, lowpass pole frequency 6000 Hz, wet mix 0.25.
Comb delays are 31/37/41/43 ms; allpass delays 5/1.3 ms, scaled and rounded to
samples. `delay_lengths` exposes these six exact prepared lengths. Allpass
feedback is 0.5 and the four comb outputs are averaged before diffusion.

Rate must be 8000..=192000 Hz, room scale 0.5..=2, nominal decay 0.1..=20 seconds,
lowpass pole frequency >=20 Hz and below Nyquist, and wet mix 0..=1. The lowpass
pole is exp(-2*pi*frequency/rate), a continuous-pole parameter rather than an
exact digital -3 dB cutoff. Comb feedback is 10^(-3*delay_samples/(rate*decay)).
Complete network decay depends on damping/diffusion; no exact overall T60 claim.

`process_sample`/`process_in_place`/`reset` allocate nothing. `set_wet` validates
before mutation and keeps histories without smoothing. Dry mode still advances
the network. Reset clears history and keeps configuration. Delay storage totals
at most 60787 f64 samples (six allocations, excluding struct overhead). There is
no physical-room fidelity, stereo decorrelation/crossfeed, freeze/modulation,
headroom/denormal policy, or native deadline guarantee. Feedback can amplify
sustained signals; consumers set levels. Nonfinite samples can poison state and
extreme finite amplitudes can overflow. See
[decision 0096](decisions/0096-prepared-mono-reverb.md).

## Real spectra and prepared windows

`RealFftPlan::new(N)` supports power-of-two N=1..1048576 and owns a full complex
FFT plus N complex scratch values. `forward` takes N f64 samples and returns
N/2+1 bins (DC through Nyquist; N=1 only DC), unnormalized negative phase.
`inverse` conjugates the omitted negative bins and returns N real samples with
1/N normalization. This half-spectrum API uses a full complex backend, without
claiming real-only performance. Length mismatches reject before caller mutation.
Inverse requires exactly zero imaginary endpoints, rejecting NaN/Inf there;
forward explicitly zeroes them. Other nonfinite values propagate.

`WindowPlan::new(kind, convention, N)` prepares 1..1048576 f64 coefficients for
rectangular, Hann, Hamming, Blackman or four-term Blackman-Harris windows.
Symmetric uses phase denominator N-1; periodic uses N, suitable for analysis.
N=1 is unity for all families. There is no sampled peak normalization: an even
symmetric window may peak below one, and tiny negative rounded taps are retained.
`apply` requires exact length, rejecting before mutation; no implicit FFT window.

`coherent_gain` is the actual coefficient mean, `energy` their squared sum, and
`equivalent_noise_bandwidth` is N*energy/sum^2 in bins (None for exact zero sum).
Consumers explicitly correct coherent-bin amplitude; for an interior real
sinusoid use 2*bin_magnitude/(N*coherent_gain), with different endpoint scaling.
This does not recover off-bin scalloping automatically. All application/transform
paths reuse prepared storage. No overlap/add, automatic gain correction, SIMD,
nonfinite sanitization or native deadline guarantee. See
[decision 0097](decisions/0097-real-spectra-and-windows.md).

## Bounded live sinc rate adjustments

`SincResampler::new_variable(input_rate, output_rate, taps, maximum_ppm)` prepares
symmetric live adjustment bounds: finite maximum in (0,10000] ppm (one percent).
The entire nominal input/output step range must remain within 1/16..16. Kernel
cutoff uses 95% of min(1, 1/maximum_step), covering the permitted downsampling
range without replacing coefficients live. The fixed constructor retains its
original cutoff/arithmetic and permits only zero adjustment.

`set_rate_adjustment_ppm(ppm)` rejects nonfinite/out-of-range values before
mutation and allocates nothing. Positive ppm increases input consumed per output
sample; `rate_step` exposes this ratio. Histories and the next output's already
scheduled position remain; new spacing applies afterward. Reset clears stream
history and retains current adjustment; set zero explicitly for nominal rate.
`maximum_adjustment_ppm` exposes the prepared bound. Caller supplies smoothing,
clock estimation and scheduling; no automatic rate/FIFO/drift controller or
implicit device conversion is added. Independent piecewise/counted fixtures do
not establish physical clock compensation or broad spectral/native deadline
support. See [decision 0102](decisions/0102-bounded-live-resampling.md).

## FIFO clock-depth controller

`FifoClockController` applies exponential queue-depth smoothing, proportional
ppm correction, a clamp and a correction slew limit. Supply complete queued plus
prefetched input frames and elapsed output-clock seconds to `observe`. Positive
correction asks a variable resampler to consume input faster. The proportional
model retains a steady occupancy offset under constant drift, rather than using
an integral estimate. Caller matches its maximum to the prepared resampler.

`FifoClockConfig` bounds capacity to 2..131072 frames with target strictly inside,
maximum ppm (0,10000], gain 0.001..1000 ppm/frame, smoothing 0..60 seconds (zero
instantaneous) and slew 0.001..1000000 ppm/second. Observations require occupancy
<=capacity and finite elapsed (0,10] seconds, rejecting before state mutation.
Defaults: capacity4096/target2048, maximum2000ppm, gain5ppm/frame, smoothing0.25s,
slew2000ppm/s. Reset restores filtered target/zero correction; processing and
reset allocate nothing, read no clock and perform no device operations.

Synthetic independent-clock queue models settle both drift signs with bounded
arrival/callback jitter, and the correction drives a prepared variable resampler.
These fixtures do not prove stability for all settings/jitter/drop patterns or
physical devices. Corrections cannot fix drift beyond the prepared bound. Native
duplex integration remains separate. See
[decision 0103](decisions/0103-fifo-clock-controller.md).

`SincResampler::validate_variable` checks the same live-adjustable rate/tap/ppm
preparation bounds without allocating tables or modifying a stream. Device
owners can reject invalid configurations before opening hardware; a successful
preflight does not prepare a converter or establish device support.


## Prepared synthetic stereo reverb

`StereoSchroederReverb::new(StereoReverbConfig)` prepares two mono networks with
common excitation `(L/2 + R/2)`. The left uses existing scaled delays; the right
adds sample-rounded `spread_seconds` (0..=0.010) to each of its six delays. Defaults
reuse `ReverbConfig`, spread 0.0005 seconds and wet width 1. Zero spread yields
identical wet branches. This synthetic spread does not guarantee decorrelation.

`width` in 0..=1 interpolates the wet result between the branch average and
separate branches. Dry audio retains each original channel; opposite-polarity
inputs cancel wet excitation. `set_mix(wet, width)` validates both finite 0..=1
values before mutation, preserves histories and adds no smoothing. Both branches
advance even while dry. Whole-frame `process_in_place(&mut [[f64; 2]])`, sample
processing and reset allocate nothing. Reset retains the prepared configuration.

Existing mono rate/room/decay/damping bounds apply. Twelve delay allocations total
at most 133094 f64 samples, excluding struct overhead. Feedback/headroom and
nonfinite/overflow behavior retain the mono policy; no freeze, modulation,
physical-room fidelity, denormal policy or native deadline claim. Tests cover
spread impulse timing, width/cancellation/dry stereo, zero-spread mono equivalence,
partition/reset, decay, configuration/mix rejection and 10000 heap-instrumented
frames. See [decision 0128](decisions/0128-prepared-stereo-reverb.md).


## Prepared periodic wavetable oscillator

`WavetableOscillator::new(rate, frequency, &table)` validates and copies 2..=65536
finite f64 cycle samples on the control thread. Samples represent uniformly spaced
phases i/N; omit a duplicated endpoint. Rates are finite positive and frequency
is finite from zero to strictly below Nyquist. No preset waveform/DC/level policy
is imposed. `table()` borrows the immutable prepared cycle.

Processing emits at current phase, linearly interpolates between neighboring
samples (including final-to-first wrap), then advances. It performs no allocation
or per-sample transcendental work. `set_frequency` preserves phase and rejects
before mutation. Reset retains table/frequency and returns phase zero. Whole/split
blocks match. Prepare, clone and destruction own table allocation outside processing;
maximum owned table storage is 512 KiB, excluding object/allocator overhead.

Interpolation is not bandlimited or alias-free; no smoothing, denormal/headroom,
nonfinite output or native deadline policy is inferred. Extreme finite table
amplitudes can overflow interpolation. Tests check exact periodic segments and
wrap, non-power-of-two cycles, analytic sine error, retuning/reset/partition,
invalid input without allocation and 10000 heap-instrumented samples. See
[decision 0129](decisions/0129-prepared-wavetable-oscillator.md).


## Multiplicative parameter smoothing

`MultiplicativeSmoother::new(initial, ramp_samples)` transitions strictly positive
finite f32 values geometrically. After a changed target, the first sample advances
one of N steps and sample N reaches the exact target. Zero duration is immediate;
one sample reaches the target on the next call. Identical targets retain active
progress. Changed targets restart from the current effective f32 value. Reset
snaps to the latest target while retaining duration. `smoothing_samples` converts
an explicit time/rate to the same u32 sample duration used by linear smoothing.

Zero/negative values reject with `SmoothingError::NonPositiveValue`; nonfinite
values retain `NonFiniteValue`. Invalid construction/retarget rejects before
mutation/allocation. Silence or signed crossings use a caller-selected linear
transition instead. No automatic gain/rate/pitch interpretation is imposed.

Retarget designs the ratio through f64 log difference/exp; each sample uses a
multiply and endpoint clipping. f64 internal progress avoids repeated f32 rounding;
returned f32 values can repeat, particularly for very long ramps, and the final
sample snaps exactly. No smoothing of smoothing duration or native deadline
claim. Construction, retuning, sample processing and reset allocate nothing.
Tests compare geometric landmarks/independent log interpolation, extreme positive
endpoints, monotonicity, tiny ratios rounded to unity, retarget/reset/partition,
invalid updates and heap activity. See
[decision 0131](decisions/0131-multiplicative-smoothing.md).


## Fourth-order Linkwitz–Riley crossover

`LinkwitzRileyCoefficients::design(rate, cutoff)` prepares two identical
second-order Butterworth sections per low/high branch. Rate is finite/positive,
cutoff lies strictly between DC/Nyquist and unrepresentable stability margins
reject. Both branches have linear amplitude 0.5 at cutoff (about -6 dB).
`magnitudes_at` inspects their amplitudes at normalized frequencies 0..=0.5.
This LR4 electrical profile follows the
[Linkwitz filter description](https://www.linkwitzlab.com/filters.htm).

`LinkwitzRileyCrossover` owns four fixed-storage f64 biquads for one input channel
and returns both branches per sample or into caller-owned block buffers. All
three block lengths must match exactly; mismatches preserve both outputs and
histories, and empty blocks are no-ops. Reset clears histories; replacing a
complete validated design retains them without smoothing. The recombined output
has unity magnitude with second-order all-pass phase, so it is not the original
waveform. This supplies no loudspeaker alignment, automatic multi-band routing,
modulation smoothing, nonfinite/denormal policy or deadline guarantee.

Tests compare independent bilinear response equations, impulse recombination
against a separate second-order all-pass, sinusoidal cutoff power, arbitrary
block partitions, exact reset/replacement histories, atomic invalid shapes and
10000 prepared reset/replacement/block workloads without counted heap actions.
See [decision 0139](decisions/0139-linkwitz-riley-crossover.md).


## Prepared stereo chorus

`StereoChorus::new(sample_rate, ChorusControls)` prepares two independent f64
fractional-delay feedback channels and a shared sine LFO with a right phase
offset. Finite sample rate is 8000..=192000 Hz; each buffer has
ceil(rate*0.2)+1 samples, at most 76802 f64 values total (614416 bytes). Complete
controls validate before allocation: rate 0..=100 Hz, center delay 1..=100 ms,
depth 0..=1 relative to center, feedback -0.99..=0.99, linear mix 0..=1, and right
phase offset 0..=1 cycles. Defaults are 1 Hz, 8 ms, depth 0.25, feedback 0.1,
mix 0.5 and right offset 0.25 cycles.

Delay is center*(1+depth*sin(phase)), clamped to at least one sample. Each wet
read precedes input-plus-feedback writing and one cursor advance. Channels do
not crossfeed. Dry/wet endpoints are exact; dry mode still runs wet histories
and LFO. `set_controls` validates the whole edit before mutation and retains
phase, histories and allocations without smoothing. Zero rate freezes the
current phase. Reset clears both histories and left phase while retaining
controls. In-place channel lengths must match; mismatch preserves samples and
history, and empty channels do not advance phase.

This is a linear interpolation profile, without bandlimited delay/pitch
modulation, fixed latency/tail reporting, automatic dry alignment, smoothing or
headroom/denormal/deadline guarantees. Nonfinite audio can poison history until
reset. Rate changes require a newly prepared instance. Tests compare an
independent indexed full-history modulation/feedback recurrence, fractional
impulses, feedback signs, stereo phase, dry-history continuity, partition/reset,
atomic invalid edits/shapes and bounded maximum preparation/10000 heap workloads.
See [decision 0141](decisions/0141-prepared-stereo-chorus.md).

## Explicit integer-rate schedule

`SincResampler::new_integer_rates(input_rate, output_rate, taps)` selects an exact
integer input/output schedule using positive u32 rates and the existing 1/16..16
ratio/even 16..256-tap quality profile. The bounded relative numerator requests
input and produces output without a duration-dependent floating recurrence. The
existing `new` and `new_variable` retain their behavior; no default switches modes.
The new constructor reuses the normalized 1025-phase Blackman coefficient/history
profile. Only bounded fractional table interpolation uses f64; PCM can differ from
the floating schedule at phase boundaries. This is not an exact spectral or native
clock/deadline guarantee. The nominal `rate_step` remains f64; nonzero ppm rejects,
zero ppm retains scheduled phase, and reset restores the exact initial phase and
zero history without allocating. Calls retain the existing bounded prefix, zero
output capacity, pending output drain and caller-owned explicit padding policies.

## Explicit integer history priming

An integer-rate `SincResampler` can compute an immutable
`IntegerResamplingPosition` with `integer_position(output_frame)` and adopt
caller-prepared history with `prime_integer_at(output_frame, history)`. For input
rate I, output rate O and output index J, its exact input coordinate is
C = 1 + ceil(J × I / O). The product and C use u128, including padded coordinates
beyond u64. Priming requires exactly the prepared tap count of finite f64 samples,
oldest to newest, ending at C−1. It restores the existing causal table filter's
phase/history for output J; it does not change coefficients, rates or delay.

Both operations allocate nothing and perform no I/O. Invalid mode, length or
nonfinite history preserves state. Floating and variable profiles reject these
operations. Source-window reads, leading/trailing zeros, finite-file retained
output bounds and source-version validation remain caller responsibilities.
`reset` still restores initial zero history and phase. The position describes a
mathematical schedule, rather than proof that a decoder read its prefix.
## Low-cost streaming reconstruction

`StreamInterpolator` provides independently owned one-channel f64 zero-order hold,
linear, Catmull–Rom, third-degree Lagrange and fourth-degree Lagrange kernels.
All storage is five stack-backed history values plus scalar phase; construction,
clone, processing, additive processing, reset and destruction allocate nothing.
Kernel and rates are immutable; explicitly create/reset another owner to change
profile. Multichannel consumers prepare one owner per channel and coordinate their
source/output partitions. This API does not implement a same-frame Foundation
processor or infer device clock/transport coordinates.

`new` validates finite positive input/output rates in matching units, ratio
1/16..=16. Its floating recurrence has no long-duration exact-count guarantee.
`new_integer_rates` accepts positive u32 rates with the same ratio and uses a
bounded signed relative numerator for exact scheduling independent of duration.
Only kernel evaluation is floating point; no live rate edit/drift mode.

`process` replaces only its produced prefix and reports exact consumed/produced
counts. Retry with unconsumed source and fresh output capacity; consumption is
irreversible. Zero output capacity consumes nothing, while empty input can drain
already pending upsample output. Neither input nor output wraps. Output suffix
remains untouched. `process_add` adds finite gain times produced samples over the
same prefix; whole-call gain validation precedes all state/input/output edits.
Gain zero still advances phase/history but leaves every output bitwise unchanged,
even with nonfinite input. Other arithmetic can propagate nonfinite values or
overflow; reset clears history without changing kind/rates.

For integer output index J, input rate I and output rate O, source bracket
k=ceil(J*I/O), consumed count C=k+1 and backward fraction f=k-J*I/O. History before
source index zero is zero. Hold returns x[floor(J*I/O)], linear returns
(1-f)x[k]+f x[k-1]. **Hold deliberately uses common ceil bracketing and may consume
an unnecessary future sample** before producing an output that uses the earlier
sample. A finite prefix therefore exposes only outputs whose common bracket is
available. Linear's nominal delay is zero; fractional source bracketing can read
up to one source sample ahead of its interpolation coordinate, so nominal delay
is separate from consumer buffering/causal delivery latency.

Catmull–Rom uses four consecutive source points and half-difference Hermite slopes
at coordinate J*I/O-1. `Lagrange3rd` evaluates the unique cubic through four
consecutive points at the same coordinate. Both have one-input-sample nominal
delay. `Lagrange4th` evaluates the quartic through five points at J*I/O-2, a
two-input-sample nominal delay. Delays are explicit and retained even at equal
rates; no hidden copy bypass or leading/trailing trimming.

With N>0 supplied frames, integer profiles expose exactly
1+floor((N-1)*O/I) outputs after pending drain; empty source exposes none. Callers
supply any trailing padding as ordinary input and own finite-file duration,
retained-tail and output-budget policy. No EOS signal or automatic padding/flush
is invented. Different kernels retain different causal tails unless the caller
explicitly supplies input padding.

These are reconstruction kernels **without anti-alias filtering**. Downsampling
aliases, and cubic/quartic reconstruction can overshoot. Use the separate
`SincResampler` when its documented filtering profile is appropriate. Independent
fixtures compare polynomial/basis landmarks, phase boundaries, all kernels over
variable partitions and common/extreme ratios, additive error preservation,
empty/pending/tail policy, bounded long integer recurrence, and whole lifecycle
heap counts. A stereo Foundation-source/actual prepared Graph/FIR composition
fixture checks unique channel histories, finite-prefix zero-output policy,
committed source counts, reset and allocation-free variable blocks. Measured
passband errors and deliberate out-of-band alias evidence establish only those
fixtures, not a universal quality bound, third-party PCM equality, SIMD/denormal
policy, hardware timing or callback deadline. See
[decision 0235](decisions/0235-low-cost-streaming-interpolation.md).
## Selectable causal fractional delay

`FractionalDelay` owns one prepared f64 channel and an immutable `Integer`,
`Linear`, `Lagrange3rd` or `Thiran1st` policy. Construction validates an integer
maximum 0..=4,194,300 samples, then fallibly reserves `maximum+4` history values,
at most 32MiB. Zero maximum is legal. Extra slots supply cubic history and add no
hidden delay. Creation/destruction are control work; sample/block processing,
validated delay edits and reset perform no heap allocation/deallocation, locks or
I/O. Reset visits the full prepared ring, so its work depends on capacity.

Requested delay starts at zero. `set_delay_samples` accepts finite 0..=maximum,
retaining all history and applying the edit immediately without smoothing or
crossfade. Invalid edits preserve requested control, cursor and every history.
One input produces exactly one output and one advancement; a stateful allpass
cannot be read repeatedly as independent taps. Existing `DelayLine` separate
read/write/advance and chorus/comb/reverb behavior remain unchanged.

For d=n+f, n=floor(d), `Integer` returns x[t-n], truncating the fractional part.
`Linear` returns (1-f)x[t-n]+f*x[t-n-1]. `Lagrange3rd` evaluates the unique cubic
through four causal points x[t-n-j], j=0..3, at fractional coordinate f on nodes
0..3. This deliberately uses one-sided support near current input; it imposes no
extra one-sample minimum or centered-kernel delay. Initial negative-index history
is zero. FIR interpolation is not bandlimited, and cubic reconstruction can
overshoot; neither normalization nor a clamp is applied.

For `Thiran1st`, v=x[t-n], a=(1-f)/(1+f), and
`y=a*v+previous_v-a*previous_y`. At a fixed noninteger delay, the transfer is
z^-n*(a+z^-1)/(1+a*z^-1). It has unity magnitude and DC group delay n+f, with
frequency-dependent group delay. This is not an ideal broadband fractional delay.
Small fractional delay places a pole near -1 and leaves an arbitrarily long
alternating tail. Positive fractions whose f64 coefficient rounds to one reject
before controls change; no hidden stability clamp substitutes a different delay.
Exactly integer delay bypasses recursion and updates both prior values to the
current integer tap. Retuning across integer boundaries changes the tap feeding
retained recursive history; static allpass facts do not describe modulation or
feedback transients.

`process_sample` writes current input before reconstruction and advancement, so
zero delay returns it exactly. `process_in_place` preserves temporal order.
`process_separate` requires exact equal lengths, rejecting mismatches before any
state/output edit; two empty slices do nothing. Nonfinite input can propagate and
poison history until reset. Consumers own channel synchronization, feedback,
headroom/denormals, sample-rate/millisecond conversion, tail/drain and host latency.

Independent fixtures cover absolute tap history, cubic polynomials/basis/overshoot,
fixed frequency transfer, allpass DC group delay and explicit impulse-tail
geometric remainder, integer-edge modulation, rounded-stability rejection,
state/output preservation, wrap/reset/partitions, maximum allocation/capacity and
callback heap counts. An actual stereo Foundation processor through prepared
Graph checks distinct channel delay histories, variable blocks, rejected outer
preflight, reset/repeat and allocation-free callbacks. Offline evidence does not
establish device/DAW/native timing/deadline or universal modulation stability.
See [decision 0237](decisions/0237-selectable-causal-fractional-delay.md).

## Selected-order Butterworth designs

`ButterworthDesign` prepares fixed f64 sections for low/high-pass digital orders
1..32 and bandpass/bandstop **total digital** even orders2..32. Band prototype
order is half that total. `ButterworthResponse` supplies Hz cutoffs strictly inside
Nyquist, with ordered band edges. Rate must be finite and positive. Construction
is allocation-free CONTROL work; unrepresentable gains, poles, intermediate
arithmetic or strict Jury stability margins return `FilterError` rather than
clamping. At most16 sections are stored; odd low/high order includes a first-order
section with zero second-order terms. Existing even const-generic cascade APIs and
their history-preserving replacement behavior remain unchanged.

Each frequency is bilinear-prewarped separately. Sections are normalized at DC
for lowpass/bandstop, Nyquist for highpass, and the **warped** center for bandpass:
`atan(sqrt(tan(pi*low/rate)*tan(pi*high/rate)))/pi` cycles per sample. This center
need not equal the arithmetic or geometric Hz center. Band edges and low/high
cutoffs have magnitude1/sqrt2. `magnitude_at` inspects linear amplitude on CONTROL
at a finite normalized frequency0..=0.5.

`lowpass_spec` and `highpass_spec` accept explicit pass/stop edges and finite
`0 < pass_loss_db < stop_loss_db <= 300`. Lowpass has pass<stop, highpass stop<pass.
The selector starts at the analytic minimum order, chooses the geometric midpoint
of its admissible warped-cutoff interval, and checks the actual rounded SOS
amplitudes against **both strict requested bounds**. An unrepresentable/failed
midpoint can select a higher order through32; it cannot relax loss/attenuation or
exceed32. `order()` and `response()` report the actual selected order/cutoff.
This verified midpoint policy is not a search for the globally smallest order
among every representable f64 cutoff. Tiny/unrepresentable constraints and orders
requiring more than32 reject.

`ButterworthFilter` is an independently owned one-channel fixed array. Sample,
block and reset processing perform no allocation, locks or I/O. A new owner starts
with zero histories; explicit `set_design` resets all histories even when the
order stays the same. This prevents unrelated poles inheriting old section state;
there is no smoothing or crossfade. Use a separate owner per channel. Nonfinite
audio propagates; headroom, denormals, resets and safe parameter transitions remain
consumer policies. IIR tails are not a declared finite flush duration. Numeric,
heap and actual Foundation/Graph fixtures establish the tested profiles, without
physical/host/platform or native callback deadline claims.

## FIR coefficient design

`FirDesign` prepares immutable symmetric f64 coefficients on CONTROL, with
fallible reservation and at most65,536 taps (512KiB). Borrow `coefficients()` for
a separately prepared `FirConvolver` or `PartitionedConvolver`. `order()` and
`tail_samples()` are taps minus one; `group_delay_samples()` is exactly half that
order, including Type II half-sample delay. This coefficient delay differs from
consumer algorithmic block latency; no automatic host latency/tail registration
is supplied. `magnitude_at` inspects actual linear amplitude at normalized0..=0.5
on CONTROL. There is no new convolution runtime, native deadline or transition
crossfade. Drop/allocation/design remain CONTROL work.

`windowed` accepts LP/HP cutoff or ordered BP/BS edges strictly inside Nyquist,
finite positive rate and `FirWindow::Cosine(WindowKind)` or Kaiser beta0..32.
Existing window enums/plans remain unchanged. LP/BP tap counts2..65536 may be odd
Type I or even Type II; HP/BS requires odd3..65535 because even symmetric filters
have Nyquist zero. Center the ideal band Fourier integral, multiply by a symmetric
window and mirror exactly. Normalize actual LP/BS DC, HP Nyquist or BP arithmetic
Hz-center gain; zero/unrepresentable reference rejects. FIR ideal cutoffs are
transition centers near half amplitude, distinct from IIR half-power cutoffs;
truncation/windowing/normalization can move actual finite-response landmarks.

`kaiser_lowpass` chooses an empirical order/beta from ordered pass/stop edges and
attenuation8..120dB, promotes to even order/odd length (at least3), and uses the
midpoint cutoff. `KaiserFirDesign` privately links the response to inspectable
`beta()`/`estimated_order()`; `design()` borrows it and `into_design()` moves it.
These empirical equations **do not enforce a continuous-band specification**.
A measured65dB example misses its stopband request at the first sidelobe. Inspect
actual response for consumer requirements; there is no hidden retry, stronger
attenuation, bound certification or assumed minimum order.

`transition_lowpass` shapes an infinite lowpass target with K=1..4 unit-area
frequency boxes, each total_width/K. Their convolved density is degree K-1 and
integrated edge transition degree K across the stated full width. Infinite
centered taps are `2fc*sinc(2fc*t)*sinc((width/K)*t)^K` with frequencies in cycles
per sample and `sinc(x)=sin(pi*x)/(pi*x)`. Positive total transition width must lie
strictly inside DC/Nyquist. Truncate to2..65536 taps, mirror and normalize actual
DC. Analytic target shaping and finite truncation are distinct; no finite-response
pass/stop bound is implied by the specified width.

`least_squares` supplies general Type I odd3..511-tap weighted integrated design:
1..32 sorted, nonoverlapping positive-width `FirBand`s inside closed DC..Nyquist,
linear desired gain endpoints[-16,16], finite weight(0,1e6]. Gaps are unconstrained
and may overshoot. Normalize weights by their maximum and form exact analytic
cosine normal-equation integrals. CONTROL Cholesky work is O(V³+bands*V²), V<=256,
with matrix<=512KiB plus small vectors/output; no hidden ridge, pseudoinverse or
order increase. Underflowed positive widths/weights reject. A pivot at or below
`64*f64::EPSILON*V*largest_Gram_diagonal` rejects; normalized infinity residual
must not exceed `1024*f64::EPSILON*V`. These documented f64 representability checks
are not exact-arithmetic/minimax certification. Preserve the solved gain; post-
normalization would change the weighted optimum. Allocation failures and invalid
specifications return no partially generated response or borrowed-band edits.

`halfband_windowed` accepts lengths4m+3<=65535 and a cosine/Kaiser window. It fixes
the center tap at0.5, noncenter even offsets at exact zero, and scales only paired
odd offsets for DC unity/Nyquist zero, preserving centered complementary response
within floating rounding. This is a windowed halfband profile, without an
equiripple/minimax claim or implicit polyphase/resampling setup. Extreme zero/
unrepresentable normalization rejects. Named equiripple and certified continuous-
band specification designers remain separate capabilities.

Independent band integrals/Bessel integral/transition quadrature/least-squares
stationarity and objective fixtures qualify the tested numerical profiles. Actual
stereo Foundation/Graph/direct FIR and partitioned latency/tail-drain fixtures
check generated-tap composition and heap-free callback processing. Consumers own
headroom, nonfinite audio, denormals and channel state; these offline fixtures do
not establish physical device/DAW/platform or real-time deadline support.

## Selected-order Chebyshev I and II

`ChebyshevDesign` prepares fixed f64 sections for Type I pass ripple and Type II
stop attenuation, LP/HP digital orders1..32 and band total even orders2..32. Band
prototype degree is half the total order. `ChebyshevKind` takes finite positive
Type I ripple<=120dB or Type II attenuation<=300dB; `ChebyshevResponse` supplies Hz
edge(s) strictly inside Nyquist, ordered for bands, with finite positive rate.
Edges have family-specific meaning: Type I passband edges at its ripple floor,
Type II stopband edges at its attenuation ceiling; neither is a -3dB cutoff.
CONTROL design allocates nothing. Strict finite arithmetic/gains/poles/Jury margins
reject unrepresentability; no frequency, stability, gain or ripple repairs.

Type I preserves the prototype's even-degree DC floor `10^(-ripple_db/20)`, with
pass peaks1; odd degree has DC1. Highpass Nyquist and band pass-reference gains
inherit prototype parity. Renormalizing even-degree DC to1 would raise its ripple
peaks above1, so this implementation retains the specified ripple convention.
Type II has prototype DC1 and true finite stopband notches; even degree has a
nonzero far-stop limit `10^(-attenuation_db/20)`, odd degree has zero. Consequently
Type II LP Nyquist/HP DC and transformed band endpoints must not be assumed to
have Butterworth endpoint zeros. Finite imaginary-axis prototype zeros become
unit-circle digital zeros through the same frequency transforms as the poles.

`kind()`, `response()`, `order()` and `stages()` expose the actual design.
`magnitude_at` inspects actual rounded sections on CONTROL at normalized0..=0.5.
Band center uses separately warped edges, not arithmetic/geometric Hz center;
Type II band edges delimit the mapped stop region. Deterministic real sections
include exact odd real prototype and its transformed two-real/conjugate cases.

`lowpass_spec`/`highpass_spec` select `ChebyshevType::{Type1,Type2}` with supplied
pass/stop edges, `0<Rp<As<=300dB` and `Rp<=120dB`. Type I derives its ripple fromRp,
Type II attenuation fromAs. Start at the analytic minimum order and choose the
geometric midpoint of that family's admissible warped-edge interval. Verify
both actual rounded supplied-edge bounds, pass>=10^(-Rp/20) and stop<=10^(-As/20),
without tolerance relaxation; failed/unrepresentable candidates retry through32
only. Returned kind/edge/order report the actual selection. Nominal analytic
ripple shape and strict supplied-edge verification are explicit; this does not
certify every rounded continuous-interval extremum or search all f64 edges for a
global minimum order. Impossible/bounded/unrepresentable specifications reject.

`ChebyshevFilter` owns one channel of at most16 fixed states. Sample/block/reset
perform no allocation, locks or I/O; `set_design` resets all histories, with no
smoothing/crossfade. Existing Butterworth/cascade and other filter defaults remain
unchanged. Use an independent owner per channel; nonfinite audio propagates and
headroom/denormals/transitions/reset remain consumer policy. IIR tails do not have
an invented finite drain completion. Independent polynomial/closed-form/time/
notch/spec/partition/heap and actual stereo Foundation/Graph fixtures qualify the
stated numeric profiles, without physical/DAW/platform/deadline or other-family
support claims. Elliptic and configured halfband designers remain separate.

## Selected-order elliptic filters

`EllipticDesign` prepares fixed f64 sections with ripple in both pass and stop
bands and genuine finite stopband zeros. LP/HP digital orders are1..32; band total
orders are even2..32, twice the prototype degree. `EllipticResponse` supplies Hz
ripple pass edges strictly inside Nyquist, ordered for bands, with finite positive
sample rate. These edges are distinct from -3dB cutoffs. Ripple/attenuation satisfy
finite `0<ripple_db<attenuation_db<=300`, with ripple<=120dB. CONTROL design has no
heap allocation or dependencies. It rejects unrepresentable margins, arithmetic,
moduli, inverse residuals, gains or strictly stable Jury coefficients without
parameter repairs. Not every extreme combination within the input range is
numerically representable in this bounded f64 profile.

Prototype odd degree has DC1; even degree has DC floor `10^(-ripple_db/20)` and
unity pass peaks. Corresponding highpass and warped band reference gains preserve
that parity. Finite imaginary-axis prototype zeros transform into unit-circle
stopband notches. Far-stop gain is zero for odd prototype degree and
`10^(-attenuation_db/20)` for even degree, so endpoints must not be assumed to
have Butterworth zeros. `response()`, `order()`, `ripple_db()`, `attenuation_db()`
and `stages()` report the design. `prototype_stop_ratio()` exposes the derived
nominal prototype stop/pass frequency ratio1/k; `magnitude_at` inspects actual
rounded sections on CONTROL at normalized0..=0.5. Band center uses separately
warped edges, rather than an arithmetic Hz center.

The implementation retains explicit complementary moduli, bounded32-step AGM and
descending Jacobi amplitude, and bounded128-step degree/inverse bisections. Degree
and inverse relative residuals must be<=1e-12. The degree solve searches actual
representable moduli from normal `MIN_POSITIVE` to the largest f64 below1; its
lower bound is conservative for the accepted positive ripple/attenuation range.
Near-one discrete moduli and unresolved extreme inverse values can reject. The
exact first-order shape uses its closed real pole, independent of nome underflow;
no alternate family is substituted. The mathematical basis is [NIST Jacobi AGM](https://dlmf.nist.gov/22.20)
and [Jacobi identities](https://dlmf.nist.gov/22.6).

`lowpass_spec`/`highpass_spec` take finite supplied pass/stop edges and the same
ripple/attenuation range. Start at the complete-integral analytic minimum order;
each candidate chooses the geometric midpoint of its admissible warped pass-edge
interval and verifies actual rounded supplied-edge amplitudes strictly against
pass>=10^(-ripple/20) and stop<=10^(-attenuation/20). Failed or unrepresentable
candidates retry through32 only; impossible bounded specifications reject. Actual
edge/order/stop ratio expose the selection. Nominal ripple and supplied-edge
verification do not certify all continuous postround extrema or promise a global
minimum over every representable f64 edge.

`EllipticFilter` owns one channel of at most16 fixed states. Sample/block/reset
have no allocation, lock or I/O; `set_design` replaces the whole design and clears
all history without smoothing. Use an independent owner per channel. Existing
filter APIs and defaults are unchanged; nonfinite input propagates, and headroom,
denormals and transitions remain consumer policy. IIR processing has no invented
finite drain completion. Independent nome/theta/rational, low-order polynomial
and time, finite-zero/parity/spec/partition/heap and actual prepared stereo Graph
fixtures qualify measured numeric profiles. A finite tiny impulse tail window is
an observation for those fixtures, not a proof about the unobserved infinite tail
or a physical-device, DAW, platform or real-time deadline claim.


## Sampled equiripple FIR design

`EquirippleFirDesign` prepares actual weighted minimax amplitude coefficients with
bounded sampled exchange. LP/HP/BP/BS have explicit Hz pass/stop transition edges,
finite positive rate and positive finite pass/stop weights<=1e6. Odd3..511 taps
use symmetric TypeI; even2..512 use symmetric TypeII, whose Nyquist zero makes
nonzero-Nyquist HP/BS invalid. No hidden tap promotion or reference-gain
normalization occurs. `from_bands` reuses `FirBand`:1..32 strictly separated sorted
positive-width closed bands0..Nyquist, finite signed desired endpoints[-16,16]
interpolated linearly inHz, finite weights(0,1e6]. Unconstrained transition gaps
remain the caller's choice. Rate must have an exactly representable positive
Nyquist: rate/2 normalized by rate equals .5 and doubling restores rate. Normalized
endpoints must remain finite in0.. .5 with original positive widths/gaps and
DC/Nyquist identities preserved. Rounded unavailable boundaries reject before
allocation; no endpoint is nudged or silently treated as TypeII Nyquist.
This implements an actual exchange engine; it does
not turn integrated least squares into a minimax claim. The standard numerical
family is described by the [Remez API](https://docs.scipy.org/doc/scipy/reference/generated/scipy.signal.remez.html).

`EquirippleOptions` defaults to density16/max64 exchanges/relative gap1e-8; accepted
bounds are density8..64, max iterations1..128 and finite tolerance1e-12..1e-3.
Each band includes actual endpoints and uniform interior points with maximum
normalized step1/[2*(taps+1)*density]. Checked counts cap observed gridpoints32768;
fallibly allocated numeric buffers and bounded support arrays together cap4MiB
of aggregate declared capacity, excluding allocator overhead. Declared capacities
are checked before any reserve, then actual allocator capacities are checked again. Scratch is reused
through bounded exchanges and dropped on CONTROL. Some high-density, narrow,
ill-conditioned or unresolved profiles fail even with otherwise valid inputs.
TypeII desiredNyquist0 is an observed structurally forced endpoint excluded from
interpolation support; `forced_nyquist()` reports it explicitly.

Weighted alternation uses degree-limited real polynomial interpolation. Finite
log-scaled barycentric weights, sampled maximin extremum selection and final
cosine-transform coefficient recovery have explicit representability checks.
Unchanged support alone is not convergence: the full grid and actual rounded
coefficients must satisfy the reported ripple gap and supported alternation.
Stalls/exhausted exchanges return `EquirippleError::NonConvergence{iterations}`;
other specification/capacity/allocation/numerical failures preserve `FirDesignError`
inside the separate error type. No partial filter is returned as completed.

Immutable result accessors expose actual `FirDesign`, iterations/grid density/
point count, normalized griderror/fitted ripple/numeric allowance, original
`weight_scale` and actual alternating extrema. `weighted_grid_error` and each
extremum's weighted error restore original units; tiny f64 products may underflow,
so normalized errors and scale preserve their interpretation and sign. Global
weight scaling leaves the solve unchanged. The normalized allowance is
128eps*(variables+1)*max(1,max|desired|,sum|coefficients|); allowance above1e-6 times
max(1,max|desired|) rejects. Original extrema also expose normalized signed error.
These are numerical sampled observations, distinct from certified continuous
interval/global-f64 minimax bounds or automatic tap/attenuation sizing. Inspect
actual coefficients and independently measured spectrum for application needs.

`halfband` uses N4m+3 taps3..511 and full normalized transition width0..0.5cycles,
with pass=.25-width/2 and reflected stop=.25+width/2; rounded missing gaps reject.
Its reduced basis is A(omega)=.5+cos(omega)*P(cos(2omega)). Coefficients have exact
central.5, noncentral even-distance0 and symmetry, giving complement identity
A(pi-omega)=1-A(omega) and quartercycle.5. Equal reflected pass/stop weights and
free-basis alternating extrema qualify the actual constrained design. Lowband
support and reflected stop extrema are exposed separately. Final rounded-coefficient
maximum evaluates every actual low and rounded reflected stop grid frequency.
Reflected support errors are recomputed at their actual rounded frequencies rather
than inferred by sign reversal; CONTROL iteration allocates nothing. DC ripple is retained;
normalizingDC to1 would change minimax/complement constraints. This uses reduced
exchange, rather than projecting a generic filter or applying a window.

Prepare existing `FirConvolver` or `PartitionedConvolver` with `design().coefficients()`
on CONTROL; `into_design` transfers the coefficients. The processors retain their
existing latency, full finite-tail, reset and heap-free callback contracts. Group
delay remains(N-1)/2, including half-sample TypeII. No dependencies/defaults or
existing windowed/Kaiser/LS factories change. Independent LP-vertex, polynomial
primal/dual, halfband closed form, signed/weight/error/capacity and actual prepared
stereo Graph/direct/partitioned fixtures qualify this profile without physical
platform, DAW, acoustic, SIMD or real-time deadline claims.

A reusable headless independent check needs Python's standard library only:

```sh
cargo run -p ravet-signal --example equiripple_evidence --locked > equiripple-evidence.json
python3 crates/ravet-signal/tests/equiripple_decimal_oracle.py equiripple-evidence.json
```

It solves independent85-digit Decimal polynomial dual systems and evaluates
actual generated coefficients on every declared grid point for all four medium
families, TypeII, halfband and nonconstant511/512 profiles. Reported primal/dual
and rounding observations are numerical evidence, rather than a formal continuous
certificate. Differentiator/Hilbert odd symmetry and certified continuous sizing
remain independent capabilities.


## Configured IIR halfband and allpass oversampling

`HalfBandIirDesign::new(order, width)` prepares an immutable elliptic halfband
profile with odd order 1..63 and full normalized transition width in (0, .5).
Frequencies refer to the high sample rate: pass = .25 - width/2 and stop = .25 +
width/2, in cycles/sample. Missing rounded edges, unresolved moduli, nonfinite
arithmetic, duplicate or unstable coefficients, or derived nominal attenuation
above 300 dB reject. Fixed arrays contain at most 31 coefficients; bounded CONTROL
design and runtime construction do not allocate or add dependencies.

The low-rate branches are A_b(q) = product((a + q^-1)/(1 + a*q^-1)). Full-rate
lowpass is .5*(A0(z²) + z^-1*A1(z²)); highpass changes the plus to minus. Sorted
squared imaginary pole radii alternate between the branches, whose counts can
differ by one. `direct_coefficients` and `delayed_coefficients` expose the actual
immutable values. Coefficients come from bounded AGM/Jacobi evaluation of a
rationalized pole-radius expression; no rounded SOS pole projection or gain repair
is used. The mathematical family is described in the author-uploaded
[halfband paper](https://www.researchgate.net/publication/228946539_Half-band_IIR_filter_design_using_MATLAB),
with special Jacobi values in [NIST 22.5](https://dlmf.nist.gov/22.5#T2).

`from_attenuation` accepts finite requested stop attenuation above 10log10(2) and
at most 300 dB. It starts at the analytic odd-order minimum, retries candidates
through 63 and strictly admits their actual rounded stop-edge amplitude against
10^(-requested/20). Unrepresentable candidates or unmet edges reject without
relaxing the request. Nominal coupled prototype ripple/attenuation and actual
pass/stop edge amplitudes remain separate accessors. `sampled_stopband_peak`
reports a maximum and its frequency measured from the actual rounded coefficients
on 4097 uniformly spaced closed stopband points. This observation is separate from
the edge sizing criterion and a continuous whole-stopband certificate.

`quantize_f32` explicitly constructs `HalfBandIirDesign32`, rejecting coefficients
rounded to 0/1, reordered or duplicated; it re-measures the actual rounded spectrum.
Its nominal metadata describes the originating f64 prototype. Its own
`from_attenuation` separately retries and qualifies actual f32-coefficient edges.
No f64 attenuation guarantee silently transfers to f32. The maximum-order fixture
at width .001 demonstrates the distinction: nominal 182.835031 dB but actual
rounded f64 stop edge 182.834619 dB. A request 0.000001 dB below the nominal edge
is refused, while one 0.000001 dB below the observed edge qualifies. This is a
measured local numerical fixture.

`HalfBandIirFilter` and its 32 variant process full-rate samples into simultaneous
[low, high] with one history advancement. LP/HP powers sum to 1 and reflected
lowpass powers complement; DC/Nyquist are the corresponding unity/zero endpoints.
LP + HP is the direct allpass path, which has frequency-dependent phase. Processing
and reset use fixed storage; `replace_design` resets all histories and provides
no smoothing. Separate/block lowpass processing preflights lengths and preserves
state/output on mismatch; empty blocks advance nothing.

`ConfiguredHalfBand2x` and its 32 variant own independent interpolation and
decimation profiles/histories. `upsample` returns even/odd samples in time order,
with unity DC sample gain (gain 2 relative to a zero-stuffed high-rate lowpass).
`downsample` retains the even phase and previous odd allpass branch. `reset` clears
both paths. Order 1 has no allpass sections and exactly implements the two-sample
full-rate average/difference; both unequal-branch order classes are qualified.
No implicit source/tail trimming or phase realignment occurs.

`ConfiguredOversampler` and its 32 variant provide 1/2/4/8 cascades. `new` repeats
one up/down pair; `from_stages` accepts three independent pairs, using only the
active factor stages. Stack scratch has eight samples. Child DSP executes exactly
factor times in time order; factor One passes through the filters. Child processing
must be prepared at host rate times factor and meet its callback obligations.
Sample/block/reset processing allocates, locks and performs I/O nowhere in the
framework kernels. Caller-sized blocks still bound total work. Existing fixed
`HalfBand2x`/`Oversampler` defaults retain their profiles and behavior.

These IIR paths have infinite recursive tails and frequency-dependent delay.
No constant integer host latency, aligned dry path, live design crossfade,
nonfinite-audio sanitization, denormal handling, headroom management or callback
deadline is supplied. Independent complete-integral series/theta poles/rational
spectrum, polynomial time/cascades, actual imaging/alias tones, precision/error/heap
and canonical native f32/f64 stereo Foundation/Graph fixtures qualify the bounded
profile. Offline fixtures do not establish physical device, DAW or platform support.


## Native peak and RMS attack/release ballistics

`BallisticsConfig::new(rate, attack_seconds, release_seconds, mode)` prepares
explicit `BallisticsMode::Peak` or `Rms` timing. Rate is finite 1..768000 Hz and
times are 0..60 seconds. Zero time is instantaneous; positive times whose prepared
alpha or actual complement rounds to 0/1 reject. RMS also checks its rounded
zero-input amplitude release factor. Alpha is computed stably as
-expm1(-1/(rate*time)). There is no minimum-time snap or implicit default mode.
`quantize_f32` explicitly creates independently validated `BallisticsConfig32`;
rate/time metadata remains the requested physical parameters, with actual native
alpha/complement accessors. Existing `EnvelopeFollower` and dynamics defaults
retain their behavior.

`BallisticsFilter` and its 32 variant hold one native amplitude history. Target
is abs(input); a rising target chooses attack, otherwise release. Peak uses
r + alpha*(target-r). RMS uses scaled energy: hi = max(r,target),
lo = min(r,target)/hi and v = lo²; for a rising target it returns
hi*sqrt(v + alpha*(1-v)), otherwise hi*sqrt(1 + alpha*(v-1)). Zero/equal amplitudes
have exact explicit branches and instantaneous alpha assigns the target. The
scaled equation avoids squaring full magnitudes or separately rounded square-root
weight mass; it supports meaningful near-maximum and near-minimum native fixtures.
Ratio-square underflow, recursive rounding and eventual release underflow remain
visible. No coefficient clamp or headroom repair occurs.

In ideal arithmetic peak time constants apply to amplitude (63.2% step settling);
RMS constants apply to energy, so its amplitude step at one time constant is
sqrt(1-exp(-1)). Equal attack/release RMS is exponentially weighted energy;
unequal timing switches the energy follower, and neither is finite-window RMS.
Actual native arithmetic differs from the ideal recurrence. In the measured
48 kHz/.03-second f32 release fixture, sqrt(actual complement) is
0.9996528029441833 versus 0.9996528380567283 from the ideal actual-alpha equation;
the 2048-zero-sample tail differs by up to 2.04e-5 amplitude while the preceding
signal's maximum discrepancy is 8.48e-7. At 768 kHz/60 seconds f32 complement
rounds to 1 and quantization rejects. A 384 kHz/60-second f32 profile admits but
its 200000-step release rounding is observably different from ideal timing. These
are local measured observations, not ideal settling promises at all admitted
precision extremes.

`reset` clears history; `reset_to` accepts a finite nonnegative amplitude in either
mode and preserves history on failure. `replace_config` resets history without
smoothing. Separate blocks preflight equal lengths and preserve output/history
on mismatch; in-place blocks have the same scalar order. Empty blocks advance
nothing. `snap_to_zero` is caller-explicit and clears only subnormal amplitude
states, preserving normal states and poisoned histories. No block implicitly snaps,
so variable partitions retain identical native output and history.

`BallisticsBank` and its 32 variant prepare 1..64 independent channels and maximum
blocks of 1..65536 frames in fixed storage. Shared immutable timing does not link
channel histories. Per-channel sample/level access rejects unavailable channels.
Planar separate and in-place calls preflight every channel count, active prefix
and frame maximum before any history/output change; backing slice suffixes remain
untouched. Empty prefixes consume nothing. Reset/config replacement affects all
channels together. Construction, process, reset and snapping allocate nothing and
perform no framework locks or I/O; caller block sizes bound work.

NaN input or history stores/returns NaN until reset. Otherwise infinite input or
history stores/returns positive infinity until reset, even with instantaneous
timing. These explicit propagation rules prevent min/max or ratio branches from
silently recovering poisoned state. There is no numeric input scan or finite-output
sanitization. RMS performs native square root per sample; no SIMD, denormal mode,
callback deadline, true-peak, perceptual loudness, channel linking, lookahead,
hysteresis or window/EOF completion policy is supplied.

Independent amplitude/energy closed forms, direct weighted-square convolution,
normalized wide-magnitude energy, actual native repeated release and measured
rounding bounds qualify both precisions. All 1..64 channel counts, prefix failures,
partition/reset/config/poison and heap fixtures plus actual canonical native
stereo Foundation/Graph gain consumers are tested. Offline evidence does not
establish physical device, platform, acoustic or DAW support.


## First-order topology-preserving LP/HP/allpass

`FirstOrderTptDesign::new(rate, cutoff_hz)` prepares a first-order trapezoidal
profile on CONTROL. Rate is finite 1..768000 Hz; cutoff is strictly positive and
below actual Nyquist. It checks the original Hz relation, normalized ratio in
(0, .5), finite positive tan prewarp and representable gain/complement/stable pole.
Collapsed ratios or rounded endpoints reject without nudging. `quantize_f32`
explicitly validates the rounded native f32 profile again. Requested rate/cutoff
metadata and actual `gain` are separate; `magnitude_at` observes the actual
coefficient's ideal LTI spectrum for normalized frequencies 0.. .5. This is not
an all-input floating recursive error bound or transferred f64 spectral guarantee.
The observation uses stable half-angle components G*cos(pi*f) and
(1-G)*sin(pi*f), avoiding cancellation of 1-p near admitted endpoints.
Existing biquad and second-order state-variable defaults remain unchanged.

The coefficient is g = tan(pi*cutoff/rate), G = g/(1+g). Solve the first-order
feedback equation y = g*(x-y) + s around a trapezoidal integrator: v = G*(x-s),
low = s+v, capacitor next = low+v, high = x-low, all = low-high. This circuit is
covered mathematically in [Zavalishin's filter book](https://www.discodsp.net/VAFilterDesign_2.1.2.pdf),
sections 3.6–3.9. The runtime owns the signed capacitor state, which is distinct
from the previous lowpass output.

`FirstOrderTptFilter` and its 32 variant advance once and return named simultaneous
`FirstOrderTptOutputs` /32 for LP, HP and allpass. LP+HP reconstructs input in ideal
arithmetic; native summation can round. The fixed allpass has unit ideal magnitude
and frequency-dependent phase. With p = 1-2G and q = z^-1, LP is
G*(1+q)/(1-p*q), HP is (1-G)*(1-q)/(1-p*q) and allpass is (-p+q)/(1-p*q).
Requested quarter-rate cutoff has ideal half gain; the local native f64 tan/prewarp
fixture produces G = 0.49999999999999994, while explicit f32 quantization produces
0.5 and the exact dyadic average/difference/one-sample-allpass fixture. Actual
coefficients are retained rather than repaired to ideal landmarks.

`set_design` preserves the signed capacitor state; `replace_design` resets it.
Callers can preprepare coefficient schedules and select profiles in the callback
without trig or allocation. There is no implicit smoothing, crossfade, inaudibility
or arbitrary modulation stability guarantee. Selected-response separate blocks
preflight equal lengths before output/history edits, in-place blocks retain scalar
order, and empty blocks advance nothing. Response selection does not reset history.
`reset_to` accepts a finite signed capacitor state, with invalid initialization
preserving the original state. `snap_to_zero` is caller-only and clears strictly
subnormal signed state, preserving normal/nonfinite history; blocks never snap.

`FirstOrderTptBank` and its 32 variant own 1..64 independent channel states with
maximum blocks 1..65536 and a shared immutable profile. Per-channel processing
returns all three responses from one update. Planar separate/in-place processing
selects a response and preflights every channel count, active prefix and frame
bound before any output/state edit. Backing suffixes remain untouched; empty
prefixes consume nothing. Set preserves all channel states, reset/replace clears
all, and validated reset-to initializes all together. Construction, runtime and
reset use fixed storage without framework heap allocation, locks or I/O. Caller
block sizes still bound work.

Nonfinite input and large finite arithmetic overflow propagate and can poison
history until reset. There is no numeric scan, finite-output sanitizer, headroom
repair, processor-wide denormal mode, SIMD or callback deadline claim. The ideal
recursive tail is infinite; actual product rounding may retain subnormal state
until caller snapping. There is no finite EOF flush, constant integer latency,
aligned dry path or alias-free modulation promise.

Independent analog/bilinear spectrum, polynomial impulse/time and signed initial/
sample-varying circuit equations qualify both precisions and responses. All channel
counts, prefix/partition/suffix/state/error/poison/snap and heap fixtures are tested.
Actual canonical native stereo Foundation/Graph consumers use distinct rapidly
changing profiles and an existing f64 FIR downstream; the f32 fixture's FIR
conversion is caller-explicit, with its Foundation buffer and TPT core staying
native f32. Offline composition is not physical device, DAW, platform or acoustic
support evidence.

### Prepared panning curves and pair ramps

`PanningGains` prepares seven explicit curves for finite pan -1..1:
`Linear`, `Balanced`, `Sine`, `SineThreeHalves`, `SineSquared`,
`SquareRoot`, and `ThreeQuarterPower`. With u=(pan+1)/2, linear uses
[1-u,u], balanced uses [min(1,2(1-u)),min(1,2u)], sine uses
[cos(πu/2),sin(πu/2)], the two sine variants raise these gains to 1.5
or 2, and the last two raise [1-u,u] to .5 or .75. These are the actual
mathematical exponents; center attenuation is approximately 3.0103,
4.51545 or 6.0206 dB, rather than an exact rounded decimal label.

The caller selects `PanningNormalization::UnityEndpoints` or
`UnityCenter`. The latter scales by the rounded reciprocal of each law's center gain so both
center coefficients are one; endpoints can then be amplified by √2,
2^.75 or 2. Balanced needs no boost. Unity endpoints sine and
square-root have constant ideal summed power. Other laws, center
normalization, coherent fold-down and arbitrary correlated stereo
material do not share a universal loudness rule. Endpoint/center cases
are explicit, and mirrored pans swap identical actual gains. Private
immutable metadata exposes the requested pan/law/normalization and
actual coefficient pair. `quantize_f32` explicitly rounds coefficients
into `PanningGains32`; actual f32 power can differ from the ideal curve.

`StereoPanner` / `StereoPanner32` require prepared gains, an explicit
ramp duration 0..u32::MAX frames and maximum block 1..65536. Stereo
processing balances the existing two channels independently, without
crossfeed. The mono method explicitly duplicates one source into two
outputs. Both gains advance exactly once per audio frame; scalar,
separate prefix and stereo in-place prefix methods share this progress.
Every prefix checks channel counts, every active length and maximum
before gains or output change; inactive suffixes stay untouched and
empty prefixes advance nothing. Scalar calls do not impose a block
length. Construction, design, control and processing allocate nothing
and use no framework locks or I/O. Trigonometry/powers occur only in
prepared CONTROL design.

After a changed target, the first frame interpolates 1/N from current
actual gains and frame N assigns the exact target. Zero duration is
immediate. Repeating identical target coefficients retains progress,
including a different curve/normalization that has the same actual
pair; target metadata still updates. Mid-ramp retargeting starts from
current rounded gains. Reset snaps to the latest target. Both native
variants interpolate coefficients in f64; coefficient rounding and
sample multiplication use the named precision. Intermediate pairs may
leave the static curve and need not maintain constant power. Very long
ramps can repeat coefficients despite exact pending-frame counts. No
implicit millisecond conversion is applied; callers can use
`smoothing_samples` explicitly.

Nonfinite samples multiply naturally, including 0×infinity producing
NaN; controls remain usable because there is no input history. Finite
boosted inputs can overflow. This is not clipping, limiting, spatial
crossfeed, correlation compensation, a deadline/SIMD qualification or
latency compensation. Actual native mono/stereo Foundation→Graph
consumers exercise changing full-family controls and downstream FIR;
the f32 fixture explicitly converts to/from the existing f64 FIR. That
fixture does not claim native f32 FIR support. A paired dry/wet FIFO
with wet-latency compensation remains an independent capability.

### Paired dry/wet queues and wet-latency compensation

`DryWetGains` prepares a finite wet fraction0..1 directly, using the
same seven `PanningLaw` curves and two explicit normalizations as
panning. Preparing wet directly preserves a tiny wet coefficient that
would disappear if converted through `pan=2*wet-1`. Actual coefficients
can still underflow or round in their named precision. `quantize_f32`
creates native `DryWetGains32` while retaining requested fraction,
curve and normalization; center normalization can boost weights up to2.
No curve provides universal compensation for arbitrary correlation.

`DryWetMixer` / `DryWetMixer32` have explicit channels1..64, maximum
block1..65536, FIFO1..65536 frames, maximum wet latency0..4194300
samples, `DelayInterpolation` and ramp duration0..u32::MAX frames in
`DryWetMixerConfig`. The individual bounds do not override the checked
aggregate32MiB budget for native queue/history and fixed recursive
values. Construction validates the declared budget before fallible
reservations, then checks actual capacities. Allocation and disposal
belong to CONTROL. Push, mix, validated edits and reset use no heap,
framework locks or I/O; reset visits all prepared history/queue storage.

`push_dry` preflights the entire prepared channel/prefix/max/free-space
and u64 accepted-frame count before accepting a call. It advances each
channel's causal delay exactly once per source frame and queues
already-delayed dry values. It does not advance mix gains.
`mix_wet` or `mix_wet_in_place` validates every channel/active length,
maximum, queued dry availability and consumed u64 count before changing
queue, gains or output. Each consumed frame advances one shared pair
ramp once, then every channel computes delayed_dry×dry_gain +
wet×wet_gain in its native precision. Products and addition are explicit,
without hidden FMA or normalization repair. FIFO batch partitions can
differ and wrap; pending suffixes are retained. Overrun/underrun reject
the complete call, preserve owners and output, and are not silently
filled or dropped. Empty prefixes advance nothing; inactive backing
suffixes stay untouched. Counters describe the accepted/consumed epoch
since reset, rather than host/device time.

For requested d=n+f, dry reconstruction is causal: `Integer` truncates
to tap n; `Linear` combines n/n+1; `Lagrange3rd` evaluates nodes
n..n+3 at f; `Thiran1st` applies a=(1-f)/(1+f) and
y=a×v+previous_v-a×previous_y to tap v at n. Initial history is zero.
An integer-valued Thiran delay bypasses recursion and updates both prior
values. No
hidden minimum latency is added. Native32 rounds f explicitly and
rejects a nonzero FIR/Thiran fraction that becomes0 or1; Thiran also
rejects actual a rounded to1. Integer deliberately floors instead.
Requested delay and actual native fraction/coefficient getters are
distinct. The fraction is unused for Integer and can round to1 in native32
while the executed tap remains floor(requested). The coefficient a is used
only by fractional Thiran; other policies and its integer bypass ignore it.
Gain-pair interpolation uses f64; delay weights/recursion and sample products
use the named native precision. Linear/cubic are not
broadband ideal delays, and cubic can overshoot. Fixed noninteger Thiran
has unity ideal magnitude and requested DC group delay up to native
rounding, frequency-varying group delay and an infinite alternating tail.

`set_wet_latency_samples` rejects changed latency while dry is queued;
exactly identical requested delay is an idempotent no-op. An accepted
edit preserves causal/recursive histories and retaps immediately, with
no smoothing/crossfade or modulation guarantee. `set_target` follows
the pair ramp's exact consumption-frame policy, including retained
progress for identical actual gains and refreshed metadata. Push-only
intervals and failed consume calls advance no ramp. Reset clears pending
dry, delay history and counter epoch and snaps gains to the latest target,
while retaining latency/configuration/storage.

Callers supply wet samples in order from the same source processing
sequence and declare the wet path's actual latency. The mixer does not
authenticate content/timestamps, estimate latency, delay wet, reorder
frames or repair missing dry pushes. Callers explicitly push/mix zeros
for any desired tails; no EOF or automatic finalization is implied.
Nonfinite audio can poison delay history until reset and finite boosted
mixes can overflow. This is not sanitization, limiting, a host latency
report, a hardware clock, a native deadline/SIMD qualification or broad
physical alignment claim. Actual native three-channel Foundation→Graph
consumers exercise differently batched dry and a real wet FIR with a
caller-selected three-sample delay. The native32 fixture explicitly
bridges to the existing f64 FIR; it does not claim native32 FIR support.

The fractional-latency composition fixture separately uses a symmetric six-tap
wet FIR with group delay2.5 and Linear dry compensation at2.5. Half dry minus
half wet cancellation is measured through actual native64/native32 three-channel
Graph callbacks, including differently batched push/consume, reset/reprepare,
late maximum rejection and zero callback heap. The independent finite convolution
and closed response distinguish common phase from unequal magnitude: its low-
frequency null amplitude is bounded by0.7×(2πf)^2 per unit input in ideal
arithmetic, with native rounding qualified separately. It does not promise
broadband exact cancellation. Native32 explicitly bridges to the f64 FIR.
Largest admissible fractional taps near maximum17, including cubic tap19,
are independently compared across repeated history/FIFO wraps in both precisions.
Denormal handling remains caller policy, including the infinite Thiran tail.

### Six-stage modulated feedback phaser

`Phaser` / `Phaser32` own six first-order TPT allpass histories per channel,
a previous wet feedback value and one shared base LFO phase. `PhaserConfig`
requires finite sample rate8000..768000, channels1..64 and maximum block1..65536.
`PhaserControls` has no implicit defaults: rate0 freezes phase, otherwise
.001..100Hz; depth0..1; finite center frequency; feedback[-.99,.99]; wet mix0..1;
and adjacent-channel phase offset0..1 cycles. Depth means a maximum four-octave
half-span: fc=center×2^(4×depth×sin(2πphase_channel)), and each channel adds
channel_index×offset modulo one cycle. Channels do not crossfeed.

Preparation and complete control replacement validate the original floating
Hz sweep extrema and normalized endpoints[1e-6,.49], plus actual native design
representability, before owner mutation. A requested center/depth whose full
sweep leaves that profile rejects; there is no clamp, nudge or coefficient
repair. A private already-admitted design path shares the existing public
first-order coefficient expressions and recurrence, making frame updates
infallible inside this profile. Public first-order validation/defaults stay
unchanged. Tests inspect floating sweep extrema and endpoint/interior
coefficient equivalence in both precisions. Getters expose requested controls,
actual native feedback/mix, next-channel cutoff/gain and floating sweep bounds.

Each frame samples base phase before advancement and computes the explicit
per-channel sine/exp2/prewarp. F64 phase and transcendental preparation are
used in both variants; coefficient and recurrence/audio arithmetic use the
named precision. Cascade input is x+feedback×previous_wet, with feedback0
bypassing that multiplication. Six allpass outputs form wet, which becomes
next feedback history. Wet mix0/1 returns exact dry/wet endpoints; dry still
advances the complete wet path and phase without leaking0×poison into dry
output. One base phase step occurs per frame, regardless of channel count.
Frame and separate/in-place planar prefix methods validate all channels,
lengths and maximum before state/output mutation; empty prefixes advance
nothing and inactive suffixes stay untouched.

Controls replace together and retain all histories/base phase. Reset clears
all stages/feedback and base phase; reset-to-phase validates finite[0,1)
before clearing. Construction/control/processing/reset use fixed storage,
no framework heap, locks or I/O. Trigonometry/exp2/tan per frame/channel are
explicit work, with no cheap-LFO or native deadline inference. Floating
phase progress need not equal an ideal arbitrary-duration oscillator exactly.
A finite one-million-frame768k/.001Hz observation had maximum circular ideal
phase difference1.863e-14; the8000/100Hz10001-frame wrap fixture had1.782e-13.
These are finite observations qualified against a rounding accumulation bound.

At fixed actual gain G, p=1-2G and A=(-p+z^-1)/(1-p×z^-1), giving wet transfer
W=A^6/(1-feedback×z^-1×A^6). Zero-feedback half mix has three cancellation
notches: f=atan((G/(1-G))×tan(π/12,π/4,5π/12))/π. DC/Nyquist wet gains are
1/(1-feedback) and1/(1+feedback). This can amplify headroom substantially.
Static ideal allpass/small-gain facts are distinct from executed rounding,
arbitrary modulation stability or broadband pure delay. Recursive tails are
infinite and no constant integer latency/host report or implicit EOF flush
is provided. Native nonfinite/overflow audio can poison histories until reset;
retuning clicks/aliasing/denormals/saturation/limiter/SIMD/deadline policy remain
caller-owned.

Independent nodal and polynomial time oracles, rational transfer/notch
observations and a seven-state matrix-power tail certificate qualify selected
static profiles. For nine static profiles (normalized cutoffs.07/.13/.25,
feedback-.99/0/.99), six frequencies and131072frames/profile, native64 maximum
time/spectrum deviations were3.33e-14/1.69e-11. Native32 measured maximum time
error1.104e-6, complex spectrum error1.470e-4 and normalized
error/(1+|H|)=3.062e-6 against the ideal actual-coefficient response; its fixture
uses4e-6×(1+|H|). This is selected finite rounding qualification, not a universal
analytic error bound or perfect executed allpass assertion. Tail certificates
use the independent static state model, not feedback alone. Actual native
three-channel Foundation→Graph consumers exercise changing complete controls,
real downstream FIR, lifecycle, callback preflight and heap freedom. The native32
fixture bridges explicitly to the existing f64 FIR, without a native32 FIR or
physical/device/DAW/real-time support claim.

## Native nonlinear multimode ladder

`LadderDesign` prepares finite 8000..768000 Hz, cutoff/rate 1e-5..0.45,
resonance 0..1 and source drive 1..32. Both original physical endpoints and the
actual divided ratio qualify; rounded unavailable endpoints reject without
nudging or clamping. `quantize_f32` retains requested metadata and exposes actual
native coefficients; nonzero resonance rounding to zero rejects. Standard-rate
endpoint and immediately adjacent-value fixtures pin this two-part admission.

`LadderFilter` / `LadderFilter32` implement one feedback-summing tanh saturator and
four linear RC stages discretized by prewarped trapezoidal integration. For taps
`y1..y4`, the selected model is:

    u = tanh(d*x - k*y4), k = 4*resonance
    LP12 = y2; LP24 = y4
    HP12 = u - 2*y1 + y2; HP24 = u - 4*y1 + 6*y2 - 4*y3 + y4
    BP12 = y1 - y2; BP24 = y2 - 2*y3 + y4

All modes advance one history once. Lower-order modal taps retain four-stage
resonant feedback poles; bandpass names indicate total order, without hidden peak
normalization. Drive multiplies the source without output compensation. This
explicit single-saturator model is not a circuit-exact distributed transistor,
OTA or diode model. Solver, audio and state arithmetic use native precision.
`capacitor_state()` exposes trapezoidal integrator memory `s = y + v`, not voltage
`y`; retained-control edits preserve that memory, with explicit retuning transients.

The monotone scalar equation uses `lambda = k*G^4 >= 0`. Its amplitude-scaled
`[0, tanh(abs(c))]` bracket avoids a fixed absolute error floor for tiny signals.
Odd symmetry, at most 64/32 bisections, actual zero residual or adjacent endpoints
and selection by **rounded native residual** bound work to 67/35 solver tanh calls
plus one staged-residual call (68/36 total). This is not an assertion that the
chosen float minimizes a high-precision Decimal residual among neighbouring
floats. Last-solve records separate scalar affine and actually executed staged
feedback residuals, iterations, input and taps. Native tanh uses platform math;
bit-exact cross-platform results are not promised.

`LadderBank` / `LadderBank32` provide 1..64 independent histories with coefficient
copies and maximum block 1..65536 in fixed storage. Scalar-channel blocks also
cap at 65536. Whole channel, length, prefix and frame admission precedes every
history/output change. Empty calls preserve observations and backing suffixes.
Explicit bypass returns dry for every mode and freezes state/observations;
re-enable resumes. `set_design` retains memory, `replace_design` resets it, reset
retains design/bypass. `snap_to_zero` explicitly clears subnormal states; there
is no implicit block-boundary flush, smoothing, crossfade, allocation, lock or I/O.
Expensive bounded transcendental work does not establish a callback deadline.

Finite drive overflow and signed source infinity intentionally saturate with
finite history. Source NaN and poisoned feedback/stages propagate until reset.
Zero resonance skips feedback history to avoid `0*NaN`, so the saturated input
can remain finite while poisoned taps require reset. Positive feedback with
rounded-zero lambda still observes feedback history: both native precisions
qualify this distinct finite/poisoned branch. Caller owns headroom, denormal CPU
policy and nonlinear aliasing. No arbitrary-retune stability or universal
headroom/accuracy/alias-free assertion follows from these fixtures.

Native32 can reach a stationary rounding floor at low admitted cutoffs. A finite
million-sample zero-resonance, drive-one, constant-0.5 fixture compares LP24 with
the model equilibrium `tanh(0.5)`. At ratios 1e-5/1e-4/1e-3/1e-2, observed absolute
LP24 errors are about 0.001897/0.0001897/0.00001892/0.000001810; BP12 residual at
1e-5 is about 0.0004743. HP12/HP24 remain much smaller in this fixture. Native64
LP24 error at 1e-5 is about 3.53e-12. These are selected native/platform recurrence
observations, not a universal error bound; no hidden precision lift repairs them.

The small-signal model has `L = 1/(1+s)`, denominator `1 + k*L^4` and the six
modal numerators above, scaled by drive. The ideal `k = 4` endpoint is marginal,
without a strict decay claim. Selected 32768-frame fixtures track all six modal
peak tails and whether values remain nonzero/subnormal. Native32 damped tails
can settle into persistent subnormal states; native64 and native32 observations
are kept distinct. The tests do not certify infinite nonlinear or modulated decay.

Independent 450-digit exp/Newton fixtures qualify 816 zero-state and 2016
nonzero-history, cancellation and nonlinear-knee equations reconstructed in
actual native arithmetic. Matching recorded/replayed equations retain the strict
4*epsilon*abs(root) + two minimum-subnormal acceptance. All 2016 history cases
match c/lambda exactly on the recorded backend, with zero equation-drift allowance.
For a different native-math backend, reconstruction reports actual versus reference
c/lambda separately. Since F(u)=u-tanh(c-lambda*u) has derivative at least one and
tanh is 1-Lipschitz, root displacement is bounded by
abs(delta_c)+abs(delta_lambda)*abs(reference_root). Nonzero operations round upward;
the native relative budget then uses abs(reference_root)+that displacement bound.
Sixty independent 450-digit alternate-equation vectors with deliberately synthetic
1e-4 perturbations admit all displaced roots (56 fail the strict-only budget),
while all 60 deliberately erroneous solver candidates reject. Those are validator
qualification cases, not observed platform drift or production solver failures.
Observed matching-equation normal relative maxima are approximately 2.170e-16/7.559e-8
for the first-sample set and 1.964e-16/9.307e-8 for the history set. Independent
four-dimensional nodal Newton, fourth-order polynomial time equations, DC
models and bilinear spectra cover every mode. Dynamic nodal observations are
about 3.094e-15 native64 and 9.978e-7 native32 when normalized by 1+abs(expected).
All numerical receipts are captured with `--nocapture`; these finite selected
profile qualifications remain separate from ideal/requested spectra.

Independent RK4 continuous-model integration with joined source endpoints gives
selected second-order time-convergence observations at 48/96/192 kHz. The
zero-resonance, high-drive coherent fixture qualifies actual folded fifth-harmonic
aliasing against converged saturation quadrature. An ideal bandlimited Fourier
signal is an explicit independent reference, not a framework oversampler or an
implicit-feedback alias qualification.

Actual three-channel Foundation → Graph → FIR consumers exercise the frame,
planar and in-place bank APIs, independent design/mode/bypass/replacement schedules,
variable/empty callbacks, repeated prepare/deactivate and NaN/reset recovery.
Separate bank tests cover every refusal, all 64 unchanged histories/observations,
and exact comparison with 64 independent filters. Measured callbacks allocate and
deallocate zero times. Native32 explicitly bridges to the existing f64 FIR; no
native32 FIR, physical device/DAW, SIMD, denormal mode, latency alignment or
hardware deadline support is inferred.

See [the ladder decision](decisions/0258-native-nonlinear-multimode-ladder.md).

## Prepared interpolated peak estimates

`TruePeakDesign`, `TruePeakMeter64` and `TruePeakBank64` provide streaming finite-FIR
peak estimates. The bank observes 1..64 channels; the meter is its single-channel
owner. They keep sample peak, interpolated peak and original source counts
separate. Native f32 audio widens exactly into the explicit f64 measurement
recurrence; this is not a separate native-f32 FIR. Observation never edits audio,
including mutable buffer views or NaN samples. Loudness weighting, integrated
gating and loudness range are independent capabilities.

Choose the complete profile explicitly: integer sample rate 8000..768000 Hz,
4/8/16 phases, source half-width 32..512 frames, and finite Kaiser beta 0..16.
There is no default or automatic rate-dependent filter selection. The sample rate
is validated metadata only: it is stored and returned, but the numerical
coefficients depend on normalized sample coordinates, that is on the factor,
half-width and beta, and the library cannot verify the actual source clock. A
caller that declares 8000 Hz for a 768 kHz stream obtains bitwise identical
coefficients and readings, so a consumer that must not mix rates compares
`sample_rate()` itself (the test Graph adapter does). The 216 rate/profile
admission combinations exercised by the tests (6 rates x 36 profiles) are
therefore 36 distinct coefficient profiles. `TruePeakDesign`
prepares immutable coefficients on CONTROL and exposes the actual newest-to-oldest
phase rows and their compensated DC sums. Cloning a design allocates on CONTROL.
`new(&design, maximum_frames)` and the bank's channel-count constructor borrow the
original design, preserving it on refusal. Maximum callbacks are 1..65536 frames.
A new bank/meter owns a fallible prepared copy; changing a profile means stopped
construction of another owner. Aggregate prepared arrays have an 8 MiB declared
budget before reservation and a check of actual capacities afterward. This is a
defensive cap kept unchanged: the largest admitted bank (64 channels, half-width
512, 16 phases) is about 665,000 bytes (history 524,800, coefficient copy 131,200,
scratch 8,200 and the owner itself), roughly 650 KiB or under 8% of the cap, so
the cap cannot be reached through the public API today and no public cap
exhaustion test is claimed.

For half-width L, phase p/F and latest source index t, a phase measures
`sum(j=0..2L) x[t-j] h_p[j]` at coordinate `t-L+p/F`. The coefficients are this
package's own Kaiser-windowed cardinal sinc with support
`abs(j-L+p/F) <= L`, explicitly normalized to unity DC. Phase zero is the exact
delayed source sample. Neumaier accumulation qualifies normalization, reported
actual DC sums and phase dot products; rounded coefficients are not subsequently
repaired. Native sine, square root, division, underflow and rounding remain
observable. No duration-growing floating phase accumulator is involved.

Negative source indices are zero. A live reading has up to L original frames in
its unresolved suffix, exposed by `pending_source_suffix_frames()`. The latest
evaluated coordinate is `last_phase_numerator()/F`; it is `None` before any phase
work. The coordinate can be negative at startup. It describes completed work,
not a claim that the unreceived suffix has already been measured. A live
`interpolated_peak` can therefore be below `sample_peak` and is not a peak bound;
coordinates beyond the last source frame are measured only by `finish()`.

`finish()` explicitly evaluates 2L zero-input steps after any nonempty source,
covering the complete leading and trailing finite support. It adds only
`tail_frames`, never original `source_frames`. For N original frames, phase work
is `(N+2L)*F`; an empty source has no artificial work. The last coordinate after
a nonempty finish is `N-1+L+(F-1)/F`, including the harmless final phases outside
the endpoint support. The peak value is not expected to discriminate a tail much
longer than L steps because far coordinates only see small window edge weights;
the full 2L tail is evidenced by the exact counters. Repeated finish is
idempotent. All subsequent observation,
including an empty call, refuses until `reset()`. Reset clears histories, flags
and counters while retaining the prepared profile. Finish belongs after exclusive
stopped ownership; neither Drop nor graph deactivation implicitly performs it.

Whole shape, maximum, complete buffer prefix, lifecycle and count admission
precedes any source/history change. `source_frames <= floor(u64::MAX/F)-2L`
reserves complete tail work before acceptance. Count refusal does not partially
measure the call. Empty live calls leave histories and readings unchanged.
Native nonfinite source counts as measurement silence (also for `sample_peak`)
and latches the per-channel `nonfinite_input`; the observed audio still passes
through unchanged. That substitution is distinct from `amplitude_overflow`,
which marks a finite-sample reconstruction whose final arithmetic result exceeds
f64::MAX. Per-window
maximum-absolute scaling avoids false intermediate overflow at extreme finite
f64 magnitudes. Genuine reconstructed amplitude beyond f64::MAX becomes positive
infinity with `amplitude_overflow`. Subnormal values are not automatically flushed.
`db_tp()` reports `20*log10(interpolated_peak)` relative to amplitude one; silence
is negative infinity, true overflow positive infinity. Values above unity remain
above unity.

Observation, reset and explicit finish allocate nothing, lock nothing and perform
no I/O or coefficient redesign. They can still be expensive: each source step per
channel scans 2L+1 history values, creates that many normalized scratch values, and
performs `(F-1)*(2L+1)` weighted products plus compensation. Phase zero is cardinal.
At 8 phases and L=128 this is 1799 weighted products per channel per source frame;
64 channels at the maximum 16/512 profile require 984000 weighted products per
source frame. Finish performs the same work for 2L steps. These are operation
counts, not measured deadlines or recommendations to use the maximum profile in
a callback. Prepare a profile that the actual consumer can afford.

The tests include 1276 independent 90-digit Taylor/Bessel/sinc coefficient values,
absolute source-coordinate dot products, arbitrary partitions, leading/trailing
support, exact counter headroom, 64 independent channels, native32 widening,
nonfinite/reset and heap observations. Added coverage: an absolute-coordinate
oracle that shares only the prepared rows; a last-source impulse and emphasised
end/start fixtures whose maximum lies outside the source (a truncated tail and a
live reading are visibly lower); an L=512 source longer than its 1025-tap ring;
mirror-row symmetry, which proves that reversing a source leaves the finished peak
unchanged up to addition order in this normalized finite discrete design; and
first/middle/last-channel nonfinite and overflow isolation in a 64-channel bank,
compared with independent single-channel meters. Actual Foundation→Graph→FIR
consumers cross actual separate and in-place Graph audio buffers with shared and
mutable meter observation views in both native precisions (24 combinations per
build profile), then return the meter through a CONTROL-only deactivation channel
for explicit stopped finish. A separate Graph episode poisons one channel with
NaN, deactivates and reads `nonfinite_input` on that channel only, with
isolated channel peaks and the NaN carried through the audio path. A second,
separate episode poisons, resets through the Graph and compares all post-reset
FIR outputs with direct convolution from zero history. The measured heap-free
audio calls are retained. Selected coefficient maximum absolute disagreement is
5.551115123125783e-16. A steady window at 0.9*f64::MAX distinguishes an overflowing
ordinary partial sum from the finite scaled result. These are numerical and
composition receipts, not hardware or real-time support.

Profile accuracy remains explicit. A generated analytic wavepacket with a known
continuous peak of one, occupying 0.4609375..0.4890625 cycles per source frame,
covers 36 profile combinations of factors 4/8/16, half-widths 32/128/512 and betas
0/6/12/16. Its centre is a half-sample, which lies on every 4/8/16 grid, so many
of the 36 combinations repeat the same sampled peak; the count of distinct sampled
peaks is only printed, not asserted, and this fixture does not by itself
demonstrate grid quantization. Twelve combinations fall outside the +0.2/−0.4 dB
observation interval, and the tests now assert exactly twelve: all twelve are
the half-width-32 profiles and no other profile fails. They include
over-reading, about +0.2952 dB, at beta 0 (three factors) and
under-reading from about −0.769 dB to −1.189524690758779 dB at beta 6, 12 and 16;
the worst under-reading is pinned to 1e-9 dB. The 512/12 profiles are within
0.01 dB in this fixture. The interval was not widened. A quarter-rate fixture
whose continuous peak lies half a grid step between coordinates has the
independently derived value cos(pi/(4F)) (about −0.1685, −0.0419 and −0.0105 dB
for 4/8/16 phases); the tests allow an explicit finite-window tolerance (0.8/L at
beta 0, 1e-3 at beta 6, 1e-4 for beta of at least 12, chosen from truncated-sinc
and Kaiser leakage rather than measured) because a finite window need not equal
the ideal sinc identity, and an off-grid fixture checks the cosine of the distance
to the nearest grid point. Finite reconstruction bandwidth and sampled phase grids
are not universal continuous-maximum bounds. No standards-conformance claim
follows from passing selected profiles, and failing profiles remain supported
estimates with these visible limitations.

An optional worker-only `true-peak-file` example reads caller-selected mono raw
little-endian f64 PCM (at most 16 MiB, bounded read even if the file grows). It never
opens a device or plays audio. A Python admission script supplied with the tests
takes an explicit prebuilt example binary and generates its own temporary raw PCM:
empty, partial-sample and 16 MiB+1 refusals, profile parsing errors, NaN/Inf flags,
a genuine overflow and a short finite fixture compared with an independent
convolution, all with exact failure text and bounded subprocesses. The exact
16 MiB success case is a separate costly option and is not part of a default run.
`tests/true_peak_ffmpeg_oracle.py` independently
generates double-PCM WAVE/raw inputs and compares an explicitly selected external
FFmpeg `ebur128=peak=true` filter without adding a dependency. The FFmpeg 6.1.1
receipt carried from the original packet uses 12 generated files at 44.1/48/96/192
kHz and an 8/128/12 meter profile.
Selected carriers at or below 20 kHz differ by at most 0.041921 dB from its
0.1-dB-precision summary. Two 48-kHz carriers at 192-kHz source rate retain
3.000004/2.158079 dB differences. The statement that that FFmpeg version
reconstructs at 192 kHz is attributed to the linked upstream source and has not
been newly verified here, whereas this selected profile evaluates eight phases at
every source rate; the reference's different grid/filter is an observation, not
an equality oracle. Its 0.1 dB rounding cannot discriminate deviations below about
0.05 dB, and no case lies in the 0.46–0.49 cycles/sample band where the twelve
fixture failures occur. All sources are generated here; no published coefficient
table or programme fixture is copied. See
[decision 0263](decisions/0263-prepared-interpolated-peak-estimates.md).

## Prepared calibrated loudness and loudness range

`KWeightingDesign` and `PreparedLoudnessMeter64` provide a complete prepared
K-weighted M/S/I/LRA measurement profile. Momentary and short-term readings use
four and thirty consecutive 10-Hz grid intervals. Integrated loudness retains
400-ms observations for retrospective power gating; loudness range retains 3-s
observations for its separate gates and percentile distribution. Native f32 audio
widens exactly into the explicit f64 measurement filters. Audio, including NaN
samples and mutable views, is never edited by observation.

The caller selects an integer sample rate 8000..768000 Hz, 1..64 channel roles,
maximum callback 1..65536 frames, initial programme-running choice and independent
`LoudnessRecordCapacities { integrated, lra }`. Each capacity is 1..1,000,000;
there is no automatic growth. Aggregate owner/record/report-scratch storage has a
defensive 32 MiB guard, checked on requested sizes before reservation and on actual
allocated capacity afterward. It is distinct from the tighter limit the current caps
impose: three 1,000,000-element arrays are 24,000,000 bytes plus the owner, below 32
MiB, so today's legal requests cannot reach the pre-reserve refusal. Actual
capacity may exceed a request, so the post-allocation check is not claimed
unreachable. `ResourceLimit` stays public for a larger future cap.
Construction borrows the immutable design/roles and preserves them on refusal.
There is no implicit default profile or channel order: `Front` has power weight
1, `Surround` 1.41 and `Excluded` 0. Weighted (`Front`/`Surround`) channels each keep
their own two K-filter memories, which survive programme standby and are cleared only
by `reset()`. `Excluded` channels perform no filter work and keep no K history; they
only flag nonfinite source. These roles do not interpret advanced spatial, height,
object or scene metadata automatically.

The two calibrated rational bilinear sections prepare on CONTROL, reuse existing
strict rounded biquad Jury validation and expose actual normalized coefficients.
`magnitude_at()` reports their actual product response at physical 0..Nyquist
frequencies. Shelf calibration facts are f0=1681.9744509555319 Hz,
Q=0.7071752369554193, gain=3.99984385397 dB and exponent=0.499666774155. The RLB
facts are f0=38.13547087613982 Hz and Q=0.5003270373253953; its discrete numerator
remains `(1,-2,1)`. Native platform tan/pow, coefficient division, f64 transposed
recurrence, rounding and subnormal state remain observable. There is no hidden
rate-dependent normalization or denormal-mode change. `Biquad::delay_state()` is
an additive readonly copy of its two actual recurrence memories; its processing
expressions and defaults are unchanged.

Nominal 10-Hz endpoints are `floor(k*Fs/10)`, generated by bounded integer
remainders without a growing floating phase. Interval lengths are `Fs/10` or one
frame longer. Four-grid windows have explicit sample quantization; thirty-grid
windows are exactly `3*Fs` frames at every supported integer rate. Power means
divide by the actual summed source-frame counts, never the number of channels.
The floor policy at odd rates is not asserted as normative discrete rounding.
A reading exposes actual completed window endpoints and source counts, rather
than labelling an arbitrary callback endpoint as a completed observation.

A fixed 30-interval ring stores positive scaled energy totals. Combining windows
uses positive sums without sliding subtraction, and neither input squaring nor
physical programme-power conversion can force false intermediate overflow at
extreme amplitudes. Reporting uses `-0.691 + 20*log10(scale) +
10*log10(normalized_power/actual_frames)`. `None` means no complete valid window;
measured silence is negative infinity. Maxima belong to the same complete live
grid and have no hidden smoothing. Log-domain complete records remain available
for retrospective analysis, including quiet windows that later become relevant.

Whole channel/shape, complete prefix, maximum, lifecycle, source-count and exact
upcoming record-capacity admission precedes history or count mutation. Original
source admission reserves `ceil(3*Fs/2)` frame headroom for optional end silence.
Empty live calls preserve all state; observation after finish, including empty
calls, refuses until reset. Nonfinite source becomes measurement silence and
latches `nonfinite_input`, while raw audio remains unchanged. Genuine finite-input
filter overflow checks both output and next recurrence memories: its first
zero-based source-plus-tail coordinate/channel/section is retained, further filter
execution stops, and accepted counts still advance. Live readings and
`analyse_programme()` become unavailable until reset. The readonly
`integrated_records()`/`lra_records()` slices are not hidden: after a fault they are
raw diagnostic history, namely complete pre-fault records, partially measured
windows (finite but missing energy that was never accumulated) and windows wholly
after the latch (`-inf`). They are not valid reports; `reading().numeric_fault`
decides validity. This is observable committed work, not a rollback or sanitized
audio receipt. No bounded/no-heap test establishes a real-time deadline.

`set_programme_running(false)` pauses I and LRA together, retaining complete
records while discarding the partial programme window and both window rings. The
weighted channels' K history and live M/S continue observing original audio;
`Excluded` channels have none. Resume starts fresh full programme windows (the first
I record needs four resumed blocks, the first LRA record three resumed seconds);
power from the two sides of a pause is not spliced. Identical requests are
idempotent. Original, admitted programme and current uninterrupted programme
coordinates are separate. `reset()` clears all histories, flags, counters,
maxima and records while retaining profile/capacities/running selection, without
allocation or implicit input rewind.

`analyse_programme(&mut self)` requires exclusive CONTROL ownership. It scans
retained records and sorts prepared scratch: O(N) integrated work and O(N log N)
LRA work, without allocation after preparation. It is not callback work or an
automatic concurrent live integrated/UI report. Integrated gates are strictly
`> -70 LUFS` and `> absolute-gated-power-mean - 10 LU`. LRA gates are inclusive
`>= -70 LUFS` and `>= absolute-gated-power-mean - 20 LU`. Means use log-sum-exp;
arithmetic dB averaging is not substituted. Reports expose gate levels, absolute/
relative counts, complete source/report endpoints and actual percentile levels.
The zero-based nearest indices are `floor(((N-1)*p+50)/100)`, p=10 and95.
No admitted distribution gives `None`; one admitted LRA value gives zero range.

`finish(None)` finalizes without manufacturing incomplete final windows.
`finish(LraSilence1500ms)` explicitly filters exactly `ceil(3*Fs/2)` zero inputs,
adds only LRA observations on the current programme grid and exposes the actual
artificial count. It neither increases original counts nor creates integrated
records or live M/S observations. Thirty-grid windows remain exact; the requested
1.5-s tail is half a source sample longer at odd rates. Exact upcoming LRA count
and storage are preflighted before any tail work. At 8001 Hz, `3*Fs-1` original
frames and 12002 artificial frames need sixteen new LRA records; an LRA capacity
of fifteen refuses without change. Repeating that request still refuses; a
caller may deliberately retry `None`, or prepare a larger owner and replay the
source. Fixed capacities are not enlarged automatically. Tail requests in standby
refuse, and an empty programme has no artificial work: with running selected but no
admitted programme frame, `LraSilence1500ms` succeeds without any tail and only
`tail_frames == 0` shows it. Successful finish is idempotent, and the first selected
policy wins even if a later request names another. The 1.5-s artificial tail is this
crate's own padding policy, not a standards requirement; it adds windows that sit
below a steady programme and can move the LRA percentiles of a short programme.
Prefer `None` when only the original duration is analysed. Neither graph deactivation
nor Drop implicitly performs finish or analysis. A finite silence interval does not
certify the infinite IIR tail has drained.

Regenerate the Decimal rows with
`python3 crates/ravet-signal/tests/loudness_decimal_oracle.py OUTPUT.csv` and compare
them with `crates/ravet-signal/tests/data/loudness-coefficient-oracle.csv`; the script
uses only the Python standard library. Qualification covers every 760001
admitted integer rate, all ten grid remainders, near-u64 headroom injection,
independent absolute-source direct-form I windows, 170 Decimal90 coefficients and 85
physical complex responses. The CSV is asserted to hold exactly those 255 distinct
identities (170 `C`, 85 `R`, seventeen rates, no duplicate or missing row). The
Decimal oracle is independent ARITHMETIC over the SAME calibration mathematical
facts as production (f0, Q, gain, exponent): it checks the bilinear algebra and
rounding, and does not independently certify those facts. The independent anchor is
separate: the 48 kHz stage coefficients are asserted against numeric facts read from
ITU-R BS.1770-5 Annex 1 Tables 1/2 (shelf b = 1.53512485958697, -2.69169618940638,
1.19839281085285, a = -1.69065929318241, 0.73248077421585; RLB b = 1, -2, 1,
a = -1.99004745483398, 0.99007225036621) within 3e-12, then a direct-form-I
filter built from those literals and the fixed mathematical transfer predict the
meter's steady tone. This anchors the 48 kHz numerical calibration only; it is not
a conformity claim and no normative tone-level target is asserted. Maximum
selected coefficient difference is 8.881784197001252e-16; response disagreement
normalized by `(1+H)` is 1.947479844881771e-10. Twenty-four own steady997-Hz
mono/stereo/5.1 profiles at eight rates differ at most 1.133138027853420e-11 LU
from their actual coefficient transfer prediction.

The calibration is rate dependent because the RLB numerator is `(1,-2,1)` without a
rate-dependent gain. Earlier measured amplitude-one mono 997-Hz sine readings are
approximately -2.9962005 LUFS at 8 kHz, -3.0102859 at 48 kHz and -3.0451421 at
768 kHz. The CSV-derived selected-transfer table, relative to 48 kHz at the same
physical frequency, is: 8 kHz +0.2148/+0.2158/+0.0141 dB at 37/137/997 Hz, 16 kHz
+0.0862/+0.0864/+0.0402 dB and 768 kHz -0.0406/-0.0406/-0.0349 dB. These are
selected-transfer facts at named frequencies, not a wideband supremum, programme
error, profile accuracy or conformity claim. Rows at Fs/4 and Nyquist are asserted
against the same relative positions of the 48 kHz response, each as its own
scoped assertion. All differences are retained, not repaired. Own 40-s 5/10/20-LU
steps, repeated programmes and added silence qualify final distributions
independently of the coefficient/time oracle; the steps also assert integrated
loudness against independent direct-form-I records, independently written strict
gates and a block-count closed form (all 397 windows pass the gate for 5 and 10
LU; for 20 LU the quiet half is excluded and 200 windows remain).

Further own tests discriminate stale filter reset (zero record after a reset
reads exactly `-inf` only if K memory and grid cleared, with the same history
proven nonzero by a twin that is not reset), live momentary/short-term values,
maxima and all report endpoints (including a tail that leaves live and integrated
state unchanged), pause/resume against a direct-form-I oracle with discarded partial
energy, retained K history and the three-second LRA ring warmup, initial standby,
per-frame f32 equal to exact widening (including `f32::MAX` and nonfinite values),
Excluded isolation (`f64::MAX`/`f32::MAX` cause no fault, only nonfinite sets the
flag) and private fault injection across channel, source coordinate, section and
output-versus-memory with accepted progress and raw record diagnostics.

The ordinary Graph matrix has twenty-four combinations: native f32/f64, rates
8000/8001/48007, both meter planar/in-place views and both external
separate/in-place audio buffers, each with variable and zero-length callbacks and two
prepare/deactivate cycles. Independent output FIR and full-source I/LRA
window/gate/rank checks qualify the consumer; the unique meter returns on CONTROL
before stopped tail and analysis. Its in-matrix poison and reset episodes use
separate external buffers and a single NaN frame. Two follow-up families run the full
twenty-four-combination matrix again, so they also cover external in-place buffers:
(1) a Graph reset after nonzero K and FIR history, where a zero window then reads
exactly `-inf` and FIR output is exactly zero, with a never-reset control proving the
history was nonzero; (2) multi-frame nonfinite audio (NaN, +inf, -inf) in Front or in
the Excluded channel, where neighbouring channels stay finite and FIR-correct on every
frame, the meter flag is set with no numeric fault and independent records use
measurement silence, followed by a Graph reset that clears the flag and recovers FIR
output. NaN remains raw audio and poisons the poisoned channel's FIR output; nothing
is claimed about that channel's FIR frames after the poison. Observation, reset,
programme control, stopped finish and prepared analysis have measured zero heap work
in the scoped tests; in the Graph follow-ups the measured closures include the
poisoned callbacks and `Graph::reset`, with every buffer allocated beforehand.
`set_programme_running` is measured on the bare meter, not through the Graph.
CONTROL construction allocates by design and is outside the no-heap scope.

The optional worker example `loudness-file RAW_F64LE RATE ROLES TAIL` reads explicit
interleaved little-endian f64 PCM, bounded to 16 MiB even if the file grows.
Roles are comma-separated `f,s,x`; tail is `none` or `lra1500`. It never opens a
device or plays audio. `tests/loudness_ffmpeg_oracle.py` generates exact own
mono/stereo/5.1 double-PCM files and uses explicitly selected external executables.
The FFmpeg 6.1.1 receipt covers fifteen files at 8/8.001/44.1/48/96 kHz. Original
I/M/S maximum printed-reference differences are 0.041725/0.047226/0.046971 LU.
LRA uses a separately zero-padded reference and retains 0.233355 LU maximum
difference: that version uses a 0.01-LU histogram and one-based rounded `N*p`
ranks, while this meter keeps full records and explicit zero-based `(N-1)*p`
ranks. A 36-record fixture uses low indices 3 versus 4; an independent diagnostic
reproduces the external 2.2/2.7-LU summaries without changing this meter's policy.
The reference summary has 0.1-LU precision; its odd-rate grid policy also differs.
The FFmpeg figures are recorded receipts from the isolated W251 qualification; the
optional comparison is not part of the workspace checks, and the 0.233355 LU LRA
mismatch stays visible. Against the primary
EBU Tech 3342 V4 text only the following were compared: the nearest zero-based
`round((N-1)p)` rank equals the example's one-based `round((N-1)p+1)` after index
conversion, and the inclusive `>=` gates and 10-Hz hop agree with it. Odd-rate
exact-hop rounding and the artificial tail remain this crate's own policy; no EBU
Mode conformance is claimed. This is selected numerical/composition/reference
evidence, without standards, EBU Mode/trademark, hardware, timestamp, acoustic or
deadline certification.

Primary mathematical provenance is [ITU-R BS.1770-5](https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.1770-5-202311-I!!PDF-E.pdf),
[EBU Tech3341 v4](https://tech.ebu.ch/docs/tech/tech3341v4_0.pdf),
[EBU Tech3342 v4](https://tech.ebu.ch/docs/tech/tech3342.pdf) and the calibration
facts in [Brecht De Man's author repository](https://github.com/BrechtDeMan/loudness.py).
Equations and generated signals are expressed here independently; no vendor
implementation body, coefficient table or external programme fixture is imported.
The author reference declares MIT; its complete provenance notice is retained in
[Signal third-party notices](../crates/ravet-signal/third-party/loudness-calibration-brecht-de-man.txt).
See [decision 0264](decisions/0264-prepared-calibrated-loudness-range.md).
