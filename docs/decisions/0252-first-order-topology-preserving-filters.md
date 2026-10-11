# 0252: First-order topology-preserving filters and explicit capacitor ownership

Status: accepted contract; local implementation reviewed. Public source
adoption and subsequent qualification are tracked in WORK_STATUS.md.

## Context

Biquad and two-integrator state-variable filters do not expose the same first-order
trapezoidal LP/HP/allpass and modulation-state contract. A reusable native family
needs all responses, explicit precision, signed initial state and channel-prefix
ownership without changing existing defaults.

## Decision

Add immutable actual-gain f64 design and explicit independently validated f32
conversion, simultaneous output/sample owners and fixed independently channelled
banks. Rate 1..768000 Hz and cutoff strictly positive/below actual Nyquist. Validate
original Hz relation before division, normalized ratio, tan prewarp, gain/complement
and rounded stable pole; collapsed endpoints reject without nudge, clamp or repair.
Requested metadata and actual coefficient-derived ideal LTI spectrum are distinct.
Evaluate observations with half-angle G*cos and (1-G)*sin components rather than
cancellation-prone expanded 1-p; no domain relaxation or pole repair.
Design performs CONTROL trig; selecting a prepared design in runtime does not.

Derive from the trapezoidal feedback equation y = g*(x-y)+s, prewarped
 g = tan(pi*cutoff/rate), G = g/(1+g): v = G*(x-s), low = s+v, next s = low+v;
high = x-low and allpass = low-high. The capacitor state is signed and is not a
previous LP sample. Fixed-state transfer has p = 1-2G, LP G*(1+q)/(1-pq),
HP (1-G)*(1-q)/(1-pq), AP (-p+q)/(1-pq), q = z^-1. The source uses own mathematical
recurrence and primary circuit derivation, without external implementation input.
[Author's mathematical reference](https://www.discodsp.net/VAFilterDesign_2.1.2.pdf),
sections 3.6–3.9.

Sample processing returns all responses from one state update. Separate/in-place
blocks select a response without history reset. Prepared design set preserves
capacitor state; replace resets. Finite signed reset-to and caller-only subnormal
snap have explicit preflight/history semantics. No automatic per-block snapping,
smoothing, crossfade or inaudibility/modulation stability claim.

Banks share a validated profile and retain 1..64 independent native states; maximum
blocks 1..65536. Validate every channel count/prefix/frame bound before any output
or history mutation, preserve suffixes and empty no-op. Set/reset/replace/reset-to
apply consistently across states; invalid channel/initialization leaves them
unchanged. Fixed storage, no dependencies or framework allocation/I/O/locks.

## Acceptance and limits

Both precisions/all responses and meaningful rate/cutoff profiles; independent
analog-bilinear spectral and direct polynomial time/impulse oracles, exact signed
initial and independently solved rapidly time-varying circuit equation; actual
requested-versus-rounded quarter-rate landmarks. Explicit infinite ideal tail and
separate product-rounding subnormal floor, not forced finite EOF. All channel
counts, selected/simultaneous scalar/planar prefixes/partitions/suffixes, late
failures/initial/set/replace/reset/poison/snap and zero heap.

Actual native f64/f32 stereo Foundation→Graph pipelines with different coefficient
schedules and existing FIR downstream qualify preparation, reset/deactivation,
committed source and callback preflight/heap against independent circuit/convolution.
The f32-to-f64 FIR bridge is explicit caller code, not implicit buffer conversion.

Nonfinite/large finite overflow can poison histories; no sanitizer/headroom repair,
processor-wide denormal flag, SIMD, native deadline, finite-tail flush, integer host
latency or dry/phase alignment. Offline numerical/Graph evidence does not establish
physical device/DAW/platform/acoustic support. Existing defaults and other named DSP
families remain independent.
