//! Per-layer Synth Gate, Pattern, KB Hold / Sync and Master Clock sync (Phase 6b B9a,
//! `ns4-4w9l`; manual p.35-37).
//!
//! IDs are additive: `synthg_<name>_1..3` for Synth A, B and C, plus the global
//! `synthg_kb_hold`. Defaults equal `SynthLayer::new`, so an untouched host leaves sounds
//! unchanged. Gate mode only takes effect while the `synthm_arp_mode` run switch is not Off
//! (the ARP RUN button on the panel); the Gate then replaces the arpeggio.

use nih_plug::prelude::*;
use nord_core::engine::Stage4Engine;
use nord_core::synth::ClockDivision;
use nord_core::synth_types::ArpMode;

use crate::synth_params::SYNTH_LAYERS;

/// Gate, Pattern, Hold, Sync and Clock parameters of one synth layer.
#[derive(Params)]
pub struct SynthGateParams {
    /// Rhythmic Gate instead of the arpeggio while the arp is running (manual p.35).
    #[id = "gate_mode"]
    pub gate_mode: BoolParam,
    /// Gate envelope hardness `0..1` (manual p.35, the Range knob in Gate mode).
    #[id = "gate_env"]
    pub gate_env: FloatParam,
    /// Pattern on for Arp, Poly and Gate (manual p.35).
    #[id = "pat_on"]
    pub pattern_on: BoolParam,
    /// Preset pattern `0..7` (preset grids are `ASSUMED`).
    #[id = "pat_preset"]
    pub pattern_preset: IntParam,
    /// Exclude this layer from KB Hold (manual p.36).
    #[id = "hold_excl"]
    pub hold_exclude: BoolParam,
    /// KB Sync: reset arp/gate on the first note after all keys were lifted (manual p.37).
    #[id = "kb_sync"]
    pub kb_sync: BoolParam,
    /// Sync the Arp/Gate rate to the Master Clock.
    #[id = "arp_sync"]
    pub arp_sync: BoolParam,
    /// Arp/Gate subdivision `0..9`: 1/2, 1/2T, 1/4, 1/4T, 1/8, 1/8T, 1/16, 1/16T, 1/32, 1/32T.
    #[id = "arp_div"]
    pub arp_div: IntParam,
    /// Sync the LFO rate to the Master Clock.
    #[id = "lfo_sync"]
    pub lfo_sync: BoolParam,
    /// LFO subdivision, same list as `arp_div`.
    #[id = "lfo_div"]
    pub lfo_div: IntParam,
}

