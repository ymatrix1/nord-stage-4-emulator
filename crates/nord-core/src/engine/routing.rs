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
//! 2. **Layer accepts note** = the note lies in a region of the layer's zone set. A layer's code maps
//!    to a set of the four LED zones ([`zone_leds`]); the active split points (`Low`, `Mid`, `High`
//!    flags of `split_zone_enabled`) cut the keyboard into regions, and an inactive point joins the
//!    LED zones next to it (manual p.39: with only the Mid point active, the two leftmost LEDs are
//!    the lower region and the two rightmost the upper one). Crossfades keep the existing widths.

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
    /// Scene I / II masks as the firmware routing uses them (bit `i` = layer `i`): the decoded
    /// masks, except that a scene whose enabled sections have no layer on plays layer A of each
    /// ([`fill_empty_scene`], OQ25). Zero means silent (no section enabled).
    pub exact_masks: [u8; 2],
    /// Scene I / II masks of the legacy path (with the default-mask fallbacks).
    pub legacy_masks: [u8; 2],
    /// Raw `kKbZone` code per layer (Organ A, B, Piano A, B, Synth A, B, C).
    pub kb_zone: [u8; 7],
    /// `split_enabled` and the Low / Mid / High point flag.
    pub split_active: [bool; 3],
}

/// Layer A of every enabled section when a scene would otherwise be silent (OQ25).
///
/// 9 of the 384 factory programs (`Vox Like Me`, `Vox Rising Sun`, `Vox Inna-Gadda`, `Vox Im Down`,
/// `Vox DC5`, `Burning Lead`, `LoFi Piano`, `Farfisa II`, `70s CP Strings`) store an enabled
/// section with an all-zero layer enable (`kLayerEnable*_I` = 0) and no other layer in scene I.
/// The ARM zone handlers give byte 0 for that view (`tools/arm_program_routing.py`, VERIFIED run),
/// so the program-load writer of the section table must differ from the plain file field.
/// ASSUMED: the loader or the UI treats an enabled section without layers as layer A, because the
/// names of these programs (and `Tines Layer`, `Piano Soft Orch`, `Rising Sun`: piano section on,
/// mask 0, next to a sounding synth) only make sense with that layer audible. Applied only when the
/// whole scene would be silent; a scene with sounding layers keeps its masks.
pub(super) fn fill_empty_scene(section_enable: u8, mask: u8) -> u8 {
    if mask != 0 {
        return mask;
    }
    let bit = |layer: LayerId| 1u8 << layer as u8;
    let mut filled = 0;
    if section_enable & 0b100 != 0 {
        filled |= bit(LayerId::OrganA);
    }
    if section_enable & 0b010 != 0 {
        filled |= bit(LayerId::PianoA);
    }
    if section_enable & 0b001 != 0 {
        filled |= bit(LayerId::SynthA);
    }
    filled
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

/// LED zone set (bit `u` = zone `u + 1`) of a `kKbZone` code.
///
/// EMPIRICAL / ASSUMED: the 12-value enumeration is not decoded in the firmware (the zone range
/// writer is unknown, `docs/research/keydown-zones.md` section 8). Over 384 factory programs the
/// codes `0` and `6` appear as the lower / upper half of a Mid-only split (`B3 Chords n Solo`,
/// `B3 Bassline`, `Quintet Bass Spl`: `0` the bass part), `4` as another lower half, and `1`, `2`, `3`
/// as single zones of 3- and 4-zone splits (`Choral Pad`, `Silver Rush`, `Guitar Concerto`); `5`
/// (firmware default) and `9` are on every layer of non-split programs. Codes `5`, `9` and the rare
/// `10`, `11` therefore mean "all zones" here (a layer spans the whole keyboard), `7` and `8` the
/// 3-zone ranges.
pub const fn zone_leds(code: u8) -> u8 {
    match code {
        0 => 0b0001,
        1 => 0b0010,
        2 => 0b0100,
        3 => 0b1000,
        4 => 0b0011,
        6 => 0b1100,
        7 => 0b0111,
        8 => 0b1110,
        _ => 0b1111,
    }
}

/// Linear weight of the region below a split point (1 below, 0 above, ramped over `±width`).
fn below_weight(note: f32, split: &SplitPoint) -> f32 {
    let w = split.crossfade.half_semitones();
    let ks = f32::from(split.midi_note);
    if w <= 0.0 {
        return if note < ks { 1.0 } else { 0.0 };
    }
    ((ks + w - note) / (2.0 * w)).clamp(0.0, 1.0)
}

impl ProgramRouting {
    /// Gain (`0.0..=1.0`) of `layer` for the key `note`, given the split points of the program.
    pub fn zone_gain(&self, layer: LayerId, note: u8, points: &[SplitPoint; 3]) -> f32 {
        let leds = zone_leds(self.kb_zone[layer as usize]);
        // Active points in Low, Mid, High order; LED zone `u` lies in region
        // "number of active points below it".
        let mut active = [None::<&SplitPoint>; 3];
        let mut count = 0;
        let mut region_of_led = [0usize; 4];
        for u in 1..4 {
            if self.split_active[u - 1] {
                active[count] = Some(&points[u - 1]);
                count += 1;
            }
            region_of_led[u] = count;
        }
        let mut regions = 0u8;
        for (u, &r) in region_of_led.iter().enumerate() {
            if leds & (1 << u) != 0 {
                regions |= 1 << r;
            }
        }
        let k = f32::from(note);
        let mut below = [1.0f32; 3];
        for (b, p) in below.iter_mut().zip(active.iter().flatten()) {
            *b = below_weight(k, p);
        }
        let mut gain = 0.0;
        for r in 0..=count {
            if regions & (1 << r) == 0 {
                continue;
            }
            let above_prev = if r > 0 { 1.0 - below[r - 1] } else { 1.0 };
            let under_next = if r < count { below[r] } else { 1.0 };
            gain += above_prev * under_next;
        }
        gain.clamp(0.0, 1.0)
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
            exact_masks: [
                fill_empty_scene(prog.global.section_enable_i, prog.global.scene_1_mask),
                fill_empty_scene(prog.global.section_enable_ii, prog.global.scene_2_mask),
            ],
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
    fn zone_codes_map_to_led_sets() {
        assert_eq!(zone_leds(0), 0b0001);
        assert_eq!(zone_leds(6), 0b1100);
        assert_eq!(zone_leds(5), 0b1111);
        assert_eq!(zone_leds(9), 0b1111);
        assert_eq!(zone_leds(11), 0b1111);
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
            kb_zone: [0, 1, 2, 3, 4, 6, 5],
            split_active: [false, true, true],
            ..ProgramRouting::default()
        };
        let mut pts = points(60, CrossfadeWidth::Off);
        pts[2].midi_note = 72;
        // codes 0, 1, 4 all cover the joined lower region
        for layer in [LayerId::OrganA, LayerId::OrganB, LayerId::SynthA] {
            assert_eq!(r.zone_gain(layer, 59, &pts), 1.0);
            assert_eq!(r.zone_gain(layer, 60, &pts), 0.0);
        }
        // code 2 = zone 3 only: the 60..71 region
        assert_eq!(r.zone_gain(LayerId::PianoA, 59, &pts), 0.0);
        assert_eq!(r.zone_gain(LayerId::PianoA, 65, &pts), 1.0);
        assert_eq!(r.zone_gain(LayerId::PianoA, 72, &pts), 0.0);
        // code 3 = zone 4 only
        assert_eq!(r.zone_gain(LayerId::PianoB, 72, &pts), 1.0);
        assert_eq!(r.zone_gain(LayerId::PianoB, 71, &pts), 0.0);
    }

    #[test]
    fn crossfade_gains_sum_to_one_across_the_split() {
        let r = mid_only([0, 6, 5, 5, 5, 5, 5]);
        let pts = points(60, CrossfadeWidth::Small);
        for note in 50..=70 {
            let lo = r.zone_gain(LayerId::OrganA, note, &pts);
            let hi = r.zone_gain(LayerId::OrganB, note, &pts);
            assert!((lo + hi - 1.0).abs() < 1e-6, "note {note}");
        }
        assert!((r.zone_gain(LayerId::OrganA, 60, &pts) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn empty_scene_plays_layer_a_of_each_enabled_section() {
        let a = |l: LayerId| 1u8 << l as u8;
        assert_eq!(fill_empty_scene(0b100, 0), a(LayerId::OrganA));
        assert_eq!(
            fill_empty_scene(0b011, 0),
            a(LayerId::PianoA) | a(LayerId::SynthA)
        );
        assert_eq!(fill_empty_scene(0, 0), 0, "no section on stays silent");
    }

    #[test]
    fn scene_with_a_sounding_layer_keeps_its_mask() {
        let b = 1u8 << LayerId::OrganB as u8;
        assert_eq!(fill_empty_scene(0b111, b), b);
    }

    #[test]
    fn default_routing_is_off_and_unloaded() {
        let e = Stage4Engine::new(48_000.0);
        assert_eq!(e.firmware_routing(), FirmwareRouting::Off);
        assert!(!e.program_routing().loaded);
    }
}
