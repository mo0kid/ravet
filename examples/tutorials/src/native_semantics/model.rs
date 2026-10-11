use ravet_webview::accessibility::*;
use ravet_webview::{Callbacks, StandaloneEditor};
use std::{cell::RefCell, rc::Rc};
#[derive(Clone)]
pub(crate) struct Model {
    pub(crate) gain: f64,
    pub(crate) revision: u32,
    pub(crate) native_revision: Option<AccessibilityRevision>,
    pub(crate) geometry: Option<[AccessibilityFrame; 3]>,
    pub(crate) native_actions: u32,
    pub(crate) messages: u32,
    pub(crate) refuse: bool,
}
impl Model {
    pub(crate) fn new() -> Self {
        Self {
            gain: 0.5,
            revision: 0,
            native_revision: None,
            geometry: None,
            native_actions: 0,
            messages: 0,
            refuse: false,
        }
    }
    fn set_gain(&mut self, value: f64) -> bool {
        if !value.is_finite() || !(0.0..=1.).contains(&value) {
            return false;
        }
        if value == self.gain {
            return true;
        }
        if let Some(next) = self.revision.checked_add(1) {
            self.gain = value;
            self.revision = next;
            return true;
        }
        false
    }
    pub(crate) fn semantics(&self) -> Result<AccessibilitySnapshot, AccessibilityError> {
        let frames = self.geometry.ok_or(AccessibilityError::Unavailable)?;
        let availability = AccessibilityAvailability::default();
        let nodes = [
            AccessibilityNode::new(
                1,
                "Output status",
                frames[0],
                availability,
                AccessibilityKind::StaticText {
                    value: format!("Gain {:.2}", self.gain),
                },
            )?,
            AccessibilityNode::new(
                2,
                "Gain",
                frames[1],
                availability,
                AccessibilityKind::Slider {
                    minimum: 0.,
                    value: self.gain,
                    maximum: 1.,
                    increment: 0.1,
                },
            )?,
            AccessibilityNode::new(
                3,
                "Reset gain",
                frames[2],
                availability,
                AccessibilityKind::Button,
            )?,
        ];
        AccessibilitySnapshot::new("Custom output control", &nodes, None)
    }
}
pub(crate) struct Bridge(pub(crate) Rc<RefCell<Model>>);
impl Callbacks for Bridge {
    fn message(&mut self, json: &str) {
        if json.len() > 512 {
            return;
        }
        if let Some(value) = json
            .strip_prefix("{\"gain\":")
            .and_then(|s| s.strip_suffix('}'))
            .and_then(|s| s.parse::<f64>().ok())
        {
            let mut model = self.0.borrow_mut();
            model.messages = model.messages.saturating_add(1);
            model.set_gain(value);
            return;
        }
        if let Some(text) = json
            .strip_prefix("{\"geometry\":\"")
            .and_then(|s| s.strip_suffix("\"}"))
        {
            let rows: Vec<_> = text.split('|').collect();
            if rows.len() != 3 {
                return;
            }
            let mut result = Vec::with_capacity(3);
            for row in rows {
                let values: Vec<_> = row.split(',').map(str::parse::<f64>).collect();
                let [Ok(x), Ok(y), Ok(w), Ok(h)] = values.as_slice() else {
                    return;
                };
                let Ok(frame) = AccessibilityFrame::new(*x, *y, *w, *h) else {
                    return;
                };
                result.push(frame);
            }
            self.0.borrow_mut().geometry = Some(result.try_into().expect("checked rows"));
        }
    }
    fn snapshot(&mut self) -> Option<String> {
        let model = self.0.borrow();
        Some(format!(
            "{{\"gain\":{},\"revision\":{}}}",
            model.gain, model.revision
        ))
    }
    fn accessibility_action(&mut self, action: AccessibilityAction) -> bool {
        let mut model = self.0.borrow_mut();
        if model.refuse || model.native_revision != Some(action.revision) {
            return false;
        }
        let next = match (action.node_id, action.kind) {
            (3, AccessibilityActionKind::Press) => 0.5,
            (2, AccessibilityActionKind::Increment) => (model.gain + 0.1).min(1.),
            (2, AccessibilityActionKind::Decrement) => (model.gain - 0.1).max(0.),
            _ => return false,
        };
        if !model.set_gain(next) {
            return false;
        }
        model.native_actions = model.native_actions.saturating_add(1);
        true
    }
}
pub(crate) fn publish(
    editor: &StandaloneEditor<'_>,
    model: &Rc<RefCell<Model>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = model.borrow().semantics()?;
    let revision = editor.set_accessibility(Some(&snapshot))?;
    model.borrow_mut().native_revision = Some(revision);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn model() -> Rc<RefCell<Model>> {
        let state = Rc::new(RefCell::new(Model::new()));
        state.borrow_mut().geometry =
            Some([AccessibilityFrame::new(1., 2., 100., 20.).unwrap(); 3]);
        state
    }
    #[test]
    fn copied_complete_semantics_and_invalid_geometry_message_preserve_source() {
        let state = model();
        let snapshot = state.borrow().semantics().unwrap();
        let before = state.borrow().geometry;
        let mut bridge = Bridge(state.clone());
        bridge.message("{\"geometry\":\"1,2,100,20|1,2,100,20|1,2,NaN,20\"}");
        assert_eq!(state.borrow().geometry, before);
        assert_eq!(snapshot.nodes().len(), 3);
        assert_eq!(
            snapshot.nodes()[1].kind(),
            &AccessibilityKind::Slider {
                minimum: 0.,
                value: 0.5,
                maximum: 1.,
                increment: 0.1
            }
        );
        bridge.message("{\"gain\":0.75}");
        assert_eq!(state.borrow().gain, 0.75);
        assert_eq!(state.borrow().revision, 1);
        assert_eq!(
            snapshot.nodes()[1].kind(),
            &AccessibilityKind::Slider {
                minimum: 0.,
                value: 0.5,
                maximum: 1.,
                increment: 0.1
            }
        );
        for value in ["{\"gain\":2}", "{\"gain\":NaN}", "{\"gain\":null}"] {
            bridge.message(value);
            assert_eq!(state.borrow().gain, 0.75);
            assert_eq!(state.borrow().revision, 1);
        }
    }
    #[test]
    fn whole_model_revision_exhaustion_and_noops() {
        let mut state = Model::new();
        state.revision = u32::MAX;
        assert!(state.set_gain(0.5));
        assert!(!state.set_gain(0.6));
        assert_eq!(state.gain, 0.5);
        assert_eq!(state.revision, u32::MAX);
        assert!(!state.set_gain(f64::NAN));
        assert_eq!(state.gain, 0.5);
    }
}
