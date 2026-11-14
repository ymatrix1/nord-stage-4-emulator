//! Extern menu settings that shape what Extern layers transmit (manual OS v1.6x p.46-47, p.56,
//! p.63-64): Echo Global To Extern Ch, per-layer Keyboard Velocity and the Device MIDI filters.
//!
//! Spec: `docs/specs/midi-and-control.md` section 2a.
//!
//! Provenance: the entries, their choices and the `On` default of the Device MIDI filters are
//! `VERIFIED` against the manual text; the velocity curve shapes are the Aux KB ones (`ASSUMED`,
//! `AuxVelocityCurve::map`); the CC numbers a forwarded device is sent as (Mod Wheel 1, Swell 4,
//! Control Pedal 11, Volume Pedal 7) follow the manual's controller list on p.56 (`VERIFIED` for 4
//! and 11; Mod Wheel 1 and Volume 7 are the General MIDI numbers, `ASSUMED`).
//! `echo_global` defaults to `true` here: in the hardware the default is Off because MIDI In and
//! the keybed are different sources, but this emulator feeds both through `handle_midi`, so Off
//! would silence every Extern layer for MIDI played into the engine. The setting only gates the
//! MIDI-In path; keyboard notes (`handle_keyboard_midi`) always reach Extern layers.

use super::*;

/// A physical controller an Extern layer can forward (Extern menu pages 8-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternDevice {
    ModWheel,
    Aftertouch,
    ControlPedal,
    SwellPedal,
    VolumePedal,
}

/// Device MIDI switches of one Extern layer (all `true` by default, manual p.64).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExternDevices {
    pub mod_wheel: bool,
    pub aftertouch: bool,
    pub control_pedal: bool,
    pub swell_pedal: bool,
    pub volume_pedal: bool,
}

impl Default for ExternDevices {
    fn default() -> Self {
        Self {
            mod_wheel: true,
            aftertouch: true,
            control_pedal: true,
            swell_pedal: true,
            volume_pedal: true,
        }
    }
}

impl ExternDevices {
    pub fn allows(&self, device: ExternDevice) -> bool {
        match device {
            ExternDevice::ModWheel => self.mod_wheel,
            ExternDevice::Aftertouch => self.aftertouch,
            ExternDevice::ControlPedal => self.control_pedal,
            ExternDevice::SwellPedal => self.swell_pedal,
            ExternDevice::VolumePedal => self.volume_pedal,
        }
    }
}

/// Extern menu state beyond [`ExternState`] (stored per program, manual p.57).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExternMenu {
    /// Echo Global To Extern Ch (page 1), see the module docs for the default.
    pub echo_global: bool,
    /// Keyboard Velocity of Extern A, B, C (page 2, default Mid = unchanged).
    pub velocity: [AuxVelocityCurve; 3],
    /// Device MIDI of Extern A, B, C (pages 8-12).
    pub devices: [ExternDevices; 3],
}

impl Default for ExternMenu {
    fn default() -> Self {
        Self {
            echo_global: true,
            velocity: [AuxVelocityCurve::Medium; 3],
            devices: [ExternDevices::default(); 3],
        }
    }
}

impl Stage4Engine {
    /// Forwards a Global-channel controller to every enabled Extern layer whose Device MIDI switch
    /// for it is on. `value` is the CC value, or the pressure for [`ExternDevice::Aftertouch`].
    pub(super) fn extern_forward_device(&mut self, device: ExternDevice, value: u8) {
        for i in 0..3 {
            if !self.extern_out.layers[i].enabled || !self.extern_menu.devices[i].allows(device) {
                continue;
            }
            self.extern_emit(i, |ch| match device {
                ExternDevice::Aftertouch => MidiOut {
                    status: 0xD0 | (ch & 0x0F),
                    data1: value & 0x7F,
                    data2: 0,
                    len: 2,
                },
                ExternDevice::ModWheel => MidiOut::control_change(ch, cc::MOD_WHEEL, value),
                ExternDevice::ControlPedal => MidiOut::control_change(ch, cc::CONTROL_PEDAL, value),
                ExternDevice::SwellPedal => MidiOut::control_change(ch, cc::ORGAN_SWELL, value),
                ExternDevice::VolumePedal => MidiOut::control_change(ch, cc::MASTER_LEVEL, value),
            });
        }
    }

