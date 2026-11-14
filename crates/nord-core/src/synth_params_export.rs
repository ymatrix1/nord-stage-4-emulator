//! Inverse of [`apply_decoded_layer`](crate::synth_params::apply_decoded_layer): the engine
//! `SynthLayer` back into the decoded layer, so STORE of a program or a Synth preset carries the
//! panel edits of the Synth sound (OQ30).
//!
//! Two entry points, both control thread only (they allocate):
//!
//! * [`invert_decoded_layer`] writes every field whose mapping is exact; [`LOSSY_FIELDS`] lists
//!   what it never touches. The decoded layer is the *base*: where the mapping is many to one
//!   (Pitch fine / coarse, FM wave index, Gate / Arp RANGE raw), the base value is kept while it
//!   still projects onto the engine value.
//! * [`export_decoded_layer`] is the one STORE uses: it applies the base to a reference layer and
//!   copies a field only when the engine value differs from what the base produces. An unedited
//!   layer therefore leaves the base untouched, whatever the decoder collapsed.
//!
//! Bits the decoder collapses (OQ31) never come back through this module: `pack` plus the bit
//! merge of `SectionPreset::to_file_like` keep them from the base file.
//!
//! Spec: `docs/specs/cbin-and-presets.md` section 5.7.

use crate::cbin::{MorphDeltas, SynthLayerDecoded};
use crate::morph::MorphParameter;
use crate::synth::{NotePriority, SynthLayer, SynthOscMode, VaWaveform, VibratoMode, FM_RATIOS};
use crate::synth_params::{
    apply_decoded_layer, glide_knob, pulse_base_width, vibrato_amount_from_cents,
};

/// Decoded fields the engine does not represent exactly: STORE leaves them with the base.
///
/// * `sample_id`, `osc_type`, `osc_category`, `osc_wave`: the panel has no category / wave browser
///   and the engine keeps only the derived waveform (`osc_mode` Sample is never written either).
/// * `pattern.steps/accent/gate/pan`: the engine's custom pattern holds gate lengths in eighths,
///   the file four gate codes (`E q Q .`); `pattern.enabled` is written.
/// * `lfo_mst_clk`, `filter_env_dedicated`, `knobs`: not mapped by `apply_decoded_layer`.
/// * `osc_ctrl_morph`, `filter_resonance_morph`: the engine has no morph slot for them.
/// * `pan`, `enabled`, `level`, `level_morph`, `octave`: owned by `ProgramState::export_into`.
pub const LOSSY_FIELDS: &[&str] = &[
    "sample_id",
    "osc_type",
    "osc_category",
    "osc_wave",
    "pattern.steps/accent/gate/pan",
    "lfo_mst_clk",
    "filter_env_dedicated",
    "knobs",
    "osc_ctrl_morph",
    "filter_resonance_morph",
];

fn deltas(m: &MorphParameter) -> MorphDeltas {
    MorphDeltas {
        wheel: m.delta_wheel,
        aftertouch: m.delta_aftertouch,
        ctrl_pedal: m.delta_pedal,
    }
}

fn ms(sec: f32) -> f32 {
    sec * 1000.0
}

/// Index of the FM ratio closest to `ratio`.
fn fm_index(ratio: f32) -> usize {
    let dist = |i: usize| (FM_RATIOS[i] - ratio).abs();
    (0..FM_RATIOS.len())
        .min_by(|&a, &b| dist(a).total_cmp(&dist(b)))
        .unwrap_or(0)
}

/// Osc Ctrl `0..1` the engine holds for the current oscillator (`None`: the knob is not wired).
fn osc_ctrl(s: &SynthLayer, dec: &SynthLayerDecoded) -> Option<f32> {
    match (s.osc_mode, s.waveform) {
        (SynthOscMode::Wavetable, _) => Some(s.wavetable_position),
        (SynthOscMode::Fm, _) => Some(s.fm_amount),
        (SynthOscMode::Analog, VaWaveform::SuperSaw) => Some(s.supersaw_detune),
        (SynthOscMode::Analog, VaWaveform::Pulse) => {
            let base = pulse_base_width(dec);
            Some((base - s.pulse_width) / (0.45 * base / 0.5))
        }
        _ => None,
    }
}

