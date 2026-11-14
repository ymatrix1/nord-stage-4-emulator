//! MIDI menu settings the engine honours (manual OS v1.6x p.56-57 and p.61-63, MIDI menu pages 1,
//! 3-5 and 7-9): Local Control, Sect On, Type / Ctrl / Dev, PC / PC Ext and Transpose MIDI At.
//! Spec: `docs/specs/midi-and-control.md` section 2a.
//!
//! All settings are global (not per program) and apply to the Global channel. The defaults keep the
//! engine exactly as it was before the menu was modelled (`tests_midix::defaults_are_inert`).
//!
//! Provenance: the entries, choices, and the groups of controllers behind Ctrl and Dev (Dev =
//! Pitch Bend, Mod Wheel, Aftertouch, Sustain, Control, Swell, Volume) are `VERIFIED` against the
//! manual (p.62). `ctrl: Send & Receive` as the engine default is a deliberate deviation from the
//! hardware default `Off` (`ASSUMED`): the engine has always accepted panel CCs on the Global
//! channel, and the menu model carries the same default so a reset does not break that. The
//! Triple Pedal CCs 66 / 67 are filed under Dev (`ASSUMED`, they are pedals). NRPN decoding stores
//! the last parameter / value pair but has no parameter map yet (`docs/OPEN-QUESTIONS.md`).

use super::*;

/// NRPN controller numbers (MIDI 1.0): parameter MSB / LSB and data entry MSB / LSB.
const NRPN_MSB: u8 = 99;
const NRPN_LSB: u8 = 98;
const DATA_MSB: u8 = 6;
const DATA_LSB: u8 = 38;
pub(super) const NO_NOTE: u8 = 0xFF;

/// Off / Send / Receive / Send & Receive (menu order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MidiMode {
    Off,
    Send,
    Receive,
    #[default]
    SendReceive,
}

impl MidiMode {
    /// The mode at menu index `i` (clamped).
    pub fn from_index(i: usize) -> Self {
        match i {
            0 => Self::Off,
            1 => Self::Send,
            2 => Self::Receive,
            _ => Self::SendReceive,
        }
    }
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn receives(self) -> bool {
        matches!(self, Self::Receive | Self::SendReceive)
    }
    pub fn sends(self) -> bool {
        matches!(self, Self::Send | Self::SendReceive)
    }
}

/// Where a transpose applies (MIDI menu page 9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransposeAt {
    /// Transpose acts on incoming MIDI and the keyboard; the MIDI out stays untransposed.
    #[default]
    In,
    /// Transpose acts on transmitted notes; incoming MIDI plays untransposed.
    Out,
}

/// Global MIDI menu settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MidiMenuSettings {
    /// Local Control (page 1): `false` disconnects the keyboard from the sound engines, notes
    /// played through [`Stage4Engine::handle_keyboard_midi`] are only transmitted. Reverts to
    /// `true` at every start (manual p.57), so hosts must not persist it.
    pub local_control: bool,
    /// Sect On of Organ, Piano, Synth (pages 3-5): Layer A's channel addresses every layer of
    /// the section.
    pub sect_on: [bool; 3],
    /// Type (page 7): `true` = CC & NRPN, `false` = CC only.
    pub nrpn: bool,
    /// Ctrl (page 7): control changes other than the Dev group, on the Global channel.
    pub ctrl: MidiMode,
    /// Dev (page 7): Pitch Bend, Mod Wheel, Aftertouch, Sustain, Control / Swell / Volume pedals.
    pub dev: MidiMode,
    /// PC (page 8): Program Change of the Program and Live banks.
    pub pc: MidiMode,
    /// PC Ext (page 8): Program Change and Bank Select of the Preset Library banks.
    pub pc_ext: MidiMode,
    /// Transpose MIDI At (page 9).
    pub transpose_at: TransposeAt,
}

impl Default for MidiMenuSettings {
    fn default() -> Self {
        Self {
            local_control: true,
            sect_on: [false; 3],
            nrpn: true,
            ctrl: MidiMode::SendReceive,
            dev: MidiMode::SendReceive,
            pc: MidiMode::SendReceive,
            pc_ext: MidiMode::SendReceive,
            transpose_at: TransposeAt::In,
        }
    }
}

