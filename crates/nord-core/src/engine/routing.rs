//! Opt-in firmware layer routing: which layers sound and which keys each layer accepts.
//!
//! Spec: `docs/specs/morph-and-zones.md` section 6, `docs/research/engine-routing.md`.
//!
//! The legacy engine falls back to default layer masks when a decoded scene mask is empty, ignores
//! the per-layer keyboard zone code (`Edt_*_kKbZone`) and gates every split point open when no zone
//! flag is set. With [`FirmwareRouting::On`] the engine follows the program as the firmware reads it:
//!
//! 1. **Layer on** = section enable AND layer enable of the active scene. VERIFIED on the ARM
//!    firmware (`tools/arm_program_routing.py`): the zone enable byte of layer `i` is
//!    `section.enable & (section.layer_mask >> i) & 1`, for the organ, piano and synth sections.
//!    The decode already combines the file's `section_enable_i / ii` with the layer enables
//!    (`combine_scene_mask`); an all-zero scene mask now means silence instead of a fallback.
//! 2. **Layer accepts note** = the note lies in the key range the firmware derives from the layer's
//!    zone code and the split state ([`zone_table`], VERIFIED against the ARM functions and the zone
//!    objects of all 384 factory programs): lower and upper bound with their crossfade widths
//!    (0, 6, 12 keys half width) and the smaller of the two firmware fade curves as gain.

use super::zone_table;
use super::*;
use crate::cbin::kb_zone;
use crate::morph::SplitPoint;

/// Layer routing mode (`ns4 render-patch --firmware-routing`). `Off` keeps the legacy behaviour
/// and the golden `render_hash` ([D22](../../../../docs/DECISIONS.md)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FirmwareRouting {
    /// Legacy: default-mask fallbacks, all layers accept all keys unless the router masks say otherwise.
    #[default]
    Off,
    /// Section / layer enables and per-layer KB zones as decoded from the program.
    On,
}

/// Routing facts of the last applied program (pure data, mode independent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramRouting {
    /// A program was applied (before that the engine keeps its own scene masks in both modes).
    pub loaded: bool,
    /// Scene I / II masks as decoded (bit `i` = layer `i`), zero means silent.
    pub exact_masks: [u8; 2],
    /// Scene I / II masks of the legacy path (with the default-mask fallbacks).
    pub legacy_masks: [u8; 2],
    /// Raw `kKbZone` code per layer (Organ A, B, Piano A, B, Synth A, B, C).
    pub kb_zone: [u8; 7],
    /// `split_enabled` and the Low / Mid / High point flag.
    pub split_active: [bool; 3],
}

impl Default for ProgramRouting {
    fn default() -> Self {
        Self {
            loaded: false,
            exact_masks: [0; 2],
            legacy_masks: [0; 2],
            kb_zone: kb_zone::default_codes(),
            split_active: [false; 3],
        }
    }
}

/// LED zone set (bit `u` = zone `u + 1`) of each firmware case 0..=10 (case 10 = empty range).
const CASE_LEDS: [u8; 11] = [
    0b0001, 0b0010, 0b0100, 0b1000, 0b0011, 0b0110, 0b1100, 0b0111, 0b1110, 0b1111, 0,
];

/// LED zone set (bit `u` = zone `u + 1`) of a `kKbZone` code when all three split points are active.
///
/// VERIFIED: row 7 of the firmware case table `0xC02CB230` ([`zone_table::CASE_TABLE`], read back
/// from the ARM image by `tools/arm_zone_map.py --check`): codes `0..=11` cover zone 1, zone 2,
/// zones 1-2, zones 2-3, zones 1-3, all, zones 2-4, zones 2-3, zones 3-4, zone 3, zone 4, zone 4.
/// With fewer active points the firmware joins the zones next to an inactive point
/// ([`zone_table::zone_range`] does exactly that).
pub const fn zone_leds(code: u8) -> u8 {
    let code = if code > 11 { 11 } else { code as usize };
    CASE_LEDS[zone_table::CASE_TABLE[7][code][0] as usize]
}

