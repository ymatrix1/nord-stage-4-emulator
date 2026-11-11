//! CLAP and VST3 DAW Plugin wrapper for the Nord Stage 4 Emulator.

pub mod params;

use nih_plug::prelude::*;
use nord_core::engine::Stage4Engine;
use nord_core::midi::MidiEvent;
use params::{MidiRoutingMode, Stage4Params};
use std::num::NonZeroU32;
use std::sync::Arc;

/// Real-time CLAP/VST3/Standalone plugin hosting `Stage4Engine`.
pub struct NordStage4Plugin {
    params: Arc<Stage4Params>,
    engine: Stage4Engine,
}

impl Default for NordStage4Plugin {
    fn default() -> Self {
        let params = Arc::new(Stage4Params::default());
        let mut engine = Stage4Engine::new(48_000.0);
        params.sync_to_engine(&mut engine);
        Self { params, engine }
    }
}

/// Translates a sample-accurate NIH-plug MIDI event into a `nord_core::midi::MidiEvent`.
pub fn note_event_to_midi<S>(event: &NoteEvent<S>, mode: MidiRoutingMode) -> Option<MidiEvent> {
    let map_ch = |ch: u8| match mode {
        MidiRoutingMode::OmniSplit => 0,
        MidiRoutingMode::MultitimbralPerLayer => ch & 0x0F,
    };

    match *event {
        NoteEvent::NoteOn {
            channel,
            note,
            velocity,
            ..
        } => {
            let vel = ((velocity * 127.0).round() as u8).clamp(1, 127);
            Some(MidiEvent::NoteOn {
                channel: map_ch(channel),
                note,
                velocity: vel,
            })
        }
        NoteEvent::NoteOff {
            channel,
            note,
            velocity,
            ..
        } => {
            let vel = (velocity * 127.0).round().clamp(0.0, 127.0) as u8;
            Some(MidiEvent::NoteOff {
                channel: map_ch(channel),
                note,
                velocity: vel,
            })
        }
        NoteEvent::MidiCC {
            channel, cc, value, ..
        } => {
            let val = (value * 127.0).round().clamp(0.0, 127.0) as u8;
            Some(MidiEvent::ControlChange {
                channel: map_ch(channel),
                controller: cc,
                value: val,
            })
        }
        NoteEvent::PolyPressure {
            channel, pressure, ..
        }
        | NoteEvent::MidiChannelPressure {
            channel, pressure, ..
        } => {
            let p = (pressure * 127.0).round().clamp(0.0, 127.0) as u8;
            Some(MidiEvent::ChannelAftertouch {
                channel: map_ch(channel),
                pressure: p,
            })
        }
        _ => None,
    }
}

impl Plugin for NordStage4Plugin {
    const NAME: &'static str = "Nord Stage 4 Emulator";
    const VENDOR: &'static str = "Nord Stage 4 Contributors";
    const URL: &'static str = env!("CARGO_PKG_REPOSITORY");
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        aux_input_ports: &[],
        aux_output_ports: &[],
        names: PortNames::const_default(),
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
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
        self.engine = Stage4Engine::new(buffer_config.sample_rate);
        self.params.sync_to_engine(&mut self.engine);
        true
    }

    fn reset(&mut self) {
        self.engine = Stage4Engine::new(self.engine.sample_rate);
        self.params.sync_to_engine(&mut self.engine);
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.params.sync_to_engine(&mut self.engine);
        let midi_mode = self.params.midi_mode.value();
        let num_samples = buffer.samples();
        let out_slices = buffer.as_slice();
        if out_slices.is_empty() || num_samples == 0 {
            return ProcessStatus::Normal;
        }

        let mut frame_cursor = 0usize;
        let mut next_event = context.next_event();

        while let Some(event) = next_event {
            let target_sample = (event.timing() as usize).min(num_samples);
            while frame_cursor < target_sample {
                let [l, r] = self.engine.render_frame();
                write_stereo_frame(out_slices, frame_cursor, l, r);
                frame_cursor += 1;
            }
            if let Some(midi) = note_event_to_midi(&event, midi_mode) {
                self.engine.handle_midi(midi);
            }
            next_event = context.next_event();
        }

        while frame_cursor < num_samples {
            let [l, r] = self.engine.render_frame();
            write_stereo_frame(out_slices, frame_cursor, l, r);
            frame_cursor += 1;
        }

        ProcessStatus::Normal
    }
}

#[inline]
fn write_stereo_frame(out_slices: &mut [&mut [f32]], idx: usize, l: f32, r: f32) {
    if out_slices.len() >= 2 {
        out_slices[0][idx] = l;
        out_slices[1][idx] = r;
        for ch in out_slices.iter_mut().skip(2) {
            ch[idx] = 0.0;
        }
    } else if let Some(mono) = out_slices.first_mut() {
        mono[idx] = 0.5 * (l + r);
    }
}

impl ClapPlugin for NordStage4Plugin {
    const CLAP_ID: &'static str = "org.nordstage4.emulator";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Nord Stage 4 7-layer stage keyboard emulator");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for NordStage4Plugin {
    const VST3_CLASS_ID: [u8; 16] = *b"NordStage4Emul01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Instrument,
        Vst3SubCategory::Synth,
        Vst3SubCategory::Piano,
    ];
}

nih_export_clap!(NordStage4Plugin);
nih_export_vst3!(NordStage4Plugin);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_event_translation_omni_and_multitimbral() {
        let ev = NoteEvent::<()>::NoteOn {
            timing: 16,
            voice_id: None,
            channel: 4,
            note: 60,
            velocity: 1.0,
        };
        assert_eq!(
            note_event_to_midi(&ev, MidiRoutingMode::OmniSplit),
            Some(MidiEvent::NoteOn {
                channel: 0,
                note: 60,
                velocity: 127
            })
        );
        assert_eq!(
            note_event_to_midi(&ev, MidiRoutingMode::MultitimbralPerLayer),
            Some(MidiEvent::NoteOn {
                channel: 4,
                note: 60,
                velocity: 127
            })
        );
    }

    #[test]
    fn test_plugin_engine_renders_stereo_audio() {
        let mut plugin = NordStage4Plugin::default();
        plugin.engine.handle_midi(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100,
        });

        let mut energy = 0.0f32;
        for _ in 0..512 {
            let [l, r] = plugin.engine.render_frame();
            assert!(l.is_finite() && r.is_finite());
            energy += l * l + r * r;
        }
        assert!(energy > 0.01);
    }
}