/// A received Program Change for the host to act on (see [`Stage4Engine::take_program_change`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramRequest {
    /// Bank MSB: 0 Program, 1 Live, 2 Synth, 3 Piano, 4 Organ presets (manual p.56).
    pub bank_msb: u8,
    pub bank_lsb: u8,
    pub program: u8,
}

/// Runtime state behind the menu: bank select, NRPN registers, queued Program Change.
#[derive(Debug, Clone)]
pub(super) struct MidiRuntime {
    bank_msb: u8,
    bank_lsb: u8,
    pending_program: Option<ProgramRequest>,
    nrpn_param: [u8; 2],
    nrpn_data_msb: u8,
    last_nrpn: Option<(u16, u16)>,
    /// A keyboard event is being dispatched (see `handle_keyboard_midi`).
    pub(super) keyboard: bool,
    /// Note transmitted for each keyboard key still down.
    pub(super) sent_note: [u8; 128],
}

impl Default for MidiRuntime {
    fn default() -> Self {
        Self {
            bank_msb: 0,
            bank_lsb: 0,
            pending_program: None,
            nrpn_param: [0; 2],
            nrpn_data_msb: 0,
            last_nrpn: None,
            keyboard: false,
            sent_note: [NO_NOTE; 128],
        }
    }
}

/// Controllers of the Dev group (manual p.62); CC 66 / 67 are the Triple Pedal (`ASSUMED`).
pub(super) fn is_device_cc(controller: u8) -> bool {
    matches!(
        controller,
        cc::MOD_WHEEL
            | cc::ORGAN_SWELL
            | cc::CONTROL_PEDAL
            | cc::SUSTAIN_PEDAL
            | cc::SOSTENUTO_PEDAL
            | cc::SOFT_PEDAL
    )
}

impl Stage4Engine {
    /// Filters a Global-channel control change by Ctrl / Dev / PC Ext / Type. `true` = process.
    /// Bank Select and the NRPN registers are consumed here (the engine never acted on them).
    pub(super) fn accept_global_cc(&mut self, controller: u8, value: u8) -> bool {
        let m = self.midi_menu;
        let rt = &mut self.midi_rt;
        match controller {
            cc::BANK_SELECT_MSB | cc::BANK_SELECT_LSB => {
                if m.pc.receives() || m.pc_ext.receives() {
                    if controller == cc::BANK_SELECT_MSB {
                        rt.bank_msb = value;
                    } else {
                        rt.bank_lsb = value;
                    }
                }
                false
            }
            NRPN_MSB | NRPN_LSB | DATA_MSB | DATA_LSB => {
                if m.nrpn && m.ctrl.receives() {
                    match controller {
                        NRPN_MSB => rt.nrpn_param[0] = value,
                        NRPN_LSB => rt.nrpn_param[1] = value,
                        DATA_MSB => {
                            rt.nrpn_data_msb = value;
                            rt.last_nrpn = Some((nrpn_param(rt), u16::from(value) << 7));
                        }
                        _ => {
                            let data = u16::from(rt.nrpn_data_msb) << 7 | u16::from(value);
                            rt.last_nrpn = Some((nrpn_param(rt), data));
                        }
                    }
                }
                false
            }
            c if is_device_cc(c) => m.dev.receives(),
            _ => m.ctrl.receives(),
        }
    }

    /// The last complete NRPN (`parameter`, `14-bit value`) received, if any (cleared by reading).
    pub fn take_nrpn(&mut self) -> Option<(u16, u16)> {
        self.midi_rt.last_nrpn.take()
    }

    /// Queues an incoming Program Change on the Global channel according to PC / PC Ext. The bank
    /// is the last Bank Select received (MSB 0 / LSB 0 before any), as on the hardware where a
    /// Program Change without Bank Select acts in the current bank (manual p.56).
    pub(super) fn receive_program_change(&mut self, program: u8) {
        let rt = &mut self.midi_rt;
        let mode = if (2..=4).contains(&rt.bank_msb) {
            self.midi_menu.pc_ext
        } else {
            self.midi_menu.pc
        };
        if mode.receives() {
            rt.pending_program = Some(ProgramRequest {
                bank_msb: rt.bank_msb,
                bank_lsb: rt.bank_lsb,
                program: program & 0x7F,
            });
        }
    }