impl ProgramRouting {
    /// Key range of `layer` from its zone code and the split points of the program.
    ///
    /// The point keys map to the nearest position setting (`SPLIT_NOTES`), the crossfade enums to
    /// the width settings `0 / 1 / 2` (0, 6, 12 keys half width).
    pub fn zone_range(&self, layer: LayerId, points: &[SplitPoint; 3]) -> zone_table::ZoneRange {
        let pos = points.map(|p| zone_table::split_setting_of(p.midi_note));
        let widths = points.map(|p| match p.crossfade {
            crate::morph::CrossfadeWidth::Off => 0,
            crate::morph::CrossfadeWidth::Small => 1,
            crate::morph::CrossfadeWidth::Large => 2,
        });
        zone_table::zone_range(
            self.kb_zone[layer as usize],
            self.split_active,
            pos,
            widths,
            0,
        )
    }

    /// Gain (`0.0..=1.0`) of `layer` for the key `note`: the firmware's smaller-of-two-curves gain.
    pub fn zone_gain(&self, layer: LayerId, note: u8, points: &[SplitPoint; 3]) -> f32 {
        self.zone_range(layer, points).gain(note)
    }
}

impl Stage4Engine {
    /// Current layer routing mode.
    pub fn firmware_routing(&self) -> FirmwareRouting {
        self.firmware_routing
    }

    /// Selects the routing mode; the scene masks of the applied program follow the mode.
    pub fn set_firmware_routing(&mut self, mode: FirmwareRouting) {
        self.firmware_routing = mode;
        self.install_scene_masks();
    }

    /// Routing facts of the last applied program.
    pub fn program_routing(&self) -> &ProgramRouting {
        &self.routing
    }

    /// Records the routing facts of `prog` (called at the end of `apply_program`, after the split
    /// points and legacy scene masks are in place).
    pub(super) fn record_program_routing(&mut self, prog: &crate::cbin::ProgramDecoded) {
        self.routing = ProgramRouting {
            loaded: true,
            exact_masks: [prog.global.scene_1_mask, prog.global.scene_2_mask],
            legacy_masks: [self.scene.scene_1_mask, self.scene.scene_2_mask],
            kb_zone: prog.global.kb_zone,
            split_active: std::array::from_fn(|i| {
                prog.global.split_enabled && prog.global.split_zone_enabled[i]
            }),
        };
        self.install_scene_masks();
    }

    fn install_scene_masks(&mut self) {
        if !self.routing.loaded {
            return;
        }
        let [m1, m2] = match self.firmware_routing {
            FirmwareRouting::Off => self.routing.legacy_masks,
            FirmwareRouting::On => self.routing.exact_masks,
        };
        self.scene.scene_1_mask = m1;
        self.scene.scene_2_mask = m2;
    }

