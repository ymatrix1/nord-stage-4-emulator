//! The per-frame render of `Stage4Engine`: layers, FX racks, rotary bus, output routing, master.
//!
//! Split out of `mod.rs` so the four-output mix (see `output_routing.rs`) has room. With the default
//! routing every source feeds outputs 1-2 in the order of the previous stereo mix, so the Main
//! pair is bit-identical (`output_routing::tests::default_routing_is_bit_identical_to_the_stereo_mix`
//! and the render hash tests).

use super::output_routing::LAYER_SOURCE;
use super::*;

impl Stage4Engine {
    /// Renders one frame of all four physical outputs `[1, 2, 3, 4]`. Outputs 1-2 are the Main
    /// pair that [`Self::render_frame`] returns. Allocation free.
    pub fn render_frame_multi(&mut self) -> [f32; 4] {
        self.master_clock.advance(self.sample_rate);
        self.pedals.triple.tick();
        self.tick_organ_trigger();
        self.sync_synth_clock();

        // Apply Morphing Matrix to Synth filter cutoffs via OS v1.66 14 Hz .. 32 kHz curve
        for i in 0..3 {
            let m = &self.synth_cutoff_morph[i];
            if m.delta_wheel.abs() > 1e-5
                || m.delta_pedal.abs() > 1e-5
                || m.delta_aftertouch.abs() > 1e-5
            {
                let cut = crate::cbin::cutoff_norm_to_hz(m.evaluate(&self.morph_controllers));
                match i {
                    0 => self.synth_a.cutoff_hz = cut,
                    1 => self.synth_b.cutoff_hz = cut,
                    _ => self.synth_c.cutoff_hz = cut,
                }
            }
        }

        let mut raw_layers = [
            self.organ_a.render_frame(),
            self.organ_b.render_frame(),
            self.piano_a.render_frame(),
            self.piano_b.render_frame(),
            self.synth_a.render_frame(),
            self.synth_b.render_frame(),
            self.synth_c.render_frame(),
        ];
        self.apply_organ_swell(&mut raw_layers);
        self.mix_outputs(raw_layers)
    }

    /// FX racks, faders, rotary bus, routing, shared reverb, volume pedal and the Main master bus.
    fn mix_outputs(&mut self, raw_layers: [[f32; 2]; 7]) -> [f32; 4] {
        let dest = self
            .output_routing
            .destinations(self.rotary_follow_source());
        let mut acc = [0.0f32; 4];
        let mut rot_in = 0.0f32;
        let mut rot_used = false;

        for (i, raw) in raw_layers.into_iter().enumerate() {
            self.fx_chains[i].apply_morph(&self.morph_controllers);
            let [fx_l, fx_r] = self.fx_chains[i].process_frame_with_bpm(raw, self.master_clock.bpm);
            let level = self.layer_levels[i].evaluate(&self.morph_controllers);
            // organ layers follow the firmware level law `(v / 127)^2` when it is selected
            let level = match i {
                0 => self.organ_a.level_law().layer_gain(level),
                1 => self.organ_b.level_law().layer_gain(level),
                2 => crate::piano::downstream::layer_gain(level, self.piano_a.firmware_layer_law),
                3 => crate::piano::downstream::layer_gain(level, self.piano_b.firmware_layer_law),
                _ => level,
            };
            let gain = level * self.program_gain;
            // TO ROTARY (Piano / Synth layers): the mono sum after the fader feeds the shared rotary.
            if i >= 2 && self.fx_chains[i].amp_cabinet == AmpCabinet::ToRotary {
                rot_in += 0.5 * (fx_l + fx_r) * gain;
                rot_used = true;
                continue;
            }
            dest[LAYER_SOURCE[i]].add(&mut acc, fx_l * gain, fx_r * gain);
        }
        if rot_used {
            self.rotary_bus.mirror_rotary_settings(&self.organ_a);
            let [rl, rr] = self.rotary_bus.process_external_rotary(rot_in);
            dest[super::output_routing::SRC_ROTARY].add(&mut acc, rl, rr);
        }
        if self.fx_sharing.reverb_global.is_some() {
            self.global_reverb.apply_morph(&self.morph_controllers);
            [acc[0], acc[1]] = self.global_reverb.process_reverb_frame([acc[0], acc[1]]);
        }

        let volume = self.pedals.volume_gain();
        if volume != 1.0 {
            for a in &mut acc {
                *a *= volume;
            }
        }
        // The master bus (compressor, EQ) and Mono Output act on the Main pair (ASSUMED).
        let main = self.master_bus.process_frame([acc[0], acc[1]]);
        let main = if self.settings.mono_output {
            let mid = 0.5 * (main[0] + main[1]);
            [mid, mid]
        } else {
            main
        };
        let trim = self.output_routing.trim_gains();
        [
            main[0] * trim[0],
            main[1] * trim[1],
            acc[2] * trim[2],
            acc[3] * trim[3],
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_engine_renders_four_silent_outputs() {
        let mut e = Stage4Engine::new(48_000.0);
        for _ in 0..16 {
            assert_eq!(e.render_frame_multi(), [0.0; 4]);
        }
    }

    #[test]
    fn mono_output_acts_on_the_main_pair_only() {
        let mut e = Stage4Engine::new(44_100.0);
        e.scene.scene_1_mask = 1 << LayerId::PianoA as u8;
        e.output_routing.global_out[2] = 6; // 12+34
        e.set_global_settings(GlobalSettings {
            mono_output: true,
            ..GlobalSettings::default()
        });
        e.handle_midi(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100,
        });
        let mut sub_differs = false;
        for _ in 0..4096 {
            let f = e.render_frame_multi();
            assert_eq!(f[0], f[1]);
            sub_differs |= f[2] != f[3];
        }
        assert!(sub_differs, "the sub pair keeps its stereo image");
    }

    #[test]
    fn piano_layer_law_squares_the_fader_only_when_selected() {
        let rms = |law: bool| {
            let mut e = Stage4Engine::new(44_100.0);
            e.scene.scene_1_mask = 1 << LayerId::PianoA as u8;
            e.piano_a.firmware_layer_law = law;
            e.layer_levels[LayerId::PianoA as usize] = MorphParameter::new(0.5);
            e.handle_midi(MidiEvent::NoteOn {
                channel: 0,
                note: 60,
                velocity: 100,
            });
            let mut sum = 0.0f64;
            for _ in 0..8192 {
                let f = e.render_frame_multi();
                sum += f64::from(f[0]) * f64::from(f[0]) + f64::from(f[1]) * f64::from(f[1]);
            }
            sum.sqrt()
        };
        let (linear, squared) = (rms(false), rms(true));
        assert!(linear > 0.0, "the piano sounds");
        // level 0.5: linear 0.5, firmware law 0.25 (the engine path is linear after the fader)
        assert!(
            (squared / linear - 0.5).abs() < 0.02,
            "{}",
            squared / linear
        );
    }
}
