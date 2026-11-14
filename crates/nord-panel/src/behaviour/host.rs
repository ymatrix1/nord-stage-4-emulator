//! The seam between panel logic and a backend (standalone `AudioHandle` or the
//! plugin `ParamSetter`): `PanelHost` and `PanelAction` (Phase 6b B5, `ns4-q6p7`).
//!
//! Backends only store and fetch values by host parameter id and mark gesture
//! boundaries (DAW automation touch). No locking of the engine happens here.

use super::shift::Trigger;

/// Something the panel wants a backend to do.
#[derive(Debug, Clone, PartialEq)]
pub enum PanelAction {
    /// Set a normalized host parameter.
    Set { id: String, value: f32 },
    /// Start of a continuous edit (knob or fader grabbed).
    BeginGesture { id: String },
    /// End of a continuous edit.
    EndGesture { id: String },
    /// A button gesture on a control, resolved against SHIFT.
    Press { control: String, trigger: Trigger },
}

/// Backend implemented by the standalone app and the plugin editor.
pub trait PanelHost {
    /// Current normalized value of a host parameter, if it exists.
    fn get(&self, id: &str) -> Option<f32>;
    /// Set a normalized host parameter.
    fn set(&mut self, id: &str, value: f32);
    /// Begin a continuous edit.
    fn begin_gesture(&mut self, id: &str);
    /// End a continuous edit.
    fn end_gesture(&mut self, id: &str);
}

/// Apply an action to a host. `Press` is panel-internal and returns `false`
/// (the behaviour layer decides which parameter, if any, it changes).
pub fn dispatch(host: &mut dyn PanelHost, action: &PanelAction) -> bool {
    match action {
        PanelAction::Set { id, value } => host.set(id, *value),
        PanelAction::BeginGesture { id } => host.begin_gesture(id),
        PanelAction::EndGesture { id } => host.end_gesture(id),
        PanelAction::Press { .. } => return false,
    }
    true
}

#[cfg(test)]
pub(crate) mod support {
    use super::PanelHost;
    use std::collections::BTreeMap;

    /// In-memory host that records values and gesture boundaries.
    #[derive(Default)]
    pub struct MemoryHost {
        pub values: BTreeMap<String, f32>,
        pub log: Vec<String>,
    }

    impl PanelHost for MemoryHost {
        fn get(&self, id: &str) -> Option<f32> {
            self.values.get(id).copied()
        }
        fn set(&mut self, id: &str, value: f32) {
            self.values.insert(id.to_string(), value);
            self.log.push(format!("set {id}"));
        }
        fn begin_gesture(&mut self, id: &str) {
            self.log.push(format!("begin {id}"));
        }
        fn end_gesture(&mut self, id: &str) {
            self.log.push(format!("end {id}"));
        }
    }

    #[test]
    fn memory_host_records_calls() {
        let mut h = MemoryHost::default();
        h.begin_gesture("a");
        h.set("a", 0.5);
        h.end_gesture("a");
        assert_eq!(h.get("a"), Some(0.5));
        assert_eq!(h.get("b"), None);
        assert_eq!(h.log, vec!["begin a", "set a", "end a"]);
    }
}

#[cfg(test)]
mod tests {
    use super::support::MemoryHost;
    use super::*;

    #[test]
    fn dispatch_forwards_value_and_gesture_actions() {
        let mut h = MemoryHost::default();
        assert!(dispatch(
            &mut h,
            &PanelAction::BeginGesture { id: "x".into() }
        ));
        assert!(dispatch(
            &mut h,
            &PanelAction::Set {
                id: "x".into(),
                value: 0.25
            }
        ));
        assert!(dispatch(
            &mut h,
            &PanelAction::EndGesture { id: "x".into() }
        ));
        assert_eq!(h.get("x"), Some(0.25));
        assert_eq!(h.log.len(), 3);
    }

    #[test]
    fn press_is_not_a_host_action() {
        let mut h = MemoryHost::default();
        let a = PanelAction::Press {
            control: "organ.on".into(),
            trigger: Trigger::Primary,
        };
        assert!(!dispatch(&mut h, &a));
        assert!(h.log.is_empty());
    }
}
