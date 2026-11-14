//! Control block shared by the Organ, Piano and Synth sections.
//!
//! Each of these sections has the same strip: ON, SOLO, FX FOCUS LED, one
//! button + focus LED + fader + LED ladder + Aux KB LED per layer, OCTAVE
//! SHIFT, KB ZONE (Shift) with four zone LEDs, SUSTPED and PSTICK (Shift)
//! (manual p.18, p.23, p.27-28).

use crate::types::{ControlEntry, ControlKind as K, Layer, Section};

/// Per-section constants for [`layer_block`].
pub struct BlockSpec {
    pub section: Section,
    /// Id prefix, `organ` / `piano` / `synth`.
    pub name: &'static str,
    pub layers: &'static [Layer],
    /// Manual page of the "ON/OFF and level" text.
    pub page: u16,
    /// Manual page of the SUSTPED/PSTICK text.
    pub ped_page: u16,
    pub pstick_label: &'static str,
    /// Host id of the per-layer octave parameter (`_{n}` suffix), if the section has one.
    pub octave_id: Option<&'static str>,
    /// Bean tracking the missing section-level behaviour.
    pub bean: &'static str,
}

fn lid(l: Layer) -> &'static str {
    match l {
        Layer::A => "a",
        Layer::B => "b",
        Layer::C => "c",
    }
}

/// Host parameter id of a layer's enable flag.
fn enabled_id(name: &str, l: Layer) -> &'static str {
    match (name, l) {
        ("organ", Layer::A) => "organ_a_enabled",
        ("organ", _) => "organ_b_enabled",
        ("piano", Layer::A) => "piano_a_enabled",
        ("piano", _) => "piano_b_enabled",
        (_, Layer::A) => "synth_a_enabled",
        (_, Layer::B) => "synth_b_enabled",
        (_, Layer::C) => "synth_c_enabled",
    }
}

/// Host parameter id of a layer's level.
fn level_id(name: &str, l: Layer) -> &'static str {
    match (name, l) {
        ("organ", Layer::A) => "organ_a_level",
        ("organ", _) => "organ_b_level",
        ("piano", Layer::A) => "piano_a_level",
        ("piano", _) => "piano_b_level",
        (_, Layer::A) => "synth_a_level",
        (_, Layer::B) => "synth_b_level",
        (_, Layer::C) => "synth_c_level",
    }
}

/// Engine path of the per-layer keyboard zone assignment (`Stage4Engine::set_layer_zone_leds`).
const ZONE_PATH: &str = "engine:split_router.layer_zone_masks";

/// Host id of the section's Aux KB layer mask (`auxkb_<section>`), when the engine honours it.
fn aux_id(name: &str) -> Option<&'static str> {
    match name {
        "organ" => Some("auxkb_organ"),
        "piano" => Some("auxkb_piano"),
        _ => None,
    }
}

/// The Organ and Piano strips drive section ON (all layers), KB ZONE and the Aux KB flags; the
/// Synth strip still lists them as unsupported.
fn zones_wired(name: &str) -> bool {
    matches!(name, "organ" | "piano")
}

