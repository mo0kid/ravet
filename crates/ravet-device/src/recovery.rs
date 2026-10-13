//! Explicit control-thread recovery for a selected output device.
use crate::{
    MAXIMUM_FRAMES, OpenError, OutputStatistics, OutputStream, open_output, output_device,
};
use ravet_foundation::{AudioLayout, PrepareError, ProcessConfig, Processor};
/// Result of one caller-scheduled recovery service. No timer/thread is created.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputRecoveryStatus {
    Disabled,
    Waiting,
    Running,
    Opened { generation: u64 },
}
/// Invalid selection or ordinary output preparation/backend failure.
#[derive(Debug)]
pub enum OutputRecoveryError {
    InvalidId,
    Open(OpenError),
}
impl std::fmt::Display for OutputRecoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidId => f.write_str("output ID must contain 1..4096 bytes without NUL"),
            Self::Open(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for OutputRecoveryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidId => None,
            Self::Open(error) => Some(error),
        }
    }
}
// CPAL xruns and automatic route changes are telemetry, not rebuild requests.
fn output_unhealthy(output: OutputStatistics) -> bool {
    output.backend_errors != 0 || output.panics != 0
}
fn duplex_unhealthy(input: crate::InputStatistics, output: OutputStatistics) -> bool {
    input.backend_errors != 0 || output_unhealthy(output)
}
trait StreamHandle {
    fn unhealthy(&self) -> bool;
    fn stop(&self);
}
impl StreamHandle for OutputStream {
    fn unhealthy(&self) -> bool {
        let stats = self.statistics();
        output_unhealthy(stats)
    }
    fn stop(&self) {
        let _ = self.pause();
    }
}
impl StreamHandle for crate::DuplexStream {
    fn unhealthy(&self) -> bool {
        let input = self.input_statistics();
        let output = self.output_statistics();
        duplex_unhealthy(input, output)
    }
    fn stop(&self) {
        let _ = self.pause();
    }
}
impl StreamHandle for crate::ClockedDuplexStream {
    fn unhealthy(&self) -> bool {
        let input = self.input_statistics();
        let output = self.output_statistics();
        duplex_unhealthy(input, output)
    }
    fn stop(&self) {
        let _ = self.pause();
    }
}
struct Session<S: StreamHandle> {
    stream: Option<S>,
    enabled: bool,
    generation: u64,
}
impl<S: StreamHandle> Session<S> {
    fn new() -> Self {
        Self {
            stream: None,
            enabled: true,
            generation: 0,
        }
    }
    fn close(&mut self) {
        if let Some(stream) = self.stream.take() {
            stream.stop();
            drop(stream);
        }
    }
    fn service<E>(
        &mut self,
        available: bool,
        open: impl FnOnce() -> Result<S, E>,
    ) -> Result<OutputRecoveryStatus, E> {
        if !self.enabled {
            return Ok(OutputRecoveryStatus::Disabled);
        }
        if !available {
            self.close();
            return Ok(OutputRecoveryStatus::Waiting);
        }
        if self
            .stream
            .as_ref()
            .is_some_and(|stream| !stream.unhealthy())
        {
            return Ok(OutputRecoveryStatus::Running);
        }
        self.close();
        let stream = open()?;
        self.generation = self.generation.wrapping_add(1);
        self.stream = Some(stream);
        Ok(OutputRecoveryStatus::Opened {
            generation: self.generation,
        })
    }
}
impl<S: StreamHandle> Drop for Session<S> {
    fn drop(&mut self) {
        self.close();
    }
}
/// Caller-scheduled output recovery with an exact host-local device ID and fixed
/// f32 configuration. Construct/service/enable/reopen/drop on a control thread.
/// Service explicitly enumerates the device, closes missing/failed streams and
/// opens fresh DSP after return/backend error/panic. CPAL xruns and automatic
/// route changes do not reopen streams. No device fallback, timer,
/// settings changes, state migration, clock compensation or hotplug subscription.
/// The processor factory runs once per open attempt; it owns restored state and
/// preparation failures. Poll cadence/retry backoff belong to the application.
/// Stream destruction/backend work is blocking and has no callback deadline claim.
/// Generic/native hosted plug-ins still need their own thread-correct lifecycle.
pub struct RecoveringOutput<P: Processor + Send + 'static, F: FnMut() -> P> {
    id: String,
    config: crate::cpal::StreamConfig,
    maximum_frames: usize,
    factory: F,
    session: Session<OutputStream>,
    marker: std::marker::PhantomData<fn() -> P>,
}
impl<P: Processor + Send + 'static, F: FnMut() -> P> RecoveringOutput<P, F> {
    /// Validate fixed configuration without opening a device or calling factory.
    /// The first enabled service opens/starts output. IDs are 1..4096 bytes/no NUL.
    pub fn new(
        id: impl Into<String>,
        config: crate::cpal::StreamConfig,
        maximum_frames: usize,
        factory: F,
    ) -> Result<Self, OutputRecoveryError> {
        let id = id.into();
        if id.is_empty() || id.len() > 4096 || id.contains('\0') {
            return Err(OutputRecoveryError::InvalidId);
        }
        let validation = (|| {
            if maximum_frames > MAXIMUM_FRAMES
                || matches!(config.buffer_size, crate::cpal::BufferSize::Fixed(0))
            {
                return Err(PrepareError::InvalidMaximumFrames);
            }
            ProcessConfig::new(
                f64::from(config.sample_rate),
                maximum_frames,
                0,
                AudioLayout::new(0, usize::from(config.channels))?,
            )?;
            Ok(())
        })();
        validation.map_err(|error| OutputRecoveryError::Open(OpenError::Preparation(error)))?;
        Ok(Self {
            id,
            config,
            maximum_frames,
            factory,
            session: Session::new(),
            marker: std::marker::PhantomData,
        })
    }
    pub fn device_id(&self) -> &str {
        &self.id
    }
    /// Disabling immediately pauses/drops the current stream. Reenable schedules
    /// a fresh open at next service, rather than resuming old DSP history.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.session.enabled = enabled;
        if !enabled {
            self.session.close();
        }
    }
    /// Pause/drop now and request fresh output at next enabled service.
    pub fn request_reopen(&mut self) {
        self.session.close();
    }
    pub fn statistics(&self) -> Option<OutputStatistics> {
        self.session.stream.as_ref().map(OutputStream::statistics)
    }
    /// Enumerate exact ID and service once; enumeration errors retain live state.
    /// Removed devices wait; backend errors/panics reopen. CPAL xruns and automatic
    /// route changes do not reopen. Processor block faults
    /// alone do not reopen. A failed open/start leaves no stream for next retry.
    /// Open generations wrap at u64 exhaustion; counters reset for each stream.
    pub fn service(&mut self) -> Result<OutputRecoveryStatus, OutputRecoveryError> {
        if !self.session.enabled {
            return Ok(OutputRecoveryStatus::Disabled);
        }
        let device = output_device(Some(&self.id))
            .map_err(|error| OutputRecoveryError::Open(OpenError::Backend(error)))?;
        self.session.service(device.is_some(), || {
            let device = device.as_ref().expect("available device");
            let stream = open_output(device, self.config, self.maximum_frames, (self.factory)())
                .map_err(OutputRecoveryError::Open)?;
            stream
                .play()
                .map_err(|error| OutputRecoveryError::Open(OpenError::Backend(error)))?;
            Ok(stream)
        })
    }
}
/// Duplex alias of the common recovery status; open generations refer to pairs.
pub type DuplexRecoveryStatus = OutputRecoveryStatus;
/// Duplex configuration/selection/backend error; the shared error representation
/// retains its output-origin name for existing callers.
pub type DuplexRecoveryError = OutputRecoveryError;
/// Exact input/output pair recovery, serviced explicitly on a control thread.
/// CPAL xruns and automatic route changes leave both streams active.
/// Both streams/FIFO are released if either device disappears or either backend
/// fails (or output DSP panics). Every successful reopen uses fresh DSP and FIFO.
/// Nominal rates must match; no drift correction or sample synchronization.
/// Factories own restored state. No fallback, timer, recording or native plug-in
/// scheduling is added. Backend enumeration/open/destruction may block.
pub struct RecoveringDuplex<P: Processor + Send + 'static, F: FnMut() -> P> {
    input_id: String,
    output_id: String,
    input_config: crate::cpal::StreamConfig,
    output_config: crate::cpal::StreamConfig,
    maximum_frames: usize,
    ring_frames: usize,
    factory: F,
    session: Session<crate::DuplexStream>,
    marker: std::marker::PhantomData<fn() -> P>,
}
impl<P: Processor + Send + 'static, F: FnMut() -> P> RecoveringDuplex<P, F> {
    /// Validate exact IDs, channels 1..64, matching positive rates, scratch
    /// 1..65536 and power-of-two FIFO 1..65536 without devices/factory work.
    /// First enabled service opens the pair and starts input before output.
    pub fn new(
        input_id: impl Into<String>,
        output_id: impl Into<String>,
        input_config: crate::cpal::StreamConfig,
        output_config: crate::cpal::StreamConfig,
        maximum_frames: usize,
        ring_frames: usize,
        factory: F,
    ) -> Result<Self, DuplexRecoveryError> {
        let input_id = input_id.into();
        let output_id = output_id.into();
        for id in [&input_id, &output_id] {
            if id.is_empty() || id.len() > 4096 || id.contains('\0') {
                return Err(OutputRecoveryError::InvalidId);
            }
        }
        let validation = (|| {
            if input_config.sample_rate != output_config.sample_rate {
                return Err(PrepareError::InvalidSampleRate);
            }
            if maximum_frames > MAXIMUM_FRAMES
                || !ring_frames.is_power_of_two()
                || ring_frames > 65536
                || matches!(input_config.buffer_size, crate::cpal::BufferSize::Fixed(0))
                || matches!(output_config.buffer_size, crate::cpal::BufferSize::Fixed(0))
            {
                return Err(PrepareError::InvalidMaximumFrames);
            }
            if input_config.channels == 0 || output_config.channels == 0 {
                return Err(PrepareError::UnsupportedLayout);
            }
            ProcessConfig::new(
                f64::from(output_config.sample_rate),
                maximum_frames,
                0,
                AudioLayout::new(
                    usize::from(input_config.channels),
                    usize::from(output_config.channels),
                )?,
            )?;
            Ok(())
        })();
        validation.map_err(|error| OutputRecoveryError::Open(OpenError::Preparation(error)))?;
        Ok(Self {
            input_id,
            output_id,
            input_config,
            output_config,
            maximum_frames,
            ring_frames,
            factory,
            session: Session::new(),
            marker: std::marker::PhantomData,
        })
    }
    pub fn device_ids(&self) -> (&str, &str) {
        (&self.input_id, &self.output_id)
    }
    /// Disable releases both streams immediately; reenable starts fresh at service.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.session.enabled = enabled;
        if !enabled {
            self.session.close();
        }
    }
    pub fn request_reopen(&mut self) {
        self.session.close();
    }
    /// Current pair's input/output counters, sampled independently; reset on reopen.
    pub fn statistics(&self) -> Option<(crate::InputStatistics, OutputStatistics)> {
        self.session
            .stream
            .as_ref()
            .map(|stream| (stream.input_statistics(), stream.output_statistics()))
    }
    /// Service exact pair once; enumeration errors retain existing streams.
    /// Missing either waits; failed open/start leaves no pair for later retry.
    /// CPAL xruns/automatic route changes, FIFO underflow/overflow and ordinary
    /// processor faults alone do not reopen.
    /// Poll cadence/backoff belong to callers; generations wrap at u64 exhaustion.
    pub fn service(&mut self) -> Result<DuplexRecoveryStatus, DuplexRecoveryError> {
        if !self.session.enabled {
            return Ok(OutputRecoveryStatus::Disabled);
        }
        let input = crate::input_device(Some(&self.input_id))
            .map_err(|error| OutputRecoveryError::Open(OpenError::Backend(error)))?;
        // Missing input already requires stopping the pair; do not let a second
        // enumeration failure keep a known-invalid pair running.
        if input.is_none() {
            return self
                .session
                .service(false, || unreachable!("missing input"));
        }
        let output = output_device(Some(&self.output_id))
            .map_err(|error| OutputRecoveryError::Open(OpenError::Backend(error)))?;
        self.session.service(output.is_some(), || {
            let stream = crate::open_duplex(
                input.as_ref().expect("available input"),
                output.as_ref().expect("available output"),
                self.input_config,
                self.output_config,
                self.maximum_frames,
                self.ring_frames,
                (self.factory)(),
            )
            .map_err(OutputRecoveryError::Open)?;
            stream
                .play()
                .map_err(|error| OutputRecoveryError::Open(OpenError::Backend(error)))?;
            Ok(stream)
        })
    }
}
/// Exact-ID clocked pair selection, preparation or native backend failure.
#[derive(Debug)]
pub enum ClockedDuplexRecoveryError {
    InvalidId,
    Preparation(crate::ClockedDuplexError),
    Backend(crate::cpal::Error),
    Open(crate::ClockedDuplexOpenError),
}
impl std::fmt::Display for ClockedDuplexRecoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "clocked duplex recovery: {self:?}")
    }
}
impl std::error::Error for ClockedDuplexRecoveryError {}
/// Caller-scheduled exact input/output pair recovery with bounded clocked
/// conversion. Service/enable/reopen/drop on a control thread. Missing devices,
/// backend errors or output panic release both streams before fresh FIFO,
/// converters/controller and factory-owned DSP are opened. No fallback, settings
/// changes, polling timer, hotplug subscription, state migration or physical
/// fault/deadline guarantee. Ordinary underflow/overflow or DSP faults do not
/// reopen; neither do CPAL xruns or automatic route changes. Native hosted
/// plug-ins retain their own thread/lifecycle obligations.
pub struct RecoveringClockedDuplex<P: Processor + Send + 'static, F: FnMut() -> P> {
    input_id: String,
    output_id: String,
    input_config: crate::cpal::StreamConfig,
    output_config: crate::cpal::StreamConfig,
    options: crate::ClockedDuplexOptions,
    factory: F,
    session: Session<crate::ClockedDuplexStream>,
    marker: std::marker::PhantomData<fn() -> P>,
}
impl<P: Processor + Send + 'static, F: FnMut() -> P> RecoveringClockedDuplex<P, F> {
    /// Validate fixed configs/clock/converter bounds before enumeration, device
    /// work or factory calls. IDs are 1..4096 bytes without NUL. No sinc tables
    /// are prepared here; first enabled service opens/starts fresh streams.
    pub fn new(
        input_id: impl Into<String>,
        output_id: impl Into<String>,
        input_config: crate::cpal::StreamConfig,
        output_config: crate::cpal::StreamConfig,
        options: crate::ClockedDuplexOptions,
        factory: F,
    ) -> Result<Self, ClockedDuplexRecoveryError> {
        let input_id = input_id.into();
        let output_id = output_id.into();
        for id in [&input_id, &output_id] {
            if id.is_empty() || id.len() > 4096 || id.contains('\0') {
                return Err(ClockedDuplexRecoveryError::InvalidId);
            }
        }
        if matches!(input_config.buffer_size, crate::cpal::BufferSize::Fixed(0))
            || matches!(output_config.buffer_size, crate::cpal::BufferSize::Fixed(0))
        {
            return Err(ClockedDuplexRecoveryError::Preparation(
                crate::ClockedDuplexError::InvalidConfiguration,
            ));
        }
        crate::clocked::validate_configuration(
            input_config.sample_rate,
            output_config.sample_rate,
            usize::from(input_config.channels),
            usize::from(output_config.channels),
            options.clock.capacity_frames,
            options,
        )
        .map_err(ClockedDuplexRecoveryError::Preparation)?;
        Ok(Self {
            input_id,
            output_id,
            input_config,
            output_config,
            options,
            factory,
            session: Session::new(),
            marker: std::marker::PhantomData,
        })
    }
    pub fn device_ids(&self) -> (&str, &str) {
        (&self.input_id, &self.output_id)
    }
    /// Disable releases both streams now; reenable opens fresh at next service.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.session.enabled = enabled;
        if !enabled {
            self.session.close();
        }
    }
    /// Release the pair now; a subsequent enabled service opens fresh state.
    pub fn request_reopen(&mut self) {
        self.session.close();
    }
    /// Current independent atomic input/output/clock counters; reset on reopen.
    pub fn statistics(
        &self,
    ) -> Option<(
        crate::InputStatistics,
        OutputStatistics,
        crate::ClockedDuplexStatistics,
    )> {
        self.session.stream.as_ref().map(|stream| {
            (
                stream.input_statistics(),
                stream.output_statistics(),
                stream.clock_statistics(),
            )
        })
    }
    /// Enumeration errors retain a current pair. Missing either waits; failed
    /// open/start leaves no pair for later retry. Caller owns cadence/backoff;
    /// successful pair generations wrap at u64 exhaustion.
    pub fn service(&mut self) -> Result<DuplexRecoveryStatus, ClockedDuplexRecoveryError> {
        if !self.session.enabled {
            return Ok(OutputRecoveryStatus::Disabled);
        }
        let input = crate::input_device(Some(&self.input_id))
            .map_err(ClockedDuplexRecoveryError::Backend)?;
        if input.is_none() {
            return self
                .session
                .service(false, || unreachable!("missing input"));
        }
        let output =
            output_device(Some(&self.output_id)).map_err(ClockedDuplexRecoveryError::Backend)?;
        self.session.service(output.is_some(), || {
            let stream = crate::open_clocked_duplex(
                input.as_ref().expect("available input"),
                output.as_ref().expect("available output"),
                self.input_config,
                self.output_config,
                self.options.clock.capacity_frames,
                (self.factory)(),
                self.options,
            )
            .map_err(ClockedDuplexRecoveryError::Open)?;
            stream.play().map_err(ClockedDuplexRecoveryError::Backend)?;
            Ok(stream)
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    #[test]
    fn callback_notifications_preserve_generation_but_failures_and_panics_reopen() {
        struct Diagnostics {
            input: Option<Rc<crate::input::CaptureCounters>>,
            output: Rc<crate::Counters>,
        }
        impl StreamHandle for Diagnostics {
            fn unhealthy(&self) -> bool {
                let output = self.output.snapshot();
                match &self.input {
                    Some(input) => duplex_unhealthy(input.snapshot(), output),
                    None => output_unhealthy(output),
                }
            }
            fn stop(&self) {}
        }
        // Matching-rate and clocked duplex share this same health predicate.
        for duplex in [false, true] {
            for fault in ["input", "output", "panic"] {
                if fault == "input" && !duplex {
                    continue;
                }
                let mut session = Session::new();
                let open = || {
                    Ok::<_, ()>(Diagnostics {
                        input: duplex.then(Rc::default),
                        output: Rc::default(),
                    })
                };
                assert_eq!(
                    session.service(true, open).unwrap(),
                    OutputRecoveryStatus::Opened { generation: 1 }
                );
                let stream = session.stream.as_ref().unwrap();
                let input = stream.input.clone();
                let output = Rc::clone(&stream.output);
                for kind in [cpal::ErrorKind::Xrun, cpal::ErrorKind::DeviceChanged] {
                    let error = cpal::Error::new(kind);
                    output.stream_errors.report(&error);
                    if let Some(input) = &input {
                        input.report_error(&error);
                    }
                    assert_eq!(
                        session
                            .service::<()>(true, || panic!("notification must not rebuild"))
                            .unwrap(),
                        OutputRecoveryStatus::Running
                    );
                    assert_eq!(session.generation, 1);
                }
                let failure = cpal::Error::new(cpal::ErrorKind::DeviceNotAvailable);
                match fault {
                    "input" => input.as_ref().unwrap().report_error(&failure),
                    "output" => output.stream_errors.report(&failure),
                    "panic" => {
                        output.panics.store(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    _ => unreachable!(),
                }
                assert_eq!(
                    session.service(true, open).unwrap(),
                    OutputRecoveryStatus::Opened { generation: 2 }
                );
                assert_eq!(
                    session
                        .service::<()>(true, || panic!("fresh stream must not rebuild"))
                        .unwrap(),
                    OutputRecoveryStatus::Running
                );
            }
        }
    }

    struct Silence;
    impl Processor for Silence {
        fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
            Ok(())
        }
        fn process(
            &mut self,
            block: &mut ravet_foundation::ProcessBlock<'_, '_, '_>,
        ) -> ravet_foundation::ProcessStatus {
            block.clear_outputs();
            ravet_foundation::ProcessStatus::Silence
        }
        fn reset(&mut self) {}
        fn deactivate(&mut self) {}
    }
    #[test]
    fn construction_validates_without_opening_or_calling_factory() {
        let calls = Cell::new(0);
        let factory = || {
            calls.set(calls.get() + 1);
            Silence
        };
        let config = crate::cpal::StreamConfig {
            channels: 2,
            sample_rate: 48000,
            buffer_size: crate::cpal::BufferSize::Default,
        };
        for id in [String::new(), "x".repeat(4097), "a\0b".to_owned()] {
            assert!(matches!(
                RecoveringOutput::new(id, config, 64, factory),
                Err(OutputRecoveryError::InvalidId)
            ));
        }
        for (channels, rate, frames, buffer_size) in [
            (0, 48000, 64, crate::cpal::BufferSize::Default),
            (65, 48000, 64, crate::cpal::BufferSize::Default),
            (2, 0, 64, crate::cpal::BufferSize::Default),
            (2, 48000, 0, crate::cpal::BufferSize::Default),
            (2, 48000, 65537, crate::cpal::BufferSize::Default),
            (2, 48000, 64, crate::cpal::BufferSize::Fixed(0)),
        ] {
            assert!(
                RecoveringOutput::new(
                    "exact-device",
                    crate::cpal::StreamConfig {
                        channels,
                        sample_rate: rate,
                        buffer_size
                    },
                    frames,
                    factory
                )
                .is_err()
            );
        }
        let mut valid = RecoveringOutput::new("exact-device", config, 64, factory).unwrap();
        assert_eq!(valid.device_id(), "exact-device");
        assert!(valid.statistics().is_none());
        valid.set_enabled(false);
        assert_eq!(valid.service().unwrap(), OutputRecoveryStatus::Disabled);
        valid.request_reopen();
        drop(valid);
        assert_eq!(calls.get(), 0);
    }
    struct Fake {
        fault: Rc<Cell<bool>>,
        events: Rc<RefCell<Vec<&'static str>>>,
    }
    #[test]
    fn duplex_constructor_validates_pair_without_native_or_factory_work() {
        let calls = Cell::new(0);
        let factory = || {
            calls.set(calls.get() + 1);
            Silence
        };
        let config = crate::cpal::StreamConfig {
            channels: 2,
            sample_rate: 48000,
            buffer_size: crate::cpal::BufferSize::Default,
        };
        assert!(matches!(
            RecoveringDuplex::new("", "output", config, config, 64, 128, factory),
            Err(OutputRecoveryError::InvalidId)
        ));
        assert!(matches!(
            RecoveringDuplex::new("input", "a\0b", config, config, 64, 128, factory),
            Err(OutputRecoveryError::InvalidId)
        ));
        for frames in [0, 3, 65537] {
            assert!(
                RecoveringDuplex::new("input", "output", config, config, 64, frames, factory)
                    .is_err()
            );
        }
        for (input, output, maximum) in [
            (
                crate::cpal::StreamConfig {
                    sample_rate: 44100,
                    ..config
                },
                config,
                64,
            ),
            (
                crate::cpal::StreamConfig {
                    channels: 0,
                    ..config
                },
                config,
                64,
            ),
            (
                config,
                crate::cpal::StreamConfig {
                    channels: 65,
                    ..config
                },
                64,
            ),
            (
                crate::cpal::StreamConfig {
                    buffer_size: crate::cpal::BufferSize::Fixed(0),
                    ..config
                },
                config,
                64,
            ),
            (
                config,
                crate::cpal::StreamConfig {
                    sample_rate: 0,
                    ..config
                },
                64,
            ),
            (config, config, 0),
            (config, config, 65537),
        ] {
            assert!(
                RecoveringDuplex::new("input", "output", input, output, maximum, 128, factory)
                    .is_err()
            );
        }
        let mut pair =
            RecoveringDuplex::new("input", "output", config, config, 64, 128, factory).unwrap();
        assert_eq!(pair.device_ids(), ("input", "output"));
        assert!(pair.statistics().is_none());
        pair.set_enabled(false);
        assert_eq!(pair.service().unwrap(), DuplexRecoveryStatus::Disabled);
        pair.request_reopen();
        drop(pair);
        assert_eq!(calls.get(), 0);
    }
    impl StreamHandle for Fake {
        fn unhealthy(&self) -> bool {
            self.fault.get()
        }
        fn stop(&self) {
            self.events.borrow_mut().push("stop");
        }
    }
    impl Drop for Fake {
        fn drop(&mut self) {
            self.events.borrow_mut().push("drop");
        }
    }
    #[test]
    fn removed_faulted_retry_disable_reenable_and_drop_lifecycle() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let fault = Rc::new(Cell::new(false));
        let mut session = Session::new();
        let open = || {
            events.borrow_mut().push("open");
            Ok::<_, &str>(Fake {
                events: events.clone(),
                fault: fault.clone(),
            })
        };
        assert_eq!(
            session.service(false, open).unwrap(),
            OutputRecoveryStatus::Waiting
        );
        assert!(events.borrow().is_empty());
        assert_eq!(
            session.service(true, open).unwrap(),
            OutputRecoveryStatus::Opened { generation: 1 }
        );
        assert_eq!(
            session
                .service::<&str>(true, || panic!("healthy stream must not reopen"))
                .unwrap(),
            OutputRecoveryStatus::Running
        );
        fault.set(true);
        assert_eq!(
            session.service(true, || Err("failed open")),
            Err("failed open")
        );
        assert_eq!(&*events.borrow(), &["open", "stop", "drop"]);
        assert!(session.stream.is_none());
        fault.set(false);
        assert_eq!(
            session.service(true, open).unwrap(),
            OutputRecoveryStatus::Opened { generation: 2 }
        );
        assert_eq!(
            session
                .service::<&str>(false, || panic!("missing must not open"))
                .unwrap(),
            OutputRecoveryStatus::Waiting
        );
        assert_eq!(
            &*events.borrow(),
            &["open", "stop", "drop", "open", "stop", "drop"]
        );
        session.enabled = false;
        assert_eq!(
            session
                .service::<&str>(true, || panic!("disabled must not open"))
                .unwrap(),
            OutputRecoveryStatus::Disabled
        );
        session.enabled = true;
        assert_eq!(
            session.service(true, open).unwrap(),
            OutputRecoveryStatus::Opened { generation: 3 }
        );
        drop(session);
        assert_eq!(
            &*events.borrow(),
            &[
                "open", "stop", "drop", "open", "stop", "drop", "open", "stop", "drop"
            ]
        );
    }
}
