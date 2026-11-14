//! Maps the stored arpeggiator mode fields of a decoded Synth layer (`SynthArpDecoded`) onto the
//! real-time `SynthLayer`, so a Poly or Gate program loads in its mode.
//!
//! The hardware keeps the mode (Arp / Poly / Gate) and the direction even while the RUN switch is
//! off; the engine's single `ArpMode` cannot (`Off` means "not running"), so a layer whose RUN
//! bit is off loads as `Off` exactly as before.

use crate::cbin::SynthLayerDecoded;
use crate::synth::{ArpMode, ClockDivision, SynthLayer};

/// Number of Master Clock subdivisions the Rate dial selects (`1/2` .. `1/32`, with triplets).
///
/// EMPIRICAL: `ns4names.ArpRateFracs` has 9 entries and `ns4decode.interpretArpRate` picks
/// `floor(9 * raw / 128)`; the engine's list starts with the same nine (`ClockDivision`).
const CLOCK_DIVISIONS: usize = 9;

/// Engine arpeggiator mode of a stored layer (`Off` while RUN is off).
pub fn engine_arp_mode(dec: &SynthLayerDecoded) -> ArpMode {
    if !dec.arp_enabled {
        return ArpMode::Off;
    }
    match dec.arp.mode {
        1 => ArpMode::Poly,
        2 => ArpMode::Gate,
        _ => match dec.arp.direction {
            1 => ArpMode::Down,
            2 => ArpMode::UpDown,
            3 => ArpMode::Random,
            _ => ArpMode::Up,
        },
    }
}

/// The stored rate raw value (`0..=127`) recovered from the decoded BPM (the decoder maps the
/// raw linearly to `30..300`; the Master Clock subdivision is stored in the same field).
fn rate_raw(dec: &SynthLayerDecoded) -> usize {
    (((dec.arp_bpm - 30.0) / 270.0).clamp(0.0, 1.0) * 127.0).round() as usize
}

