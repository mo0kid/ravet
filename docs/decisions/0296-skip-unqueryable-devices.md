# 0296: Leave unqueryable devices out of device lists

**Date:** 5 November 2026  
**Status:** Implemented in `ravet-device` (`output_devices`, `input_devices`,
`output_device`, `input_device`). Evidence: Arch Linux PipeWire/ALSA run below;
macOS listing unchanged. No new device, platform or deadline qualification.

## Context

`output_devices()` and `input_devices()` read every CPAL device's ID, name and
supported configurations and returned the first error. On Linux the CPAL ALSA
host enumerates hardware PCMs (`sysdefault:`, `front:`, `hw:`, `plughw:`)
alongside the sound-server PCMs. Querying a hardware PCM that another
application holds open fails with `DeviceBusy`. During the 0.1.0 standalone
evidence run on Arch Linux, Ardour held the onboard `hw:PCH` analog PCM, so
`ravet-device-probe --list` failed with `DeviceBusy`, and the DW-303 standalone
application, which lists devices when it starts, exited before opening a window.
The PipeWire PCM it would have used was available throughout.

## Decision

A device whose ID, name or configuration query fails is omitted from
`output_devices()`/`input_devices()`. Only the host-level `output_devices()`/
`input_devices()` iterator error remains an error. `output_device(Some(id))` and
`input_device(Some(id))` skip devices whose ID cannot be read while searching for
the requested ID, and still return `None` when it is not found, with no fallback.

The omission is silent: a list cannot say why a device is missing. A caller that
needs the reason can open the device by its exact ID and receive the CPAL error.
Opening a device is unchanged; a device that is busy at open time still fails.

## Validation and limits

On Arch Linux (PipeWire 1.6.9, ALSA host), with `hw:PCH` held by Ardour, the
list returned 19 devices (the null, JACK, PipeWire, PulseAudio and default PCMs,
the S/PDIF and HDMI PCMs) and omitted the busy analog PCMs; DW-303 then started
and played through `alsa:pipewire`. The macOS list is unchanged on this machine.
`ravet-device`'s existing tests pass; there is no device-free test that makes
CPAL report a busy device. See the [device guide](../audio-devices.md) and the
[work status](../WORK_STATUS.md#release-010-standalone-win-linux).