impl Default for SynthGateParams {
    fn default() -> Self {
        let int =
            |name, default, max| IntParam::new(name, default, IntRange::Linear { min: 0, max });
        Self {
            gate_mode: BoolParam::new("Gate Mode", false),
            gate_env: FloatParam::new("Gate Env", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_step_size(0.01),
            pattern_on: BoolParam::new("Pattern", false),
            pattern_preset: int("Pattern Preset", 0, 7),
            hold_exclude: BoolParam::new("KB Hold Exclude", false),
            kb_sync: BoolParam::new("KB Sync", false),
            arp_sync: BoolParam::new("Arp Clock Sync", false),
            arp_div: int("Arp Division", 4, 9),
            lfo_sync: BoolParam::new("LFO Clock Sync", false),
            lfo_div: int("LFO Division", 4, 9),
        }
    }
}

/// All three synth layers plus the global KB Hold switch.
#[derive(Params)]
pub struct SynthGateBank {
    /// KB Hold on (all Synth layers except those that exclude themselves).
    #[id = "kb_hold"]
    pub kb_hold: BoolParam,
    #[nested(array, group = "Synth Gate")]
    pub layers: [SynthGateParams; SYNTH_LAYERS],
}

impl Default for SynthGateBank {
    fn default() -> Self {
        Self {
            kb_hold: BoolParam::new("KB Hold", false),
            layers: std::array::from_fn(|_| SynthGateParams::default()),
        }
    }
}

impl SynthGateBank {
    /// Copy every value into the engine (no allocation; audio-thread safe). Runs after the
    /// `synthm` bank so the Gate switch can refine its arp mode.
    pub fn sync_to_engine(&self, engine: &mut Stage4Engine) {
        let targets = [
            &mut engine.synth_a,
            &mut engine.synth_b,
            &mut engine.synth_c,
        ];
        for (p, t) in self.layers.iter().zip(targets) {
            if p.gate_mode.value() && t.arp_mode != ArpMode::Off {
                t.arp_mode = ArpMode::Gate;
            }
            let perf = &mut t.perf;
            perf.gate_env = p.gate_env.value();
            perf.pattern_on = p.pattern_on.value();
            perf.pattern_preset = p.pattern_preset.value() as u8;
            perf.kb_hold = self.kb_hold.value();
            perf.kb_hold_exclude = p.hold_exclude.value();
            perf.kb_sync = p.kb_sync.value();
            perf.arp_clock_sync = p.arp_sync.value();
            perf.arp_division = ClockDivision::from_index(p.arp_div.value());
            perf.lfo_clock_sync = p.lfo_sync.value();
            perf.lfo_division = ClockDivision::from_index(p.lfo_div.value());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth_mod_params::SynthModBank;

    fn on() -> BoolParam {
        BoolParam::new("t", true)
    }

    fn arp_run(bank: &mut SynthModBank) {
        bank.layers[0].arp_mode = IntParam::new("m", 1, IntRange::Linear { min: 0, max: 5 });
        bank.layers[0].arp_rate = FloatParam::new(
            "r",
            240.0,
            FloatRange::Linear {
                min: 20.0,
                max: 300.0,
            },
        );
    }

    #[test]
    fn defaults_match_engine_defaults() {
        let reference = Stage4Engine::new(44100.0);
        let mut e = Stage4Engine::new(44100.0);
        SynthGateBank::default().sync_to_engine(&mut e);
        for (a, b) in [
            (&e.synth_a, &reference.synth_a),
            (&e.synth_c, &reference.synth_c),
        ] {
            assert_eq!(a.arp_mode, b.arp_mode);
            assert_eq!(a.perf.gate_env, b.perf.gate_env);
            assert_eq!(a.perf.pattern_on, b.perf.pattern_on);
            assert_eq!(a.perf.pattern_preset, b.perf.pattern_preset);
            assert_eq!(a.perf.kb_hold, b.perf.kb_hold);
            assert_eq!(a.perf.kb_hold_exclude, b.perf.kb_hold_exclude);
            assert_eq!(a.perf.kb_sync, b.perf.kb_sync);
            assert_eq!(a.perf.arp_clock_sync, b.perf.arp_clock_sync);
            assert_eq!(a.perf.arp_division, b.perf.arp_division);
            assert_eq!(a.perf.lfo_clock_sync, b.perf.lfo_clock_sync);
            assert_eq!(a.perf.lfo_division, b.perf.lfo_division);
        }
    }

    #[test]
    fn values_reach_only_their_layer() {
        let mut bank = SynthGateBank::default();
        bank.layers[1].kb_sync = on();
        bank.layers[1].arp_sync = on();
        bank.layers[1].arp_div = IntParam::new("d", 2, IntRange::Linear { min: 0, max: 9 });
        bank.kb_hold = on();
        let mut e = Stage4Engine::new(44100.0);
        bank.sync_to_engine(&mut e);
        assert!(e.synth_b.perf.kb_sync && e.synth_b.perf.arp_clock_sync);
        assert_eq!(e.synth_b.perf.arp_division, ClockDivision::Quarter);
        assert!(!e.synth_a.perf.kb_sync);
        assert!(e.synth_a.perf.kb_hold && e.synth_c.perf.kb_hold);
    }

    #[test]
    fn gate_mode_needs_the_arp_run_switch() {
        let mut gate = SynthGateBank::default();
        gate.layers[0].gate_mode = on();
        let mut e = Stage4Engine::new(44100.0);
        gate.sync_to_engine(&mut e);
        assert_eq!(e.synth_a.arp_mode, ArpMode::Off);

        let mut modb = SynthModBank::default();
        arp_run(&mut modb);
        modb.sync_to_engine(&mut e);
        gate.sync_to_engine(&mut e);
        assert_eq!(e.synth_a.arp_mode, ArpMode::Gate);
        assert_eq!(e.synth_b.arp_mode, ArpMode::Off);
    }

    fn render_with(gate: &SynthGateBank) -> Vec<[f32; 2]> {
        let mut e = Stage4Engine::new(44100.0);
        let mut modb = SynthModBank::default();
        arp_run(&mut modb);
        modb.sync_to_engine(&mut e);
        gate.sync_to_engine(&mut e);
        e.synth_a.note_on(60, 100);
        (0..16_384).map(|_| e.synth_a.render_frame()).collect()
    }

    #[test]
    fn gate_mode_changes_the_rendered_signal() {
        let arp = render_with(&SynthGateBank::default());
        let mut bank = SynthGateBank::default();
        bank.layers[0].gate_mode = on();
        let gated = render_with(&bank);
        assert!(arp
            .iter()
            .chain(&gated)
            .all(|f| f[0].is_finite() && f[1].is_finite()));
        assert!(gated.iter().any(|f| f[0].abs() > 1e-4), "gate is silent");
        assert!(arp != gated, "gate renders identically to the arpeggio");
    }

    #[test]
    fn ids_unique_and_complete() {
        let ids: Vec<String> = SynthGateBank::default()
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(ids.len(), 10 * SYNTH_LAYERS + 1);
        let mut s = ids.clone();
        s.sort();
        s.dedup();
        assert_eq!(s.len(), ids.len());
    }
}
