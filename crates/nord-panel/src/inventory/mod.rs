//! The control inventory, one file per section (`docs/specs/control-inventory.md`).

mod common;
mod effects;
mod keybed;
mod master;
mod organ;
mod piano;
mod program;
mod program_perf;
mod synth;
mod synth_perf;

use crate::types::ControlEntry;

/// Every front-panel control, in panel order (left to right).
pub fn all_controls() -> Vec<ControlEntry> {
    let mut v = Vec::new();
    v.extend(master::controls());
    v.extend(organ::controls());
    v.extend(piano::controls());
    v.extend(program::controls());
    v.extend(program_perf::controls());
    v.extend(synth::controls());
    v.extend(synth_perf::controls());
    v.extend(effects::controls());
    v.extend(keybed::controls());
    v
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod order_tests {
    use super::all_controls;
    use crate::types::Section;

    #[test]
    fn sections_appear_in_panel_groups() {
        let all = all_controls();
        assert_eq!(all.first().map(|c| c.section), Some(Section::Master));
        assert_eq!(all.last().map(|c| c.section), Some(Section::Keybed));
    }
}
