//! Explicit copied semantic snapshots for custom native editor content.
//!
//! This allocating UI model does not inspect HTML, move focus or certify an
//! assistive technology. Native installation is available only on macOS.

/// Validation or native installation failed before publication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessibilityError {
    /// Malformed ID, label, geometry, typed value or focus.
    InvalidInput,
    /// A declared node or UTF-8 byte bound was exceeded.
    Capacity,
    /// The original attached owner is unavailable or retired.
    Unavailable,
    /// A callback or native notification transition is in progress.
    Busy,
    /// The native revision space was exhausted.
    Exhausted,
    /// Native installation was requested outside the Cocoa UI thread.
    WrongThread,
}
impl std::fmt::Display for AccessibilityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "native accessibility error: {self:?}")
    }
}
impl std::error::Error for AccessibilityError {}

/// Local top-left logical rectangle, independently of window/device scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AccessibilityFrame {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}
impl AccessibilityFrame {
    /// Components and far edges must be finite and within +/-16384; extents
    /// are strictly positive. Negative origins are permitted for clipped content.
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Result<Self, AccessibilityError> {
        if [x, y, width, height, x + width, y + height]
            .into_iter()
            .any(|v| !v.is_finite() || v.abs() > 16384.)
            || width <= 0.
            || height <= 0.
        {
            return Err(AccessibilityError::InvalidInput);
        }
        Ok(Self {
            x,
            y,
            width,
            height,
        })
    }
    /// `(x, y, width, height)` in original local logical coordinates.
    pub fn rectangle(self) -> (f64, f64, f64, f64) {
        (self.x, self.y, self.width, self.height)
    }
}

/// Role-specific copied value. Editable text and value setters are excluded.
#[derive(Clone, Debug, PartialEq)]
pub enum AccessibilityKind {
    StaticText {
        value: String,
    },
    Button,
    Slider {
        minimum: f64,
        value: f64,
        maximum: f64,
        increment: f64,
    },
}
/// Consumer-declared availability, independently of native owner visibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccessibilityAvailability {
    pub enabled: bool,
    pub hidden: bool,
}
impl Default for AccessibilityAvailability {
    fn default() -> Self {
        Self {
            enabled: true,
            hidden: false,
        }
    }
}
/// Checked immutable semantic node. IDs are unique within one snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct AccessibilityNode {
    id: u64,
    label: String,
    frame: AccessibilityFrame,
    availability: AccessibilityAvailability,
    kind: AccessibilityKind,
}
fn string(value: &str, minimum: usize, maximum: usize) -> Result<(), AccessibilityError> {
    if value.len() > maximum {
        return Err(AccessibilityError::Capacity);
    }
    if value.len() < minimum || value.as_bytes().contains(&0) {
        return Err(AccessibilityError::InvalidInput);
    }
    Ok(())
}
impl AccessibilityNode {
    /// Copy a1..1024-byte label and role-specific value. IDs must be nonzero;
    /// static text is0..4096 UTF-8 bytes. Sliders require finite ordered bounds,
    /// an in-range value and a positive finite increment (clamping is consumer-owned).
    pub fn new(
        id: u64,
        label: &str,
        frame: AccessibilityFrame,
        availability: AccessibilityAvailability,
        kind: AccessibilityKind,
    ) -> Result<Self, AccessibilityError> {
        if id == 0 {
            return Err(AccessibilityError::InvalidInput);
        }
        string(label, 1, 1024)?;
        match &kind {
            AccessibilityKind::StaticText { value } => string(value, 0, 4096)?,
            AccessibilityKind::Button => (),
            AccessibilityKind::Slider {
                minimum,
                value,
                maximum,
                increment,
            } => {
                if [*minimum, *value, *maximum, *increment]
                    .into_iter()
                    .any(|v| !v.is_finite())
                    || minimum > value
                    || value > maximum
                    || *increment <= 0.
                {
                    return Err(AccessibilityError::InvalidInput);
                }
            }
        }
        Ok(Self {
            id,
            label: label.to_owned(),
            frame,
            availability,
            kind,
        })
    }
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn frame(&self) -> AccessibilityFrame {
        self.frame
    }
    pub fn availability(&self) -> AccessibilityAvailability {
        self.availability
    }
    pub fn kind(&self) -> &AccessibilityKind {
        &self.kind
    }
}