/// Applies mode, direction, zig zag, Master Clock sync, KB sync and range to `synth`.
pub fn apply_arp_modes(synth: &mut SynthLayer, dec: &SynthLayerDecoded) {
    synth.arp_mode = engine_arp_mode(dec);
    let perf = &mut synth.perf;
    perf.zigzag = dec.arp.zigzag;
    perf.kb_sync = dec.arp.kb_sync;
    perf.arp_clock_sync = dec.arp.mst_clk;
    if dec.arp.mst_clk {
        perf.arp_division =
            ClockDivision::from_index((CLOCK_DIVISIONS * rate_raw(dec) / 128) as i32);
    }
    if dec.arp.mode == 2 {
        // Gate mode: the RANGE value is the envelope hardness.
        perf.gate_env = dec.arp.gate_env();
    } else {
        let thirds = dec.arp.range_thirds();
        synth.arp_octave_range = 1 + thirds / 3;
        synth.perf.range_extra_thirds = thirds % 3;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cbin::synth_arp::SynthArpDecoded;

    fn layer(arp: SynthArpDecoded, run: bool) -> SynthLayerDecoded {
        SynthLayerDecoded {
            arp_enabled: run,
            arp,
            ..Default::default()
        }
    }

    fn applied(dec: &SynthLayerDecoded) -> SynthLayer {
        let mut s = SynthLayer::new(48_000.0);
        apply_arp_modes(&mut s, dec);
        s
    }

    #[test]
    fn mode_and_direction_pick_the_engine_mode_only_while_running() {
        let with = |mode, direction| SynthArpDecoded {
            mode,
            direction,
            ..Default::default()
        };
        assert_eq!(engine_arp_mode(&layer(with(0, 0), true)), ArpMode::Up);
        assert_eq!(engine_arp_mode(&layer(with(0, 1), true)), ArpMode::Down);
        assert_eq!(engine_arp_mode(&layer(with(0, 2), true)), ArpMode::UpDown);
        assert_eq!(engine_arp_mode(&layer(with(0, 3), true)), ArpMode::Random);
        // Poly and Gate ignore the direction.
        assert_eq!(engine_arp_mode(&layer(with(1, 3), true)), ArpMode::Poly);
        assert_eq!(engine_arp_mode(&layer(with(2, 3), true)), ArpMode::Gate);
        // RUN off: nothing plays, whatever the stored mode.
        assert_eq!(engine_arp_mode(&layer(with(2, 3), false)), ArpMode::Off);
    }

    #[test]
    fn a_gate_program_survives_pack_unpack_and_loads_in_gate_mode() {
        use crate::cbin::synth_preset::SynthPresetDecoded;
        let mut preset = SynthPresetDecoded::default();
        preset.layer_c.arp_enabled = true;
        preset.layer_c.arp = SynthArpDecoded {
            mode: 2,
            direction: 1,
            zigzag: false,
            mst_clk: true,
            kb_sync: true,
            range: 100,
        };
        let bytes = preset.pack().unwrap();
        let back = SynthPresetDecoded::unpack(&bytes).unwrap();
        assert_eq!(back.layer_c.arp, preset.layer_c.arp);
        assert_eq!(back.layer_a.arp, SynthArpDecoded::default());
        assert_eq!(back.pack().unwrap(), bytes, "lossless round trip");

        let s = applied(&back.layer_c);
        assert_eq!(s.arp_mode, ArpMode::Gate);
        assert!(s.perf.kb_sync && s.perf.arp_clock_sync);
        assert!((s.perf.gate_env - 100.0 / 127.0).abs() < 1e-6);
        assert_eq!(applied(&back.layer_a).arp_mode, ArpMode::Off);
    }

    #[test]
    fn a_poly_program_loads_in_poly_mode_and_renders_sound() {
        let dec = layer(
            SynthArpDecoded {
                mode: 1,
                ..Default::default()
            },
            true,
        );
        let mut s = applied(&dec);
        assert_eq!(s.arp_mode, ArpMode::Poly);
        s.arp_rate_bpm = 240.0;
        for note in [60, 64, 67] {
            s.note_on(note, 100);
        }
        let energy: f32 = (0..6_000)
            .map(|_| {
                let [l, r] = s.render_frame();
                l * l + r * r
            })
            .sum();
        assert!(
            energy > 0.01,
            "the Poly arpeggiator must sound, got {energy}"
        );
    }

    #[test]
    fn zigzag_kb_sync_and_master_clock_are_applied() {
        let s = applied(&layer(
            SynthArpDecoded {
                zigzag: true,
                kb_sync: true,
                mst_clk: true,
                ..Default::default()
            },
            true,
        ));
        assert!(s.perf.zigzag && s.perf.kb_sync && s.perf.arp_clock_sync);
        let plain = applied(&layer(SynthArpDecoded::default(), true));
        assert!(!plain.perf.zigzag && !plain.perf.kb_sync && !plain.perf.arp_clock_sync);
    }

    #[test]
    fn master_clock_rate_selects_the_subdivision() {
        // The decoder turns raw 0..=127 into 30..300 BPM; the subdivision is floor(9 * raw / 128).
        let div = |raw: usize| {
            let bpm = 30.0 + raw as f32 / 127.0 * 270.0;
            let dec = SynthLayerDecoded {
                arp_bpm: bpm,
                ..layer(
                    SynthArpDecoded {
                        mst_clk: true,
                        ..Default::default()
                    },
                    true,
                )
            };
            applied(&dec).perf.arp_division
        };
        assert_eq!(div(0), ClockDivision::Half);
        assert_eq!(div(67), ClockDivision::Eighth); // the most common factory value
        assert_eq!(div(127), ClockDivision::ThirtySecond);
    }

    #[test]
    fn range_sets_octaves_and_thirds_and_gate_uses_it_as_envelope() {
        let arp = |mode, range| SynthArpDecoded {
            mode,
            range,
            ..Default::default()
        };
        let low = applied(&layer(arp(0, 0), true));
        assert_eq!((low.arp_octave_range, low.perf.range_extra_thirds), (1, 0));
        let high = applied(&layer(arp(0, 127), true));
        assert_eq!(
            (high.arp_octave_range, high.perf.range_extra_thirds),
            (4, 0)
        );
        let mid = applied(&layer(arp(1, 64), true));
        assert_eq!((mid.arp_octave_range, mid.perf.range_extra_thirds), (2, 2));
        let gate = applied(&layer(arp(2, 127), true));
        assert_eq!(gate.perf.gate_env, 1.0);
        assert_eq!(
            gate.arp_octave_range, 1,
            "Gate leaves the octave range alone"
        );
    }
}