    /// Keybed gain of `layer` for the played key (split crossfade or firmware KB zone).
    pub(super) fn keybed_gain(&self, layer: LayerId, note: u8) -> f32 {
        match self.firmware_routing {
            FirmwareRouting::Off => self.split_router.note_gain(layer, note),
            FirmwareRouting::On => self
                .routing
                .zone_gain(layer, note, &self.split_router.splits),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::morph::CrossfadeWidth;

    fn points(mid: u8, width: CrossfadeWidth) -> [SplitPoint; 3] {
        let p = |note, crossfade| SplitPoint {
            enabled: true,
            midi_note: note,
            crossfade,
        };
        [
            p(48, CrossfadeWidth::Off),
            p(mid, width),
            p(84, CrossfadeWidth::Off),
        ]
    }

    fn mid_only(codes: [u8; 7]) -> ProgramRouting {
        ProgramRouting {
            kb_zone: codes,
            split_active: [false, true, false],
            ..ProgramRouting::default()
        }
    }

    #[test]
    fn zone_codes_map_to_led_sets_of_the_firmware_table() {
        let want = [
            0b0001, 0b0010, 0b0011, 0b0110, 0b0111, 0b1111, 0b1110, 0b0110, 0b1100, 0b0100, 0b1000,
            0b1000,
        ];
        for (code, leds) in want.iter().enumerate() {
            assert_eq!(zone_leds(code as u8), *leds, "code {code}");
        }
        assert_eq!(zone_leds(200), zone_leds(11));
    }

    #[test]
    fn mid_only_split_gives_lower_and_upper_halves() {
        let r = mid_only([0, 6, 5, 5, 5, 5, 5]);
        let pts = points(65, CrossfadeWidth::Off);
        assert_eq!(r.zone_gain(LayerId::OrganA, 64, &pts), 1.0);
        assert_eq!(r.zone_gain(LayerId::OrganA, 65, &pts), 0.0);
        assert_eq!(r.zone_gain(LayerId::OrganB, 64, &pts), 0.0);
        assert_eq!(r.zone_gain(LayerId::OrganB, 65, &pts), 1.0);
        assert_eq!(r.zone_gain(LayerId::PianoA, 40, &pts), 1.0);
        assert_eq!(r.zone_gain(LayerId::PianoA, 100, &pts), 1.0);
    }

    #[test]
    fn no_active_split_accepts_every_key() {
        let r = ProgramRouting {
            kb_zone: [0, 6, 4, 2, 5, 9, 1],
            ..ProgramRouting::default()
        };
        let pts = points(65, CrossfadeWidth::Off);
        for layer in LayerId::ALL {
            assert_eq!(r.zone_gain(layer, 30, &pts), 1.0);
            assert_eq!(r.zone_gain(layer, 100, &pts), 1.0);
        }
    }

    #[test]
    fn inactive_low_point_joins_zones_one_and_two() {
        // Mid (60) and High (72) active: regions {1,2} | {3} | {4}.
        let r = ProgramRouting {
            kb_zone: [0, 0, 9, 10, 0, 5, 5],
            split_active: [false, true, true],
            ..ProgramRouting::default()
        };
        let mut pts = points(60, CrossfadeWidth::Off);
        pts[2].midi_note = 72;
        // code 0 covers the joined lower region (zones 1-2)
        for layer in [LayerId::OrganA, LayerId::OrganB, LayerId::SynthA] {
            assert_eq!(r.zone_gain(layer, 59, &pts), 1.0);
            assert_eq!(r.zone_gain(layer, 60, &pts), 0.0);
        }
        // code 9 = zone 3 only: the 60..71 region
        assert_eq!(r.zone_gain(LayerId::PianoA, 59, &pts), 0.0);
        assert_eq!(r.zone_gain(LayerId::PianoA, 65, &pts), 1.0);
        assert_eq!(r.zone_gain(LayerId::PianoA, 72, &pts), 0.0);
        // code 10 = zone 4 only
        assert_eq!(r.zone_gain(LayerId::PianoB, 72, &pts), 1.0);
        assert_eq!(r.zone_gain(LayerId::PianoB, 71, &pts), 0.0);
    }

    #[test]
    fn crossfade_gain_is_the_smaller_firmware_curve() {
        let r = mid_only([0, 6, 5, 5, 5, 5, 5]);
        let pts = points(60, CrossfadeWidth::Small);
        // width setting 1: 6 keys half width around key 60 (the fade covers keys 54..=65)
        assert_eq!(r.zone_gain(LayerId::OrganB, 60, &pts), 0.5);
        assert_eq!(r.zone_gain(LayerId::OrganA, 53, &pts), 1.0);
        assert_eq!(r.zone_gain(LayerId::OrganB, 53, &pts), 0.0);
        assert_eq!(r.zone_gain(LayerId::OrganB, 66, &pts), 1.0);
        assert_eq!(r.zone_gain(LayerId::OrganA, 66, &pts), 0.0);
        for note in 54..=65 {
            let hi = r.zone_gain(LayerId::OrganB, note, &pts);
            assert!(hi > 0.0 && hi < 1.0, "note {note}");
        }
    }

    #[test]
    fn default_routing_is_off_and_unloaded() {
        let e = Stage4Engine::new(48_000.0);
        assert_eq!(e.firmware_routing(), FirmwareRouting::Off);
        assert!(!e.program_routing().loaded);
    }
}
