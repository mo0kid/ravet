# Runtime contract automation

The [runtime workflow](../.github/workflows/runtime.yml) configures macOS,
Linux and Windows workspace checks on pushes, pull requests and manual dispatch.
Each runner installs stable Rust plus formatting/lint tools, tests all workspace
targets with all Cargo features (including optional compressed audio fixtures) and Rustdoc examples, and builds warning-free API documentation. Linux
installs ALSA development headers/pkg-config for optional audio/MIDI backends.
Cargo concurrency is two jobs. Jobs fail independently and time out explicitly.

A separate macOS job builds the local development-signed gain and sine CLAP bundles and
loads them through their public ABI fixtures. The sine fixture exercises timed
MIDI, level recall and prepared callback allocation alongside repeated activation.
The own gain bundle also runs a guarded native editor child/lifecycle fixture. Licensed external SDK wrappers,
production signing/notarization, DAW automation, physical audio/MIDI hardware,
native window/assistive-tech checks and timing/deadline measurements are separate
validation jobs requiring their actual resources. The workflow does not install
plug-ins globally, publish artifacts or contact product authorization services.

Configuration is committed source, not an executed-platform support claim.
Current local macOS contract, native format/editor and REAPER evidence is listed
in [work status](WORK_STATUS.md). GitHub matrix execution remains pending until
these changes reach the owner's selected remote workflow. Do not infer Windows/
Linux runtime behavior or production reliability from this matrix declaration.
