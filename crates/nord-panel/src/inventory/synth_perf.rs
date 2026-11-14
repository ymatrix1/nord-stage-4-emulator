//! Synth section, part 2 (manual p.33-37): LFO, voice, unison, vibrato,
//! arpeggiator/gate, KB Hold and the amp envelope.

use crate::types::{ControlEntry, ControlKind as K, Section};

pub fn controls() -> Vec<ControlEntry> {
    let s = Section::Synth;
    let e = ControlEntry::new;
    let bean = "ns4-4w9l";
    vec![
        // LFO
        e("synth.lfo.waveform", s, K::Button, "LFO WAVEFORM", 34).bound("synthm_lfo_shape_{n}"),
        e("synth.lfo.dest", s, K::Button, "LFO DESTINATION", 34).bound("synthm_lfo_dest_{n}"),
        e("synth.lfo.rate", s, K::Knob, "LFO RATE/TIME", 34)
            .morph()
            .bound("synthm_lfo_rate_{n}"),
        e(
            "synth.lfo.rate_mst_clk",
            s,
            K::Knob,
            "LFO MST CLK (Shift+turn)",
            34,
        )
        .shift()
        .bound("synthg_lfo_sync_{n}"),
        e("synth.lfo.mod_amt", s, K::Knob, "LFO MOD AMT", 34)
            .morph()
            .bound("synthm_lfo_amount_{n}"),
        e(
            "synth.lfo.group",
            s,
            K::Button,
            "LFO GROUP (Shift+Waveform)",
            35,
        )
        .shift()
        .todo(bean),
        // Voice
        e("synth.voice.mode", s, K::Button, "VOICE MONO/LEGATO", 34).bound("synthx_voice_{n}"),
        e("synth.voice.priority", s, K::Button, "LO / HI PRIORITY", 34)
            .assumed("manual names Lo/Hi modes but not the button that selects them")
            .bound("synthx_priority_{n}"),
        e("synth.voice.glide", s, K::Knob, "GLIDE", 35).bound("synthx_glide_{n}"),
        e("synth.unison", s, K::Button, "UNISON 1/2/3", 35).bound("synthx_unison_{n}"),
        // Vibrato
        e(
            "synth.vibrato.selector",
            s,
            K::Button,
            "VIBRATO ON/DLY/WHL/A.T./PED",
            37,
        )
        .bound("synthm_vib_mode_{n}"),
        e("synth.vibrato.menu", s, K::Button, "VIBRATO MENU", 37)
            .assumed("menu entry point is not described in the manual text")
            .todo(bean),
        // Arpeggiator / Gate
        e("synth.arp.run", s, K::Button, "ARP RUN", 35).todo(bean),
        e("synth.arp.mode", s, K::Button, "ARP / POLY / GATE", 35).bound("synthm_arp_mode_{n}"),
        e(
            "synth.arp.pattern",
            s,
            K::Button,
            "PATTERN (Shift+mode)",
            35,
        )
        .shift()
        .bound("synthg_pat_on_{n}"),
        e(
            "synth.arp.kb_sync",
            s,
            K::Button,
            "KB SYNC (Shift+Arp Run)",
            37,
        )
        .shift()
        .bound("synthg_kb_sync_{n}"),
        e("synth.arp.rate", s, K::Knob, "ARP RATE/TIME", 36)
            .morph()
            .bound("synthm_arp_rate_{n}"),
        e(
            "synth.arp.rate_mst_clk",
            s,
            K::Knob,
            "ARP MST CLK (Shift+turn)",
            36,
        )
        .shift()
        .bound("synthg_arp_sync_{n}"),
        e("synth.arp.range", s, K::Knob, "ARP RANGE / GATE ENV", 36).bound("synthm_arp_range_{n}"),
        e("synth.arp.menu", s, K::Button, "ARP MENU", 36).todo(bean),
        e(
            "synth.arp.group",
            s,
            K::Button,
            "ARP GROUP (Shift+Menu)",
            36,
        )
        .shift()
        .todo(bean),
        // KB Hold
        e("synth.kb_hold", s, K::Button, "KB HOLD", 36).bound("synthg_kb_hold"),
        e(
            "synth.kb_hold_exclude",
            s,
            K::Button,
            "EXCLUDE (Shift+KB Hold)",
            36,
        )
        .shift()
        .bound("synthg_hold_excl_{n}"),
        // Amp envelope
        e("synth.amp.envelope", s, K::Button, "AMP ENVELOPE", 33).todo(bean),
        e(
            "synth.amp.velocity",
            s,
            K::Button,
            "AMP VELOCITY (Shift+Envelope)",
            33,
        )
        .shift()
        .todo(bean),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synth_part2_total() {
        assert_eq!(controls().len(), 25);
    }

    #[test]
    fn rate_knobs_are_morphable() {
        let c = controls();
        for id in ["synth.lfo.rate", "synth.arp.rate", "synth.lfo.mod_amt"] {
            assert!(c.iter().find(|x| x.id == id).unwrap().morphable, "{id}");
        }
    }
}
