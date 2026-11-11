//! 1:1 Nord Stage 4 Front-Panel GUI Library.
//!
//! Spec: `docs/specs/visual-style-and-panel.md`

pub mod audio;
pub mod panel;
pub mod style;
pub mod widgets;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gui_palette_colors() {
        assert_eq!(style::CHASSIS_RED.to_hex(), "#8e1b1fff");
        assert_eq!(style::MODULE_SLATE.to_hex(), "#323d47ff");
        assert_eq!(style::OLED_BG.to_hex(), "#080a0cff");
    }
}
