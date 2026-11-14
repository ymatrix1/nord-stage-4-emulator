//! Synth edits to host parameter writes. `synthx_*`, `synthm_*` and `synthg_*` banks count the
//! three synth layers 1..3; the cutoff, resonance, drive and envelope are global host params.

use super::map::{flag, osc_mode_index};
use super::map_pattern::map_pattern;
use super::write::ParamWrite as W;
use nord_core::synth_types::{ArpMode, LfoDestination, LfoShape, NotePriority, VibratoMode};
use nord_gui::audio::{SynthEdit, SynthParam, SynthTarget};

const SYNTH_FIRST: u8 = 1;

// Host indices of the synth selectors (`synth_mod_params`).
fn lfo_shape_index(s: LfoShape) -> f32 {
    match s {
        LfoShape::Triangle => 0.0,
        LfoShape::Saw => 1.0,
        LfoShape::Square => 2.0,
        LfoShape::SampleAndHold => 3.0,
        LfoShape::SawDown => 4.0,
    }
}

fn lfo_dest_index(d: LfoDestination) -> f32 {
    match d {
        LfoDestination::FilterCutoff => 0.0,
        LfoDestination::OscPitch => 1.0,
        LfoDestination::OscCtrl => 2.0,
        LfoDestination::Off => 3.0,
    }
}

/// Host arp mode index; `Gate` has no host value (the host decoder never produces it).
fn arp_mode_index(m: ArpMode) -> Option<f32> {
    match m {
        ArpMode::Off => Some(0.0),
        ArpMode::Up => Some(1.0),
        ArpMode::Down => Some(2.0),
        ArpMode::UpDown => Some(3.0),
        ArpMode::Random => Some(4.0),
        ArpMode::Poly => Some(5.0),
        _ => None,
    }
}

fn vibrato_mode_index(m: VibratoMode) -> f32 {
    match m {
        VibratoMode::Off => 0.0,
        VibratoMode::On => 1.0,
        VibratoMode::Delay => 2.0,
        VibratoMode::Wheel => 3.0,
        VibratoMode::Aftertouch => 4.0,
        VibratoMode::Pedal => 5.0,
    }
}

fn synth_layers(t: SynthTarget) -> std::ops::RangeInclusive<u8> {
    match t {
        SynthTarget::Layer(i) => {
            let n = SYNTH_FIRST + i.min(2);
            n..=n
        }
        SynthTarget::All => SYNTH_FIRST..=SYNTH_FIRST + 2,
    }
}

