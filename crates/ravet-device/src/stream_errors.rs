use std::sync::atomic::{AtomicU64, Ordering};

/// Shared callback diagnostics; only failure kinds contribute to recovery faults.
#[derive(Default)]
pub(crate) struct StreamErrorCounters {
    pub(crate) backend_errors: AtomicU64,
    pub(crate) xruns: AtomicU64,
    pub(crate) route_changes: AtomicU64,
}

impl StreamErrorCounters {
    pub(crate) fn report(&self, error: &cpal::Error) {
        let counter = match error.kind() {
            cpal::ErrorKind::Xrun => &self.xruns,
            cpal::ErrorKind::DeviceChanged => &self.route_changes,
            _ => &self.backend_errors,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Counters, input::CaptureCounters};

    #[test]
    fn callback_errors_are_classified_in_both_statistics() {
        let cases = [
            (cpal::ErrorKind::Xrun, (0, 1, 0)),
            (cpal::ErrorKind::DeviceChanged, (0, 0, 1)),
            (cpal::ErrorKind::DeviceNotAvailable, (1, 0, 0)),
            (cpal::ErrorKind::DeviceBusy, (1, 0, 0)),
            (cpal::ErrorKind::HostUnavailable, (1, 0, 0)),
            (cpal::ErrorKind::InvalidInput, (1, 0, 0)),
            (cpal::ErrorKind::PermissionDenied, (1, 0, 0)),
            (cpal::ErrorKind::RealtimeDenied, (1, 0, 0)),
            (cpal::ErrorKind::ResourceExhausted, (1, 0, 0)),
            (cpal::ErrorKind::StreamInvalidated, (1, 0, 0)),
            (cpal::ErrorKind::UnsupportedConfig, (1, 0, 0)),
            (cpal::ErrorKind::UnsupportedOperation, (1, 0, 0)),
            (cpal::ErrorKind::BackendError, (1, 0, 0)),
            (cpal::ErrorKind::Other, (1, 0, 0)),
        ];
        for (kind, expected) in cases {
            let output = Counters::default();
            let input = CaptureCounters::default();
            // Classification follows the kind, never diagnostic message text.
            let error = cpal::Error::with_message(kind, "xrun / route changed");
            for count in 1..=2 {
                output.stream_errors.report(&error);
                input.report_error(&error);
                assert_eq!(
                    output.snapshot(),
                    crate::OutputStatistics {
                        backend_errors: expected.0 * count,
                        xruns: expected.1 * count,
                        route_changes: expected.2 * count,
                        ..Default::default()
                    },
                    "output {kind:?}"
                );
                assert_eq!(
                    input.snapshot(),
                    crate::InputStatistics {
                        backend_errors: expected.0 * count,
                        xruns: expected.1 * count,
                        route_changes: expected.2 * count,
                        ..Default::default()
                    },
                    "input {kind:?}"
                );
            }
        }
    }

    #[test]
    fn callback_counters_wrap_without_heap_work() {
        let counters = StreamErrorCounters::default();
        for counter in [
            &counters.backend_errors,
            &counters.xruns,
            &counters.route_changes,
        ] {
            counter.store(u64::MAX, Ordering::Relaxed);
        }
        let errors = [
            cpal::Error::new(cpal::ErrorKind::Other),
            cpal::Error::new(cpal::ErrorKind::Xrun),
            cpal::Error::new(cpal::ErrorKind::DeviceChanged),
        ];
        let allocation = allocation_counter::measure(|| {
            for error in &errors {
                counters.report(error);
            }
        });
        assert_eq!(allocation.count_total, 0);
        assert_eq!(allocation.count_current, 0);

        assert_eq!(counters.backend_errors.load(Ordering::Relaxed), 0);
        assert_eq!(counters.xruns.load(Ordering::Relaxed), 0);
        assert_eq!(counters.route_changes.load(Ordering::Relaxed), 0);
    }
}