    /// Velocity an Extern layer transmits after its Keyboard Velocity curve.
    pub(super) fn extern_velocity(&self, i: usize, velocity: u8) -> u8 {
        self.extern_menu.velocity[i].map(velocity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Stage4Engine {
        let mut e = Stage4Engine::new(48_000.0);
        e.extern_out.layers[0].enabled = true;
        e.extern_out.layers[1].enabled = true;
        e
    }

    fn cc_in(e: &mut Stage4Engine, controller: u8, value: u8) {
        e.handle_midi(MidiEvent::ControlChange {
            channel: 0,
            controller,
            value,
        });
    }

    fn drain(e: &mut Stage4Engine) -> Vec<MidiOut> {
        std::iter::from_fn(|| e.pop_midi_out()).collect()
    }

    #[test]
    fn defaults_forward_everything_and_keep_velocity() {
        let m = ExternMenu::default();
        assert!(m.devices.iter().all(|d| *d == ExternDevices::default()));
        assert!(m.velocity.iter().all(|c| *c == AuxVelocityCurve::Medium));
        assert!(m.echo_global);
    }

    #[test]
    fn device_switches_gate_each_extern_layer_separately() {
        let mut e = engine();
        e.extern_menu.devices[1].mod_wheel = false;
        cc_in(&mut e, cc::MOD_WHEEL, 99);
        let out = drain(&mut e);
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].bytes(), [0xB0 | 12, cc::MOD_WHEEL, 99]);
        // Extern C is not enabled: nothing is sent for it.
        e.extern_menu.devices[1].mod_wheel = true;
        cc_in(&mut e, cc::MOD_WHEEL, 1);
        assert_eq!(drain(&mut e).len(), 2);
    }

    #[test]
    fn every_device_has_its_own_message() {
        let mut e = engine();
        e.extern_out.layers[1].enabled = false;
        cc_in(&mut e, cc::CONTROL_PEDAL, 10);
        cc_in(&mut e, cc::ORGAN_SWELL, 20);
        e.handle_midi(MidiEvent::ChannelAftertouch {
            channel: 0,
            pressure: 30,
        });
        let out = drain(&mut e);
        let bytes: Vec<_> = out.iter().map(|m| (m.bytes(), m.len)).collect();
        assert_eq!(
            bytes,
            vec![
                ([0xBC, cc::CONTROL_PEDAL, 10], 3),
                ([0xBC, cc::ORGAN_SWELL, 20], 3),
                ([0xDC, 30, 0], 2),
            ]
        );
        e.extern_menu.devices[0].aftertouch = false;
        e.handle_midi(MidiEvent::ChannelAftertouch {
            channel: 0,
            pressure: 31,
        });
        assert!(drain(&mut e).is_empty());
    }

    #[test]
    fn volume_pedal_is_the_swell_input_in_volume_mode() {
        let mut e = engine();
        e.extern_out.layers[1].enabled = false;
        e.pedals.swell_func = SwellFunc::Volume;
        cc_in(&mut e, cc::ORGAN_SWELL, 64);
        assert_eq!(drain(&mut e)[0].bytes(), [0xBC, cc::MASTER_LEVEL, 64]);
        e.extern_menu.devices[0].volume_pedal = false;
        cc_in(&mut e, cc::ORGAN_SWELL, 65);
        assert!(
            drain(&mut e).is_empty(),
            "swell switch is not the volume one"
        );
    }

    #[test]
    fn keyboard_velocity_curve_scales_the_transmitted_velocity() {
        let velocity_of = |curve| {
            let mut e = engine();
            e.extern_menu.velocity[0] = curve;
            e.extern_out.layers[1].enabled = false;
            e.scene.scene_1_mask = 1 << LayerId::SynthA as u8;
            e.handle_midi(MidiEvent::NoteOn {
                channel: 0,
                note: 60,
                velocity: 64,
            });
            drain(&mut e)[0].data2
        };
        assert_eq!(velocity_of(AuxVelocityCurve::Medium), 64);
        assert!(velocity_of(AuxVelocityCurve::Soft) > 64);
        assert!(velocity_of(AuxVelocityCurve::Hard) < 64);
    }

    #[test]
    fn echo_off_keeps_midi_in_notes_off_the_extern_layers_but_not_the_keyboard() {
        let mut e = engine();
        e.extern_out.layers[1].enabled = false;
        e.scene.scene_1_mask = 1 << LayerId::SynthA as u8;
        e.extern_menu.echo_global = false;
        e.handle_midi(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 90,
        });
        assert!(drain(&mut e).is_empty(), "MIDI In is not echoed");
        e.handle_keyboard_midi(MidiEvent::NoteOn {
            channel: 0,
            note: 61,
            velocity: 90,
        });
        assert!(
            drain(&mut e)
                .iter()
                .any(|m| m.status == 0x9C && m.data1 == 61),
            "the keybed still plays the Extern layer"
        );
    }
}
