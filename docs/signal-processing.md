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
