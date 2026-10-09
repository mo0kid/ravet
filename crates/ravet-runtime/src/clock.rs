//! Wall-clock conversion, independent of product expiry or trial policy.
use std::time::{SystemTime, UNIX_EPOCH};
/// Convert a supplied clock reading to signed Unix milliseconds. Submillisecond
/// fractions truncate toward zero; readings outside i64 return None.
pub fn unix_milliseconds(time: SystemTime) -> Option<i64> {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_millis()).ok(),
        Err(error) => i64::try_from(error.duration().as_millis()).ok().map(|n| -n),
    }
}
/// Current wall-clock milliseconds; wall clocks can move backwards. Use
/// monotonic Instant/Duration for deadlines, independently of this reading.
pub fn now_milliseconds() -> Option<i64> {
    unix_milliseconds(SystemTime::now())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn converts_both_sides_of_epoch_and_truncates_submilliseconds() {
        assert_eq!(unix_milliseconds(UNIX_EPOCH), Some(0));
        assert_eq!(
            unix_milliseconds(UNIX_EPOCH + Duration::from_micros(1234)),
            Some(1)
        );
        assert_eq!(
            unix_milliseconds(UNIX_EPOCH - Duration::from_micros(1234)),
            Some(-1)
        );
        assert!(now_milliseconds().is_some());
    }
}
