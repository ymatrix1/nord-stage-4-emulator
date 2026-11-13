//! Aux KB (auxiliary keyboard) state and per-layer MIDI channel routing.
//!
//! Spec: `docs/specs/midi-and-control.md` (Aux KB, manual OS v1.6x p.43-45, p.55, p.59, p.62).

use super::*;

/// Velocity scaling of notes arriving on the Aux KB channel (MIDI menu "Keyboard Curve", p.59).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AuxVelocityCurve {
    Soft,
    #[default]
    Medium,
    Hard,
}

impl AuxVelocityCurve {
    /// Maps an incoming velocity `1..=127` through the curve.
    /// ASSUMED: power-law exponents 0.7 (Soft) and 1.4 (Hard); the manual gives only the names.
    /// Medium is the identity so the default leaves velocities untouched.
    pub fn map(self, velocity: u8) -> u8 {
        let exponent = match self {
            Self::Medium => return velocity,
            Self::Soft => 0.7f32,
            Self::Hard => 1.4f32,
        };
        let v = f32::from(velocity.clamp(1, 127)) / 127.0;
        ((v.powf(exponent) * 127.0).round() as u8).clamp(1, 127)
    }
}

/// Where a note reached a layer from; decides split-zone gain and velocity handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteRoute {
    /// Global channel: the internal keybed, subject to the keyboard zones.
    Global,
    /// Direct per-layer channel: bypasses the zones.
    Layer,
    /// Aux KB channel: bypasses the zones, passes through the Aux velocity curve.
    Aux,
}

/// Per-program Aux KB configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuxKb {
    /// "Aux KB Enable" (Aux KB menu page 1, default Off).
    pub enabled: bool,
    /// Aux KB MIDI channel `0..=15` (MIDI menu, default 16 -> `15`; `None` = Off).
    pub channel: Option<u8>,
    /// Layers played by the aux keyboard instead of the internal keybed (Aux KB menu page 2).
    pub layers: [bool; 7],
    pub curve: AuxVelocityCurve,
}

impl Default for AuxKb {
    fn default() -> Self {
        Self {
            enabled: false,
            channel: Some(15),
            layers: [false; 7],
            curve: AuxVelocityCurve::Medium,
        }
    }
}

impl AuxKb {
    /// True when `layer` is played by the aux keyboard (its Aux KB LED is lit).
    pub fn is_aux_layer(&self, layer: LayerId) -> bool {
        self.enabled && self.layers[layer as usize]
    }

    /// True when `channel` is the active Aux KB channel.
    pub fn listens_on(&self, channel: u8) -> bool {
        self.enabled && self.channel == Some(channel)
    }

    /// Sets the aux layers from per-section bitmasks (bit 0 = Layer A), mirroring the Aux KB
    /// menu codes Organ `--/A-/-B/AB`, Piano likewise, Synth `---..ABC`.
    pub fn set_section_masks(&mut self, organ: u8, piano: u8, synth: u8) {
        for i in 0..2 {
            self.layers[i] = organ & (1 << i) != 0;
            self.layers[2 + i] = piano & (1 << i) != 0;
        }
        for i in 0..3 {
            self.layers[4 + i] = synth & (1 << i) != 0;
        }
    }
}

impl Stage4Engine {
    /// Decides whether a note on `channel` reaches `layer`, and via which path.
    ///
    /// Precedence: direct layer channel, then Aux KB channel (aux layers only), then the Global
    /// channel. A layer that is in Aux KB mode ignores the internal keybed (Global channel).
    pub fn route_for(&self, layer: LayerId, channel: u8) -> Option<NoteRoute> {
        let idx = layer as usize;
        if self.layer_midi_channels[idx] == Some(channel) {
            return Some(NoteRoute::Layer);
        }
        let aux_layer = self.aux_kb.is_aux_layer(layer);
        if aux_layer && self.aux_kb.listens_on(channel) {
            return Some(NoteRoute::Aux);
        }
        if !aux_layer && self.global_midi_channel == Some(channel) {
            return Some(NoteRoute::Global);
        }
        None
    }

    /// Sets the Global MIDI channel (`1..=16` as on the panel, `None` = Off).
    pub fn set_global_channel(&mut self, panel_channel: Option<u8>) {
        self.global_midi_channel = panel_channel.map(panel_to_zero_based);
    }

