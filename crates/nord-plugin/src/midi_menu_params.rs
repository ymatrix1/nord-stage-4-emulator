//! MIDI and Extern menu host parameters (manual OS v1.6x p.56, p.61-64; spec
//! `docs/specs/midi-and-control.md` section 2a).
//!
//! IDs are additive under the `midix` prefix: `midix_sect_{organ,piano,synth}`, `midix_nrpn`,
//! `midix_{ctrl,dev,pc,pc_ext}`, `midix_transpose_at`, `midix_echo` and the per Extern layer
//! `midix_{vel,mod,aft,ctl,swl,vol}_1..3`. Defaults equal `MidiMenuSettings::default()` and
//! `ExternMenu::default()`, so an untouched host renders exactly as before.
//!
//! Local Control is deliberately not a host parameter: the manual says it reverts to On at every
//! power-up (p.57), which a persisted parameter would defeat. `sync_to_engine` keeps the engine's
//! value.

use nih_plug::prelude::*;
use nord_core::engine::{
    AuxVelocityCurve, ExternDevices, ExternMenu, MidiMenuSettings, MidiMode, Stage4Engine,
    TransposeAt,
};

fn int(name: &str, default: i32, max: i32) -> IntParam {
    IntParam::new(name, default, IntRange::Linear { min: 0, max })
}

/// Device MIDI switches and Keyboard Velocity of one Extern layer.
#[derive(Params)]
pub struct ExternLayerMenuParams {
    /// Keyboard Velocity: 0 Soft, 1 Mid (default), 2 Hard.
    #[id = "vel"]
    pub velocity: IntParam,
    /// Device MIDI: Mod Wheel (default On).
    #[id = "mod"]
    pub mod_wheel: BoolParam,
    /// Device MIDI: Aftertouch (default On).
    #[id = "aft"]
    pub aftertouch: BoolParam,
    /// Device MIDI: Control Pedal (default On).
    #[id = "ctl"]
    pub control_pedal: BoolParam,
    /// Device MIDI: Swell Pedal (default On).
    #[id = "swl"]
    pub swell_pedal: BoolParam,
    /// Device MIDI: Volume Pedal (default On).
    #[id = "vol"]
    pub volume_pedal: BoolParam,
}

impl Default for ExternLayerMenuParams {
    fn default() -> Self {
        Self {
            velocity: int("Extern Keyboard Velocity", 1, 2),
            mod_wheel: BoolParam::new("Extern Mod Wheel", true),
            aftertouch: BoolParam::new("Extern Aftertouch", true),
            control_pedal: BoolParam::new("Extern Control Pedal", true),
            swell_pedal: BoolParam::new("Extern Swell Pedal", true),
            volume_pedal: BoolParam::new("Extern Volume Pedal", true),
        }
    }
}

/// Global MIDI menu and Extern menu settings.
#[derive(Params)]
pub struct MidiMenuParams {
    /// Sect On of the Organ section (MIDI page 3, default Off).
    #[id = "sect_organ"]
    pub sect_organ: BoolParam,
    /// Sect On of the Piano section (MIDI page 4, default Off).
    #[id = "sect_piano"]
    pub sect_piano: BoolParam,
    /// Sect On of the Synth section (MIDI page 5, default Off).
    #[id = "sect_synth"]
    pub sect_synth: BoolParam,
    /// Control Change Type (MIDI page 7): On = CC & NRPN (default), Off = CC.
    #[id = "nrpn"]
    pub nrpn: BoolParam,
    /// Ctrl: 0 Off, 1 Send, 2 Receive, 3 Send & Receive (default, see `midi_menu.rs`).
    #[id = "ctrl"]
    pub ctrl: IntParam,
    /// Dev: same coding, default Send & Receive.
    #[id = "dev"]
    pub dev: IntParam,
    /// PC: same coding, default Send & Receive.
    #[id = "pc"]
    pub pc: IntParam,
    /// PC Ext: same coding, default Send & Receive.
    #[id = "pc_ext"]
    pub pc_ext: IntParam,
    /// Transpose MIDI At: 0 In (default), 1 Out.
    #[id = "transpose_at"]
    pub transpose_at: IntParam,
    /// Echo Global To Extern Ch (Extern page 1; engine default On, see `extern_menu.rs`).
    #[id = "echo"]
    pub echo: BoolParam,
    /// Keyboard Velocity and Device MIDI of Extern A, B, C.
    #[nested(array, group = "Extern Layers")]
    pub extern_layers: [ExternLayerMenuParams; 3],
}

impl Default for MidiMenuParams {
    fn default() -> Self {
        Self {
            sect_organ: BoolParam::new("Organ Sect On", false),
            sect_piano: BoolParam::new("Piano Sect On", false),
            sect_synth: BoolParam::new("Synth Sect On", false),
            nrpn: BoolParam::new("Control Change Type CC & NRPN", true),
            ctrl: int("MIDI Ctrl", 3, 3),
            dev: int("MIDI Dev", 3, 3),
            pc: int("MIDI PC", 3, 3),
            pc_ext: int("MIDI PC Ext", 3, 3),
            transpose_at: int("Transpose MIDI At", 0, 1),
            echo: BoolParam::new("Echo Global To Extern Ch", true),
            extern_layers: Default::default(),
        }
    }
}