fn invert_oscillator(s: &SynthLayer, dec: &mut SynthLayerDecoded) {
    if s.osc_mode != SynthOscMode::Sample && dec.osc_mode != SynthOscMode::Sample {
        dec.osc_mode = s.osc_mode;
        dec.waveform = s.waveform;
    }
    match s.osc_mode {
        SynthOscMode::Wavetable => {
            let n = crate::synth::NUM_WAVETABLES as u8;
            if dec.wave_id % n != s.wavetable_index {
                dec.wave_id = s.wavetable_index;
            }
        }
        SynthOscMode::Fm => {
            let idx = fm_index(s.fm_ratio);
            if usize::from(dec.wave_id) % FM_RATIOS.len() != idx {
                dec.wave_id = (dec.wave_id & !15) | idx as u8;
            }
        }
        _ => {}
    }
    if let Some(c) = osc_ctrl(s, dec) {
        dec.detune = c.clamp(0.0, 1.0) * 10.0;
    }
    // Fine + coarse: keep the base split while the fine part still fits `-64..=63`.
    let total = s.pitch_cents.round() as i32;
    let mut coarse = i32::from(dec.pitch_coarse_semi);
    if !(-64..=63).contains(&(total - 100 * coarse)) {
        coarse = (total as f32 / 100.0).round() as i32;
    }
    dec.pitch_coarse_semi = coarse.clamp(-32, 31) as i8;
    dec.pitch_fine_cents = (total - 100 * coarse).clamp(-64, 63) as i8;
}

fn invert_filter(s: &SynthLayer, cutoff: &MorphParameter, dec: &mut SynthLayerDecoded) {
    dec.filter_on = s.filter_on;
    dec.filter_model = s.filter_model;
    dec.filter_cutoff_hz = s.cutoff_hz;
    dec.filter_cutoff_morph = deltas(cutoff);
    dec.filter_resonance = s.resonance;
    dec.filter_drive = s.filter_drive;
    dec.filter_env_amount = s.filter_env_amount;
    dec.kb_track = s.kb_track.min(3);
    dec.filter_env_attack_ms = ms(s.filter_env_attack_sec);
    dec.filter_env_decay_ms = ms(s.filter_env_decay_sec);
    dec.filter_env_release_ms = ms(s.filter_env_release_sec);
    dec.filter_velocity = s.filter_env_vel_amount > 0.5;
}

fn invert_envelopes(s: &SynthLayer, dec: &mut SynthLayerDecoded) {
    dec.env_attack_ms = ms(s.attack_sec);
    dec.env_decay_ms = ms(s.decay_sec);
    dec.env_release_ms = ms(s.release_sec);
    dec.amp_velocity = (s.amp_velocity * 3.0).round().clamp(0.0, 3.0) as u8;
    dec.osc_env_attack_ms = ms(s.osc_env_attack_sec);
    dec.osc_env_decay_ms = ms(s.osc_env_decay_sec);
    dec.osc_env_release_ms = ms(s.osc_env_release_sec);
    dec.osc_env_amount = s.osc_env_amount.clamp(-1.0, 1.0);
    dec.osc_env_to_pitch = s.osc_env_to_pitch;
    dec.osc_env_velocity = s.osc_env_velocity;
}

fn invert_modulation(s: &SynthLayer, dec: &mut SynthLayerDecoded) {
    dec.lfo_rate_hz = s.lfo_rate_hz;
    dec.lfo_amount = s.lfo_amount;
    dec.lfo_shape = s.lfo_shape;
    dec.lfo_dest = s.lfo_dest;
    dec.vibrato_mode = match s.vibrato_mode {
        VibratoMode::Off => 0,
        VibratoMode::On => 1,
        VibratoMode::Delay => 2,
        VibratoMode::Wheel => 3,
        VibratoMode::Aftertouch => 4,
        VibratoMode::Pedal => 5,
    };
    dec.vibrato_delay_sec = s.vibrato_delay_sec;
    dec.vibrato_rate_hz = s.vibrato_rate_hz;
    dec.vibrato_amount = vibrato_amount_from_cents(s.vibrato_depth_cents);
    crate::synth_params_arp::invert_arp_modes(s, dec);
    dec.pattern.enabled = s.perf.pattern_on;
}

fn invert_voice_mode(s: &SynthLayer, dec: &mut SynthLayerDecoded) {
    dec.unison = s.unison.min(3);
    // `apply` stores mono || legato in the engine's mono: keep the base Mono bit while Legato
    // explains it.
    dec.legato = s.legato;
    dec.mono = if s.legato { dec.mono } else { s.mono };
    dec.voice_priority = match s.note_priority {
        NotePriority::Off => 0,
        NotePriority::Lo => 1,
        NotePriority::Hi => 2,
    };
    dec.glide = glide_knob(s.glide_sec);
}

