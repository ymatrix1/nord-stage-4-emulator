//! Panel-side per-layer keyboard zone and Aux KB assignment (KB ZONE buttons, Aux KB LEDs).
//!
//! Spec: `docs/specs/morph-and-zones.md` section 6 (zones), `docs/specs/midi-and-control.md`
//! (Aux KB), `docs/specs/control-inventory.md` section 5b. Both routing modes follow one edit:
//! the legacy router (`split_router.layer_zone_masks`, bit `u` = LED zone `u + 1`) and the
//! firmware `kKbZone` code of [`ProgramRouting`](super::ProgramRouting) used with
//! [`FirmwareRouting::On`](super::FirmwareRouting). Defaults are untouched: every layer starts on
//! all four zones, so an engine nobody edits renders as before.

use super::*;

/// `kKbZone` code whose LED zone set is exactly `leds` (inverse of [`zone_leds`]), if there is one.
///
/// EMPIRICAL / ASSUMED: same table as [`zone_leds`] (the 12-value enumeration is not decoded in
/// the firmware); `0b1111` maps to the firmware default `5`. A set with no code (for example
/// `0b0101`) is still honoured by the legacy router.
pub const fn zone_code(leds: u8) -> Option<u8> {
    match leds & 0b1111 {
        0b0001 => Some(0),
        0b0010 => Some(1),
        0b0100 => Some(2),
        0b1000 => Some(3),
        0b0011 => Some(4),
        0b1111 => Some(5),
        0b1100 => Some(6),
        0b0111 => Some(7),
        0b1110 => Some(8),
        _ => None,
    }
}

impl Stage4Engine {
    /// Assigns `layer` to the LED zones in `leds` (bit `u` = zone `u + 1`; only the low 4 bits are
    /// used). An empty set is ignored: a layer always keeps at least one zone.
    ///
    /// With no split point active every non-empty set spans the whole keyboard (manual p.39, the
    /// zones only matter once the keyboard is split). Field writes only: allocation free.
    pub fn set_layer_zone_leds(&mut self, layer: LayerId, leds: u8) {
        let leds = leds & 0b1111;
        if leds == 0 {
            return;
        }
        self.split_router.layer_zone_masks[layer as usize] = leds;
        if let Some(code) = zone_code(leds) {
            self.routing.kb_zone[layer as usize] = code;
        }
    }

    /// LED zone set of `layer` as the legacy router sees it (bit `u` = zone `u + 1`).
    pub fn layer_zone_leds(&self, layer: LayerId) -> u8 {
        self.split_router.layer_zone_masks[layer as usize] & 0b1111
    }

    /// Puts `layer` on (or takes it off) the aux keyboard (its Aux KB LED).
    ///
    /// ASSUMED: switching a layer on also sets "Aux KB Enable", otherwise the flag would have no
    /// effect (`AuxKb::is_aux_layer` needs both); switching it off leaves the enable alone because
    /// other layers may still use it. A hardware check of the panel LED path would confirm.
    pub fn set_layer_aux(&mut self, layer: LayerId, on: bool) {
        self.aux_kb.layers[layer as usize] = on;
        if on {
            self.aux_kb.enabled = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_code_inverts_zone_leds() {
        for code in [0u8, 1, 2, 3, 4, 6, 7, 8] {
            assert_eq!(zone_code(zone_leds(code)), Some(code));
        }
        assert_eq!(zone_code(0b1111), Some(5));
        assert_eq!(zone_code(0b0101), None);
        assert_eq!(zone_code(0), None);
    }

    #[test]
    fn defaults_keep_every_layer_on_all_zones() {
        let e = Stage4Engine::new(48_000.0);
        for l in LayerId::ALL {
            assert_eq!(e.layer_zone_leds(l), 0b1111);
        }
        assert_eq!(e.routing.kb_zone, [5; 7]);
        assert!(!e.aux_kb.enabled && e.aux_kb.layers == [false; 7]);
    }

    #[test]
    fn assignment_updates_both_routers_and_ignores_empty_sets() {
        let mut e = Stage4Engine::new(48_000.0);
        e.set_layer_zone_leds(LayerId::PianoB, 0b0100);
        assert_eq!(e.layer_zone_leds(LayerId::PianoB), 0b0100);
        assert_eq!(e.routing.kb_zone[LayerId::PianoB as usize], 2);
        assert_eq!(e.layer_zone_leds(LayerId::PianoA), 0b1111, "other layer");
        e.set_layer_zone_leds(LayerId::PianoB, 0);
        assert_eq!(e.layer_zone_leds(LayerId::PianoB), 0b0100);
        e.set_layer_zone_leds(LayerId::PianoB, 0b0101);
        assert_eq!(e.layer_zone_leds(LayerId::PianoB), 0b0101);
        assert_eq!(e.routing.kb_zone[LayerId::PianoB as usize], 2, "no code");
    }

    #[test]
    fn a_split_makes_the_assignment_audible_in_both_modes() {
        for mode in [FirmwareRouting::Off, FirmwareRouting::On] {
            let mut e = Stage4Engine::new(48_000.0);
            e.set_firmware_routing(mode);
            e.split_router.splits[1].enabled = true;
            e.split_router.splits[1].midi_note = 60;
            e.split_router.splits[1].crossfade = crate::morph::CrossfadeWidth::Off;
            e.routing.split_active = [false, true, false];
            e.set_layer_zone_leds(LayerId::OrganA, 0b0011);
            e.set_layer_zone_leds(LayerId::OrganB, 0b1100);
            assert_eq!(e.keybed_gain(LayerId::OrganA, 48), 1.0, "{mode:?}");
            assert_eq!(e.keybed_gain(LayerId::OrganA, 72), 0.0, "{mode:?}");
            assert_eq!(e.keybed_gain(LayerId::OrganB, 48), 0.0, "{mode:?}");
            assert_eq!(e.keybed_gain(LayerId::OrganB, 72), 1.0, "{mode:?}");
        }
    }

    #[test]
    fn without_a_split_every_assignment_spans_the_keyboard() {
        for mode in [FirmwareRouting::Off, FirmwareRouting::On] {
            let mut e = Stage4Engine::new(48_000.0);
            e.set_firmware_routing(mode);
            e.set_layer_zone_leds(LayerId::SynthA, 0b1000);
            for note in [24u8, 60, 100] {
                assert_eq!(e.keybed_gain(LayerId::SynthA, note), 1.0, "{mode:?}");
            }
        }
    }

    #[test]
    fn aux_flag_sets_the_layer_and_enables_the_aux_keyboard() {
        let mut e = Stage4Engine::new(48_000.0);
        e.set_layer_aux(LayerId::PianoB, true);
        assert!(e.aux_kb.is_aux_layer(LayerId::PianoB));
        assert!(!e.aux_kb.is_aux_layer(LayerId::PianoA));
        e.set_layer_aux(LayerId::PianoB, false);
        assert!(!e.aux_kb.is_aux_layer(LayerId::PianoB));
        assert!(e.aux_kb.enabled, "other layers may still use it");
    }
}
