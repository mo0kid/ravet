//! Each umbrella feature exposes its component crate as a module.
//! Run with `--all-features` to check every re-export.

#[cfg(feature = "audio-file")]
#[allow(unused_imports)]
use ravet::audio_file as _;
#[cfg(feature = "clap")]
#[allow(unused_imports)]
use ravet::clap as _;
#[cfg(feature = "core-midi")]
#[allow(unused_imports)]
use ravet::core_midi as _;
#[cfg(feature = "data")]
#[allow(unused_imports)]
use ravet::data as _;
#[cfg(feature = "device")]
#[allow(unused_imports)]
use ravet::device as _;
#[cfg(feature = "graph")]
#[allow(unused_imports)]
use ravet::graph as _;
#[cfg(feature = "host")]
#[allow(unused_imports)]
use ravet::host as _;
#[cfg(feature = "instrument")]
#[allow(unused_imports)]
use ravet::instrument as _;
#[cfg(feature = "media")]
#[allow(unused_imports)]
use ravet::media as _;
#[cfg(feature = "metal")]
#[allow(unused_imports)]
use ravet::metal as _;
#[cfg(feature = "midi-device")]
#[allow(unused_imports)]
use ravet::midi_device as _;
#[cfg(feature = "native")]
#[allow(unused_imports)]
use ravet::native as _;
#[cfg(feature = "native-dsp")]
#[allow(unused_imports)]
use ravet::native_dsp as _;
#[cfg(feature = "osc")]
#[allow(unused_imports)]
use ravet::osc as _;
#[cfg(feature = "plugin")]
#[allow(unused_imports)]
use ravet::plugin as _;
#[cfg(feature = "preset")]
#[allow(unused_imports)]
use ravet::preset as _;
#[cfg(feature = "runtime")]
#[allow(unused_imports)]
use ravet::runtime as _;
#[cfg(feature = "runtime-ffi")]
#[allow(unused_imports)]
use ravet::runtime_ffi as _;
#[cfg(feature = "state")]
#[allow(unused_imports)]
use ravet::state as _;
#[cfg(feature = "webview")]
#[allow(unused_imports)]
use ravet::webview as _;
#[allow(unused_imports)]
use ravet::{foundation as _, signal as _, surface as _};

#[test]
fn core_components_are_always_available() {
    let _ = core::any::type_name::<ravet::foundation::ProcessStatus>();
}
