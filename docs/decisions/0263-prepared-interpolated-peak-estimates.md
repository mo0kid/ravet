# 0263: Prepared interpolated peak estimates

Status: accepted design; isolated qualification pending guarded adoption.

Provide an explicit finite-FIR measurement family, rather than assuming an audio
oversampler has calibrated metering semantics. Design owns immutable CONTROL
coefficients; single-channel and 1..64-channel prepared owners share the same f64
measurement policy, including exact widening of native f32 source audio. Factors
4/8/16, source half-widths 32..512, Kaiser beta 0..16 and integer rates
8000..768000 Hz are caller-selected. No default, hidden profile replacement or
standards certificate is implied. The rate is validated metadata: coefficients
depend on normalized sample coordinates (factor, half-width, beta), so the 216
rate/profile admission combinations are 36 coefficient profiles, and the library
cannot verify the actual source clock. A consumer that must not mix rates compares
`sample_rate()` itself.

Own Kaiser-windowed cardinal-sinc equations produce phase rows. Explicit unity-DC
normalization uses Neumaier summation, with actual rounded DC observations exposed.
Phase zero is exact delayed source; phase p measures t-L+p/F from newest-to-oldest
history. The next not-yet-received source sample lies outside finite support, so
every emitted phase is causally complete. Live readings retain an unresolved
original suffix of min(N,L) and can be below the sample peak. Integer counters
describe source and phase work; there is no long-stream floating phase drift.

Finish belongs to the stopped owner and performs 2L zero steps for nonempty input,
completing the finite support without inflating original counts. Empty finish does
no phase work; repeat finish is idempotent. Input after finish, even empty input,
refuses until reset. Admission reserves source <= floor(u64::MAX/F)-2L before
source/history mutation. Whole channel/prefix/maximum/lifecycle/count preflight
is atomic across the prepared bank. Aggregate array budget is 8 MiB before reserve
and checked again against actual capacities. The cap is a defensive limit kept
unchanged: admitted maxima (64 channels, half-width 512, 16 phases) are about
665,000 bytes, roughly 650 KiB and under 8% of 8 MiB, so the cap cannot be reached
through the public API today. Its arithmetic is exercised only through private
helpers and no public cap-exhaustion test is claimed. Cloning a design is CONTROL
work.

Per-window max-absolute scaling plus compensated weighted accumulation prevents
false intermediate overflow for all finite f64 source magnitudes. Actual final
amplitude overflow remains visible as infinity and a flag. Nonfinite input is
measurement silence with a latched per-channel flag, distinct from the arithmetic
overflow flag; observation does not change audio. Native
underflow/rounding and subnormal state are retained without implicit snapping.
Heap-free observation/reset/finish are bounded in operation count, not qualified
as meeting a device deadline. No Drop finalization or callback report channel.

Independent Decimal90 coefficient vectors, absolute support coordinates,
partitions, one/empty-frame endpoints, near-u64 synthetic counter headroom,
64-channel state preservation and extreme amplitude oracles qualify the contract.
Added oracles use an absolute-coordinate dot product that shares only the prepared
rows: a last-source impulse, emphasised end/start fixtures whose maximum lies
outside the source, an L=512 source longer than its ring, and first/middle/last
channel nonfinite and overflow isolation. Reversal equality is asserted only
because mirror rows (row F-p is row p reversed on taps 0..2L-1) make the grid and
the products identical in this normalized finite discrete design; numerically only
addition order differs. Actual Foundation→Graph→FIR consumers cross
separate/in-place Graph buffers with shared/mutable meter views in both native
precisions, recover stopped meter ownership before finish, read a NaN-poisoned
bank's flags after deactivation but before any reset, and compare all post-reset
FIR outputs with zero-history convolution. These tests qualify the component
composition, not physical audio or an entire platform.

The generated near-Nyquist wavepacket has 36 profile combinations; its half-sample
centre lies on every 4/8/16 grid, so it does not show grid quantization and the
number of distinct sampled peaks is only reported. Exactly twelve combinations are
outside the +0.2/−0.4 dB observation interval, which is not widened. They are
precisely the half-width-32 profiles: over-reading, about +0.2952 dB, at beta 0 and
under-reading from about −0.769 to −1.189524690758779 dB at beta 6, 12 and 16.
Selected long profiles perform better; neither more phases nor wider support
proves universal accuracy. A quarter-rate fixture compares against the
independently derived cos(pi/(4F)) with explicit finite-window tolerances rather
than assuming an ideal sinc identity. The optional FFmpeg 6.1.1 receipt carried
from the original packet retains rate/grid disagreements at 192-kHz source rate
instead of treating a different reference policy as exact truth; its upstream
source attribution has not been newly verified.
K-weighted loudness/gating/LRA remain independent implementation work.

Primary background: [ITU-R BS.1770-5](https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.1770-5-202311-I!!PDF-E.pdf),
[EBU Tech3341 v4](https://tech.ebu.ch/docs/tech/tech3341v4_0.pdf),
and [FFmpeg 6.1.1 filter implementation](https://raw.githubusercontent.com/FFmpeg/FFmpeg/n6.1.1/libavfilter/f_ebur128.c).
No published coefficient table, proprietary source, standards programme fixture or
vendor implementation is included. Public claims describe this implementation's
measured finite profile rather than conformance.
