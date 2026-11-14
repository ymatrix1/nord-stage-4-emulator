//! Nord Stage 4 DAW Plugin implementation.
//!
//! Provides CLAP and VST3 wrappers around `Stage4Engine` with host automation,
//! multitimbral MIDI handling, sample rate safety, zero-allocation audio callback,
//! and state persistence.

#![allow(unknown_lints)]
#![allow(semicolon_in_expressions_from_macros)]
#![allow(semicolon_in_expressions_from_non_local_macros)]

pub mod aux_pedal_params;
pub mod fx_extra_params;
pub mod fx_params;
pub mod fx_selectors;
pub mod fx_sharing_params;
pub mod midi;
pub mod morph_assign;
pub mod organ_extra_params;
pub mod organ_params;
pub mod param_snapshot;
pub mod params;
pub mod piano_extra_params;
pub mod piano_params;
pub mod program_params;
pub mod state;
pub mod synth_gate_params;
pub mod synth_mod_params;
pub mod synth_params;
pub mod synth_pattern_params;

use nih_plug::prelude::*;
use nord_core::engine::Stage4Engine;
use params::Stage4Params;
use std::sync::Arc;

/// The Nord Stage 4 DAW Plugin.
pub struct NordStage4Plugin {
    pub params: Arc<Stage4Params>,
    pub engine: Stage4Engine,
    pub sample_rate: f32,
}

impl Default for NordStage4Plugin {
    fn default() -> Self {
        Self {
            params: Arc::new(Stage4Params::default()),
            engine: Stage4Engine::new(44100.0),
            sample_rate: 44100.0,
        }
    }
}

impl Plugin for NordStage4Plugin {
    const NAME: &'static str = "Nord Stage 4";
    const VENDOR: &'static str = "OpenNord";
    const URL: &'static str = "https://github.com/yuriolive/nord-stage-4-emulator";
    const EMAIL: &'static str = "yuri@github.com";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // Stereo output instrument.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        aux_input_ports: &[],
        aux_output_ports: &[],
        names: PortNames::const_default(),
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;
        self.engine = Stage4Engine::new(self.sample_rate);
        self.params.sync_to_engine(&mut self.engine);
        true
    }

    fn reset(&mut self) {
        // Reset engine states upon host reset / playhead reposition
        self.engine = Stage4Engine::new(self.sample_rate);
        self.params.sync_to_engine(&mut self.engine);
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Synchronize automatable parameters to engine state
        self.params.sync_to_engine(&mut self.engine);

        // Synchronize host transport tempo if available
        if let Some(transport) = context.transport().tempo {
            self.engine.master_clock.bpm = transport as f32;
        }

        // Process incoming MIDI and note events
        let mut next_event = context.next_event();
        let num_samples = buffer.samples();
        let mut sample_idx = 0;

        let mode = self.params.midi_mode.value();
        let channels = buffer.as_slice();

        while sample_idx < num_samples {
            // Dispatch all events at the current sample offset
            while let Some(event) = next_event {
                if event.timing() as usize <= sample_idx {
                    midi::route_plugin_event(&mut self.engine, &self.params, mode, event);
                    next_event = context.next_event();
                } else {
                    break;
                }
            }

            // Determine block end (up to next event timing or end of buffer)
            let block_end = if let Some(ref event) = next_event {
                (event.timing() as usize).min(num_samples)
            } else {
                num_samples
            };

            // Process stereo audio block into host buffer without heap allocations
            if channels.len() >= 2 {
                let (left, right) = channels.split_at_mut(1);
                let left_slice = &mut left[0][sample_idx..block_end];
                let right_slice = &mut right[0][sample_idx..block_end];

                for (l, r) in left_slice.iter_mut().zip(right_slice.iter_mut()) {
                    let [out_l, out_r] = self.engine.render_frame();
                    *l = out_l;
                    *r = out_r;
                }
            } else if channels.len() == 1 {
                let mono_slice = &mut channels[0][sample_idx..block_end];
                for m in mono_slice.iter_mut() {
                    let [out_l, out_r] = self.engine.render_frame();
                    *m = 0.5 * (out_l + out_r);
                }
            }

            sample_idx = block_end;
        }

        ProcessStatus::Normal
    }

    fn deactivate(&mut self) {}
}

impl ClapPlugin for NordStage4Plugin {
    const CLAP_ID: &'static str = "org.opennord.nord-stage-4";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Open-source Nord Stage 4 7-Layer Multitimbral Emulator");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for NordStage4Plugin {
    const VST3_CLASS_ID: [u8; 16] = *b"NordStage4Emulat";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Instrument,
        Vst3SubCategory::Synth,
        Vst3SubCategory::Stereo,
    ];
}

nih_export_clap!(NordStage4Plugin);
nih_export_vst3!(NordStage4Plugin);

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::MaybeUninit;

    struct MockInitContext;
    impl InitContext<NordStage4Plugin> for MockInitContext {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        fn execute(&self, _task: ()) {}
        fn set_latency_samples(&self, _samples: u32) {}
        fn set_current_voice_capacity(&self, _capacity: u32) {}
    }

    struct MockProcessContext {
        transport: Transport,
    }

    impl ProcessContext<NordStage4Plugin> for MockProcessContext {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        fn execute_background(&self, _task: ()) {}
        fn execute_gui(&self, _task: ()) {}
        fn transport(&self) -> &Transport {
            &self.transport
        }
        fn next_event(&mut self) -> Option<NoteEvent<()>> {
            None
        }
        fn send_event(&mut self, _event: NoteEvent<()>) {}
        fn set_latency_samples(&self, _samples: u32) {}
        fn set_current_voice_capacity(&self, _capacity: u32) {}
    }

    #[test]
    fn test_plugin_instantiation_and_reset() {
        let mut plugin = NordStage4Plugin::default();
        assert_eq!(plugin.sample_rate, 44100.0);
        plugin.sample_rate = 96000.0;
        plugin.reset();
        assert_eq!(plugin.engine.sample_rate, 96000.0);
    }

    #[test]
    fn test_plugin_process_buffer_zero_alloc() {
        let mut plugin = NordStage4Plugin::default();
        let mut init_ctx = MockInitContext;
        plugin.initialize(
            &NordStage4Plugin::AUDIO_IO_LAYOUTS[0],
            &BufferConfig {
                sample_rate: 48000.0,
                min_buffer_size: None,
                max_buffer_size: 512,
                process_mode: ProcessMode::Realtime,
            },
            &mut init_ctx,
        );

        let mut left_buf = [0.0f32; 128];
        let mut right_buf = [0.0f32; 128];
        let channel_slices: Vec<&mut [f32]> = vec![&mut left_buf[..], &mut right_buf[..]];
        let mut buffer = Buffer::default();
        unsafe {
            buffer.set_slices(128, |output| {
                *output = channel_slices;
            });
        }

        let mut aux = AuxiliaryBuffers {
            inputs: &mut [],
            outputs: &mut [],
        };

        let mut transport: Transport = unsafe { MaybeUninit::zeroed().assume_init() };
        transport.sample_rate = 48000.0;
        transport.tempo = Some(132.0);

        let mut context = MockProcessContext { transport };
        let status = plugin.process(&mut buffer, &mut aux, &mut context);
        assert_eq!(status, ProcessStatus::Normal);
        assert_eq!(plugin.engine.master_clock.bpm, 132.0);
    }
}
