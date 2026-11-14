//! Bridge to the older `synth_*` fields of `Stage4PanelState`.
//!
//! `Stage4PanelState::apply_program` and the library loader still write those fields (they
//! describe Synth layer A). [`SynthUi::sync_legacy`] adopts a change made there into layer A of
//! the Synth UI, and returns the values the state fields should now hold, so both views agree.

use super::tables::{filter_type_of, ANALOG, FILTER_TYPES};
use super::SynthUi;
use nord_core::synth::{FilterModel, SynthOscMode, VaWaveform};

/// The `synth_*` fields of the panel state that mirror layer A.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Legacy {
    pub osc_mode: SynthOscMode,
    pub waveform: VaWaveform,
    pub filter_model: FilterModel,
    pub cutoff: f32,
    pub resonance: f32,
    pub drive: f32,
    pub attack: f32,
    pub decay: f32,
    pub release: f32,
    pub octave: i8,
}

fn drive_step(d: f32) -> u8 {
    (d.clamp(0.0, 1.0) * 3.0).round() as u8
}

impl SynthUi {
    /// Adopt fields of `now` that differ from what was last exported (a program load or another
    /// panel wrote them), then return the values the state fields should hold.
    pub fn sync_legacy(&mut self, now: Legacy) -> Legacy {
        let seen = self.legacy_seen;
        if seen.is_none_or(|s| s != now) {
            let s = seen;
            let l = &mut self.layers[0];
            let diff = |f: fn(&Legacy) -> f32| s.is_none_or(|p| f(&p) != f(&now));
            if s.is_none_or(|p| p.osc_mode != now.osc_mode) {
                match now.osc_mode {
                    SynthOscMode::Sample => l.sample_mode = true,
                    SynthOscMode::Analog => {
                        l.sample_mode = false;
                        l.wave_type = 0;
                    }
                    SynthOscMode::Fm => {
                        l.sample_mode = false;
                        l.wave_type = 1;
                    }
                    SynthOscMode::Wavetable => {
                        l.sample_mode = false;
                        l.wave_type = 3;
                    }
                }
            }
            if s.is_none_or(|p| p.waveform != now.waveform) && l.wave_type == 0 {
                let found = ANALOG.iter().enumerate().find_map(|(c, cat)| {
                    cat.waves
                        .iter()
                        .position(|w| w.wave == now.waveform && w.exact)
                        .or_else(|| cat.waves.iter().position(|w| w.wave == now.waveform))
                        .map(|w| (c as u8, w as u8))
                });
                if let Some((c, w)) = found {
                    l.wave_cat = c;
                    l.wave_idx = w;
                }
            }
            if s.is_none_or(|p| p.filter_model != now.filter_model) {
                l.filter_type = filter_type_of(now.filter_model);
            }
            if diff(|x| x.cutoff) {
                l.cutoff_hz = now.cutoff;
            }
            if diff(|x| x.resonance) {
                l.resonance = now.resonance;
            }
            if diff(|x| x.drive) {
                l.drive = drive_step(now.drive);
            }
            if diff(|x| x.attack) {
                l.amp_env.attack = now.attack;
            }
            if diff(|x| x.decay) {
                l.amp_env.decay = now.decay;
            }
            if diff(|x| x.release) {
                l.amp_env.release = now.release;
            }
            if s.is_none_or(|p| p.octave != now.octave) {
                l.octave = now.octave;
            }
            if self.focus == 0 {
                self.reset_pots();
            }
        }
        let l = &self.layers[0];
        let out = Legacy {
            osc_mode: l.osc_mode(),
            waveform: if l.wave_type == 0 {
                l.va_waveform()
            } else {
                now.waveform
            },
            filter_model: FILTER_TYPES[usize::from(l.filter_type) % FILTER_TYPES.len()]
                .model
                .unwrap_or(now.filter_model),
            cutoff: l.cutoff_hz,
            resonance: l.resonance,
            // Keep the state's own value while it maps to the same drive step (no churn).
            drive: if drive_step(now.drive) == l.drive {
                now.drive
            } else {
                l.drive_value()
            },
            attack: l.amp_env.attack,
            decay: l.amp_env.decay,
            release: l.amp_env.release,
            octave: l.octave,
        };
        self.legacy_seen = Some(out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Legacy {
        Legacy {
            osc_mode: SynthOscMode::Analog,
            waveform: VaWaveform::SuperSaw,
            filter_model: FilterModel::Ladder24,
            cutoff: 2400.0,
            resonance: 0.3,
            drive: 0.2,
            attack: 0.01,
            decay: 0.35,
            release: 0.25,
            octave: 0,
        }
    }

    #[test]
    fn first_sync_adopts_the_state_values_into_layer_a() {
        let mut s = SynthUi::new();
        let out = s.sync_legacy(base());
        assert_eq!(s.layers[0].va_waveform(), VaWaveform::SuperSaw);
        assert_eq!(s.layers[0].amp_env.attack, 0.01);
        assert_eq!(
            out.drive, 0.2,
            "drive keeps the state value on the same step"
        );
        assert_eq!(out, s.sync_legacy(out), "a second sync is a fixed point");
    }

    #[test]
    fn a_program_load_changing_state_fields_moves_layer_a_and_the_pots() {
        let mut s = SynthUi::new();
        s.sync_legacy(base());
        let mut b = base();
        b.cutoff = 3200.0;
        b.osc_mode = SynthOscMode::Sample;
        b.filter_model = FilterModel::HighPass;
        b.octave = 2;
        s.sync_legacy(b);
        assert_eq!(s.layers[0].cutoff_hz, 3200.0);
        assert!(s.layers[0].sample_mode);
        assert_eq!(s.layers[0].filter_type, 4);
        assert_eq!(s.layers[0].octave, 2);
        assert!(s.pots.is_engaged(super::super::Knob::FilterFreq));
        assert_eq!(s.layers[1].cutoff_hz, 2400.0, "other layers untouched");
    }

    #[test]
    fn panel_edits_are_exported_back_to_the_state_fields() {
        let mut s = SynthUi::new();
        s.sync_legacy(base());
        s.layers[0].cutoff_hz = 900.0;
        s.layers[0].drive = 3;
        let out = s.sync_legacy(base());
        assert_eq!(out.cutoff, 900.0);
        assert_eq!(out.drive, 1.0);
    }

    #[test]
    fn analog_waveform_maps_to_a_category_entry() {
        let mut s = SynthUi::new();
        let mut b = base();
        b.waveform = VaWaveform::Triangle;
        s.sync_legacy(b);
        assert_eq!(s.layers[0].wave_name(), "Triangle");
    }

    #[test]
    fn lp_hp_keeps_the_state_filter_model() {
        let mut s = SynthUi::new();
        s.sync_legacy(base());
        s.layers[0].filter_type = 3;
        let out = s.sync_legacy(base());
        assert_eq!(out.filter_model, FilterModel::Ladder24);
    }
}
