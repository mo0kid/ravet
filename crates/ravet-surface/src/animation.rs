//! Scalar UI animation with an explicit consumer clock and no scheduler.
/// Bounded interpolation curves. SmoothStep has zero slope at both endpoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Easing {
    Linear,
    SmoothStep,
    CubicInOut,
}
impl Easing {
    fn apply(self, t: f64) -> f64 {
        match self {
            Self::Linear => t,
            Self::SmoothStep => t * t * (3. - 2. * t),
            Self::CubicInOut => {
                if t < 0.5 {
                    4. * t * t * t
                } else {
                    1. - (-2. * t + 2.).powi(3) / 2.
                }
            }
        }
    }
}
/// Copyable finite scalar tween. Time units are chosen by the consumer and must
/// match start/duration/sample; there is no wall-clock query, allocation or I/O.
/// Sampling may rewind explicitly. Before start returns from; at/after the exact
/// duration returns to. Zero duration snaps at start (for reduced-motion policy).
#[derive(Clone, Copy, Debug)]
pub struct Tween {
    from: f64,
    to: f64,
    start: u64,
    duration: u64,
    easing: Easing,
}
impl Tween {
    pub fn new(from: f64, to: f64, start: u64, duration: u64, easing: Easing) -> Option<Self> {
        if !from.is_finite() || !to.is_finite() {
            return None;
        }
        Some(Self {
            from,
            to,
            start,
            duration,
            easing,
        })
    }
    pub fn sample(self, now: u64) -> f64 {
        if now < self.start {
            return self.from;
        }
        let elapsed = now - self.start;
        if elapsed >= self.duration {
            return self.to;
        }
        let t = self.easing.apply(elapsed as f64 / self.duration as f64);
        // Weighted endpoints avoid overflow from subtracting opposite large values.
        (1. - t) * self.from + t * self.to
    }
    /// Uses elapsed subtraction so start+duration cannot overflow.
    pub fn finished(self, now: u64) -> bool {
        now >= self.start && now - self.start >= self.duration
    }
    /// Start a new tween from the current sample. Invalid endpoints preserve self.
    pub fn retarget(&mut self, to: f64, now: u64, duration: u64, easing: Easing) -> bool {
        let Some(next) = Self::new(self.sample(now), to, now, duration, easing) else {
            return false;
        };
        *self = next;
        true
    }
}
