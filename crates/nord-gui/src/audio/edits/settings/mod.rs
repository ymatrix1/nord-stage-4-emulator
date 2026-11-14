//! Menu setting edits applied on the DSP thread (`ProgramEdit::Settings`).
//!
//! The Program bay's menu screens (`nord_panel::menus`, manual OS v1.6x p.57-64) write one item at
//! a time: [`SettingsEdit`] names the item by its stable model id and carries the new
//! [`Value`]. `apply` routes it to the engine's own setter: `settings` (System/Sound/Output/Aux
//! KB), `pedals` and `pedal_assign` (Pedal), `extern_out` and the channel setters (MIDI, Extern).
//! Everything here is plain assignments on pre-allocated engine fields: no heap allocation, no
//! lock, no I/O, so it is safe to run inside the audio callback.
//!
//! Provenance: the id to field mapping follows the host parameter docs in `nord-plugin`
//! (`settings_params.rs`, `aux_pedal_params.rs`); the index to enum mapping follows the choice
//! lists of the model (`nord_panel::menus`), each pinned by a test in the submodules.

use nord_core::engine::Stage4Engine;
use nord_panel::menus::Value;

mod channels;
mod global;
mod pedal;

/// One menu item write.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SettingsEdit {
    /// Stable item id of the model (`menu.page.name`).
    pub id: &'static str,
    /// New value; its variant matches the item kind.
    pub value: Value,
}

impl SettingsEdit {
    /// Writes the item to the engine. Returns `false` when no engine field backs `id`.
    pub fn apply(&self, engine: &mut Stage4Engine) -> bool {
        global::apply(self, engine) || pedal::apply(self, engine) || channels::apply(self, engine)
    }

    /// Choice index (also the On/Off of a toggle: 0 / 1); `0` for other kinds.
    pub(super) fn index(&self) -> usize {
        match self.value {
            Value::Index(i) => i,
            Value::Bool(b) => usize::from(b),
            Value::Int(i) => usize::try_from(i).unwrap_or(0),
            _ => 0,
        }
    }

    /// Integer value (rounds a float); `0` for other kinds.
    pub(super) fn int(&self) -> i32 {
        match self.value {
            Value::Int(i) => i,
            Value::Index(i) => i32::try_from(i).unwrap_or(0),
            Value::Float(f) if f.is_finite() => f.round() as i32,
            Value::Bool(b) => i32::from(b),
            Value::Float(_) | Value::None => 0,
        }
    }

    /// Float value; `0.0` for other kinds or a non-finite number.
    pub(super) fn float(&self) -> f32 {
        match self.value {
            Value::Float(f) if f.is_finite() => f,
            Value::Int(i) => i as f32,
            _ => 0.0,
        }
    }

    /// Boolean value (a non-zero index or int counts as on).
    pub(super) fn on(&self) -> bool {
        match self.value {
            Value::Bool(b) => b,
            _ => self.index() != 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(id: &'static str, value: Value) -> SettingsEdit {
        SettingsEdit { id, value }
    }

    #[test]
    fn value_accessors_coerce_between_kinds() {
        assert_eq!(edit("x", Value::Index(3)).index(), 3);
        assert_eq!(edit("x", Value::Bool(true)).index(), 1);
        assert_eq!(edit("x", Value::Int(-4)).int(), -4);
        assert_eq!(edit("x", Value::Float(2.6)).int(), 3);
        assert_eq!(edit("x", Value::Int(2)).float(), 2.0);
        assert_eq!(edit("x", Value::Float(f32::NAN)).float(), 0.0);
        assert!(edit("x", Value::Index(1)).on() && !edit("x", Value::None).on());
    }

    #[test]
    fn unknown_ids_are_reported_not_applied() {
        let mut e = Stage4Engine::new(48_000.0);
        assert!(!edit("nope.1.nothing", Value::Int(1)).apply(&mut e));
    }
}
