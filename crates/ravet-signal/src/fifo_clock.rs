/// Prepared FIFO clock-correction configuration. Positive occupancy error asks
/// the resampler to consume input faster per output sample. This proportional
/// controller has a steady occupancy offset under constant drift, rather than
/// an integral estimator. It cannot correct drift outside maximum_ppm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FifoClockConfig {
    /// 2..=131072 frames, including any caller-owned prefetched input.
    pub capacity_frames: usize,
    /// Strictly inside capacity: 1..capacity_frames.
    pub target_frames: usize,
    /// Finite (0,10000] ppm, matching a prepared variable resampler bound.
    pub maximum_ppm: f64,
    /// Finite 0.001..=1000 ppm correction per frame of filtered error.
    pub ppm_per_frame: f64,
    /// Finite 0..=60 seconds. Zero disables measurement smoothing.
    pub smoothing_seconds: f64,
    /// Finite 0.001..=1000000 ppm/second maximum correction slew.
    pub slew_ppm_per_second: f64,
}
impl Default for FifoClockConfig {
    fn default() -> Self {
        Self {
            capacity_frames: 4096,
            target_frames: 2048,
            maximum_ppm: 2000.,
            ppm_per_frame: 5.,
            smoothing_seconds: 0.25,
            slew_ppm_per_second: 2000.,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FifoClockError;
impl std::fmt::Display for FifoClockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid FIFO clock configuration or observation")
    }
}
impl std::error::Error for FifoClockError {}
/// Fixed-state FIFO depth feedback controller, independent of devices/threads.
/// Caller supplies complete queued+prefetched input occupancy and elapsed output
/// clock time. No allocation, clock reads or automatic resampling. Constructor
/// starts at target occupancy/zero correction; reset returns there.
/// Clamp and slew bounds do not guarantee stability for arbitrary configurations,
/// callback jitter, dropped input or hardware drift. No native deadline claim.
#[derive(Clone, Debug)]
pub struct FifoClockController {
    config: FifoClockConfig,
    filtered: f64,
    correction: f64,
}
impl FifoClockController {
    pub fn new(config: FifoClockConfig) -> Result<Self, FifoClockError> {
        if !(2..=131072).contains(&config.capacity_frames)
            || config.target_frames == 0
            || config.target_frames >= config.capacity_frames
            || !config.maximum_ppm.is_finite()
            || config.maximum_ppm <= 0.
            || config.maximum_ppm > 10000.
            || !config.ppm_per_frame.is_finite()
            || !(0.001..=1000.).contains(&config.ppm_per_frame)
            || !config.smoothing_seconds.is_finite()
            || !(0. ..=60.).contains(&config.smoothing_seconds)
            || !config.slew_ppm_per_second.is_finite()
            || !(0.001..=1000000.).contains(&config.slew_ppm_per_second)
        {
            return Err(FifoClockError);
        }
        Ok(Self {
            filtered: config.target_frames as f64,
            correction: 0.,
            config,
        })
    }
    pub fn config(&self) -> FifoClockConfig {
        self.config
    }
    pub fn correction_ppm(&self) -> f64 {
        self.correction
    }
    pub fn filtered_frames(&self) -> f64 {
        self.filtered
    }
    pub fn reset(&mut self) {
        self.filtered = self.config.target_frames as f64;
        self.correction = 0.;
    }
    /// Occupancy <=prepared capacity, elapsed finite (0,10] seconds. Reject
    /// before mutation. Exponential smoothing, proportional clamp and bounded
    /// slew use the supplied duration; positive correction drains faster.
    pub fn observe(
        &mut self,
        queued_frames: usize,
        elapsed_seconds: f64,
    ) -> Result<f64, FifoClockError> {
        if queued_frames > self.config.capacity_frames
            || !elapsed_seconds.is_finite()
            || elapsed_seconds <= 0.
            || elapsed_seconds > 10.
        {
            return Err(FifoClockError);
        }
        let fraction = if self.config.smoothing_seconds == 0. {
            1.
        } else {
            -(-elapsed_seconds / self.config.smoothing_seconds).exp_m1()
        };
        self.filtered += fraction * (queued_frames as f64 - self.filtered);
        let desired = ((self.filtered - self.config.target_frames as f64)
            * self.config.ppm_per_frame)
            .clamp(-self.config.maximum_ppm, self.config.maximum_ppm);
        let limit = self.config.slew_ppm_per_second * elapsed_seconds;
        self.correction = (self.correction + (desired - self.correction).clamp(-limit, limit))
            .clamp(-self.config.maximum_ppm, self.config.maximum_ppm);
        Ok(self.correction)
    }
}
