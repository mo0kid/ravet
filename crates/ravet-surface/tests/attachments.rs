use ravet_foundation::{
    ParameterDescriptor, ParameterId, ParameterRange, ParameterScale, ParameterSchema,
};
use ravet_surface::{AttachmentError, ParameterAttachment, ParameterControl};
use std::cell::Cell;
#[derive(Default)]
struct Control {
    count: Cell<usize>,
    last: Cell<(u8, u32, f32)>,
    reject: Cell<bool>,
}
impl Control {
    fn send(&self, kind: u8, id: ParameterId, value: f32) -> Result<(), &'static str> {
        if self.reject.get() {
            return Err("full");
        }
        self.count.set(self.count.get() + 1);
        self.last.set((kind, id.0, value));
        Ok(())
    }
}
impl ParameterControl for Control {
    type Error = &'static str;
    fn commit(&self, id: ParameterId, value: f32) -> Result<(), Self::Error> {
        self.send(0, id, value)
    }
    fn begin(&self, id: ParameterId) -> Result<(), Self::Error> {
        self.send(1, id, 0.0)
    }
    fn update(&self, id: ParameterId, value: f32) -> Result<(), Self::Error> {
        self.send(2, id, value)
    }
    fn end(&self, id: ParameterId) -> Result<(), Self::Error> {
        self.send(3, id, 0.0)
    }
}
fn descriptor(id: u32, range: ParameterRange, default: f32) -> ParameterDescriptor<'static> {
    ParameterDescriptor {
        id: ParameterId(id),
        name: "Control",
        unit: "",
        range,
        default,
        automatable: true,
    }
}
#[test]
fn linear_logarithmic_and_discrete_controls_use_stable_metadata_without_feedback() {
    let descriptors = [
        descriptor(
            17,
            ParameterRange::new(0.0, 100.0, ParameterScale::Linear, None).unwrap(),
            50.0,
        ),
        descriptor(
            99,
            ParameterRange::new(20.0, 20000.0, ParameterScale::Logarithmic, None).unwrap(),
            20.0,
        ),
        descriptor(
            7,
            ParameterRange::new(0.0, 1.0, ParameterScale::Linear, Some(1)).unwrap(),
            0.0,
        ),
    ];
    let schema = ParameterSchema::new(&descriptors).unwrap();
    assert!(ParameterAttachment::new(schema, ParameterId(5)).is_none());
    let control = Control::default();
    for (id, normalized, expected) in [(17, 0.75, 75.0), (99, 1.0, 20000.0), (7, 0.5, 1.0)] {
        let mut binding = ParameterAttachment::new(schema, ParameterId(id)).unwrap();
        binding.commit_normalized(&control, normalized).unwrap();
        assert_eq!(control.last.get(), (0, id, expected));
        assert_eq!(binding.physical(), binding.descriptor().default); // waits for host
        let before = control.count.get();
        assert_eq!(binding.refresh(expected), Some(true));
        assert_eq!(binding.refresh(expected), Some(false));
        assert_eq!(binding.refresh(f32::NAN), None);
        assert_eq!(control.count.get(), before);
        assert_eq!(binding.physical(), expected);
        assert_eq!(binding.normalized(), if id == 7 { 1.0 } else { normalized });
    }
}
#[test]
fn failed_begin_update_and_end_preserve_ownership_and_host_display() {
    let descriptors = [descriptor(
        8,
        ParameterRange::new(0.0, 4.0, ParameterScale::Linear, None).unwrap(),
        1.0,
    )];
    let mut binding =
        ParameterAttachment::new(ParameterSchema::new(&descriptors).unwrap(), ParameterId(8))
            .unwrap();
    let control = Control::default();
    assert_eq!(
        binding.update(&control, 2.0),
        Err(AttachmentError::GestureState)
    );
    control.reject.set(true);
    assert_eq!(
        binding.begin(&control),
        Err(AttachmentError::Transport("full"))
    );
    assert!(!binding.gesture_active());
    control.reject.set(false);
    binding.begin(&control).unwrap();
    assert_eq!(binding.begin(&control), Err(AttachmentError::GestureState));
    assert_eq!(
        binding.commit(&control, 1.0),
        Err(AttachmentError::GestureState)
    );
    assert_eq!(
        binding.update_normalized(&control, f32::INFINITY),
        Err(AttachmentError::InvalidValue)
    );
    binding.update_normalized(&control, 2.0).unwrap();
    assert_eq!(control.last.get(), (2, 8, 4.0));
    assert_eq!(binding.physical(), 1.0);
    control.reject.set(true);
    assert_eq!(
        binding.update(&control, 0.0),
        Err(AttachmentError::Transport("full"))
    );
    assert_eq!(
        binding.end(&control),
        Err(AttachmentError::Transport("full"))
    );
    assert!(binding.gesture_active());
    control.reject.set(false);
    binding.end(&control).unwrap();
    assert!(!binding.gesture_active());
    assert_eq!(binding.end(&control), Err(AttachmentError::GestureState));
    binding.commit(&control, -1.0).unwrap();
    assert_eq!(control.last.get(), (0, 8, 0.0));
}
