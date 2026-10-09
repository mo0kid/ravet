//! Host-independent, explicit parameter/control attachments for UI threads.
use ravet_foundation::{ParameterDescriptor, ParameterId, ParameterSchema};

/// Product/host adapter for accepted parameter edits and gesture scheduling.
/// Implementations validate ownership, bound their queues and notify the host.
/// Success means submitted, not necessarily applied; refresh from host snapshots.
pub trait ParameterControl {
    type Error;
    fn commit(&self, id: ParameterId, physical: f32) -> Result<(), Self::Error>;
    fn begin(&self, id: ParameterId) -> Result<(), Self::Error>;
    fn update(&self, id: ParameterId, physical: f32) -> Result<(), Self::Error>;
    fn end(&self, id: ParameterId) -> Result<(), Self::Error>;
}

/// Failed edits leave attachment ownership and displayed host value unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachmentError<E> {
    InvalidValue,
    GestureState,
    Transport(E),
}

impl<E: std::fmt::Display> std::fmt::Display for AttachmentError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidValue => f.write_str("invalid parameter attachment value"),
            Self::GestureState => f.write_str("invalid parameter attachment gesture sequence"),
            Self::Transport(error) => {
                write!(f, "parameter attachment transport rejected edit: {error}")
            }
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for AttachmentError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transport(error) => Some(error),
            _ => None,
        }
    }
}

/// UI-owned binding to a validated stable-ID descriptor. No allocation or polling.
/// Host refreshes never send edits, avoiding feedback loops. Submission does not
/// optimistically change the displayed host value. Explicitly end open gestures
/// before teardown; an attachment cannot notify a transport from `Drop`.
///
/// ```
/// use ravet_foundation::{ParameterDescriptor, ParameterId, ParameterRange, ParameterScale, ParameterSchema};
/// use ravet_surface::ParameterAttachment;
/// let descriptors = [ParameterDescriptor {
///     id: ParameterId(7), name: "Mix", unit: "%",
///     range: ParameterRange::new(0.0, 100.0, ParameterScale::Linear, None).unwrap(),
///     default: 50.0, automatable: true,
/// }];
/// let schema = ParameterSchema::new(&descriptors).unwrap();
/// let mut mix = ParameterAttachment::new(schema, ParameterId(7)).unwrap();
/// assert_eq!(mix.normalized(), 0.5);
/// assert_eq!(mix.refresh(75.0), Some(true)); // no host notification
/// assert_eq!(mix.physical(), 75.0);
/// ```
pub struct ParameterAttachment<'a> {
    descriptor: &'a ParameterDescriptor<'a>,
    physical: f32,
    active: bool,
}
impl<'a> ParameterAttachment<'a> {
    pub fn new<const N: usize>(schema: ParameterSchema<'a, N>, id: ParameterId) -> Option<Self> {
        let descriptor = &schema.descriptors()[schema.index(id)?];
        Some(Self {
            descriptor,
            physical: descriptor.default,
            active: false,
        })
    }
    pub fn descriptor(&self) -> &'a ParameterDescriptor<'a> {
        self.descriptor
    }
    pub fn physical(&self) -> f32 {
        self.physical
    }
    pub fn normalized(&self) -> f32 {
        self.descriptor
            .range
            .normalize(self.physical)
            .expect("canonical attachment value")
    }
    pub fn gesture_active(&self) -> bool {
        self.active
    }
    /// Clamp/snap finite host values; returns whether the displayed value changed.
    /// Nonfinite refreshes return None without changing the previous reading.
    pub fn refresh(&mut self, physical: f32) -> Option<bool> {
        let physical = self.descriptor.range.canonicalize(physical)?;
        let changed = physical != self.physical;
        self.physical = physical;
        Some(changed)
    }
    fn canonical<E>(&self, physical: f32) -> Result<f32, AttachmentError<E>> {
        self.descriptor
            .range
            .canonicalize(physical)
            .ok_or(AttachmentError::InvalidValue)
    }
    pub fn commit<C: ParameterControl>(
        &self,
        control: &C,
        physical: f32,
    ) -> Result<(), AttachmentError<C::Error>> {
        if self.active {
            return Err(AttachmentError::GestureState);
        }
        control
            .commit(self.descriptor.id, self.canonical(physical)?)
            .map_err(AttachmentError::Transport)
    }
    /// Map normalized 0..1 control positions using linear/logarithmic/discrete metadata.
    /// Finite out-of-range positions clamp; nonfinite positions reject.
    pub fn commit_normalized<C: ParameterControl>(
        &self,
        control: &C,
        normalized: f32,
    ) -> Result<(), AttachmentError<C::Error>> {
        let physical = self
            .descriptor
            .range
            .denormalize(normalized)
            .ok_or(AttachmentError::InvalidValue)?;
        self.commit(control, physical)
    }
    pub fn begin<C: ParameterControl>(
        &mut self,
        control: &C,
    ) -> Result<(), AttachmentError<C::Error>> {
        if self.active {
            return Err(AttachmentError::GestureState);
        }
        control
            .begin(self.descriptor.id)
            .map_err(AttachmentError::Transport)?;
        self.active = true;
        Ok(())
    }
    pub fn update<C: ParameterControl>(
        &self,
        control: &C,
        physical: f32,
    ) -> Result<(), AttachmentError<C::Error>> {
        if !self.active {
            return Err(AttachmentError::GestureState);
        }
        control
            .update(self.descriptor.id, self.canonical(physical)?)
            .map_err(AttachmentError::Transport)
    }
    pub fn update_normalized<C: ParameterControl>(
        &self,
        control: &C,
        normalized: f32,
    ) -> Result<(), AttachmentError<C::Error>> {
        let physical = self
            .descriptor
            .range
            .denormalize(normalized)
            .ok_or(AttachmentError::InvalidValue)?;
        self.update(control, physical)
    }
    /// Retain gesture ownership if transport rejects end, allowing a later retry.
    pub fn end<C: ParameterControl>(
        &mut self,
        control: &C,
    ) -> Result<(), AttachmentError<C::Error>> {
        if !self.active {
            return Err(AttachmentError::GestureState);
        }
        control
            .end(self.descriptor.id)
            .map_err(AttachmentError::Transport)?;
        self.active = false;
        Ok(())
    }
}