/// Build the shared strip for one section.
pub fn layer_block(b: &BlockSpec) -> Vec<ControlEntry> {
    let (s, n, p) = (b.section, b.name, b.page);
    let id = |tail: &str| format!("{n}.{tail}");
    let mut v = vec![
        {
            let on = ControlEntry::new(&id("on"), s, K::Button, "ON", p);
            // Section ON switches every layer of the section off, or back on, so it is bound to
            // the layer enable parameters.
            if zones_wired(n) {
                on.bound(enabled_id(n, Layer::A))
            } else {
                on.todo(b.bean)
            }
        },
        ControlEntry::new(&id("solo"), s, K::Button, "SOLO (hold ON)", p)
            .hold()
            .ui("temporary monitor mode, not stored in the program (p.43)"),
        ControlEntry::new(&id("fx_focus_led"), s, K::Led, "FX FOCUS", p)
            .ui("mirrors which section's effects are focused"),
    ];
    for &l in b.layers {
        let t = lid(l);
        v.push(
            ControlEntry::new(&id(&format!("layer.{t}")), s, K::Button, "LAYER", p)
                .layer(l)
                .bound(enabled_id(n, l)),
        );
        v.push(
            ControlEntry::new(&id(&format!("focus_led.{t}")), s, K::Led, "LAYER FOCUS", p)
                .layer(l)
                .ui("blinking LED shows the focused layer"),
        );
        v.push(
            ControlEntry::new(&id(&format!("fader.{t}")), s, K::Fader, "LEVEL", p)
                .layer(l)
                .morph()
                .bound(level_id(n, l)),
        );
        v.push(
            ControlEntry::new(
                &id(&format!("ladder.{t}")),
                s,
                K::LedLadder,
                "LEVEL LEDs",
                p,
            )
            .layer(l)
            .assumed("LED count on the fader ladder is not stated in the manual text")
            .ui("shows the morphed level (telemetry, B10)"),
        );
        let aux = ControlEntry::new(&id(&format!("aux_led.{t}")), s, K::Led, "AUX KB", 45).layer(l);
        v.push(match aux_id(n) {
            Some(host) => aux.bound(host),
            None => aux.todo("ns4-ymci"),
        });
    }
    for (tail, label) in [
        ("octave_down", "OCTAVE SHIFT -"),
        ("octave_up", "OCTAVE SHIFT +"),
    ] {
        let e = ControlEntry::new(&id(tail), s, K::Button, label, p);
        v.push(match b.octave_id {
            Some(host) => e.bound(host),
            None => e.todo(b.bean),
        });
    }
    let zone = |e: ControlEntry| {
        if zones_wired(n) {
            e.bound(ZONE_PATH)
        } else {
            e.todo(b.bean)
        }
    };
    v.push(zone(
        ControlEntry::new(&id("kb_zone_down"), s, K::Button, "KB ZONE -", p).shift(),
    ));
    v.push(zone(
        ControlEntry::new(&id("kb_zone_up"), s, K::Button, "KB ZONE +", p).shift(),
    ));
    v.push(zone(ControlEntry::new(
        &id("zone_leds"),
        s,
        K::Led,
        "KB ZONE LEDs (4 green)",
        p,
    )));
    v.push(
        ControlEntry::new(&id("sustped"), s, K::Button, "SUSTPED", b.ped_page)
            .shift()
            .bound("ped_sustped_{n}"),
    );
    v.push(
        ControlEntry::new(&id("pstick"), s, K::Button, b.pstick_label, b.ped_page)
            .shift()
            .bound("ped_pstick_{n}"),
    );
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORGAN: BlockSpec = BlockSpec {
        section: Section::Organ,
        name: "organ",
        layers: &[Layer::A, Layer::B],
        page: 18,
        ped_page: 18,
        pstick_label: "PSTICK",
        octave_id: None,
        bean: "ns4-h7he",
    };

    #[test]
    fn two_layer_block_has_20_entries() {
        assert_eq!(layer_block(&ORGAN).len(), 20);
    }

    #[test]
    fn three_layer_block_adds_five_per_layer() {
        let synth = BlockSpec {
            layers: &[Layer::A, Layer::B, Layer::C],
            section: Section::Synth,
            name: "synth",
            ..ORGAN
        };
        assert_eq!(layer_block(&synth).len(), 25);
    }

    #[test]
    fn organ_and_piano_strips_are_bound_but_synth_is_not() {
        use crate::types::Binding;
        let organ = BlockSpec {
            octave_id: Some("organx_octave_{n}"),
            ..ORGAN
        };
        let piano = BlockSpec {
            section: Section::Piano,
            name: "piano",
            octave_id: Some("pianox_octave_{n}"),
            ..ORGAN
        };
        let synth = BlockSpec {
            layers: &[Layer::A, Layer::B, Layer::C],
            section: Section::Synth,
            name: "synth",
            ..ORGAN
        };
        let unsupported = |v: Vec<ControlEntry>| {
            v.into_iter()
                .filter(|c| matches!(c.binding, Binding::Unsupported(_)))
                .map(|c| c.id)
                .collect::<Vec<_>>()
        };
        assert!(unsupported(layer_block(&organ)).is_empty());
        assert!(unsupported(layer_block(&piano)).is_empty());
        let s = unsupported(layer_block(&synth));
        for id in [
            "synth.on",
            "synth.kb_zone_down",
            "synth.kb_zone_up",
            "synth.zone_leds",
            "synth.aux_led.a",
            "synth.aux_led.c",
        ] {
            assert!(s.contains(&id.to_string()), "{id} stays unsupported");
        }
    }

    #[test]
    fn layer_ids_map_to_host_params() {
        assert_eq!(enabled_id("synth", Layer::C), "synth_c_enabled");
        assert_eq!(level_id("piano", Layer::B), "piano_b_level");
        assert_eq!(lid(Layer::C), "c");
    }
}
