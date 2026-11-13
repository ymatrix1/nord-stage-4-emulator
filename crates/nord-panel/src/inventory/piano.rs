//! Piano section (manual p.23-26): shared strip plus Piano Select, acoustics,
//! touch, compression, timbre and unison.

use super::common::{layer_block, BlockSpec};
use crate::types::{ControlEntry, ControlKind as K, Layer, Section};

pub fn controls() -> Vec<ControlEntry> {
    let s = Section::Piano;
    let e = ControlEntry::new;
    let mut v = layer_block(&BlockSpec {
        section: s,
        name: "piano",
        layers: &[Layer::A, Layer::B],
        page: 23,
        ped_page: 23,
        pstick_label: "PSTICK",
        octave_id: Some("pianox_octave_{n}"),
        bean: "ns4-h7he",
    });
    v.push(e("piano.type", s, K::Button, "TYPE", 24).bound("piano_category"));
    v.push(
        e("piano.type_leds", s, K::Led, "TYPE LEDs (6)", 24)
            .ui("Grand, Upright, Electric, Clav, Digital, Misc"),
    );
    v.push(e("piano.select_dial", s, K::Encoder, "PIANO SELECT", 24).todo("ns4-h7he"));
    v.push(
        e("piano.list", s, K::Encoder, "LIST (Shift+dial)", 25)
            .shift()
            .ui("list view of models"),
    );
    v.push(
        e("piano.info", s, K::Button, "INFO", 25)
            .shift()
            .ui("model version and size screen"),
    );
    v.push(
        e("piano.soft_release", s, K::Button, "SOFT RELEASE", 25).bound("pianox_soft_release_{n}"),
    );
    v.push(e("piano.string_res", s, K::Button, "STRING RES", 25).bound("piano_string_res"));
    v.push(
        e("piano.ped_noise", s, K::Button, "PED NOISE", 25)
            .shift()
            .bound("piano_pedal_noise"),
    );
    v.push(e("piano.kb_touch", s, K::Button, "KB TOUCH", 25).bound("piano_touch"));
    v.push(e("piano.dyn_comp", s, K::Button, "DYN COMP 1/2/3", 25).bound("pianox_dyn_comp_{n}"));
    v.push(e("piano.timbre", s, K::Button, "TIMBRE", 26).bound("piano_timbre"));
    v.push(e("piano.unison", s, K::Button, "UNISON 1/2/3", 26).bound("pianox_unison_{n}"));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn piano_total() {
        assert_eq!(controls().len(), 32);
    }

    #[test]
    fn piano_has_two_faders() {
        assert_eq!(controls().iter().filter(|x| x.kind == K::Fader).count(), 2);
    }
}