/// Complete opt-in content hierarchy:1..64 flat nodes and at most65536 combined
/// label/static-value UTF-8 bytes. This replaces automatic content traversal.
#[derive(Clone, Debug, PartialEq)]
pub struct AccessibilitySnapshot {
    label: String,
    nodes: Vec<AccessibilityNode>,
    focused: Option<u64>,
}
impl AccessibilitySnapshot {
    /// Validate the whole model, then copy. Focus is reported consumer state;
    /// it must name an enabled, unhidden button/slider and does not move focus.
    pub fn new(
        label: &str,
        nodes: &[AccessibilityNode],
        focused: Option<u64>,
    ) -> Result<Self, AccessibilityError> {
        string(label, 1, 1024)?;
        if nodes.is_empty() {
            return Err(AccessibilityError::InvalidInput);
        }
        if nodes.len() > 64 {
            return Err(AccessibilityError::Capacity);
        }
        let mut bytes = label.len();
        for (index, node) in nodes.iter().enumerate() {
            if nodes[..index].iter().any(|other| other.id == node.id) {
                return Err(AccessibilityError::InvalidInput);
            }
            bytes += node.label.len();
            if let AccessibilityKind::StaticText { value } = &node.kind {
                bytes += value.len();
            }
            if bytes > 65536 {
                return Err(AccessibilityError::Capacity);
            }
        }
        if let Some(id) = focused
            && !nodes.iter().any(|n| {
                n.id == id
                    && n.availability.enabled
                    && !n.availability.hidden
                    && !matches!(n.kind, AccessibilityKind::StaticText { .. })
            })
        {
            return Err(AccessibilityError::InvalidInput);
        }
        Ok(Self {
            label: label.to_owned(),
            nodes: nodes.to_vec(),
            focused,
        })
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn nodes(&self) -> &[AccessibilityNode] {
        &self.nodes
    }
    pub fn focused_id(&self) -> Option<u64> {
        self.focused
    }
}
/// Native installation revision, scoped to an editor, not an authorization token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccessibilityRevision(pub(crate) u64);
impl AccessibilityRevision {
    pub fn value(self) -> u64 {
        self.0
    }
}
/// Supported discrete native intent. Value/focus setters are unsupported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessibilityActionKind {
    Press,
    Increment,
    Decrement,
}
/// Accepted native intent's original node and installation revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccessibilityAction {
    pub node_id: u64,
    pub revision: AccessibilityRevision,
    pub kind: AccessibilityActionKind,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame() -> AccessibilityFrame {
        AccessibilityFrame::new(0., 0., 100., 20.).unwrap()
    }
    fn node(id: u64) -> AccessibilityNode {
        AccessibilityNode::new(
            id,
            "Gain",
            frame(),
            AccessibilityAvailability::default(),
            AccessibilityKind::Slider {
                minimum: 0.,
                value: 0.5,
                maximum: 1.,
                increment: 0.1,
            },
        )
        .unwrap()
    }
    #[test]
    fn finite_geometry_and_far_edges() {
        assert!(AccessibilityFrame::new(-16384., -16384., 16384., 16384.).is_ok());
        assert!(AccessibilityFrame::new(0., 0., 16384., 16384.).is_ok());
        for [x, y, w, h] in [
            [0., 0., 0., 1.],
            [0., 0., 1., -1.],
            [1., 0., 16384., 1.],
            [0., 0., f64::NAN, 1.],
            [f64::INFINITY, 0., 1., 1.],
        ] {
            assert_eq!(
                AccessibilityFrame::new(x, y, w, h),
                Err(AccessibilityError::InvalidInput)
            );
        }
    }
    #[test]
    fn strict_strings_values_and_ids() {
        let make = |id, label: &str, kind| {
            AccessibilityNode::new(
                id,
                label,
                frame(),
                AccessibilityAvailability::default(),
                kind,
            )
        };
        assert!(make(1, &"é".repeat(512), AccessibilityKind::Button).is_ok());
        assert_eq!(
            make(1, &"é".repeat(513), AccessibilityKind::Button),
            Err(AccessibilityError::Capacity)
        );
        for label in ["", "bad\0label"] {
            assert_eq!(
                make(1, label, AccessibilityKind::Button),
                Err(AccessibilityError::InvalidInput)
            );
        }
        assert_eq!(
            make(0, "Button", AccessibilityKind::Button),
            Err(AccessibilityError::InvalidInput)
        );
        assert!(
            make(
                1,
                "Text",
                AccessibilityKind::StaticText {
                    value: "x".repeat(4096)
                }
            )
            .is_ok()
        );
        assert_eq!(
            make(
                1,
                "Text",
                AccessibilityKind::StaticText {
                    value: "x".repeat(4097)
                }
            ),
            Err(AccessibilityError::Capacity)
        );
        for (minimum, value, maximum, increment) in [
            (0., f64::NAN, 1., 0.1),
            (1., 0., 1., 0.1),
            (0., 2., 1., 0.1),
            (0., 0., 1., 0.),
            (0., 0., f64::INFINITY, 1.),
        ] {
            assert_eq!(
                make(
                    1,
                    "Gain",
                    AccessibilityKind::Slider {
                        minimum,
                        value,
                        maximum,
                        increment
                    }
                ),
                Err(AccessibilityError::InvalidInput)
            );
        }
        assert!(
            make(
                1,
                "Gain",
                AccessibilityKind::Slider {
                    minimum: 1.,
                    value: 1.,
                    maximum: 1.,
                    increment: f64::from_bits(1)
                }
            )
            .is_ok()
        );
    }
    #[test]
    fn snapshots_copy_and_bound_dense_ids_focus() {
        let mut nodes = vec![node(1), node(2)];
        let snapshot = AccessibilitySnapshot::new("Panel", &nodes, Some(1)).unwrap();
        nodes.clear();
        assert_eq!(snapshot.nodes().len(), 2);
        assert_eq!(snapshot.focused_id(), Some(1));
        assert_eq!(
            AccessibilitySnapshot::new("Panel", &[], None),
            Err(AccessibilityError::InvalidInput)
        );
        assert_eq!(
            AccessibilitySnapshot::new("Panel", &[node(1), node(1)], None),
            Err(AccessibilityError::InvalidInput)
        );
        let nodes: Vec<_> = (1..=64).map(node).collect();
        assert!(AccessibilitySnapshot::new("Panel", &nodes, Some(64)).is_ok());
        assert_eq!(
            AccessibilitySnapshot::new("Panel", &(1..=65).map(node).collect::<Vec<_>>(), None),
            Err(AccessibilityError::Capacity)
        );
        for unavailable in [
            AccessibilityAvailability {
                enabled: false,
                hidden: false,
            },
            AccessibilityAvailability {
                enabled: true,
                hidden: true,
            },
        ] {
            let n =
                AccessibilityNode::new(1, "Gain", frame(), unavailable, AccessibilityKind::Button)
                    .unwrap();
            assert!(AccessibilitySnapshot::new("Panel", std::slice::from_ref(&n), None).is_ok());
            assert_eq!(
                AccessibilitySnapshot::new("Panel", &[n], Some(1)),
                Err(AccessibilityError::InvalidInput)
            );
        }
        let n = AccessibilityNode::new(
            1,
            "Status",
            frame(),
            AccessibilityAvailability::default(),
            AccessibilityKind::StaticText {
                value: String::new(),
            },
        )
        .unwrap();
        assert_eq!(
            AccessibilitySnapshot::new("Panel", &[n], Some(1)),
            Err(AccessibilityError::InvalidInput)
        );
        assert_eq!(
            AccessibilitySnapshot::new("Panel", &[node(1)], Some(2)),
            Err(AccessibilityError::InvalidInput)
        );
    }
    #[test]
    fn complete_byte_budget_including_panel_is_exact() {
        let make = |id, value| {
            AccessibilityNode::new(
                id,
                "x",
                frame(),
                AccessibilityAvailability::default(),
                AccessibilityKind::StaticText { value },
            )
            .unwrap()
        };
        let mut nodes: Vec<_> = (1..=15).map(|id| make(id, "x".repeat(4096))).collect();
        nodes.push(make(16, "x".repeat(4079))); // panel1 + labels16 + values65519 ==65536
        assert!(AccessibilitySnapshot::new("P", &nodes, None).is_ok());
        nodes[15] = make(16, "x".repeat(4080));
        assert_eq!(
            AccessibilitySnapshot::new("P", &nodes, None),
            Err(AccessibilityError::Capacity)
        );
    }
}
