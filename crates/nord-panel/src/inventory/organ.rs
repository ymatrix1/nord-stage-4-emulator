//! Organ section (manual p.18-22): shared strip plus model, drawbars,
//! vibrato/chorus and percussion.

use super::common::{layer_block, BlockSpec};
use crate::types::{ControlEntry, ControlKind as K, Layer, Section};

const DRAWBAR_IDS: [&str; 9] = [
    "drawbar_1_16",
    "drawbar_2_5_1_3",
    "drawbar_3_8",
    "drawbar_4_4",
    "drawbar_5_2_2_3",
    "drawbar_6_2",
    "drawbar_7_1_3_5",
    "drawbar_8_1_1_3",
    "drawbar_9_1",
];

pub fn controls() -> Vec<ControlEntry> {
    let s = Section::Organ;
    let e = ControlEntry::new;
    let mut v = layer_block(&BlockSpec {
        section: s,
        name: "organ",
        layers: &[Layer::A, Layer::B],
        page: 18,
        ped_page: 18,
        pstick_label: "PSTICK",
        octave_id: Some("organx_octave_{n}"),
        bean: "ns4-h7he",
    });
    v.push(e("organ.model", s, K::Button, "ORGAN MODEL", 18).bound("organ_model"));
    v.push(
        e("organ.model_leds", s, K::Led, "MODEL LEDs (6)", 18)
            .ui("six models: B3, B3 Bass, Vox, Farf, Pipe 1, Pipe 2"),
    );
    v.push(e("organ.preset", s, K::Button, "PRESET", 21).todo("ns4-h7he"));
    v.push(
        e("organ.sync", s, K::Button, "SYNC", 21)
            .shift()
            .todo("ns4-h7he"),
    );
    for (i, host) in DRAWBAR_IDS.iter().enumerate() {
        let n = i + 1;
        v.push(
            e(&format!("organ.drawbar.{n}"), s, K::Drawbar, "DRAWBAR", 19)
                .morph()
                .bound(host),
        );
        v.push(
            e(
                &format!("organ.drawbar_ladder.{n}"),
                s,
                K::LedLadder,
                "DRAWBAR LEDs (1-8)",
                21,
            )
            .ui("shows the stored preset value, unlit in Drawbar Live"),
        );
    }
    v.push(
        e(
            "organ.vibrato_selector",
            s,
            K::Button,
            "VIB/CHORUS V1-C3",
            19,
        )
        .bound("organx_scanner_{n}"),
    );
    v.push(
        e("organ.vibrato_on.a", s, K::Button, "VIB/CHORUS ON A", 19)
            .layer(Layer::A)
            .todo("ns4-h7he"),
    );
    v.push(
        e("organ.vibrato_on.b", s, K::Button, "VIB/CHORUS ON B", 19)
            .layer(Layer::B)
            .todo("ns4-h7he"),
    );
    v.push(e("organ.perc_on", s, K::Button, "PERCUSSION ON", 20).bound("organ_perc_enable"));
    v.push(e("organ.perc_volume", s, K::Button, "VOLUME SOFT", 20).bound("organ_perc_soft"));
    v.push(e("organ.perc_decay", s, K::Button, "DECAY FAST", 20).bound("organ_perc_fast"));
    v.push(e("organ.perc_third", s, K::Button, "HARMONIC THIRD", 20).bound("organ_perc_third"));
    v.push(
        e("organ.perc_poly", s, K::Button, "POLY (Shift+Volume)", 20)
            .shift()
            .todo("ns4-h7he"),
    );
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn organ_has_nine_drawbars_and_nine_ladders() {
        let c = controls();
        assert_eq!(c.iter().filter(|x| x.kind == K::Drawbar).count(), 9);
        assert_eq!(
            c.iter()
                .filter(|x| x.id.starts_with("organ.drawbar_ladder"))
                .count(),
            9
        );
    }

    #[test]
    fn organ_total() {
        assert_eq!(controls().len(), 50);
    }
}
