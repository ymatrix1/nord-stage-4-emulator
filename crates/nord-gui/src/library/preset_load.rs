//! Preset Library loads: a `.ns4o` / `.ns4n` / `.ns4y` entry of the opened `.ns4b` (or a preset
//! stored with STORE) goes into its section of the running program.
//!
//! The decoded preset goes to the DSP thread as `ProgramEdit::LoadPreset` (undoable), the panel
//! controls follow it (`Stage4PanelState::apply_section_preset`), and the sounds it names (Piano
//! `.npno`, Synth Sample `.nsmp4`) are scheduled like a program's.

use super::BackupLibraryState;
use crate::audio::{AudioCommand, AudioHandle, ProgramEdit};
use crate::panel::bay_program::preset_user::UserPreset;
use crate::panel::{FwMessage, Stage4PanelState};
use nord_core::cbin::{PresetSection, ProgramSoundRefs, SectionPreset};
use nord_core::morph::LayerId;
use std::fs::File;
use std::io::BufReader;

impl BackupLibraryState {
    /// Bytes of an archive entry of the opened backup.
    pub(super) fn read_archive_entry(&self, name: &str) -> Result<Vec<u8>, String> {
        let (Some(path), Some(archive)) = (&self.backup_path, &self.archive) else {
            return Err("Open an .ns4b archive first".to_string());
        };
        let file = File::open(path).map_err(|e| format!("{e}"))?;
        archive
            .read_entry(&mut BufReader::new(file), name)
            .map_err(|e| format!("{e}"))
    }

    /// The preset behind `entry`: decoded from the archive, or captured from a stored preset
    /// (its snapshot on top of the archive entry it was based on).
    pub(super) fn read_preset(
        &self,
        section: PresetSection,
        entry: &str,
        user: Option<&UserPreset>,
    ) -> Result<SectionPreset, String> {
        let decode = |name: &str| {
            let bytes = self.read_archive_entry(name)?;
            SectionPreset::from_bytes(&bytes).map_err(|e| format!("{name}: {e}"))
        };
        let preset = match user {
            Some(u) => {
                let base = u.base.as_deref().map(decode).transpose()?;
                u.state.capture_section_preset(section, base.as_ref())
            }
            None => decode(entry)?,
        };
        if preset.section() != section {
            return Err(format!("{entry} is not a {section:?} preset"));
        }
        Ok(preset)
    }

    /// Loads `entry` into `section` (or the layer `single`) and schedules its sounds.
    pub fn load_preset_entry(
        &mut self,
        section: PresetSection,
        entry: &str,
        single: Option<LayerId>,
        panel: &mut Stage4PanelState,
        audio: &AudioHandle,
    ) {
        let user = panel.program_ui.library.presets.user(entry).cloned();
        let preset = match self.read_preset(section, entry, user.as_ref()) {
            Ok(p) => p,
            Err(e) => {
                self.error_message = Some(format!("Failed to load '{entry}': {e}"));
                return;
            }
        };
        let refs = self
            .archive
            .as_ref()
            .map(|a| preset.resolve_sounds(a))
            .unwrap_or_default();
        self.notices.extend(missing_sounds(&preset, single, &refs));
        panel.apply_section_preset(&preset, single);
        audio.send_command(AudioCommand::Program(ProgramEdit::LoadPreset {
            preset: Box::new(preset.clone()),
            single,
        }));
        self.schedule_preset_sounds(&preset, single, &refs);
        let name = crate::panel::bay_program::catalog::display_name(entry);
        self.error_message = None;
        self.status_message = format!("Preset: {name}");
    }