/// Writes every exactly mapped field of `s` (and the cutoff morph deltas) into `dec`.
pub fn invert_decoded_layer(s: &SynthLayer, cutoff: &MorphParameter, dec: &mut SynthLayerDecoded) {
    invert_oscillator(s, dec);
    invert_filter(s, cutoff, dec);
    invert_envelopes(s, dec);
    invert_modulation(s, dec);
    invert_voice_mode(s, dec);
}

/// `dec.<field> = inv.<field>` for each entry whose engine inputs differ between `rf` (what the
/// base produces) and `s` (the panel state).
macro_rules! sync {
    ($dec:ident, $inv:ident, $rf:ident, $s:ident; $($($d:ident).+ => $($e:ident).+),+ $(,)?) => {
        $( if $rf.$($e).+ != $s.$($e).+ { $dec.$($d).+ = $inv.$($d).+; } )+
    };
}
pub(crate) use sync;

/// STORE of a Synth layer: `dec` (the base) patched with the panel edits of `s`.
pub fn export_decoded_layer(s: &SynthLayer, cutoff: &MorphParameter, dec: &mut SynthLayerDecoded) {
    let mut rf = SynthLayer::new(s.sample_rate);
    let mut rf_cutoff = MorphParameter::new(0.0);
    apply_decoded_layer(&mut rf, &mut rf_cutoff, dec);
    let mut inv = dec.clone();
    invert_decoded_layer(s, cutoff, &mut inv);
    if deltas(&rf_cutoff) != deltas(cutoff) {
        dec.filter_cutoff_morph = inv.filter_cutoff_morph;
    }
    sync!(dec, inv, rf, s;
        osc_mode => osc_mode, waveform => waveform, wave_id => wavetable_index, wave_id => fm_ratio,
        detune => wavetable_position, detune => fm_amount, detune => supersaw_detune,
        detune => pulse_width, pitch_fine_cents => pitch_cents, pitch_coarse_semi => pitch_cents,
        filter_on => filter_on, filter_model => filter_model, filter_cutoff_hz => cutoff_hz,
        filter_resonance => resonance, filter_drive => filter_drive,
        filter_env_amount => filter_env_amount, kb_track => kb_track,
        filter_env_attack_ms => filter_env_attack_sec,
        filter_env_decay_ms => filter_env_decay_sec,
        filter_env_release_ms => filter_env_release_sec,
        filter_velocity => filter_env_vel_amount,
        env_attack_ms => attack_sec, env_decay_ms => decay_sec, env_release_ms => release_sec,
        amp_velocity => amp_velocity,
        osc_env_attack_ms => osc_env_attack_sec, osc_env_decay_ms => osc_env_decay_sec,
        osc_env_release_ms => osc_env_release_sec, osc_env_amount => osc_env_amount,
        osc_env_to_pitch => osc_env_to_pitch, osc_env_velocity => osc_env_velocity,
        lfo_rate_hz => lfo_rate_hz, lfo_amount => lfo_amount, lfo_shape => lfo_shape,
        lfo_dest => lfo_dest, vibrato_mode => vibrato_mode,
        vibrato_delay_sec => vibrato_delay_sec, vibrato_rate_hz => vibrato_rate_hz,
        vibrato_amount => vibrato_depth_cents, pattern.enabled => perf.pattern_on,
        unison => unison, legato => legato, mono => mono, mono => legato,
        voice_priority => note_priority, glide => glide_sec,
    );
    crate::synth_params_arp::sync_arp_modes(&rf, s, &inv, dec);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cbin::SynthPresetDecoded;
    use crate::synth::{ArpMode, LfoDestination, LfoShape};

    /// Deterministic xorshift payloads: bit patterns the decoder accepts, no files needed.
    fn random_preset(seed: u64) -> SynthPresetDecoded {
        let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let bytes: Vec<u8> = (0..SynthPresetDecoded::PAYLOAD_LEN)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x >> 24) as u8
            })
            .collect();
        SynthPresetDecoded::unpack(&bytes).unwrap()
    }

    fn applied(dec: &SynthLayerDecoded) -> (SynthLayer, MorphParameter) {
        let mut s = SynthLayer::new(48_000.0);
        let mut m = MorphParameter::new(0.0);
        apply_decoded_layer(&mut s, &mut m, dec);
        (s, m)
    }

    fn packed(layer: &SynthLayerDecoded) -> Vec<u8> {
        let mut p = SynthPresetDecoded::default();
        p.layer_a = layer.clone();
        p.pack().unwrap()
    }

    #[test]
    fn the_inverse_of_apply_returns_the_same_bits_for_random_layers() {
        for seed in 1..=150 {
            let preset = random_preset(seed);
            for dec in [&preset.layer_a, &preset.layer_b, &preset.layer_c] {
                let (s, m) = applied(dec);
                let mut back = dec.clone();
                invert_decoded_layer(&s, &m, &mut back);
                assert_eq!(packed(&back), packed(dec), "seed {seed}");
            }
        }
    }

    #[test]
    fn an_unedited_layer_is_left_exactly_as_the_base() {
        for seed in 1..=100 {
            let preset = random_preset(seed);
            for dec in [&preset.layer_a, &preset.layer_b, &preset.layer_c] {
                let (s, m) = applied(dec);
                let mut out = dec.clone();
                export_decoded_layer(&s, &m, &mut out);
                assert_eq!(&out, dec, "seed {seed}");
            }
        }
    }

    #[test]
    fn panel_edits_of_the_sound_survive_pack_unpack_and_apply() {
        let base = SynthLayerDecoded::default();
        let (mut s, mut m) = applied(&base);
        s.cutoff_hz = 1000.0;
        s.resonance = 0.5;
        s.attack_sec = 0.2;
        s.filter_env_attack_sec = 0.4;
        s.lfo_shape = LfoShape::Square;
        s.lfo_dest = LfoDestination::OscPitch;
        s.arp_mode = ArpMode::Down;
        s.glide_sec = 0.5;
        s.unison = 2;
        s.waveform = VaWaveform::Pulse;
        s.pitch_cents = 250.0;
        s.note_priority = NotePriority::Hi;
        s.vibrato_mode = VibratoMode::Delay;
        m.delta_wheel = 0.4;
        let mut dec = base.clone();
        export_decoded_layer(&s, &m, &mut dec);
        let back = SynthPresetDecoded::unpack(&packed(&dec)).unwrap().layer_a;
        let (r, rm) = applied(&back);
        assert!((r.cutoff_hz - 1000.0).abs() < 25.0, "{}", r.cutoff_hz);
        assert!((r.resonance - 0.5).abs() < 0.01);
        assert!((r.attack_sec - 0.2).abs() < 0.03 && r.filter_env_attack_sec > 0.3);
        assert_eq!(r.lfo_shape, LfoShape::Square);
        assert_eq!(r.lfo_dest, LfoDestination::OscPitch);
        assert_eq!(r.arp_mode, ArpMode::Down);
        assert!((r.glide_sec - 0.5).abs() < 0.02);
        assert_eq!((r.unison, r.waveform), (2, VaWaveform::Pulse));
        assert!((r.pitch_cents - 250.0).abs() < 1e-3);
        assert_eq!(r.note_priority, NotePriority::Hi);
        assert_eq!(r.vibrato_mode, VibratoMode::Delay);
        assert!((rm.delta_wheel - 0.4).abs() < 0.02);
    }

    #[test]
    fn many_to_one_fields_keep_the_base_until_edited() {
        let base = SynthLayerDecoded {
            pitch_fine_cents: 60,
            pitch_coarse_semi: 1,
            ..Default::default()
        };
        let (mut s, m) = applied(&base);
        let mut dec = base.clone();
        export_decoded_layer(&s, &m, &mut dec);
        assert_eq!((dec.pitch_fine_cents, dec.pitch_coarse_semi), (60, 1));
        s.pitch_cents += 10.0;
        export_decoded_layer(&s, &m, &mut dec);
        assert_eq!((dec.pitch_fine_cents, dec.pitch_coarse_semi), (-30, 2));
    }

    #[test]
    fn lossy_fields_are_never_written() {
        let base = random_preset(7).layer_b;
        let (s, m) = applied(&SynthLayerDecoded::default());
        let mut dec = base.clone();
        invert_decoded_layer(&s, &m, &mut dec);
        assert_eq!(dec.sample_id, base.sample_id);
        assert_eq!(dec.osc_ctrl_morph, base.osc_ctrl_morph);
        assert_eq!(dec.filter_resonance_morph, base.filter_resonance_morph);
        assert_eq!(dec.pattern.gate, base.pattern.gate);
        assert_eq!(dec.pan, base.pan);
        assert!(LOSSY_FIELDS.contains(&"sample_id"));
    }
}