    /// Sets the direct channel of `layer` (`1..=16`, `None` = Off).
    pub fn set_layer_channel(&mut self, layer: LayerId, panel_channel: Option<u8>) {
        self.layer_midi_channels[layer as usize] = panel_channel.map(panel_to_zero_based);
    }

    /// Sets the Aux KB channel (`1..=16`, `None` = Off).
    pub fn set_aux_channel(&mut self, panel_channel: Option<u8>) {
        self.aux_kb.channel = panel_channel.map(panel_to_zero_based);
    }
}

fn panel_to_zero_based(panel_channel: u8) -> u8 {
    panel_channel.clamp(1, 16) - 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(e: &mut Stage4Engine, channel: u8, note: u8) {
        e.handle_midi(MidiEvent::NoteOn {
            channel,
            note,
            velocity: 100,
        });
    }

    #[test]
    fn defaults_are_disabled_with_channel_16() {
        let aux = AuxKb::default();
        assert!(!aux.enabled);
        assert_eq!(aux.channel, Some(15));
        assert!(!aux.is_aux_layer(LayerId::OrganB));
        assert!(!aux.listens_on(15));
    }

    #[test]
    fn aux_layer_ignores_keybed_and_follows_aux_channel() {
        let mut e = Stage4Engine::new(48_000.0);
        e.scene.scene_1_mask = 0b0111_1111;
        e.aux_kb.enabled = true;
        e.aux_kb.set_section_masks(0b10, 0, 0);
        press(&mut e, 0, 60);
        let global = e.note_recipients_for_test(0, 60);
        assert_eq!(
            global & (1 << LayerId::OrganB as u8),
            0,
            "keybed reached aux"
        );
        assert_ne!(global & (1 << LayerId::OrganA as u8), 0);
        press(&mut e, 15, 62);
        assert_eq!(
            e.note_recipients_for_test(15, 62),
            1 << LayerId::OrganB as u8,
            "only the aux layer answers the aux channel"
        );
    }

    #[test]
    fn disabled_aux_kb_does_nothing() {
        let mut e = Stage4Engine::new(48_000.0);
        e.aux_kb.set_section_masks(0b11, 0, 0);
        press(&mut e, 15, 60);
        assert_eq!(e.note_recipients_for_test(15, 60), 0);
        press(&mut e, 0, 60);
        assert_ne!(e.note_recipients_for_test(0, 60), 0);
    }

    #[test]
    fn aux_notes_bypass_zones_and_apply_the_curve() {
        let mut e = Stage4Engine::new(48_000.0);
        e.scene.scene_1_mask = 0b0111_1111;
        e.aux_kb.enabled = true;
        e.aux_kb.layers[LayerId::PianoA as usize] = true;
        e.aux_kb.curve = AuxVelocityCurve::Hard;
        e.split_router.layer_zone_masks[LayerId::PianoA as usize] = 0;
        press(&mut e, 15, 60);
        let (response, split_gain, _) = e.piano_a.voice_excitation_for_test(60).unwrap();
        assert_eq!(split_gain, 1.0);
        assert!(response < 100.0 / 127.0, "hard curve must lower velocity");
    }

    #[test]
    fn velocity_curve_orders_soft_medium_hard() {
        assert_eq!(AuxVelocityCurve::Medium.map(64), 64);
        assert!(AuxVelocityCurve::Soft.map(64) > 64);
        assert!(AuxVelocityCurve::Hard.map(64) < 64);
        assert_eq!(AuxVelocityCurve::Hard.map(127), 127);
        assert_eq!(AuxVelocityCurve::Hard.map(1), 1);
    }

    #[test]
    fn channel_helpers_convert_panel_numbers() {
        let mut e = Stage4Engine::new(48_000.0);
        e.set_global_channel(Some(3));
        assert_eq!(e.global_midi_channel, Some(2));
        e.set_global_channel(None);
        assert_eq!(e.global_midi_channel, None);
        e.set_layer_channel(LayerId::SynthC, Some(16));
        assert_eq!(e.layer_midi_channels[6], Some(15));
        e.set_aux_channel(Some(1));
        assert_eq!(e.aux_kb.channel, Some(0));
        assert_eq!(e.route_for(LayerId::SynthC, 15), Some(NoteRoute::Layer));
        assert_eq!(e.route_for(LayerId::SynthC, 4), None);
    }
}