    /// The Program Change received since the last call. The host owns the program list: it polls
    /// this once per block and loads the program. Allocation free.
    pub fn take_program_change(&mut self) -> Option<ProgramRequest> {
        self.midi_rt.pending_program.take()
    }

    /// Transpose applied to a note arriving on `channel`: the keyboard and the Aux KB channel are
    /// always transposed (manual p.55), other MIDI only with Transpose MIDI At = In.
    pub(super) fn incoming_transpose(&self, channel: u8) -> i8 {
        if self.midi_rt.keyboard
            || self.midi_menu.transpose_at == TransposeAt::In
            || self.aux_kb.listens_on(channel)
        {
            self.effective_transpose()
        } else {
            0
        }
    }

    /// Direct channel of `layer`, honouring Sect On (Layer A's channel for the whole section).
    pub(super) fn layer_channel(&self, layer: LayerId) -> Option<u8> {
        let i = layer as usize;
        let (section, first) = match i {
            0 | 1 => (0, 0),
            2 | 3 => (1, 2),
            _ => (2, 4),
        };
        let i = if self.midi_menu.sect_on[section] {
            first
        } else {
            i
        };
        self.layer_midi_channels[i]
    }

    /// Echo Global To Extern Ch: whether a Global-channel MIDI-In note must skip Extern `layer`.
    pub(super) fn echo_blocks(&self, layer: LayerId, route: NoteRoute) -> bool {
        route == NoteRoute::Global
            && !self.midi_rt.keyboard
            && !self.extern_menu.echo_global
            && self.extern_index(layer).is_some()
    }
}

fn nrpn_param(rt: &MidiRuntime) -> u16 {
    u16::from(rt.nrpn_param[0]) << 7 | u16::from(rt.nrpn_param[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_map_to_receive_and_send_halves() {
        let table = [
            (MidiMode::Off, false, false),
            (MidiMode::Send, false, true),
            (MidiMode::Receive, true, false),
            (MidiMode::SendReceive, true, true),
        ];
        for (i, (m, rx, tx)) in table.into_iter().enumerate() {
            assert_eq!(MidiMode::from_index(i), m);
            assert_eq!((m.index(), m.receives(), m.sends()), (i, rx, tx));
        }
        assert_eq!(MidiMode::from_index(9), MidiMode::SendReceive);
    }

    #[test]
    fn defaults_follow_the_manual_except_ctrl() {
        let d = MidiMenuSettings::default();
        assert!(d.local_control && d.nrpn);
        assert_eq!(d.sect_on, [false; 3]);
        assert_eq!(
            (d.dev, d.pc, d.pc_ext),
            (
                MidiMode::SendReceive,
                MidiMode::SendReceive,
                MidiMode::SendReceive
            )
        );
        assert_eq!(d.transpose_at, TransposeAt::In);
    }

    #[test]
    fn device_group_matches_the_manual_list() {
        for c in [1u8, 4, 11, 64, 66, 67] {
            assert!(is_device_cc(c), "{c}");
        }
        for c in [7u8, 16, 60, 82, 85, 115] {
            assert!(!is_device_cc(c), "{c}");
        }
    }

    #[test]
    fn layer_channel_follows_layer_a_with_sect_on() {
        let mut e = Stage4Engine::new(48_000.0);
        e.layer_midi_channels[4] = Some(3);
        e.layer_midi_channels[5] = Some(9);
        assert_eq!(e.layer_channel(LayerId::SynthB), Some(9));
        e.midi_menu.sect_on[2] = true;
        assert_eq!(e.layer_channel(LayerId::SynthB), Some(3));
        assert_eq!(e.layer_channel(LayerId::SynthC), Some(3));
        assert_eq!(
            e.layer_channel(LayerId::PianoB),
            None,
            "other sections keep theirs"
        );
    }
}
