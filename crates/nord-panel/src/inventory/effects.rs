//! Layer Effects section (manual p.14, p.48-53): FX Focus, Mod 1, Mod 2,
//! Delay, Amp Sim/EQ, Compressor, Reverb.
//!
//! Host ids are the global (not per-layer) ones; per-layer `fx_*_{n}` ids are
//! tracked by `ns4-e3zs`.

use crate::types::{ControlEntry, ControlKind as K, Section};

pub fn controls() -> Vec<ControlEntry> {
    let s = Section::Effects;
    let e = ControlEntry::new;
    let bean = "ns4-e3zs";
    vec![
        // FX Focus and master switches (p.48)
        e("fx.layer_effects_on", s, K::Button, "LAYER EFFECTS ON", 48).todo(bean),
        e("fx.focus.organ", s, K::Button, "FX FOCUS ORGAN", 48).todo(bean),
        e("fx.focus.piano", s, K::Button, "FX FOCUS PIANO", 48).todo(bean),
        e("fx.focus.synth", s, K::Button, "FX FOCUS SYNTH", 48).todo(bean),
        e(
            "fx.focus.group_piano",
            s,
            K::Button,
            "GROUP PIANO (Shift+Piano)",
            48,
        )
        .shift()
        .bound("fxy_group_{n}"),
        e(
            "fx.focus.group_synth",
            s,
            K::Button,
            "GROUP SYNTH (Shift+Synth)",
            48,
        )
        .shift()
        .bound("fxy_group_{n}"),
        e("fx.focus.leds", s, K::Led, "FX FOCUS LEDs", 48).ui("show the focused section or layer"),
        // Mod 1 (p.49-50)
        e("mod1.on", s, K::Button, "MOD 1 ON", 49).todo(bean),
        e("mod1.selector", s, K::Button, "MOD 1 TYPE (6)", 49).bound("mod1_type"),
        e(
            "mod1.variation",
            s,
            K::Button,
            "VARIATION / PED (Shift+Selector)",
            49,
        )
        .shift()
        .bound("fxy_mod1_var_{n}"),
        e("mod1.rate", s, K::Knob, "MOD 1 RATE", 49)
            .morph()
            .bound("mod1_rate"),
        e(
            "mod1.mst_clk",
            s,
            K::Knob,
            "MOD 1 MST CLK (Shift+turn Rate)",
            50,
        )
        .shift()
        .bound("fxz_mod1_beats_{n}"),
        e("mod1.amount", s, K::Knob, "MOD 1 AMOUNT", 49)
            .morph()
            .bound("fxx_mod1_amount_{n}"),
        // Mod 2 (p.50)
        e("mod2.on", s, K::Button, "MOD 2 ON", 50).todo(bean),
        e("mod2.selector", s, K::Button, "MOD 2 TYPE (6)", 50).bound("mod2_type"),
        e(
            "mod2.variation",
            s,
            K::Button,
            "VARIATION (Shift+Selector)",
            50,
        )
        .shift()
        .bound("fxy_mod2_var_{n}"),
        e("mod2.rate", s, K::Knob, "MOD 2 RATE", 50).bound("mod2_rate"),
        e("mod2.amount", s, K::Knob, "MOD 2 AMOUNT", 50)
            .morph()
            .bound("fxx_mod2_amount_{n}"),
        // Delay (p.50-51)
        e("delay.on", s, K::Button, "DELAY ON", 50).bound("delay_on"),
        e("delay.global", s, K::Button, "GLOBAL (Shift+On)", 48)
            .shift()
            .bound("fxy_glob_delay_{n}"),
        e("delay.tempo", s, K::Knob, "DELAY TEMPO", 50)
            .morph()
            .bound("delay_ms"),
        e("delay.mst_clk", s, K::Knob, "MST CLK (Shift+Tempo)", 51)
            .shift()
            .bound("delay_sync"),
        e("delay.feedback", s, K::Knob, "DELAY FEEDBACK", 50)
            .morph()
            .bound("delay_fb"),
        e("delay.dry_wet", s, K::Knob, "DELAY DRY/WET", 50)
            .morph()
            .bound("delay_mix"),
        e("delay.effects", s, K::Button, "DELAY EFFECTS (6)", 50).bound("delay_fx"),
        e(
            "delay.effects_variation",
            s,
            K::Button,
            "DELAY EFFECT VARIATION",
            51,
        )
        .shift()
        .bound("fxx_delay_fx_variation_{n}"),
        e("delay.filter", s, K::Button, "DELAY FILTER LP/HP/BP", 51).bound("delay_flt"),
        e(
            "delay.ping_pong",
            s,
            K::Button,
            "PING PONG (Shift+Filter)",
            50,
        )
        .shift()
        .bound("delay_pp"),
        e("delay.tap_set", s, K::Button, "TAP/SET", 50).todo(bean),
        e("delay.analog", s, K::Button, "ANALOG (Shift+Tap/Set)", 51)
            .shift()
            .bound("fxx_delay_analog_{n}"),
        // Amp Sim / EQ (p.51-52)
        e("amp.on", s, K::Button, "AMP SIM/EQ ON", 51).todo(bean),
        e("amp.drive", s, K::Knob, "DRIVE", 51)
            .morph()
            .bound("drive"),
        e("amp.model", s, K::Button, "AMP MODEL (Twin/JC/Small)", 51).bound("cabinet"),
        e(
            "amp.variation",
            s,
            K::Button,
            "AMP VARIATION (Shift+Selector)",
            51,
        )
        .shift()
        .bound("fxx_amp_variation_{n}"),
        e("amp.bass", s, K::Knob, "EQ BASS", 51).bound("eq_bass"),
        e("amp.mid", s, K::Knob, "EQ MID", 51).bound("eq_mid"),
        e("amp.mid_freq", s, K::Knob, "EQ MID FREQ", 51)
            .morph()
            .bound("fxx_eq_mid_freq_{n}"),
        e("amp.treble", s, K::Knob, "EQ TREBLE", 51).bound("eq_treble"),
        e("amp.res", s, K::Knob, "FILTER RES", 52)
            .assumed("RES knob named on p.52; its panel position is not described")
            .bound("fxx_filter_res_{n}"),
        e("amp.filter", s, K::Button, "FILTER LP24/HP24", 52).todo(bean),
        e("amp.to_rotary", s, K::Button, "TO ROTARY", 52).bound("fxsel_cabinet_{n}"),
        // Compressor (p.52)
        e("comp.on", s, K::Button, "COMP ON", 52).bound("comp_on"),
        e("comp.global", s, K::Button, "GLOBAL (Shift+On)", 48)
            .shift()
            .bound("fxy_glob_comp_{n}"),
        e("comp.amount", s, K::Knob, "COMP AMOUNT", 52)
            .morph()
            .bound("comp_amt"),
        e("comp.fast", s, K::Button, "FAST (Shift+Amount)", 52)
            .shift()
            .bound("comp_fast"),
        // Reverb (p.52-53)
        e("reverb.on", s, K::Button, "REVERB ON", 52).todo(bean),
        e("reverb.global", s, K::Button, "GLOBAL (Shift+On)", 52)
            .shift()
            .bound("fxy_glob_rev_{n}"),
        e("reverb.type", s, K::Button, "REVERB TYPE (6)", 52).bound("rev_model"),
        e(
            "reverb.variation",
            s,
            K::Button,
            "VARIATION / CHORALE (Shift+Selector)",
            52,
        )
        .shift()
        .bound("fxz_rev_var_{n}"),
        e("reverb.dry_wet", s, K::Knob, "REVERB DRY/WET", 52)
            .morph()
            .bound("rev_mix"),
        e("reverb.bright_dark", s, K::Button, "BRIGHT / DARK", 53)
            .assumed("mode named on p.53; the control that selects it is not described")
            .bound("rev_color"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_total() {
        assert_eq!(controls().len(), 51);
    }

    #[test]
    fn mod_units_have_six_types_documented() {
        let c = controls();
        assert!(c
            .iter()
            .any(|x| x.id == "mod1.selector" && x.label.contains("(6)")));
        assert!(c
            .iter()
            .any(|x| x.id == "mod2.selector" && x.label.contains("(6)")));
    }

    #[test]
    fn three_units_have_a_global_switch() {
        let c = controls();
        let g = c.iter().filter(|x| x.id.ends_with(".global")).count();
        assert_eq!(g, 3);
    }
}
