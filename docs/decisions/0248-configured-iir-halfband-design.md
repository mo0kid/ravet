# 0248: Configured elliptic allpass halfband design and consumers

Status: accepted contract; local implementation reviewed. Public source
adoption and subsequent qualification are tracked in WORK_STATUS.md.

## Context

Fixed prepared f32 halfband profiles do not expose order, transition, attenuation
or per-stage choices. Elliptic SOS and FIR halfbands have different coefficient,
state and conversion contracts. A reusable allpass family needs complete precision,
polyphase, full-rate and oversampling consumers.

## Decision

Add fixed-storage selected odd order 1..63/full high-rate width (0, .5), f64 design
and explicit validated f32 quantization. No dependencies, original API/default
changes, heap allocation or callback I/O/locks. Nominal attenuation above 300 dB,
unrepresentable boundaries/moduli, unordered/duplicate/unstable radii reject.

Set pass = .25 - width/2, k = tan²(pi*pass); u = 2jK(k)/N for j = 1..(N-1)/2.
With Jacobi s/c/d, squared digital pole radius a = [(1+k)s/(1+k s²+c d)]². This
rationalizes (1-C)/(1+C), C = c d/(1+k s²), avoiding numerator cancellation and
rounded pole projection. NIST special half-complete values reduce the elliptic
pole shift to Kprime(k)/2. Use the independently derived radius, rather than
projected SOS poles. Own bounded AGM/Jacobi mechanics follow the separately
qualified elliptic family.

Degree relation R(k1) = N R(k), R = Kprime/K, uses at most 128 log-coordinate
bisections and residual <= 1e-12, explicit complementary pair and normal
MIN_POSITIVE..nextdown1. N = 1 has k1 = k. Ep² = k1 and Es² = 1/k1 couple nominal
ripple and attenuation. The lower attenuation bound 10log10(2) follows k1 < 1.
Strict requested sizing starts at the analytic odd minimum and retries through 63;
all candidate design/response failures continue. Only actual rounded stop-edge
amplitude is the admission criterion. Separate 4097-point sampled stopband
peak/frequency and nominal/actual edge observations prevent a whole-stopband
certification claim.

Low-rate A_b(q) = product((a+q^-1)/(1+a*q^-1)). Full-rate
Hlow = .5*(A0(z²)+z^-1*A1(z²)), Hhigh subtracts. The already full-rate z^-2 section
must not be squared again. Sorted radii alternate direct/delayed. N = 1 is exactly
(1+z^-1)/2; N = 4m+1 versus N = 4m+3 unequal branches keep explicit ordering/delay.
Interpolation gain 2 relative to zero stuffing and even-phase decimation with
previous odd branch reproduce full-rate transfer without phase trimming.

Add full-rate complementary filter, independent up/down 2x owners and configurable
1/2/4/8 cascade consumers in both precisions. Validated immutable profiles own
fixed states; replacement resets histories, unsmoothed. Three stage pairs/eight
stack samples bound oversampling; child executes exactly factor times in time
order. Length preflight preserves child/state/output; empty is a no-op. Nonfinite
audio and headroom propagate; no implicit dry alignment, host latency or finite
IIR EOF tail.

Primary family audit:
[author halfband paper](https://www.researchgate.net/publication/228946539_Half-band_IIR_filter_design_using_MATLAB),
[NIST special values](https://dlmf.nist.gov/22.5#T2),
[NIST AGM](https://dlmf.nist.gov/19.8).
Implementation uses own mathematics, without third-party implementation input.

## Acceptance and limits

Every odd order, both native precisions, exact N = 1 and both branch classes;
independent complete-integral power series/log-nome/theta prototype poles/rational
spectrum; independent direct polynomial time and different-profile nonlinear
nested cascades; strict measured sizing/refusal/analytic order bounds and
independently sampled phase peaks; actual steady interpolation image, decimation
and nonlinear alias observations; reset/partition/length/failure/quantization/heap
and actual canonical native f32/f64 stereo Foundation/Graph consumers. Nominal
182.835031 dB at order 63, width .001 versus actual 182.834619 dB illustrates why
ideal-edge sizing can refuse at the order ceiling.

These are bounded numerical/consumer qualifications, not continuous whole-stopband
certificates, all-extreme representability, physical platform/DAW/acoustic support,
native deadlines, SIMD or denormal handling. Broader named DSP remains independent.
