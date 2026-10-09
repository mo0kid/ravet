# 0099: Native VST3 meters through host output parameters

Status: implemented optional macOS gain editor/reference handoff profile.

## Decision

Register twelve hidden/read-only/nonautomatable telemetry parameters only with
the native editor option, retaining default DSP-only registration. Stable IDs
10..21 form peak/RMS/diagnostic triples for input L/R/output L/R; program ID3
retains its ordering before telemetry. Audio exclusively copies existing native
meter records and asks host-owned output queues to deliver last-sample values.
No SDK messages/locks/framework heap storage or DSP pointer sharing is introduced.
Host queue allocation/timing/availability remains the host's contract. Missing
or rejecting queues lose telemetry without failing audio; empty/fault/stop retain.
Levels saturate at four, diagnostics reflect nonfinite samples; native audio and
f64 meter precision remain unchanged. Host delivery is per parameter, without
claiming atomic complete-block UI snapshots. State records exclude telemetry.

Rust UI-only JSON validates normalized host values and renders four local
AccessibleMeter controls. Initial size becomes 480x480; existing resize bounds
remain. Native editor lifetime/edit ownership rules are unchanged. AU/AAX/LV2
meter handoffs, REAPER and actual assistive technology remain separate validation.

## Evidence

Rust checks meter JSON scaling/flags/bounds/atomic rejection and known edit IDs.
SDK-side f32/f64 contracts exercise all twelve output points/metadata/controller
forwarding and hosts without queues. The actual exported module's f32 processor
feeds host queues, its actual controller, Rust snapshots and WKWebView across
three editor cycles; checks cover displayed RMS and diagnostic/accessible text.
An initial independent fixture had a newline inside its C++ logging literal;
corrected build passes. No new real-time/platform/DAW claim follows.
