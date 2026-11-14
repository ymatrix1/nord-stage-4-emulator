//! Buttons and pots of the Synth section outside the layer strip: Oscillators, Filter, Amp, LFO,
//! Voice, Vibrato, Arpeggiator/Gate, KB Hold (manual p.28-37).

use super::widgets::{pad, pot, tiny, LEGEND};
use crate::panel::layout_px::rect_or_empty;
use crate::panel::prelude::*;
use crate::panel::ui_state::{Btn, Knob, SynthUi};
use egui::Pos2;

/// Pad buttons: (button, legend printed above it).
const PADS: [(Btn, &str); 17] = [
    (Btn::Mode, "MODE"),
    (Btn::Waveform, "WAVEFORM"),
    (Btn::PitchSmp, "PITCH/SMP"),
    (Btn::OscEnvelope, "ENVELOPE"),
    (Btn::FilterType, "TYPE"),
    (Btn::FilterEnvelope, "ENVELOPE"),
    (Btn::AmpEnvelope, "ENVELOPE"),
    (Btn::LfoWaveform, "WAVEFORM"),
    (Btn::LfoDest, "DEST"),
    (Btn::VoiceMode, "VOICE"),
    (Btn::Unison, "UNISON"),
    (Btn::VibratoSel, "SELECT"),
    (Btn::VibratoMenu, "MENU"),
    (Btn::ArpRun, "ARP RUN"),
    (Btn::ArpMode, "ARP/POLY/GATE"),
    (Btn::ArpMenu, "MENU"),
    (Btn::KbHold, "KB HOLD"),
];

/// Pots: (knob, legend).
const POTS: [(Knob, &str); 10] = [
    (Knob::OscCtrl, "OSC CTRL"),
    (Knob::OscEnvAmt, "ENV AMT"),
    (Knob::FilterFreq, "FREQ"),
    (Knob::FilterRes, "RES"),
    (Knob::FilterEnvAmt, "ENV AMT"),
    (Knob::LfoRate, "RATE"),
    (Knob::LfoModAmt, "MOD AMT"),
    (Knob::Glide, "GLIDE"),
    (Knob::ArpRate, "RATE"),
    (Knob::ArpRange, "RANGE"),
];

/// Area headings printed on the panel: (heading, id of a control whose centre sets the x, id whose top sets the y).
const HEADINGS: [(&str, &str, &str); 6] = [
    ("OSCILLATORS", "synth.osc.pitch_smp", "synth.osc.pitch_smp"),
    ("FILTER", "synth.filter.type", "synth.filter.type"),
    ("AMP", "synth.amp.envelope", "synth.amp.envelope"),
    ("LFO", "synth.lfo.waveform", "synth.lfo.waveform"),
    (
        "VIBRATO",
        "synth.vibrato.selector",
        "synth.vibrato.selector",
    ),
    ("ARPEGGIATOR / GATE", "synth.arp.mode", "synth.arp.rate"),
];

/// Paint and drive every pad and pot of the section.
pub(super) fn paint_controls(ui: &mut Ui, p: &Painter, o: Pos2, sui: &mut SynthUi) {
    for (x_id, y_id, head) in HEADINGS.map(|(h, x, y)| (x, y, h)) {
        let x = rect_or_empty(x_id, o).center().x;
        let y = rect_or_empty(y_id, o).top() - 12.0;
        tiny(p, pos2(x, y), head, LEGEND, Align2::CENTER_BOTTOM);
    }
    for (b, label) in PADS {
        pad(ui, p, o, sui, b, label);
    }
    for (k, label) in POTS {
        pot(ui, p, o, sui, k, label);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::support::shapes_painted;

    #[test]
    fn every_button_and_knob_is_placed_exactly_once() {
        let mut ids: Vec<&str> = PADS.iter().map(|(b, _)| b.id()).collect();
        ids.extend(POTS.iter().map(|(k, _)| k.id()));
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), n);
        // The layer-strip pads (ON, OCTAVE) are drawn by strip.rs.
        assert_eq!(PADS.len() + 3, Btn::ALL.len());
    }

    #[test]
    fn controls_paint_pads_pots_and_their_shift_legends() {
        let mut sui = SynthUi::new();
        let n = shapes_painted(|ui, p, rect, _s, _a| {
            sui.begin_frame();
            paint_controls(ui, p, rect.min, &mut sui);
        });
        assert!(n > 100, "painted {n} shapes");
        for id in [
            "synth.extern",
            "synth.sound_init",
            "synth.filter.group",
            "synth.lfo.rate_mst_clk",
            "synth.arp.kb_sync",
            "synth.arp.pattern",
            "synth.kb_hold_exclude",
            "synth.voice.priority",
            "synth.osc.env_to_pitch",
        ] {
            assert!(sui.drawn().contains(&id), "{id}");
        }
    }
}
