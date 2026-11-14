//! Controls that share another control's rectangle.
//!
//! A SHIFT/HOLD function is printed under the control it hangs off and uses
//! the same hardware control, so it gets the same rectangle. A few direct
//! controls that the photo shows as one physical button (several LED
//! choices behind one button) are listed too. This table is the allow-list
//! for the "no overlap" test: two rectangles may overlap only when both
//! resolve to the same base id.
//!
//! ASSUMED: pairs read from the printed labels in the photo; the rows marked
//! "(direct)" are also in [`super::ESTIMATED`] because the photo does not
//! show which LED belongs to which function. A higher-resolution photo or
//! the panel drawing would confirm.

/// `(id, base id)`; the base id must not itself appear on the left.
pub const SHARED: &[(&str, &str)] = &[
    // Master / Rotary: ANGLE is Shift on the STOP MODE button, CLOSE MIC on ORGAN.
    ("rotary.stop_angle", "rotary.stop_mode"),
    ("rotary.angle", "rotary.stop_mode"),
    ("rotary.close_mic", "rotary.organ"),
    // Organ.
    ("organ.solo", "organ.on"),
    ("organ.kb_zone_down", "organ.octave_down"),
    ("organ.kb_zone_up", "organ.octave_up"),
    ("organ.sustped", "organ.layer.a"),
    ("organ.pstick", "organ.layer.b"),
    ("organ.sync", "organ.preset"),
    ("organ.perc_poly", "organ.perc_volume"),
    // Piano.
    ("piano.solo", "piano.on"),
    ("piano.kb_zone_down", "piano.octave_down"),
    ("piano.kb_zone_up", "piano.octave_up"),
    ("piano.sustped", "piano.layer.a"),
    ("piano.pstick", "piano.layer.b"),
    ("piano.list", "piano.select_dial"),
    ("piano.info", "piano.type"),
    ("piano.string_res", "piano.soft_release"), // (direct) one ACOUSTICS button
    ("piano.ped_noise", "piano.soft_release"),
    // Program area.
    ("program.list", "program.dial"),
    ("program.bank_down", "program.page_down"),
    ("program.bank_up", "program.page_up"),
    ("program.menu.system", "program.button.1"),
    ("program.menu.sound", "program.button.2"),
    ("program.menu.organize", "program.button.3"),
    ("program.menu.aux_kb", "program.button.4"),
    ("program.menu.output", "program.button.5"),
    ("program.menu.pedal", "program.button.6"),
    ("program.menu.midi", "program.button.7"),
    ("program.menu.extern", "program.button.8"),
    ("program.preset_name", "program.prog_view"),
    ("program.num_pad", "program.live_mode"),
    ("program.store_as", "program.store"),
    ("program.undo", "program.solo"),
    ("program.layer_init", "program.section_edit"),
    ("program.paste", "program.mon_copy"),
    ("morph.clear", "morph.at"),
    ("split.set_key", "split.on_set"),
    ("mst_clk.pedal_tap", "mst_clk.tap_set"),
    ("transp.panic", "transp.on_set"),
    ("preset_library.single_piano", "preset_library.piano"),
    ("preset_library.single_synth", "preset_library.synth"),
    // Synth.
    ("synth.solo", "synth.on"),
    ("synth.kb_zone_down", "synth.octave_down"),
    ("synth.kb_zone_up", "synth.octave_up"),
    ("synth.sustped", "synth.layer.a"),
    ("synth.pstick", "synth.layer.b"),
    ("synth.dial_info", "synth.dial.left"),
    ("synth.dial_list", "synth.dial.right"),
    ("synth.extern", "synth.mode"),
    ("synth.sound_init", "synth.waveform"),
    ("synth.osc.env_to_pitch", "synth.osc.pitch_smp"),
    ("synth.osc.velocity", "synth.osc.envelope"),
    ("synth.filter.velocity", "synth.filter.envelope"),
    ("synth.filter.group", "synth.filter.type"),
    ("synth.lfo.rate_mst_clk", "synth.lfo.rate"),
    ("synth.lfo.group", "synth.lfo.waveform"),
    ("synth.voice.priority", "synth.voice.mode"), // (direct) one MONO/LEGATO button
    ("synth.arp.pattern", "synth.arp.mode"),
    ("synth.arp.kb_sync", "synth.arp.run"),
    ("synth.arp.rate_mst_clk", "synth.arp.rate"),
    ("synth.arp.group", "synth.arp.menu"),
    ("synth.kb_hold_exclude", "synth.kb_hold"),
    ("synth.amp.velocity", "synth.amp.envelope"),
    // Layer Effects.
    ("fx.focus.group_piano", "fx.focus.piano"),
    ("fx.focus.group_synth", "fx.focus.synth"),
    ("mod1.variation", "mod1.selector"),
    ("mod1.mst_clk", "mod1.rate"),
    ("mod2.variation", "mod2.selector"),
    ("delay.global", "delay.on"),
    ("delay.mst_clk", "delay.tempo"),
    ("delay.effects_variation", "delay.effects"),
    ("delay.ping_pong", "delay.filter"),
    ("delay.analog", "delay.tap_set"),
    ("amp.variation", "amp.model"),
    ("amp.res", "amp.mid_freq"), // (direct) RES printed under the FREQ knob
    ("amp.filter", "amp.model"), // (direct) LED choice behind the model button
    ("amp.to_rotary", "amp.model"), // (direct) LED choice behind the model button
    ("comp.global", "comp.on"),
    ("comp.fast", "comp.amount"),
    ("reverb.global", "reverb.on"),
    ("reverb.variation", "reverb.type"),
];

/// Base id that `id` shares a rectangle with, if any.
pub fn shared_base(id: &str) -> Option<&'static str> {
    SHARED.iter().find(|(a, _)| *a == id).map(|(_, b)| *b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_hits_and_misses() {
        assert_eq!(shared_base("organ.sync"), Some("organ.preset"));
        assert_eq!(shared_base("organ.preset"), None);
    }

    #[test]
    fn bases_are_never_themselves_shared_and_ids_are_unique() {
        for (i, (a, b)) in SHARED.iter().enumerate() {
            assert!(shared_base(b).is_none(), "{b} is a base but also shared");
            assert!(SHARED[..i].iter().all(|(x, _)| x != a), "duplicate {a}");
        }
    }
}
