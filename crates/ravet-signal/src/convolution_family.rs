//! Explicit native-precision zero/fixed/nonuniform prepared convolution.
//!
//! [`Convolver32`] and [`Convolver64`] own one prepared finite response in the
//! named native sample precision. [`ConvolutionBank32`]/[`ConvolutionBank64`]
//! own 1..=64 independent diagonal channels. The schedule is fixed at CONTROL
//! preparation by [`ConvolutionPartitioning`]. The engines themselves perform no
//! live response replacement, sample-rate conversion, trimming or normalization:
//! [`prepare_impulse_response`](crate::prepare_impulse_response) prepares a
//! response explicitly beforehand, and live transitions are owned outside this
//! crate (`ravet-graph`'s optional `convolution-transitions` feature).
//!
//! ```
//! use ravet_signal::{ConvolutionPartitioning, Convolver64};
//! let h = [0.5, 0.25, -0.125];
//! let mut zero =
//!     Convolver64::new(&h, ConvolutionPartitioning::ZeroLatency { head: 16 }, 64, 1 << 20)
//!         .unwrap();
//! assert_eq!(zero.info().latency_samples, 0);
//! let input = [1.0, 0.0, 0.0, 0.0];
//! let mut output = [0.0; 4];
//! zero.process(&input, &mut output, 4).unwrap();
//! assert_eq!(output, [0.5, 0.25, -0.125, 0.0]);
//! ```
use crate::{Complex, Complex32, FftPlan, FftPlan32, FftPreparationError};
const MAX_TAPS: usize = 262_144;
const MAX_BYTES: usize = 256 * 1024 * 1024;
/// Immutable partition schedule. Sizes are powers of two; no hidden latency repair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConvolutionPartitioning {
    /// One uniform overlap-add stage with exactly `partition` frames of
    /// algorithmic latency. `partition` is a power of two in 16..=4096.
    UniformDelayed {
        /// Partition and latency in frames.
        partition: usize,
    },
    /// Direct FIR for taps `0..head` plus a `head`-partitioned stage for the
    /// remaining taps; zero algorithmic latency. `head` is a power of two in
    /// 16..=4096. Responses no longer than `head` have no FFT stage.
    ZeroLatency {
        /// Direct-head length and middle partition in frames.
        head: usize,
    },
    /// Direct head, `head`-partitioned middle stage for taps `head..tail` and
    /// `tail`-partitioned late stage for taps from `tail`; zero algorithmic
    /// latency. `tail` is a power of two in `head..=16384`. Empty intervals are
    /// omitted. A two-stage schedule, not an optimal arbitrary partitioning.
    NonUniform {
        /// Direct-head length and middle partition in frames.
        head: usize,
        /// Late-stage offset and partition in frames.
        tail: usize,
    },
}
/// CONTROL preparation refusal. Borrowed responses remain untouched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConvolutionPreparationError {
    /// Partition sizes are not admitted powers of two or violate `tail >= head`.
    InvalidPartitioning,
    /// Response is empty, longer than 262,144 taps or contains a nonfinite value.
    InvalidResponse,
    /// A bank must contain 1..=64 channels.
    InvalidChannels,
    /// Maximum block must be 1..=65,536 frames.
    InvalidMaximum,
    /// Declared or actual numeric storage exceeds the caller budget, or the
    /// budget is outside 1..=256 MiB.
    Capacity,
    /// The allocator refused a fallible reservation.
    Allocation,
    /// A stage FFT plan could not be prepared.
    Fft(FftPreparationError),
}
/// Whole-prefix rejection before any sample/history is consumed or edited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConvolutionProcessError {
    /// Requested frames exceed the prepared maximum block.
    TooManyFrames,
    /// An input or output slice is shorter than the requested prefix.
    PrefixTooShort,
    /// Bank input/output channel counts differ from the prepared bank.
    ChannelMismatch,
    /// Channel index is outside the bank.
    InvalidChannel,
}
/// Actual static stage coordinates, not a claim about ideal IR onset/CPU deadline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConvolutionStageInfo {
    /// First original response tap owned by this stage.
    pub original_offset: usize,
    /// Number of original taps owned by this stage.
    pub response_taps: usize,
    /// Partition (block) size; equal to the stage's intrinsic delay.
    pub partition: usize,
    /// Number of response partitions.
    pub partitions: usize,
    /// Complex FFT size (`2 * partition`).
    pub fft_size: usize,
}
/// Immutable exact finite-response/storage/schedule observations.
/// Numeric bytes include Vec capacities of samples/spectra/twiddles, excluding
/// allocator overhead and fixed control metadata. Tail includes block latency.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConvolutionInfo {
    /// Prepared schedule.
    pub partitioning: ConvolutionPartitioning,
    /// Original response length.
    pub taps: usize,
    /// Taps executed by the direct head (0 for uniform delayed mode).
    pub direct_taps: usize,
    /// Middle/late (or single uniform) FFT stages; `None` for empty intervals.
    pub stages: [Option<ConvolutionStageInfo>; 2],
    /// Exact algorithmic latency in frames.
    pub latency_samples: usize,
    /// `taps - 1 + latency_samples`: frames after the last nonzero input
    /// before output is guaranteed to depend only on later input.
    pub tail_samples: usize,
    /// Numeric bytes computed before allocation.
    pub declared_numeric_bytes: usize,
    /// Numeric bytes from actual Vec capacities after allocation.
    pub actual_numeric_bytes: usize,
}
/// Bounded work observation. Saturation is explicit; these counters never wrap.
/// Completed blocks each execute one forward and one inverse FFT. Reset clears
/// counts; this is not an instruction-cost, clock or deadline receipt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConvolutionStageWork {
    /// Completed stage blocks (one forward plus one inverse FFT each).
    pub completed_blocks: u64,
    /// The block counter reached `u64::MAX` and stopped counting.
    pub saturated: bool,
    /// Source frames buffered toward the next stage block.
    pub pending_source_frames: usize,
}
#[derive(Clone, Copy)]
struct Schedule {
    direct: usize,
    stages: [Option<ConvolutionStageInfo>; 2],
    latency: usize,
}
impl ConvolutionPartitioning {
    fn schedule(self, taps: usize) -> Result<Schedule, ConvolutionPreparationError> {
        let valid = |n: usize| (16..=4096).contains(&n) && n.is_power_of_two();
        let stage = |offset: usize, len: usize, b: usize| {
            if len == 0 {
                None
            } else {
                Some(ConvolutionStageInfo {
                    original_offset: offset,
                    response_taps: len,
                    partition: b,
                    partitions: len.div_ceil(b),
                    fft_size: 2 * b,
                })
            }
        };
        match self {
            Self::UniformDelayed { partition: b } if valid(b) => Ok(Schedule {
                direct: 0,
                stages: [stage(0, taps, b), None],
                latency: b,
            }),
            Self::ZeroLatency { head: b } if valid(b) => Ok(Schedule {
                direct: b.min(taps),
                stages: [stage(b, taps.saturating_sub(b), b), None],
                latency: 0,
            }),
            Self::NonUniform { head: b, tail: t }
                if valid(b) && t >= b && t <= 16384 && t.is_power_of_two() =>
            {
                Ok(Schedule {
                    direct: b.min(taps),
                    stages: [
                        stage(b, taps.min(t).saturating_sub(b), b),
                        stage(t, taps.saturating_sub(t), t),
                    ],
                    latency: 0,
                })
            }
            _ => Err(ConvolutionPreparationError::InvalidPartitioning),
        }
    }
}
fn add_bytes(
    total: &mut usize,
    count: usize,
    width: usize,
) -> Result<(), ConvolutionPreparationError> {
    *total = total
        .checked_add(
            count
                .checked_mul(width)
                .ok_or(ConvolutionPreparationError::Capacity)?,
        )
        .ok_or(ConvolutionPreparationError::Capacity)?;
    Ok(())
}
fn requested_bytes(
    schedule: Schedule,
    sample_width: usize,
    complex_width: usize,
) -> Result<usize, ConvolutionPreparationError> {
    let mut n = 0;
    add_bytes(&mut n, 2 * schedule.direct, sample_width)?;
    for stage in schedule.stages.into_iter().flatten() {
        add_bytes(&mut n, stage.partition, complex_width)?; // twiddles
        add_bytes(&mut n, 2 * stage.partitions * stage.fft_size, complex_width)?; // IR + input spectra
        add_bytes(&mut n, stage.fft_size, complex_width)?; // FFT work
        add_bytes(&mut n, 3 * stage.partition, sample_width)?; // input/output/overlap
    }
    Ok(n)
}
struct Budget {
    maximum: usize,
    actual: usize,
}
impl Budget {
    fn new(requested: usize, maximum: usize) -> Result<Self, ConvolutionPreparationError> {
        if maximum == 0 || maximum > MAX_BYTES || requested > maximum {
            return Err(ConvolutionPreparationError::Capacity);
        }
        Ok(Self { maximum, actual: 0 })
    }
    fn remaining(&self) -> usize {
        self.maximum - self.actual
    }
    fn take(&mut self, bytes: usize) -> Result<(), ConvolutionPreparationError> {
        if bytes > self.remaining() {
            return Err(ConvolutionPreparationError::Capacity);
        }
        self.actual += bytes;
        Ok(())
    }
    fn zeros<T: Default + Clone>(
        &mut self,
        n: usize,
    ) -> Result<Vec<T>, ConvolutionPreparationError> {
        let requested = n
            .checked_mul(size_of::<T>())
            .ok_or(ConvolutionPreparationError::Capacity)?;
        if requested > self.remaining() {
            return Err(ConvolutionPreparationError::Capacity);
        }
        let mut v = Vec::new();
        v.try_reserve_exact(n)
            .map_err(|_| ConvolutionPreparationError::Allocation)?;
        self.take(
            v.capacity()
                .checked_mul(size_of::<T>())
                .ok_or(ConvolutionPreparationError::Capacity)?,
        )?;
        v.resize(n, T::default());
        Ok(v)
    }
}
fn validate_maximum(maximum: usize) -> Result<(), ConvolutionPreparationError> {
    if !(1..=65536).contains(&maximum) {
        Err(ConvolutionPreparationError::InvalidMaximum)
    } else {
        Ok(())
    }
}
macro_rules! family {
    ($engine:ident,$bank:ident,$direct:ident,$stage:ident,$sample:ty,$complex:ident,$plan:ident) => {
        struct $direct {
            impulse: Vec<$sample>,
            history: Vec<$sample>,
            cursor: usize,
        }
        impl $direct {
            fn new(
                h: &[$sample],
                budget: &mut Budget,
            ) -> Result<Self, ConvolutionPreparationError> {
                let mut impulse = budget.zeros(h.len())?;
                impulse.copy_from_slice(h);
                Ok(Self {
                    impulse,
                    history: budget.zeros(h.len())?,
                    cursor: 0,
                })
            }
            fn process(&mut self, x: $sample) -> $sample {
                self.history[self.cursor] = x;
                let mut y = 0.;
                let mut read = self.cursor;
                for h in &self.impulse {
                    let product = *h * self.history[read];
                    y += product;
                    read = if read == 0 {
                        self.history.len() - 1
                    } else {
                        read - 1
                    };
                }
                self.cursor = (self.cursor + 1) % self.history.len();
                y
            }
            fn reset(&mut self) {
                self.history.fill(0.);
                self.cursor = 0;
            }
        }
        struct $stage {
            plan: $plan,
            info: ConvolutionStageInfo,
            impulse: Vec<$complex>,
            history: Vec<$complex>,
            work: Vec<$complex>,
            input: Vec<$sample>,
            output: Vec<$sample>,
            overlap: Vec<$sample>,
            position: usize,
            head: usize,
            blocks: u64,
            saturated: bool,
        }
        impl $stage {
            fn new(
                h: &[$sample],
                info: ConvolutionStageInfo,
                budget: &mut Budget,
            ) -> Result<Self, ConvolutionPreparationError> {
                let plan = $plan::try_new(info.fft_size, budget.remaining())
                    .map_err(ConvolutionPreparationError::Fft)?;
                budget.take(plan.twiddle_storage_bytes())?;
                let mut impulse = budget.zeros::<$complex>(info.partitions * info.fft_size)?;
                for (h, spectrum) in h
                    .chunks(info.partition)
                    .zip(impulse.chunks_exact_mut(info.fft_size))
                {
                    for (x, z) in h.iter().zip(spectrum.iter_mut()) {
                        z.re = *x;
                    }
                    plan.forward(spectrum).expect("admitted FFT extent");
                }
                Ok(Self {
                    plan,
                    info,
                    impulse,
                    history: budget.zeros(info.partitions * info.fft_size)?,
                    work: budget.zeros(info.fft_size)?,
                    input: budget.zeros(info.partition)?,
                    output: budget.zeros(info.partition)?,
                    overlap: budget.zeros(info.partition)?,
                    position: 0,
                    head: 0,
                    blocks: 0,
                    saturated: false,
                })
            }
            fn process(&mut self, x: $sample) -> $sample {
                let y = self.output[self.position];
                self.input[self.position] = x;
                self.position += 1;
                if self.position == self.info.partition {
                    self.transform();
                    self.position = 0;
                }
                y
            }
            fn transform(&mut self) {
                let size = self.info.fft_size;
                let count = self.info.partitions;
                let current = &mut self.history[self.head * size..(self.head + 1) * size];
                current.fill($complex::default());
                for (z, x) in current.iter_mut().zip(&self.input) {
                    z.re = *x;
                }
                self.plan.forward(current).expect("admitted FFT extent");
                self.work.fill($complex::default());
                for p in 0..count {
                    let index = (self.head + count - p) % count;
                    let input = &self.history[index * size..(index + 1) * size];
                    let response = &self.impulse[p * size..(p + 1) * size];
                    for ((sum, x), h) in self.work.iter_mut().zip(input).zip(response) {
                        let real = x.re * h.re - x.im * h.im;
                        let imaginary = x.re * h.im + x.im * h.re;
                        sum.re += real;
                        sum.im += imaginary;
                    }
                }
                self.plan
                    .inverse(&mut self.work)
                    .expect("admitted FFT extent");
                for i in 0..self.info.partition {
                    self.output[i] = self.work[i].re + self.overlap[i];
                    self.overlap[i] = self.work[i + self.info.partition].re;
                }
                self.head = (self.head + 1) % count;
                if let Some(next) = self.blocks.checked_add(1) {
                    self.blocks = next;
                } else {
                    self.saturated = true;
                }
            }
            fn reset(&mut self) {
                self.history.fill($complex::default());
                self.work.fill($complex::default());
                self.input.fill(0.);
                self.output.fill(0.);
                self.overlap.fill(0.);
                self.position = 0;
                self.head = 0;
                self.blocks = 0;
                self.saturated = false;
            }
            fn observation(&self) -> ConvolutionStageWork {
                ConvolutionStageWork {
                    completed_blocks: self.blocks,
                    saturated: self.saturated,
                    pending_source_frames: self.position,
                }
            }
        }
        /// Complete native sample-precision convolution with immutable explicit schedule.
        /// CONTROL preparation/Drop owns storage; prepared process/reset never allocate
        /// or deallocate. Head and FFT arithmetic execute in the named sample precision.
        /// Nonfinite/overflow samples propagate; reset clears all history. No implicit
        /// EOF, denormal/normalization policy, live IR replacement or deadline promise.
        pub struct $engine {
            info: ConvolutionInfo,
            maximum: usize,
            direct: Option<$direct>,
            stages: [Option<$stage>; 2],
        }
        impl $engine {
            /// Copy a finite response1..262144; maximum frames1..65536, aggregate numeric
            /// budget1..256MiB. Complete argument/declared-byte checks precede allocation.
            /// Actual Vec capacities are rechecked; allocator/control metadata excluded.
            pub fn new(
                h: &[$sample],
                partitioning: ConvolutionPartitioning,
                maximum_frames: usize,
                maximum_numeric_bytes: usize,
            ) -> Result<Self, ConvolutionPreparationError> {
                validate_maximum(maximum_frames)?;
                if h.is_empty() || h.len() > MAX_TAPS || h.iter().any(|x| !x.is_finite()) {
                    return Err(ConvolutionPreparationError::InvalidResponse);
                }
                let schedule = partitioning.schedule(h.len())?;
                let requested =
                    requested_bytes(schedule, size_of::<$sample>(), size_of::<$complex>())?;
                let mut budget = Budget::new(requested, maximum_numeric_bytes)?;
                Self::build(
                    h,
                    partitioning,
                    maximum_frames,
                    schedule,
                    requested,
                    &mut budget,
                )
            }
            fn build(
                h: &[$sample],
                partitioning: ConvolutionPartitioning,
                maximum: usize,
                schedule: Schedule,
                requested: usize,
                budget: &mut Budget,
            ) -> Result<Self, ConvolutionPreparationError> {
                let start = budget.actual;
                let direct = if schedule.direct == 0 {
                    None
                } else {
                    Some($direct::new(&h[..schedule.direct], budget)?)
                };
                let mut stages = [None, None];
                for (slot, info) in stages.iter_mut().zip(schedule.stages) {
                    if let Some(info) = info {
                        *slot = Some($stage::new(
                            &h[info.original_offset..info.original_offset + info.response_taps],
                            info,
                            budget,
                        )?);
                    }
                }
                let info = ConvolutionInfo {
                    partitioning,
                    taps: h.len(),
                    direct_taps: schedule.direct,
                    stages: schedule.stages,
                    latency_samples: schedule.latency,
                    tail_samples: h.len() - 1 + schedule.latency,
                    declared_numeric_bytes: requested,
                    actual_numeric_bytes: budget.actual - start,
                };
                Ok(Self {
                    info,
                    maximum,
                    direct,
                    stages,
                })
            }
            /// Immutable prepared schedule, latency, tail and storage observation.
            pub fn info(&self) -> ConvolutionInfo {
                self.info
            }
            /// Prepared maximum frames per `process`/`process_in_place` call.
            pub fn maximum_frames(&self) -> usize {
                self.maximum
            }
            /// Finite saturating per-stage work observations, no clock/cost inference.
            pub fn stage_work(&self) -> [Option<ConvolutionStageWork>; 2] {
                self.stages
                    .each_ref()
                    .map(|s| s.as_ref().map(|s| s.observation()))
            }
            /// One source frame emits one output frame. All required history is owned;
            /// callers explicitly supply zero samples to drain the declared finite tail.
            pub fn process_sample(&mut self, x: $sample) -> $sample {
                if let Some(direct) = &mut self.direct {
                    let mut y = direct.process(x);
                    for stage in self.stages.iter_mut().flatten() {
                        y += stage.process(x);
                    }
                    y
                } else {
                    self.stages[0]
                        .as_mut()
                        .expect("uniform stage exists")
                        .process(x)
                }
            }
            fn preflight(
                &self,
                n: usize,
                lengths: &[usize],
            ) -> Result<(), ConvolutionProcessError> {
                if n > self.maximum {
                    return Err(ConvolutionProcessError::TooManyFrames);
                }
                if lengths.iter().any(|len| *len < n) {
                    return Err(ConvolutionProcessError::PrefixTooShort);
                }
                Ok(())
            }
            /// Process the first `n` frames from `input` into `output`. The whole
            /// call is refused before any history or output changes when `n`
            /// exceeds the prepared maximum or either slice is shorter than `n`.
            /// Unused backing suffixes stay untouched; `n == 0` is a no-op.
            pub fn process(
                &mut self,
                input: &[$sample],
                output: &mut [$sample],
                n: usize,
            ) -> Result<(), ConvolutionProcessError> {
                self.preflight(n, &[input.len(), output.len()])?;
                for i in 0..n {
                    output[i] = self.process_sample(input[i]);
                }
                Ok(())
            }
            /// In-place variant of [`Self::process`] with identical admission.
            pub fn process_in_place(
                &mut self,
                samples: &mut [$sample],
                n: usize,
            ) -> Result<(), ConvolutionProcessError> {
                self.preflight(n, &[samples.len()])?;
                for x in &mut samples[..n] {
                    *x = self.process_sample(*x);
                }
                Ok(())
            }
            /// Capacity-dependent zeroing retains IR/configuration; no heap/deallocation.
            pub fn reset(&mut self) {
                if let Some(d) = &mut self.direct {
                    d.reset();
                }
                for stage in self.stages.iter_mut().flatten() {
                    stage.reset();
                }
            }
        }
        /// Independent diagonal1..64-channel bank with whole-call admission.
        /// No hidden stereo crossfeed, channel-linked normalization or clock policy.
        pub struct $bank {
            engines: Vec<$engine>,
            maximum: usize,
            declared: usize,
            actual: usize,
        }
        impl $bank {
            /// Prepare one engine per response with a shared schedule. All channel
            /// counts, responses, schedules and the aggregate declared budget are
            /// checked before any engine storage is allocated.
            pub fn new(
                responses: &[&[$sample]],
                partitioning: ConvolutionPartitioning,
                maximum_frames: usize,
                maximum_numeric_bytes: usize,
            ) -> Result<Self, ConvolutionPreparationError> {
                validate_maximum(maximum_frames)?;
                if !(1..=64).contains(&responses.len()) {
                    return Err(ConvolutionPreparationError::InvalidChannels);
                }
                let mut requested = 0usize;
                let mut schedules = [None; 64];
                for (h, schedule) in responses.iter().zip(&mut schedules) {
                    if h.is_empty() || h.len() > MAX_TAPS || h.iter().any(|x| !x.is_finite()) {
                        return Err(ConvolutionPreparationError::InvalidResponse);
                    }
                    let s = partitioning.schedule(h.len())?;
                    let bytes = requested_bytes(s, size_of::<$sample>(), size_of::<$complex>())?;
                    requested = requested
                        .checked_add(bytes)
                        .ok_or(ConvolutionPreparationError::Capacity)?;
                    *schedule = Some((s, bytes));
                }
                let mut budget = Budget::new(requested, maximum_numeric_bytes)?;
                let mut engines = Vec::new();
                engines
                    .try_reserve_exact(responses.len())
                    .map_err(|_| ConvolutionPreparationError::Allocation)?;
                for (h, s) in responses.iter().zip(schedules.into_iter().flatten()) {
                    engines.push($engine::build(
                        h,
                        partitioning,
                        maximum_frames,
                        s.0,
                        s.1,
                        &mut budget,
                    )?);
                }
                Ok(Self {
                    engines,
                    maximum: maximum_frames,
                    declared: requested,
                    actual: budget.actual,
                })
            }
            /// Prepared channel count (1..=64).
            pub fn channels(&self) -> usize {
                self.engines.len()
            }
            /// Prepared maximum frames per call.
            pub fn maximum_frames(&self) -> usize {
                self.maximum
            }
            /// Aggregate numeric bytes computed before allocation.
            pub fn declared_numeric_bytes(&self) -> usize {
                self.declared
            }
            /// Aggregate numeric bytes from actual Vec capacities.
            pub fn actual_numeric_bytes(&self) -> usize {
                self.actual
            }
            /// Borrow one channel engine for info and work observations.
            pub fn engine(&self, channel: usize) -> Option<&$engine> {
                self.engines.get(channel)
            }
            fn preflight(
                &self,
                n: usize,
                inputs: usize,
                outputs: usize,
            ) -> Result<(), ConvolutionProcessError> {
                if n > self.maximum {
                    return Err(ConvolutionProcessError::TooManyFrames);
                }
                if inputs != self.channels() || outputs != self.channels() {
                    return Err(ConvolutionProcessError::ChannelMismatch);
                }
                Ok(())
            }
            /// Process `n` frames for every channel. Channel count, maximum and
            /// every prefix are admitted for the whole bank before any channel runs.
            pub fn process_planar(
                &mut self,
                input: &[&[$sample]],
                output: &mut [&mut [$sample]],
                n: usize,
            ) -> Result<(), ConvolutionProcessError> {
                self.preflight(n, input.len(), output.len())?;
                if input.iter().any(|x| x.len() < n) || output.iter().any(|x| x.len() < n) {
                    return Err(ConvolutionProcessError::PrefixTooShort);
                }
                for ((e, a), b) in self.engines.iter_mut().zip(input).zip(output) {
                    e.process(a, b, n).expect("whole bank admission");
                }
                Ok(())
            }
            /// In-place variant of [`Self::process_planar`] with identical admission.
            pub fn process_in_place(
                &mut self,
                samples: &mut [&mut [$sample]],
                n: usize,
            ) -> Result<(), ConvolutionProcessError> {
                self.preflight(n, samples.len(), samples.len())?;
                if samples.iter().any(|x| x.len() < n) {
                    return Err(ConvolutionProcessError::PrefixTooShort);
                }
                for (e, s) in self.engines.iter_mut().zip(samples) {
                    e.process_in_place(s, n).expect("whole bank admission");
                }
                Ok(())
            }
            /// Clear all channel histories; responses and schedule are retained.
            pub fn reset(&mut self) {
                for e in &mut self.engines {
                    e.reset();
                }
            }
            /// Clear one channel's history.
            pub fn reset_channel(&mut self, channel: usize) -> Result<(), ConvolutionProcessError> {
                self.engines
                    .get_mut(channel)
                    .ok_or(ConvolutionProcessError::InvalidChannel)?
                    .reset();
                Ok(())
            }
            /// Mutably borrow one channel engine, for example to process
            /// frame-interleaved audio one sample at a time. Each engine keeps its
            /// own admission; bank-wide prefix admission applies only to bank calls.
            pub fn engine_mut(&mut self, channel: usize) -> Option<&mut $engine> {
                self.engines.get_mut(channel)
            }
        }
    };
}
family!(
    Convolver64,
    ConvolutionBank64,
    Direct64,
    Stage64,
    f64,
    Complex,
    FftPlan
);
family!(
    Convolver32,
    ConvolutionBank32,
    Direct32,
    Stage32,
    f32,
    Complex32,
    FftPlan32
);
impl core::fmt::Display for ConvolutionPreparationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::InvalidPartitioning => {
                "convolution partitions are outside the declared power-of-two schedule"
            }
            Self::InvalidResponse => "response must contain 1..262144 finite native samples",
            Self::InvalidChannels => "convolution bank must contain 1..64 channels",
            Self::InvalidMaximum => "maximum convolution block must be 1..65536 frames",
            Self::Capacity => "aggregate numeric convolution storage exceeds its byte budget",
            Self::Allocation => "convolution preparation allocation failed",
            Self::Fft(_) => "convolution FFT preparation failed",
        })
    }
}
impl core::error::Error for ConvolutionPreparationError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Fft(e) => Some(e),
            _ => None,
        }
    }
}
impl core::fmt::Display for ConvolutionProcessError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::TooManyFrames => "convolution block exceeds its prepared maximum",
            Self::PrefixTooShort => "convolution prefix is shorter than requested",
            Self::ChannelMismatch => "convolution channel bank does not match preparation",
            Self::InvalidChannel => "convolution channel is unavailable",
        })
    }
}
impl core::error::Error for ConvolutionProcessError {}