pub(super) fn map_synth(e: &SynthEdit, out: &mut Vec<W>) {
    let (target, p) = match *e {
        SynthEdit::Param(target, p) => (target, p),
        // SUSTPED / PSTICK of the three synth layers (`ped_*` counts all seven layers).
        SynthEdit::Sustped(on) => {
            for n in 5..=7 {
                out.push(W::layer("ped_sustped", n, flag(on)));
            }
            return;
        }
        SynthEdit::Pstick(on) => {
            for n in 5..=7 {
                out.push(W::layer("ped_pstick", n, flag(on)));
            }
            return;
        }
        // GROUP copies and SOUND INIT have no host parameter; per-layer values stay independent.
        SynthEdit::Group { .. } | SynthEdit::SoundInit(_) => return,
    };
    let per_layer = |out: &mut Vec<W>, family: &'static str, plain: f32| {
        for n in synth_layers(target) {
            out.push(W::layer(family, n, plain));
        }
    };
    use SynthParam as P;
    match p {
        P::OscMode(m) => out.push(W::global("synth_osc_mode", osc_mode_index(m))),
        P::FilterDrive(v) => out.push(W::global("synth_filter_drive", v)),
        P::Cutoff(v) => out.push(W::global("synth_filter_cutoff", v)),
        P::Resonance(v) => out.push(W::global("synth_filter_resonance", v)),
        P::AmpAttack(v) => out.push(W::global("synth_attack", v)),
        P::AmpDecay(v) => out.push(W::global("synth_decay", v)),
        P::AmpRelease(v) => out.push(W::global("synth_release", v)),
        P::KbTrack(v) => per_layer(out, "synthx_kb_track", f32::from(v)),
        P::FilterEnvAmount(v) => per_layer(out, "synthx_filter_env", v),
        P::OscEnvAmount(v) => per_layer(out, "synthx_osc_env", v),
        P::OscEnvToPitch(on) => per_layer(out, "synthx_osc_env_pitch", flag(on)),
        P::Voice(v) => per_layer(out, "synthx_voice", f32::from(v)),
        P::Glide(v) => per_layer(out, "synthx_glide", v),
        P::Unison(v) => per_layer(out, "synthx_unison", f32::from(v)),
        P::Priority(pr) => per_layer(
            out,
            "synthx_priority",
            match pr {
                NotePriority::Off => 0.0,
                NotePriority::Lo => 1.0,
                NotePriority::Hi => 2.0,
            },
        ),
        P::LfoRate(v) => per_layer(out, "synthm_lfo_rate", v),
        P::LfoAmount(v) => per_layer(out, "synthm_lfo_amount", v),
        P::ArpRate(v) => per_layer(out, "synthm_arp_rate", v),
        P::ArpRange(v) => per_layer(out, "synthm_arp_range", f32::from(v)),
        P::VibratoRate(v) => per_layer(out, "synthm_vib_rate", v),
        P::VibratoDepth(v) => per_layer(out, "synthm_vib_depth", v),
        P::VibratoDelay(v) => per_layer(out, "synthm_vib_delay", v),
        P::Pattern(pp) => {
            for n in synth_layers(target) {
                map_pattern(pp, n, out);
            }
        }
        P::LfoDivision(i) => per_layer(out, "synthg_lfo_div", f32::from(i)),
        P::ArpDivision(i) => per_layer(out, "synthg_arp_div", f32::from(i)),
        P::GateEnv(v) => per_layer(out, "synthg_gate_env", v),
        P::PatternPreset(i) => per_layer(out, "synthg_pat_preset", f32::from(i)),
        P::KbHold(on) => out.push(W::global("synthg_kb_hold", flag(on))),
        P::KbHoldExclude(on) => per_layer(out, "synthg_hold_excl", flag(on)),
        P::KbSync(on) => per_layer(out, "synthg_kb_sync", flag(on)),
        P::LfoClockSync(on) => per_layer(out, "synthg_lfo_sync", flag(on)),
        P::ArpClockSync(on) => per_layer(out, "synthg_arp_sync", flag(on)),
        P::PatternOn(on) => per_layer(out, "synthg_pat_on", flag(on)),
        P::LfoShape(s) => per_layer(out, "synthm_lfo_shape", lfo_shape_index(s)),
        P::LfoDest(d) => per_layer(out, "synthm_lfo_dest", lfo_dest_index(d)),
        P::ArpMode(m) => {
            if let Some(i) = arp_mode_index(m) {
                per_layer(out, "synthm_arp_mode", i)
            }
        }
        P::VibratoMode(m) => per_layer(out, "synthm_vib_mode", vibrato_mode_index(m)),
        // The remaining synth controls (oscillator shapes, wavetable/FM/sample selectors, filter
        // model, envelopes of the dedicated filter/osc paths, LFO/arp selectors, pattern
        // steps) have no host parameter yet; their controls are dimmed.
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synth_selector_indices_agree_with_the_host_decoders() {
        use crate::synth_mod_params::{arp, dest, shape, vibrato};
        for m in 0..6 {
            assert_eq!(arp_mode_index(arp(m)), Some(m as f32));
            assert_eq!(vibrato_mode_index(vibrato(m)), m as f32);
        }
        for i in 0..5 {
            assert_eq!(lfo_shape_index(shape(i)), i as f32);
        }
        for i in 0..4 {
            assert_eq!(lfo_dest_index(dest(i)), i as f32);
        }
    }

    #[test]
    fn synth_all_target_fans_out_to_three_layers_and_globals_write_once() {
        let mut v = Vec::new();
        map_synth(
            &SynthEdit::Param(SynthTarget::All, SynthParam::Glide(0.5)),
            &mut v,
        );
        assert_eq!(
            v,
            vec![
                W::layer("synthx_glide", 1, 0.5),
                W::layer("synthx_glide", 2, 0.5),
                W::layer("synthx_glide", 3, 0.5)
            ]
        );
        let mut g = Vec::new();
        map_synth(
            &SynthEdit::Param(SynthTarget::All, SynthParam::Cutoff(900.0)),
            &mut g,
        );
        assert_eq!(g, vec![W::global("synth_filter_cutoff", 900.0)]);
        let mut none = Vec::new();
        map_synth(
            &SynthEdit::Param(SynthTarget::Layer(0), SynthParam::WavetableIndex(3)),
            &mut none,
        );
        assert!(none.is_empty());
    }
}
