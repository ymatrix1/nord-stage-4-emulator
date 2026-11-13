//! GLOBAL / GROUP effect sharing and the `TO ROTARY` bus of `Stage4Engine`.
//!
//! Manual p.48-49 and p.52-53 (OS v1.6x):
//! - GROUP (Piano / Synth FX Focus + Shift): all layers of the section take the focused layer's settings.
//! - GLOBAL (Shift + On) for Delay, Compressor, Reverb: the focused layer's settings apply to all layers.
//!   Reverb is then one instance placed after the Rotary and after each layer's level fader.
//! - TO ROTARY (Amp/EQ unit) routes a Piano / Synth layer to the single Rotary Speaker, the last unit of
//!   that layer's chain; speed and drive are those of the Organ section's Rotary.
//!
//! Defaults are all off, so an untouched engine renders exactly as before.

use super::*;

/// Index range of the Piano layers in `Stage4Engine::fx_chains`.
const PIANO_LAYERS: std::ops::Range<usize> = 2..4;
/// Index range of the Synth layers in `Stage4Engine::fx_chains`.
const SYNTH_LAYERS: std::ops::Range<usize> = 4..7;

/// Source layer (index into `fx_chains`) of every active sharing mode; `None` = mode off.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FxSharing {
    /// Delay GLOBAL: this layer's delay settings apply to all layers.
    pub delay_global: Option<usize>,
    /// Compressor GLOBAL.
    pub comp_global: Option<usize>,
    /// Reverb GLOBAL: one shared reverb after the rotary and the level faders.
    pub reverb_global: Option<usize>,
    /// Piano GROUP (source must be layer 2 or 3).
    pub piano_group: Option<usize>,
    /// Synth GROUP (source must be layer 4, 5 or 6).
    pub synth_group: Option<usize>,
}

impl FxSharing {
    /// `true` when any mode is active.
    pub fn any(&self) -> bool {
        *self != Self::default()
    }
}

/// Applies `f(dst, src)` for every `dst` in `dsts` except `src` (split borrow, no allocation).
fn copy_to(
    chains: &mut [LayerFxChain; 7],
    src: usize,
    dsts: std::ops::Range<usize>,
    f: fn(&mut LayerFxChain, &LayerFxChain),
) {
    for i in dsts.filter(|&i| i != src) {
        let (a, b) = chains.split_at_mut(i.max(src));
        if i < src {
            f(&mut a[i], &b[0]);
        } else {
            f(&mut b[0], &a[src]);
        }
    }
}

impl Stage4Engine {
    /// Applies the GROUP and GLOBAL modes of `fx_sharing` to the layer chains.
    ///
    /// Call after the host / program wrote the per-layer settings and before rendering a block
    /// (`process_block` does it). Group runs first, so a Global source wins over a Group copy. Field
    /// copies only: allocation free.
    pub fn apply_fx_sharing(&mut self) {
        let s = self.fx_sharing;
        for (src, range) in [(s.piano_group, PIANO_LAYERS), (s.synth_group, SYNTH_LAYERS)] {
            if let Some(src) = src.filter(|i| range.contains(i)) {
                copy_to(&mut self.fx_chains, src, range, |d, s| {
                    d.copy_all_settings_from(s)
                });
            }
        }
        if let Some(src) = s.delay_global.filter(|&i| i < 7) {
            copy_to(&mut self.fx_chains, src, 0..7, |d, s| {
                d.copy_delay_settings_from(s)
            });
        }
        if let Some(src) = s.comp_global.filter(|&i| i < 7) {
            copy_to(&mut self.fx_chains, src, 0..7, |d, s| {
                d.copy_comp_settings_from(s)
            });
        }
        let rev_src = s.reverb_global.filter(|&i| i < 7);
        if let Some(src) = rev_src {
            self.global_reverb
                .copy_reverb_settings_from(&self.fx_chains[src]);
        }
        for fx in &mut self.fx_chains {
            fx.bypass_reverb = rev_src.is_some();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::{Mod1Type, ReverbModel};

    #[test]
    fn default_sharing_is_off() {
        assert!(!FxSharing::default().any());
        assert!(!Stage4Engine::new(48_000.0).fx_sharing.any());
    }

    #[test]
    fn synth_group_copies_the_source_to_the_other_synth_layers_only() {
        let mut e = Stage4Engine::new(48_000.0);
        e.fx_chains[5].mod1_type = Mod1Type::Tremolo;
        e.fx_chains[5].reverb_mix = 0.6;
        e.fx_sharing.synth_group = Some(5);
        e.apply_fx_sharing();
        for i in [4, 6] {
            assert_eq!(e.fx_chains[i].mod1_type, Mod1Type::Tremolo);
            assert_eq!(e.fx_chains[i].reverb_mix, 0.6);
        }
        for i in [0, 1, 2, 3] {
            assert_eq!(e.fx_chains[i].mod1_type, Mod1Type::Off);
        }
        // a piano source index is not a valid synth group source
        let mut e = Stage4Engine::new(48_000.0);
        e.fx_chains[2].mod1_type = Mod1Type::Wah;
        e.fx_sharing.synth_group = Some(2);
        e.apply_fx_sharing();
        assert_eq!(e.fx_chains[4].mod1_type, Mod1Type::Off);
    }

    #[test]
    fn piano_group_works_from_either_source() {
        for src in [2, 3] {
            let mut e = Stage4Engine::new(48_000.0);
            e.fx_chains[src].comp_amount = 0.5;
            e.fx_sharing.piano_group = Some(src);
            e.apply_fx_sharing();
            assert_eq!(e.fx_chains[5 - src].comp_amount, 0.5);
            assert_eq!(e.fx_chains[4].comp_amount, 0.0);
        }
    }

    #[test]
    fn global_delay_and_comp_reach_every_layer() {
        let mut e = Stage4Engine::new(48_000.0);
        e.fx_chains[3].delay_enabled = true;
        e.fx_chains[3].delay_time_ms = 123.0;
        e.fx_chains[3].comp_enabled = true;
        e.fx_chains[3].comp_amount = 0.8;
        e.fx_sharing.delay_global = Some(3);
        e.fx_sharing.comp_global = Some(3);
        e.apply_fx_sharing();
        for fx in &e.fx_chains {
            assert!(fx.delay_enabled && fx.comp_enabled);
            assert_eq!(fx.delay_time_ms, 123.0);
            assert_eq!(fx.comp_amount, 0.8);
        }
    }

    #[test]
    fn global_reverb_bypasses_layer_reverbs_and_clears_when_off() {
        let mut e = Stage4Engine::new(48_000.0);
        e.fx_chains[0].reverb_model = ReverbModel::Hall;
        e.fx_sharing.reverb_global = Some(0);
        e.apply_fx_sharing();
        assert!(e.fx_chains.iter().all(|f| f.bypass_reverb));
        e.fx_sharing.reverb_global = None;
        e.apply_fx_sharing();
        assert!(e.fx_chains.iter().all(|f| !f.bypass_reverb));
    }
}
