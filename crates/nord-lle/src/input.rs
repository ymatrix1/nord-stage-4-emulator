//! Neutral live events (`nord_core::lle_io`) to ARM0 input traffic.
//!
//! MIDI-class events become raw MIDI bytes on the ARM UART2 receive path
//! ([`ArmHost::midi`], the path `arm-lle-bench` drives). Pedals go in as their CC numbers (the
//! mapping of `lle_io::event::Pedal`, ASSUMED to be what the firmware reads). Panel events go to
//! [`ArmHost::panel`], which models nothing yet for the real ARM0: the firmware control id to key
//! matrix position table is not ported (stub, counted as `Unmodelled`).

use crate::arm::ArmHost;
use nord_core::lle_io::{LleEvent, Pedal};

/// What happened to one event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Applied {
    Midi,
    Panel,
    /// `Nop`, or a panel edit the ARM host does not model.
    Unmodelled,
}

/// CC number of a pedal (`midi::cc`: 64, 66, 67, 11).
pub fn pedal_cc(p: Pedal) -> u8 {
    match p {
        Pedal::Sustain => 64,
        Pedal::Sostenuto => 66,
        Pedal::Soft => 67,
        Pedal::Control => 11,
    }
}

/// Raw MIDI bytes of an event, `(bytes, length)`; `None` for panel events and `Nop`.
pub fn event_midi(e: &LleEvent) -> Option<([u8; 3], usize)> {
    let ch = |c: u8| c & 0x0F;
    Some(match *e {
        LleEvent::NoteOn {
            channel,
            note,
            velocity,
        } => ([0x90 | ch(channel), note, velocity], 3),
        LleEvent::NoteOff {
            channel,
            note,
            velocity,
        } => ([0x80 | ch(channel), note, velocity], 3),
        LleEvent::ControlChange {
            channel,
            controller,
            value,
        } => ([0xB0 | ch(channel), controller, value], 3),
        LleEvent::ChannelAftertouch { channel, pressure } => ([0xD0 | ch(channel), pressure, 0], 2),
        LleEvent::PitchBend { channel, value } => (
            [
                0xE0 | ch(channel),
                (value & 0x7F) as u8,
                ((value >> 7) & 0x7F) as u8,
            ],
            3,
        ),
        LleEvent::ProgramChange { channel, program } => ([0xC0 | ch(channel), program, 0], 2),
        LleEvent::Pedal {
            channel,
            pedal,
            value,
        } => ([0xB0 | ch(channel), pedal_cc(pedal), value], 3),
        LleEvent::Panel(_) | LleEvent::Nop => return None,
    })
}

/// Applies one event to `arm`.
pub fn apply(arm: &mut impl ArmHost, e: LleEvent) -> Applied {
    if let Some((b, n)) = event_midi(&e) {
        arm.midi(&b[..n]);
        return Applied::Midi;
    }
    match e {
        LleEvent::Panel(p) if arm.panel(p) => Applied::Panel,
        _ => Applied::Unmodelled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arm::FakeArm;
    use nord_core::lle_io::PanelEvent;

    #[test]
    fn midi_class_events_become_uart_bytes() {
        let mut a = FakeArm::default();
        let evs = [
            LleEvent::NoteOn {
                channel: 1,
                note: 60,
                velocity: 100,
            },
            LleEvent::NoteOff {
                channel: 1,
                note: 60,
                velocity: 0,
            },
            LleEvent::PitchBend {
                channel: 0,
                value: 8192,
            },
            LleEvent::ProgramChange {
                channel: 2,
                program: 9,
            },
            LleEvent::ChannelAftertouch {
                channel: 3,
                pressure: 5,
            },
            LleEvent::Pedal {
                channel: 0,
                pedal: Pedal::Sustain,
                value: 127,
            },
        ];
        for e in evs {
            assert_eq!(apply(&mut a, e), Applied::Midi);
        }
        assert_eq!(
            a.midi_seen,
            vec![
                vec![0x91, 60, 100],
                vec![0x81, 60, 0],
                vec![0xE0, 0, 64],
                vec![0xC2, 9],
                vec![0xD3, 5],
                vec![0xB0, 64, 127],
            ]
        );
    }

    #[test]
    fn panel_events_are_unmodelled_on_a_host_without_a_panel() {
        let mut a = FakeArm::default();
        let p = LleEvent::Panel(PanelEvent::Key {
            id: 3,
            pressed: true,
        });
        assert_eq!(apply(&mut a, p), Applied::Unmodelled);
        assert_eq!(apply(&mut a, LleEvent::Nop), Applied::Unmodelled);
        assert!(a.midi_seen.is_empty());
        assert_eq!(pedal_cc(Pedal::Control), 11);
    }
}