    /// Queues the `.npno` / `.nsmp4` loads of a preset: every enabled layer of a section load, or
    /// the one layer of a single-layer load (which takes layer A of the file).
    fn schedule_preset_sounds(
        &mut self,
        preset: &SectionPreset,
        single: Option<LayerId>,
        refs: &ProgramSoundRefs,
    ) {
        match preset {
            SectionPreset::Organ(_) => {}
            SectionPreset::Piano(p) => {
                let mut jobs: Vec<(bool, &Option<String>)> = Vec::new();
                match single {
                    Some(id) => jobs.push((id == LayerId::PianoB, &refs.piano_a)),
                    None => {
                        if p.layer_a.enabled {
                            jobs.push((false, &refs.piano_a));
                        }
                        if p.layer_b.enabled {
                            jobs.push((true, &refs.piano_b));
                        }
                    }
                }
                for (slot_b, entry) in jobs {
                    if let Some(e) = entry {
                        self.schedule_piano_entry(e, slot_b, false);
                    }
                }
            }
            SectionPreset::Synth(s) => {
                let jobs: Vec<(usize, &Option<String>)> = match single {
                    Some(id) => vec![(usize::from(id as u8).saturating_sub(4), &refs.synth_a)],
                    None => [
                        (s.layer_a.enabled, &refs.synth_a),
                        (s.layer_b.enabled, &refs.synth_b),
                        (s.layer_c.enabled, &refs.synth_c),
                    ]
                    .into_iter()
                    .enumerate()
                    .filter(|(_, (on, _))| *on)
                    .map(|(i, (_, e))| (i, e))
                    .collect(),
                };
                for (layer, entry) in jobs {
                    if let Some(e) = entry {
                        self.schedule_sample_entry(e, layer, false);
                    }
                }
            }
        }
    }
}

/// `Piano not found` / `Sample not found` for the layers the preset loads whose sound the archive
/// does not hold (same trigger as a program load, see `library/program.rs`).
fn missing_sounds(
    preset: &SectionPreset,
    single: Option<LayerId>,
    refs: &ProgramSoundRefs,
) -> Vec<FwMessage> {
    let sample = |l: &nord_core::cbin::SynthLayerDecoded, r: &Option<String>| {
        l.osc_mode == nord_core::synth::SynthOscMode::Sample && r.is_none()
    };
    match preset {
        SectionPreset::Organ(_) => Vec::new(),
        SectionPreset::Piano(p) => {
            let a = p.layer_a.enabled && refs.piano_a.is_none();
            let b = p.layer_b.enabled && refs.piano_b.is_none();
            let miss = if single.is_some() { a } else { a || b };
            miss.then_some(FwMessage::PianoNotFound)
                .into_iter()
                .collect()
        }
        SectionPreset::Synth(s) => {
            let a = s.layer_a.enabled && sample(&s.layer_a, &refs.synth_a);
            let rest = s.layer_b.enabled && sample(&s.layer_b, &refs.synth_b)
                || s.layer_c.enabled && sample(&s.layer_c, &refs.synth_c);
            let miss = if single.is_some() { a } else { a || rest };
            miss.then_some(FwMessage::SampleNotFound)
                .into_iter()
                .collect()
        }
    }
}

#[cfg(test)]
#[path = "preset_load_tests.rs"]
mod more_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::cbin::{PianoPresetDecoded, SynthPresetDecoded};
    use nord_core::synth::SynthOscMode;

    #[test]
    fn missing_sounds_follow_the_enabled_layers_and_the_single_layer_choice() {
        let none = ProgramSoundRefs::default();
        let mut p = PianoPresetDecoded::default();
        p.layer_b.enabled = false;
        let piano = SectionPreset::Piano(Box::new(p));
        assert_eq!(
            missing_sounds(&piano, None, &none),
            [FwMessage::PianoNotFound]
        );
        let found = ProgramSoundRefs {
            piano_a: Some("x.npno".into()),
            ..Default::default()
        };
        assert!(missing_sounds(&piano, None, &found).is_empty());
        let mut s = SynthPresetDecoded::default();
        s.layer_a.enabled = false;
        s.layer_b.enabled = true;
        s.layer_b.osc_mode = SynthOscMode::Sample;
        let synth = SectionPreset::Synth(Box::new(s));
        assert_eq!(
            missing_sounds(&synth, None, &none),
            [FwMessage::SampleNotFound]
        );
        assert!(missing_sounds(&synth, Some(LayerId::SynthC), &none).is_empty());
        let organ = SectionPreset::init(PresetSection::Organ);
        assert!(missing_sounds(&organ, None, &none).is_empty());
    }
}
