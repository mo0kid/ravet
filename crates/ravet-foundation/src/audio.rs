mod sealed {
    pub trait Sealed {}
    impl Sealed for f32 {}
    impl Sealed for f64 {}
}

/// Native sample precision supported by borrowed portable audio views.
///
/// Sealed to `f32` and `f64`. Precision is selected by the processor/adapter;
/// buffers never convert or allocate samples implicitly.
pub trait Sample: sealed::Sealed + Copy + Default + std::fmt::Debug {}
impl Sample for f32 {}
impl Sample for f64 {}

/// A channel's backing storage is too short for the requested active frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferError {
    InputTooShort {
        channel: usize,
        available: usize,
        required: usize,
    },
    OutputTooShort {
        channel: usize,
        available: usize,
        required: usize,
    },
}

#[derive(Debug)]
enum Storage<'channels, 'samples, S: Sample> {
    InPlace(&'channels mut [&'samples mut [S]]),
    Separate {
        inputs: &'channels [&'samples [S]],
        outputs: &'channels mut [&'samples mut [S]],
    },
}

/// Borrowed planar f32/f64 samples, with separate or in-place ownership.
///
/// Constructors check that every channel contains at least `frames` samples.
/// Different backing capacities are legal; only the active prefix is exposed.
/// A zero-frame view never reads or writes samples. For an instrument, pass an
/// empty input slice to [`Self::separate`]. Format adapters must represent host
/// aliasing using [`Self::in_place`] or their own preallocated scratch storage.
#[derive(Debug)]
pub struct AudioBuffer<'channels, 'samples, S: Sample = f32> {
    storage: Storage<'channels, 'samples, S>,
    frames: usize,
}

impl<'channels, 'samples, S: Sample> AudioBuffer<'channels, 'samples, S> {
    pub fn in_place(
        channels: &'channels mut [&'samples mut [S]],
        frames: usize,
    ) -> Result<Self, BufferError> {
        Self::validate_outputs(channels, frames)?;
        Ok(Self {
            storage: Storage::InPlace(channels),
            frames,
        })
    }

    pub fn separate(
        inputs: &'channels [&'samples [S]],
        outputs: &'channels mut [&'samples mut [S]],
        frames: usize,
    ) -> Result<Self, BufferError> {
        for (channel, samples) in inputs.iter().enumerate() {
            if samples.len() < frames {
                return Err(BufferError::InputTooShort {
                    channel,
                    available: samples.len(),
                    required: frames,
                });
            }
        }
        Self::validate_outputs(outputs, frames)?;
        Ok(Self {
            storage: Storage::Separate { inputs, outputs },
            frames,
        })
    }

    fn validate_outputs(outputs: &[&mut [S]], frames: usize) -> Result<(), BufferError> {
        for (channel, samples) in outputs.iter().enumerate() {
            if samples.len() < frames {
                return Err(BufferError::OutputTooShort {
                    channel,
                    available: samples.len(),
                    required: frames,
                });
            }
        }
        Ok(())
    }

    pub fn frames(&self) -> usize {
        self.frames
    }

    pub fn input_channels(&self) -> usize {
        match &self.storage {
            Storage::InPlace(channels) => channels.len(),
            Storage::Separate { inputs, .. } => inputs.len(),
        }
    }

    pub fn output_channels(&self) -> usize {
        match &self.storage {
            Storage::InPlace(channels) => channels.len(),
            Storage::Separate { outputs, .. } => outputs.len(),
        }
    }

    /// In-place inputs reflect any writes already made to the same channel.
    pub fn input(&self, channel: usize) -> Option<&[S]> {
        match &self.storage {
            Storage::InPlace(channels) => {
                channels.get(channel).map(|samples| &samples[..self.frames])
            }
            Storage::Separate { inputs, .. } => {
                inputs.get(channel).map(|samples| &samples[..self.frames])
            }
        }
    }

    pub fn output(&self, channel: usize) -> Option<&[S]> {
        match &self.storage {
            Storage::InPlace(channels) => {
                channels.get(channel).map(|samples| &samples[..self.frames])
            }
            Storage::Separate { outputs, .. } => {
                outputs.get(channel).map(|samples| &samples[..self.frames])
            }
        }
    }

    pub fn output_mut(&mut self, channel: usize) -> Option<&mut [S]> {
        let outputs = match &mut self.storage {
            Storage::InPlace(channels) => channels,
            Storage::Separate { outputs, .. } => outputs,
        };
        outputs
            .get_mut(channel)
            .map(|samples| &mut samples[..self.frames])
    }

    /// Map each active frame to stereo output, branching on storage once.
    ///
    /// The callback receives the frame offset and two input samples. Missing
    /// inputs are zero; mono is not duplicated into the right input. Both
    /// in-place samples are read before either output is written. Backing tails
    /// are retained, and a zero-frame view invokes no callback.
    ///
    /// Returns `false` without changes or callbacks unless there are exactly
    /// two outputs and at most two inputs. This does not validate processor
    /// preparation, events, or layout; processors must validate those first.
    /// The callback owns the allocation-free and real-time obligations.
    #[inline]
    pub fn map_stereo_frames(&mut self, mut map: impl FnMut(usize, [S; 2]) -> [S; 2]) -> bool {
        match &mut self.storage {
            Storage::InPlace(channels) => {
                let [left, right] = &mut **channels else {
                    return false;
                };
                for (frame, (left, right)) in left[..self.frames]
                    .iter_mut()
                    .zip(&mut right[..self.frames])
                    .enumerate()
                {
                    let result = map(frame, [*left, *right]);
                    *left = result[0];
                    *right = result[1];
                }
            }
            Storage::Separate { inputs, outputs } => {
                let [left, right] = &mut **outputs else {
                    return false;
                };
                let output = left[..self.frames]
                    .iter_mut()
                    .zip(&mut right[..self.frames]);
                match *inputs {
                    [] => {
                        for (frame, (left, right)) in output.enumerate() {
                            let result = map(frame, [S::default(); 2]);
                            *left = result[0];
                            *right = result[1];
                        }
                    }
                    [input] => {
                        for (frame, ((left, right), &input)) in
                            output.zip(&input[..self.frames]).enumerate()
                        {
                            let result = map(frame, [input, S::default()]);
                            *left = result[0];
                            *right = result[1];
                        }
                    }
                    [input_left, input_right] => {
                        for (frame, (((left, right), &a), &b)) in output
                            .zip(&input_left[..self.frames])
                            .zip(&input_right[..self.frames])
                            .enumerate()
                        {
                            let result = map(frame, [a, b]);
                            *left = result[0];
                            *right = result[1];
                        }
                    }
                    _ => return false,
                }
            }
        }
        true
    }

    /// Initialize all active output frames without touching backing tails.
    pub fn clear_outputs(&mut self) {
        let outputs = match &mut self.storage {
            Storage::InPlace(channels) => channels,
            Storage::Separate { outputs, .. } => outputs,
        };
        for channel in outputs.iter_mut() {
            channel[..self.frames].fill(S::default());
        }
    }
}

impl std::fmt::Display for BufferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InputTooShort {
                channel,
                available,
                required,
            } => write!(
                f,
                "input channel {channel} has {available} samples; {required} required"
            ),
            Self::OutputTooShort {
                channel,
                available,
                required,
            } => write!(
                f,
                "output channel {channel} has {available} samples; {required} required"
            ),
        }
    }
}
impl std::error::Error for BufferError {}
