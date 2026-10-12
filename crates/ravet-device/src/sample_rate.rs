//! Control-thread stream admission; never called from an audio callback.
use cpal::traits::DeviceTrait;

#[cfg(target_os = "macos")]
mod coreaudio;

#[derive(Clone, Copy)]
pub(crate) enum Direction {
    Input,
    Output,
}

/// Read this device's current nominal sample rate on the control thread.
///
/// On macOS CoreAudio this reads `kAudioDevicePropertyNominalSampleRate` for
/// the exact CPAL device UID, independently of its input/output stream formats.
/// Other backends expose only CPAL's current default configuration rate (output
/// when available, otherwise input); that is a backend proxy, not a measured
/// hardware clock. No device is opened or reconfigured. The snapshot can change
/// immediately; stream openers query again after building, before returning.
pub fn device_nominal_sample_rate(device: &cpal::Device) -> Result<u32, cpal::Error> {
    nominal_rate(device, Direction::Output)
}

fn nominal_rate(device: &cpal::Device, direction: Direction) -> Result<u32, cpal::Error> {
    #[cfg(target_os = "macos")]
    if device.id()?.host() == cpal::platform::HostId::CoreAudio {
        return coreaudio::nominal_rate(device);
    }
    let rate = match direction {
        Direction::Input => device.default_input_config()?.sample_rate(),
        Direction::Output if device.supports_output() => {
            device.default_output_config()?.sample_rate()
        }
        Direction::Output => device.default_input_config()?.sample_rate(),
    };
    if rate == 0 {
        return Err(cpal::Error::with_message(
            cpal::ErrorKind::BackendError,
            "Device reported a zero sample rate",
        ));
    }
    Ok(rate)
}

#[derive(Debug)]
pub(crate) enum StreamOpenError {
    Backend(cpal::Error),
    SampleRateMismatch { requested: u32, actual: u32 },
}

pub(crate) fn verified_stream<S>(
    device: &cpal::Device,
    direction: Direction,
    requested: u32,
    build: impl FnOnce() -> Result<S, cpal::Error>,
) -> Result<S, StreamOpenError> {
    verify_result(build(), requested, || nominal_rate(device, direction))
}

fn rate_update_failure(error: &cpal::Error) -> bool {
    // CPAL 0.18.2 CoreAudio's explicit rate-listener diagnostics. Do not turn
    // unrelated device-busy/disconnected/unsupported-format errors into retries.
    error.message().is_some_and(|message| {
        let message = message.to_ascii_lowercase();
        message.contains("sample rate update timed out")
            || message.contains("sample rate update failed")
            || message.contains("sample rate listener disconnected unexpectedly")
    })
}

pub(crate) fn verify_result<S>(
    result: Result<S, cpal::Error>,
    requested: u32,
    read_rate: impl FnOnce() -> Result<u32, cpal::Error>,
) -> Result<S, StreamOpenError> {
    match result {
        Ok(stream) => {
            let actual = read_rate().map_err(StreamOpenError::Backend)?;
            if actual != requested {
                // Includes callback disposal/retirement before the error escapes.
                drop(stream);
                return Err(StreamOpenError::SampleRateMismatch { requested, actual });
            }
            Ok(stream)
        }
        Err(error) => {
            if rate_update_failure(&error)
                && let Ok(actual) = read_rate()
            {
                // A listener timeout can occur even when the nominal rate has
                // already reached the request. Still no usable stream was built.
                return Err(StreamOpenError::SampleRateMismatch { requested, actual });
            }
            Err(StreamOpenError::Backend(error))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct FakeStream<'a>(&'a Cell<bool>);
    impl Drop for FakeStream<'_> {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    fn backend(message: &'static str) -> cpal::Error {
        cpal::Error::with_message(cpal::ErrorKind::DeviceNotAvailable, message)
    }
    #[test]
    fn matching_stream_survives_but_mismatch_and_query_failure_drop_it() {
        let dropped = Cell::new(false);
        let result = verify_result(Ok(FakeStream(&dropped)), 48000, || Ok(48000));
        assert!(result.is_ok());
        assert!(!dropped.get());
        drop(result);
        dropped.set(false);
        assert!(matches!(
            verify_result(Ok(FakeStream(&dropped)), 48000, || Ok(96000)),
            Err(StreamOpenError::SampleRateMismatch {
                requested: 48000,
                actual: 96000
            })
        ));
        assert!(dropped.get());
        dropped.set(false);
        assert!(matches!(
            verify_result(Ok(FakeStream(&dropped)), 48000, || Err(backend(
                "query failed"
            ))),
            Err(StreamOpenError::Backend(_))
        ));
        assert!(dropped.get());
    }
    #[test]
    fn rate_failures_map_known_rates_including_an_equal_rate_timeout() {
        for message in [
            "Sample rate update timed out",
            "CoreAudio: Sample rate update failed",
            "Sample rate listener disconnected unexpectedly",
        ] {
            for actual in [48000, 96000] {
                assert!(matches!(
                    verify_result::<()>(Err(backend(message)), 48000, || Ok(actual)),
                    Err(StreamOpenError::SampleRateMismatch { requested: 48000, actual: rate }) if rate == actual
                ));
            }
            let original = backend(message);
            assert!(matches!(
                verify_result::<()>(Err(original.clone()), 48000, || Err(backend("unavailable"))),
                Err(StreamOpenError::Backend(e)) if e == original
            ));
        }
    }
    #[test]
    fn unrelated_failures_preserve_the_error_without_querying() {
        for message in [
            "Device busy",
            "Sample rate 48000 Hz is not supported",
            "Buffer update timed out",
        ] {
            let original = backend(message);
            assert!(matches!(
                verify_result::<()>(Err(original.clone()), 48000, || panic!("unrelated error queried")),
                Err(StreamOpenError::Backend(e)) if e == original
            ));
        }
    }

    #[test]
    fn second_build_changing_a_shared_clock_rejects_and_drops_both_streams() {
        let input_dropped = Cell::new(false);
        let output_dropped = Cell::new(false);
        let open_pair = || {
            let input = verify_result(Ok(FakeStream(&input_dropped)), 48000, || Ok(48000))?;
            let output = verify_result(Ok(FakeStream(&output_dropped)), 96000, || Ok(96000))?;
            let input = verify_result(Ok(input), 48000, || Ok(96000))?;
            Ok::<_, StreamOpenError>((input, output))
        };
        assert!(matches!(
            open_pair(),
            Err(StreamOpenError::SampleRateMismatch {
                requested: 48000,
                actual: 96000
            })
        ));
        assert!(input_dropped.get() && output_dropped.get());
    }
}
