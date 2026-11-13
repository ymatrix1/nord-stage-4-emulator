//! Synth section, part 1 (manual p.27-31, p.46): shared strip, OLED and dials,
//! mode, oscillators and filter. Arpeggiator, voice, LFO and envelopes are in
//! `synth_perf.rs`.

use super::common::{layer_block, BlockSpec};
use crate::types::{ControlEntry, ControlKind as K, Layer, Section};

pub fn controls() -> Vec<ControlEntry> {
    let s = Section::Synth;
    let e = ControlEntry::new;
    let bean = "ns4-4w9l";
    let mut v = layer_block(&BlockSpec {
        section: s,
        name: "synth",
        layers: &[Layer::A, Layer::B, Layer::C],
        page: 27,
        ped_page: 28,
        pstick_label: "PSTICK/RNG",
        octave_id: None,
        bean,
    });
    v.push(e("synth.oled", s, K::Oled, "SYNTH DISPLAY", 27).ui("drawn by the OLED renderer (B8d)"));
    for (tail, label) in [
        ("left", "LEFT DIAL"),
        ("middle", "MIDDLE DIAL"),
        ("right", "RIGHT DIAL"),
    ] {
        v.push(e(&format!("synth.dial.{tail}"), s, K::Encoder, label, 28).todo(bean));
    }
    v.push(
        e(
            "synth.dial_info",
            s,
            K::Encoder,
            "INFO (Shift+left dial)",
            29,
        )
        .shift()
        .ui("sample info screen"),
    );
    v.push(
        e("synth.dial_list", s, K::Encoder, "LIST (Shift+dial)", 29)
            .shift()
            .ui("list view of waveforms or samples"),
    );
    v.push(e("synth.mode", s, K::Button, "MODE (Samples/Analog)", 28).bound("synth_osc_mode"));
    v.push(
        e("synth.extern", s, K::Button, "EXTERN (Shift+Mode)", 46)
            .shift()
            .todo("ns4-ymci"),
    );
    v.push(e("synth.waveform", s, K::Button, "WAVEFORM", 29).todo(bean));
    v.push(
        e(
            "synth.sound_init",
            s,
            K::Button,
            "SOUND INIT (Shift+Waveform)",
            37,
        )
        .shift()
        .todo(bean),
    );
    // Oscillators
    v.push(
        e("synth.osc.ctrl", s, K::Knob, "OSC CTRL", 28)
            .morph()
            .todo(bean),
    );
    v.push(e("synth.osc.pitch_smp", s, K::Button, "PITCH/SMP", 28).todo(bean));
    v.push(
        e(
            "synth.osc.env_to_pitch",
            s,
            K::Button,
            "ENV TO PITCH (Shift+Pitch/Smp)",
            33,
        )
        .shift()
        .bound("synthx_osc_env_pitch_{n}"),
    );
    v.push(e("synth.osc.envelope", s, K::Button, "OSC ENVELOPE", 33).todo(bean));
    v.push(
        e("synth.osc.env_amt", s, K::Knob, "OSC ENV AMT", 33)
            .morph()
            .bound("synthx_osc_env_{n}"),
    );
    v.push(
        e(
            "synth.osc.velocity",
            s,
            K::Button,
            "OSC VELOCITY (Shift+Envelope)",
            33,
        )
        .shift()
        .todo(bean),
    );
    // Filter
    v.push(e("synth.filter.type", s, K::Button, "FILTER TYPE", 31).todo(bean));
    v.push(
        e("synth.filter.freq", s, K::Knob, "FILTER FREQ", 32)
            .morph()
            .bound("synth_filter_cutoff"),
    );
    v.push(
        e("synth.filter.res", s, K::Knob, "FILTER RES", 32)
            .morph()
            .bound("synth_filter_resonance"),
    );
    v.push(
        e("synth.filter.env_amt", s, K::Knob, "FILTER ENV AMT", 32)
            .morph()
            .bound("synthx_filter_env_{n}"),
    );
    v.push(e("synth.filter.envelope", s, K::Button, "FILTER ENVELOPE", 33).todo(bean));
    v.push(
        e(
            "synth.filter.velocity",
            s,
            K::Button,
            "FILTER VELOCITY (Shift+Envelope)",
            33,
        )
        .shift()
        .todo(bean),
    );
    v.push(
        e(
            "synth.filter.group",
            s,
            K::Button,
            "FILTER GROUP (Shift+Type)",
            32,
        )
        .shift()
        .todo(bean),
    );
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synth_part1_total() {
        assert_eq!(controls().len(), 48);
    }

    #[test]
    fn synth_has_three_faders_and_three_dials() {
        let c = controls();
        assert_eq!(c.iter().filter(|x| x.kind == K::Fader).count(), 3);
        assert_eq!(
            c.iter().filter(|x| x.id.starts_with("synth.dial.")).count(),
            3
        );
    }

    #[test]
    fn filter_freq_and_res_are_bound() {
        let c = controls();
        assert!(c.iter().any(|x| x.id == "synth.filter.freq" && x.morphable));
    }
}
