# 0096: Prepared damped mono Schroeder-style reverb

Status: implemented synthetic fixed-network one-channel f64 profile.

## Decision

Prepare four parallel lowpass-feedback combs averaged into two serial canonical
allpasses. Use explicit rate/room/nominal decay/damping-pole/wet bounds and own
rounded delay times. The allpass recurrence writes input+g*output; the existing
DelayDiffuser has a different non-unity transfer function and is not substituted.
All delays allocate during construction and reset/processing retain storage.
Only wet mix changes live; it preserves history without smoothing, including
advancing the wet network at dry mix. Reprepare other settings off the callback.

Mathematical references: Julius O. Smith's
[Schroeder allpass sections](https://www.dsprelated.com/freebooks/pasp/Schroeder_Allpass_Sections.html)
and [lowpass-feedback comb](https://www.dsprelated.com/freebooks/pasp/Lowpass_Feedback_Comb_Filter.html).
No reference source/asset/dependency is incorporated. This synthetic mono effect
has no physical-room or exact overall T60 claim; nominal decay describes each
comb's DC feedback attenuation. Stereo/freeze/modulation, headroom, denormals,
nonfinite sanitization and native deadline evidence remain separate work.

## Evidence

Three groups compare impulse/noise/silence with an independently time-indexed
network across rates/room/mix/decay/damping, check first arrival, measured finite
fixture decay, partition identity, reset, dry-preserved histories, rejected live
mix/configuration, maximum delay lengths, heap-free loops and nonfinite recovery.
These numerical tests do not establish perceptual quality or device deadlines.
