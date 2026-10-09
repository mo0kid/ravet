# 0036 — Independent consumer project starters

Status: implemented for the explicit offline gain starter.

Public reusable templates may live in RAVET; generated consumers stay outside
its repository/workspace. Require a new explicit destination with an existing
parent, caller package name and licence metadata. Exclusive mkdir prevents
overwriting an existing consumer. A separate `[workspace]` and publish-disabled
manifest keep ownership clear; no workspace mutation or build/network/device
action is implicit.

The starter consumes local RAVET/public gain APIs and demonstrates lifecycle,
borrowed audio and timed events. It does not generate proprietary assets/wrappers
or embed consumer policy. Absolute dependency paths and a separately generated
lockfile make the local source relationship explicit. Generator boundary tests
and an actual external offline build/run validate this limited starter; further
formats, device/UI applications and production release templates remain open.
