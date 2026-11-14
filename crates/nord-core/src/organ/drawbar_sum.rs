//! Drawbar and percussion sum of the held keys over one or two tonewheel lanes.
//!
//! Spec: `docs/specs/organ-engine.md` sections 3.2, 3.3. The per-key arithmetic is the one the
//! mono path always used; lane 1 repeats it on the second lane when the stereo blend is active.

use super::drawbar::{drawbar_gain_law, foldback_tonewheel, DRAWBAR_OFFSETS};
use super::types::NUM_TONEWHEELS;
use super::weights::UNIT_WEIGHT;
use super::{OrganLayer, OrganModel};

impl OrganLayer {
    /// Sums drawbars and percussion of all held keys. `buses[lane]` are the tonewheel buses of
    /// `lanes` active lanes (1 or 2); only the first `lanes` entries of the result are used.
    ///
    /// ASSUMED: the `0.08` drawbar bus scale, the `0.12` / `0.06` percussion gains and the B3
    /// Normal-percussion `0.708` dip (unchanged, see the spec).
    pub(super) fn sum_drawbars(
        &mut self,
        buses: &[[f32; NUM_TONEWHEELS]; 2],
        lanes: usize,
    ) -> [f32; 2] {
        if self.farfisa_keys_active() {
            return [self.sum_farfisa_keys(&buses[0]), 0.0];
        }
        if self.b3_bass_active() {
            return [self.sum_b3_bass(&buses[0]), 0.0];
        }
        if self.b3_weights_active() {
            return [self.sum_b3_weights(&buses[0]), 0.0];
        }
        let oct_semis = i32::from(self.octave_shift.clamp(-2, 2)) * 12;
        let db_bus_scale = if self.model == OrganModel::B3
            && self.percussion_enabled
            && !self.percussion_soft
            && self.percussion_env > 1e-4
        {
            0.708
        } else {
            1.0
        };

        let firmware = self.firmware_weights();
        let bus_gain = self.level_law().bus_gain();
        let mut dry = [0.0f32; 2];
        if firmware {
            // Smoothed firmware weights through the mix stage (level L, ASSUMED 1.0): the bus
            // already carries the wheel signals (x table, no per-wheel calibration).
            let drawbars = *self.active_drawbars();
            let wa = self.weight_state.step(
                &self.held_notes,
                &drawbars,
                self.octave_shift.clamp(-2, 2),
                self.sample_rate,
            );
            let l = self.weight_level * bus_gain * db_bus_scale;
            for (acc, bus) in dry.iter_mut().zip(buses.iter()).take(lanes) {
                *acc = l * wa.iter().zip(bus.iter()).map(|(w, x)| w * x).sum::<f32>();
            }
        }
        // Percussion keeps the legacy gains; in firmware mode they follow the ratio of the
        // firmware weight unit to the legacy 0.08 bus scale so the drawbar/percussion balance holds.
        let perc_scale = if firmware {
            UNIT_WEIGHT * self.weight_level * bus_gain / 0.08
        } else {
            1.0
        };
        for note in 0u8..=127u8 {
            if !self.held_notes[note as usize] {
                continue;
            }
            let base_midi = i32::from(note) + oct_semis;
            let key_gain = self.note_gains[note as usize];
            let legacy_drawbars = if firmware {
                &[][..]
            } else {
                &self.active_drawbars()[..]
            };
            for (db_idx, &level) in legacy_drawbars.iter().enumerate() {
                if level == 0 {
                    continue;
                }
                let gain = drawbar_gain_law(level, self.drawbar_law);
                let tw = foldback_tonewheel(base_midi + DRAWBAR_OFFSETS[db_idx]);
                for (acc, bus) in dry.iter_mut().zip(buses.iter()).take(lanes) {
                    *acc += gain * bus[tw] * key_gain * db_bus_scale * 0.08;
                }
            }
            if self.percussion_env > 1e-4 {
                let perc_offset = if self.percussion_third_harmonic {
                    19
                } else {
                    12
                };
                let tw = foldback_tonewheel(base_midi + perc_offset);
                let perc_gain = if self.percussion_soft { 0.06 } else { 0.12 };
                for (acc, bus) in dry.iter_mut().zip(buses.iter()).take(lanes) {
                    *acc += self.perc_env_for(note as usize)
                        * bus[tw]
                        * perc_gain
                        * key_gain
                        * perc_scale;
                }
            }
        }
        dry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lanes_are_summed_independently() {
        let mut o = OrganLayer::new(48_000.0);
        o.percussion_enabled = false;
        o.drawbars = [0, 0, 8, 0, 0, 0, 0, 0, 0];
        o.note_on(69); // wheel 45 at 8'
        let mut buses = [[0.0f32; NUM_TONEWHEELS]; 2];
        buses[0][45] = 1.0;
        buses[1][45] = -0.5;
        let two = o.sum_drawbars(&buses, 2);
        assert!(
            (two[0] - 0.08).abs() < 1e-6 && (two[1] + 0.04).abs() < 1e-6,
            "{two:?}"
        );
        let one = o.sum_drawbars(&buses, 1);
        assert_eq!(one, [two[0], 0.0]);
    }

    #[test]
    fn no_held_keys_sum_to_zero() {
        let mut o = OrganLayer::new(48_000.0);
        assert_eq!(o.sum_drawbars(&[[1.0; NUM_TONEWHEELS]; 2], 2), [0.0, 0.0]);
    }
}
