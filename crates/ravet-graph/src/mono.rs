//! Independently evolving mono histories sharing one caller-owned control object.
//!
//! Status: draft, isolated packet (ADR 0260 is proposed, not integrated). This
//! module contains no `unsafe` code; its guarantees are ordinary Rust ownership.
use crate::GraphSample;
use ravet_foundation::{
    PrepareError, ProcessBlock, ProcessConfig, ProcessError, ProcessStatus, Processor, Tail,
};
const OWNER_BYTES: usize = 4 * 1024 * 1024;
/// Foreign mono kernel obligations match Processor: finite work, no callback
/// allocation/locks/I/O, prepare failures inactive.
///
/// The framework hands every kernel the same `&C` and never mutates `C` while
/// kernels are prepared: `shared_controls_mut_stopped` deactivates first. Shared
/// access rules out overlapping `&mut` access to `C`, but it does **not** make `C`
/// immutable. If `C` contains interior mutability (`Cell`, atomics, locks), a
/// kernel may change what later channels read within the same frame. Keeping `C`
/// effectively immutable while prepared, or making any interior mutation
/// deterministic and real-time safe, is an implementor obligation that the framework
/// neither checks nor enforces.
///
/// Arithmetic policy is likewise the implementor's: the framework does not filter,
/// clamp, reject or flush nonfinite or denormal samples and does not inspect kernel
/// output.
pub trait SharedMonoKernel<S: GraphSample, C> {
    fn prepare(
        &mut self,
        sample_rate: f64,
        maximum_frames: usize,
        controls: &C,
    ) -> Result<(), PrepareError>;
    fn process_sample(&mut self, input: S, controls: &C) -> S;
    fn reset(&mut self, controls: &C);
    fn deactivate(&mut self);
    fn latency_samples(&self) -> u32 {
        0
    }
    fn tail(&self) -> Tail {
        Tail::None
    }
}
/// Original controls/factory, exact successfully constructed kernel prefix and
/// first factory error. Effects already performed by the factory do not roll back.
pub struct MonoFactoryFailure<C, K, F, E> {
    pub controls: C,
    pub kernels: Vec<K>,
    pub factory: F,
    pub error: MonoFactoryError<E>,
}
#[derive(Debug, PartialEq, Eq)]
pub enum MonoFactoryError<E> {
    Channels,
    Frames,
    Capacity,
    Factory { index: usize, error: E },
}
/// One prepared control object shared by reference and 1..64 separate native histories.
/// The factory returns inactive kernels; successful kernels may own additional
/// caller-defined storage outside the checked 4MiB kernel-Vec payload cap.
/// Effect-only, no events/auxiliary/generated events; construction/preparation
/// are CONTROL. Bypass returns exact dry input and freezes all histories.
///
/// Rendering paths: `Processor::process` validates the whole block and, on a
/// refusal, silences the active output frames (backing tails and histories are
/// untouched); `process_sample`, `process_frame`, `process_planar` and
/// `process_in_place` preflight the whole call and return an error without writing
/// outputs or advancing any kernel. Nonfinite samples are passed to the kernels and
/// through bypass unchanged; what a kernel makes of them (including leaking into
/// its own history) is the kernel's policy.
///
/// `prepare` deactivates first, so a refused or failed `prepare` leaves the
/// duplicator unprepared; kernel histories are whatever the foreign kernels keep
/// across deactivation. Dropping an active duplicator deactivates the prepared
/// kernel prefix once, in reverse order, before the kernels are dropped.
/// `prepare_failure()` is cleared only by the next `prepare`, not by
/// `deactivate` or `shared_controls_mut_stopped`.
pub struct MonoDuplicator<S: GraphSample, C, K: SharedMonoKernel<S, C>> {
    controls: Option<C>,
    kernels: Vec<K>,
    maximum_frames: usize,
    config: Option<ProcessConfig>,
    prepared: usize,
    bypassed: bool,
    prepare_failure: Option<(usize, PrepareError)>,
    sample: std::marker::PhantomData<S>,
}
impl<S: GraphSample, C, K: SharedMonoKernel<S, C>> MonoDuplicator<S, C, K> {
    /// The allocator-failure branch of the kernel reservation and the post-
    /// reservation capacity check cannot be reached by ordinary tests (an exact
    /// reservation of at most the admitted payload); they are defensive refusals
    /// without dedicated coverage.
    pub fn new<F, E>(
        controls: C,
        channels: usize,
        maximum_frames: usize,
        mut factory: F,
    ) -> Result<Self, MonoFactoryFailure<C, K, F, E>>
    where
        F: FnMut(usize, &C) -> Result<K, E>,
    {
        let error = if channels == 0 || channels > 64 {
            Some(MonoFactoryError::Channels)
        } else if maximum_frames == 0 || maximum_frames > 65536 {
            Some(MonoFactoryError::Frames)
        } else if channels
            .checked_mul(std::mem::size_of::<K>())
            .is_none_or(|n| n > OWNER_BYTES)
        {
            Some(MonoFactoryError::Capacity)
        } else {
            None
        };
        if let Some(error) = error {
            return Err(MonoFactoryFailure {
                controls,
                kernels: Vec::new(),
                factory,
                error,
            });
        }
        let mut kernels = Vec::new();
        if kernels.try_reserve_exact(channels).is_err()
            || kernels
                .capacity()
                .checked_mul(std::mem::size_of::<K>())
                .is_none_or(|n| n > OWNER_BYTES)
        {
            return Err(MonoFactoryFailure {
                controls,
                kernels,
                factory,
                error: MonoFactoryError::Capacity,
            });
        }
        for i in 0..channels {
            match factory(i, &controls) {
                Ok(k) => kernels.push(k),
                Err(error) => {
                    return Err(MonoFactoryFailure {
                        controls,
                        kernels,
                        factory,
                        error: MonoFactoryError::Factory { index: i, error },
                    });
                }
            }
        }
        Ok(Self {
            controls: Some(controls),
            kernels,
            maximum_frames,
            config: None,
            prepared: 0,
            bypassed: false,
            prepare_failure: None,
            sample: std::marker::PhantomData,
        })
    }
    pub fn channels(&self) -> usize {
        self.kernels.len()
    }
    pub fn shared_controls(&self) -> &C {
        self.controls.as_ref().expect("owned")
    }
    /// Invalidates preparation first; changing controls requires reprepare.
    pub fn shared_controls_mut_stopped(&mut self) -> &mut C {
        self.deactivate();
        self.controls.as_mut().expect("owned")
    }
    pub fn kernel(&self, channel: usize) -> Option<&K> {
        self.kernels.get(channel)
    }
    /// Index and error of the last failed kernel `prepare`. Cleared by the next
    /// `prepare` only; `deactivate` and `shared_controls_mut_stopped` keep it.
    pub fn prepare_failure(&self) -> Option<(usize, PrepareError)> {
        self.prepare_failure
    }
    pub fn set_bypassed(&mut self, value: bool) {
        self.bypassed = value;
    }
    pub fn is_bypassed(&self) -> bool {
        self.bypassed
    }
    fn ready(&self) -> Result<ProcessConfig, ProcessError> {
        self.config.ok_or(ProcessError::NotPrepared)
    }
    pub fn process_sample(&mut self, channel: usize, input: S) -> Result<S, ProcessError> {
        self.ready()?;
        if channel >= self.kernels.len() {
            return Err(ProcessError::LayoutMismatch);
        }
        Ok(if self.bypassed {
            input
        } else {
            self.kernels[channel].process_sample(input, self.controls.as_ref().expect("owned"))
        })
    }
    pub fn process_frame(&mut self, input: &[S], output: &mut [S]) -> Result<(), ProcessError> {
        self.ready()?;
        if input.len() != self.channels() || output.len() != self.channels() {
            return Err(ProcessError::LayoutMismatch);
        }
        for (i, (x, y)) in input.iter().zip(output).enumerate() {
            *y = if self.bypassed {
                *x
            } else {
                self.kernels[i].process_sample(*x, self.controls.as_ref().expect("owned"))
            };
        }
        Ok(())
    }
    fn planar_preflight(
        &self,
        inputs: &[&[S]],
        outputs: &[&mut [S]],
        frames: usize,
    ) -> Result<(), ProcessError> {
        let c = self.ready()?;
        if frames > c.maximum_frames() {
            return Err(ProcessError::TooManyFrames {
                frames,
                maximum: c.maximum_frames(),
            });
        }
        if inputs.len() != self.channels()
            || outputs.len() != self.channels()
            || inputs.iter().any(|x| x.len() < frames)
            || outputs.iter().any(|x| x.len() < frames)
        {
            return Err(ProcessError::LayoutMismatch);
        }
        Ok(())
    }
    pub fn process_planar(
        &mut self,
        inputs: &[&[S]],
        outputs: &mut [&mut [S]],
        frames: usize,
    ) -> Result<(), ProcessError> {
        self.planar_preflight(inputs, outputs, frames)?;
        for (i, (input, output)) in inputs.iter().zip(outputs).enumerate() {
            for (x, y) in input[..frames].iter().zip(&mut output[..frames]) {
                *y = if self.bypassed {
                    *x
                } else {
                    self.kernels[i].process_sample(*x, self.controls.as_ref().expect("owned"))
                };
            }
        }
        Ok(())
    }
    pub fn process_in_place(
        &mut self,
        channels: &mut [&mut [S]],
        frames: usize,
    ) -> Result<(), ProcessError> {
        let c = self.ready()?;
        if frames > c.maximum_frames() {
            return Err(ProcessError::TooManyFrames {
                frames,
                maximum: c.maximum_frames(),
            });
        }
        if channels.len() != self.channels() || channels.iter().any(|x| x.len() < frames) {
            return Err(ProcessError::LayoutMismatch);
        }
        for (i, data) in channels.iter_mut().enumerate() {
            for x in &mut data[..frames] {
                if !self.bypassed {
                    *x = self.kernels[i].process_sample(*x, self.controls.as_ref().expect("owned"));
                }
            }
        }
        Ok(())
    }
    /// CONTROL-only owner recovery after once-only deactivation.
    pub fn into_parts(mut self) -> (C, Vec<K>) {
        self.deactivate();
        (
            self.controls.take().expect("owned"),
            std::mem::take(&mut self.kernels),
        )
    }
}
impl<S: GraphSample, C, K: SharedMonoKernel<S, C>> Processor<S> for MonoDuplicator<S, C, K> {
    /// Deactivates first. A kernel `prepare` panic leaves the successfully
    /// prepared prefix active; it is deactivated once, in reverse, by the next
    /// `deactivate`, `prepare` or by `Drop`, including during unwinding.
    fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
        self.deactivate();
        self.prepare_failure = None;
        if c.layout().input_channels() != self.channels()
            || c.layout().output_channels() != self.channels()
            || !c.auxiliary_buses().is_empty()
            || c.maximum_events() != 0
            || c.maximum_output_events() != 0
        {
            return Err(PrepareError::UnsupportedLayout);
        }
        if c.maximum_frames() > self.maximum_frames {
            return Err(PrepareError::InvalidMaximumFrames);
        }
        for i in 0..self.kernels.len() {
            if let Err(error) = self.kernels[i].prepare(
                c.sample_rate(),
                c.maximum_frames(),
                self.controls.as_ref().expect("owned"),
            ) {
                self.prepare_failure = Some((i, error));
                self.deactivate();
                return Err(error);
            }
            self.prepared += 1;
        }
        self.config = Some(c);
        Ok(())
    }
    fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_, S>) -> ProcessStatus {
        let admission = self.ready().and_then(|c| c.validate_block(b));
        if let Err(error) = admission {
            b.clear_outputs();
            return ProcessStatus::Fault(error);
        }
        for channel in 0..self.channels() {
            for frame in 0..b.audio.frames() {
                let x = b.audio.input(channel).expect("validated")[frame];
                let y = if self.bypassed {
                    x
                } else {
                    self.kernels[channel].process_sample(x, self.controls.as_ref().expect("owned"))
                };
                b.audio.output_mut(channel).expect("validated")[frame] = y;
            }
        }
        ProcessStatus::Continue
    }
    fn reset(&mut self) {
        for k in &mut self.kernels {
            k.reset(self.controls.as_ref().expect("owned"));
        }
    }
    fn deactivate(&mut self) {
        self.config = None;
        while self.prepared > 0 {
            self.prepared -= 1;
            self.kernels[self.prepared].deactivate();
        }
    }
    /// Maximum over kernels while prepared, else zero. Kernels are not summed;
    /// bypass does not change the declared value.
    fn latency_samples(&self) -> u32 {
        if self.config.is_some() {
            self.kernels
                .iter()
                .map(|k| k.latency_samples())
                .max()
                .unwrap_or(0)
        } else {
            0
        }
    }
    /// Longest kernel tail while prepared (`Infinite` dominates, `None` is the
    /// identity), else `Tail::None`.
    fn tail(&self) -> Tail {
        if self.config.is_none() {
            return Tail::None;
        }
        self.kernels
            .iter()
            .fold(Tail::None, |a, k| match (a, k.tail()) {
                (Tail::Infinite, _) | (_, Tail::Infinite) => Tail::Infinite,
                (Tail::Samples(a), Tail::Samples(b)) => Tail::Samples(a.max(b)),
                (Tail::None, b) => b,
                (a, Tail::None) => a,
            })
    }
}
impl<S: GraphSample, C, K: SharedMonoKernel<S, C>> Drop for MonoDuplicator<S, C, K> {
    fn drop(&mut self) {
        self.deactivate();
    }
}