impl MidiMenuParams {
    /// The MIDI menu settings these parameters describe; `local_control` is the engine's.
    pub fn to_settings(&self, local_control: bool) -> MidiMenuSettings {
        let mode = |p: &IntParam| MidiMode::from_index(p.value().max(0) as usize);
        MidiMenuSettings {
            local_control,
            sect_on: [
                self.sect_organ.value(),
                self.sect_piano.value(),
                self.sect_synth.value(),
            ],
            nrpn: self.nrpn.value(),
            ctrl: mode(&self.ctrl),
            dev: mode(&self.dev),
            pc: mode(&self.pc),
            pc_ext: mode(&self.pc_ext),
            transpose_at: if self.transpose_at.value() == 1 {
                TransposeAt::Out
            } else {
                TransposeAt::In
            },
        }
    }

    /// The Extern menu settings these parameters describe.
    pub fn to_extern_menu(&self) -> ExternMenu {
        let curve = |v: i32| match v {
            0 => AuxVelocityCurve::Soft,
            2.. => AuxVelocityCurve::Hard,
            _ => AuxVelocityCurve::Medium,
        };
        ExternMenu {
            echo_global: self.echo.value(),
            velocity: std::array::from_fn(|i| curve(self.extern_layers[i].velocity.value())),
            devices: std::array::from_fn(|i| {
                let l = &self.extern_layers[i];
                ExternDevices {
                    mod_wheel: l.mod_wheel.value(),
                    aftertouch: l.aftertouch.value(),
                    control_pedal: l.control_pedal.value(),
                    swell_pedal: l.swell_pedal.value(),
                    volume_pedal: l.volume_pedal.value(),
                }
            }),
        }
    }

    /// Copy every value into the engine (no allocation; audio-thread safe).
    pub fn sync_to_engine(&self, engine: &mut Stage4Engine) {
        engine.midi_menu = self.to_settings(engine.midi_menu.local_control);
        engine.extern_menu = self.to_extern_menu();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::midi::MidiEvent;

    #[test]
    fn defaults_equal_engine_defaults() {
        let p = MidiMenuParams::default();
        assert_eq!(p.to_settings(true), MidiMenuSettings::default());
        assert_eq!(p.to_extern_menu(), ExternMenu::default());
    }

    #[test]
    fn default_sync_changes_nothing_and_keeps_local_control() {
        let mut e = Stage4Engine::new(44_100.0);
        e.midi_menu.local_control = false;
        MidiMenuParams::default().sync_to_engine(&mut e);
        assert!(!e.midi_menu.local_control, "not a host parameter");
        e.midi_menu.local_control = true;
        assert_eq!(e.midi_menu, MidiMenuSettings::default());
        let run = |sync: bool| {
            let mut e = Stage4Engine::new(44_100.0);
            if sync {
                MidiMenuParams::default().sync_to_engine(&mut e);
            }
            for n in [48u8, 60, 67] {
                e.handle_midi(MidiEvent::NoteOn {
                    channel: 0,
                    note: n,
                    velocity: 100,
                });
            }
            (0..2048).map(|_| e.render_frame()).collect::<Vec<_>>()
        };
        assert_eq!(run(false), run(true));
    }

    #[test]
    fn every_value_maps_to_its_engine_field() {
        let p = MidiMenuParams {
            sect_organ: BoolParam::new("a", true),
            sect_synth: BoolParam::new("b", true),
            nrpn: BoolParam::new("c", false),
            ctrl: int("d", 2, 3),
            dev: int("e", 1, 3),
            pc: int("f", 0, 3),
            pc_ext: int("g", 2, 3),
            transpose_at: int("h", 1, 1),
            echo: BoolParam::new("i", false),
            extern_layers: [
                ExternLayerMenuParams {
                    velocity: int("v", 0, 2),
                    mod_wheel: BoolParam::new("m", false),
                    ..ExternLayerMenuParams::default()
                },
                ExternLayerMenuParams {
                    velocity: int("v", 2, 2),
                    volume_pedal: BoolParam::new("m", false),
                    ..ExternLayerMenuParams::default()
                },
                ExternLayerMenuParams::default(),
            ],
            ..MidiMenuParams::default()
        };
        let mut e = Stage4Engine::new(44_100.0);
        p.sync_to_engine(&mut e);
        let m = e.midi_menu;
        assert_eq!(m.sect_on, [true, false, true]);
        assert!(!m.nrpn && m.local_control);
        assert_eq!(
            (m.ctrl, m.dev, m.pc, m.pc_ext),
            (
                MidiMode::Receive,
                MidiMode::Send,
                MidiMode::Off,
                MidiMode::Receive
            )
        );
        assert_eq!(m.transpose_at, TransposeAt::Out);
        let x = e.extern_menu;
        assert!(!x.echo_global);
        assert_eq!(x.velocity[0], AuxVelocityCurve::Soft);
        assert_eq!(x.velocity[1], AuxVelocityCurve::Hard);
        assert_eq!(x.velocity[2], AuxVelocityCurve::Medium);
        assert!(!x.devices[0].mod_wheel && x.devices[0].volume_pedal);
        assert!(!x.devices[1].volume_pedal && x.devices[1].mod_wheel);
    }

    #[test]
    fn ids_unique_and_complete() {
        let ids: Vec<String> = MidiMenuParams::default()
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        // 10 globals + 3 Extern layers x 6
        assert_eq!(ids.len(), 28);
        let mut s = ids.clone();
        s.sort();
        s.dedup();
        assert_eq!(s.len(), ids.len());
    }
}
