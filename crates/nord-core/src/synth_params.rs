//! Maps a decoded Synth layer (`SynthLayerDecoded`, from the `0x32` + `0x2F` blocks) onto the
//! real-time `SynthLayer`. Pure parameter plumbing: no allocation, no audio-thread state.
//!
//! Spec: `docs/specs/synth-engine.md`, `docs/specs/cbin-and-presets.md` (Synth Block).

use crate::cbin::{cutoff_hz_to_norm, SynthLayerDecoded};
use crate::morph::MorphParameter;
use crate::synth::{
    ArpMode, NotePriority, SynthLayer, SynthOscMode, VaWaveform, VibratoMode, FM_RATIOS,
    NUM_WAVETABLES,
};

/// Envelope time floor/ceiling shared by every ADR: OS v1.66 table range `0.5 ms..45 s`.
const ENV_MIN_SEC: f32 = 0.0005;
const ENV_MAX_SEC: f32 = 45.0;
/// Decay at or above this is the panel "Hold" setting (envelope stays at its peak).
const HOLD_DECAY_MS: f32 = 44_000.0;

/// ASSUMED: Glide knob `0..10` maps to `(knob/10)^2 * 2 s` per octave. The manual only says the
/// glide is constant-rate; confirm by recording a one-octave glide at several knob values.
const GLIDE_MAX_SEC_PER_OCTAVE: f32 = 2.0;
/// ASSUMED: Vibrato Amount `0..10` maps to `0..100` cents peak depth.
const VIBRATO_MAX_CENTS: f32 = 100.0;
/// ASSUMED: Amp velocity setting `1..=3` scales sensitivity linearly (`n / 3`); `0` is off.
const AMP_VELOCITY_STEPS: f32 = 3.0;

fn ms_to_sec(ms: f32) -> f32 {
    (ms / 1000.0).clamp(ENV_MIN_SEC, ENV_MAX_SEC)
}

fn is_hold(decay_ms: f32) -> bool {
    decay_ms >= HOLD_DECAY_MS || !decay_ms.is_finite()
}

fn apply_oscillator(synth: &mut SynthLayer, dec: &SynthLayerDecoded) {
    let ctrl_norm = (dec.detune / 10.0).clamp(0.0, 1.0);
    synth.osc_mode = dec.osc_mode;
    synth.waveform = dec.waveform;
    synth.fm_amount = ctrl_norm;
    synth.fm_ratio = FM_RATIOS[usize::from(dec.wave_id) % FM_RATIOS.len()];
    synth.fm_feedback = if (dec.wave_id & 1) != 0 { 0.25 } else { 0.0 };
    synth.fm_sub_amount = if dec.osc_category > 0 { 0.35 } else { 0.0 };
    synth.supersaw_detune = ctrl_norm;
    // ASSUMED: PURE Pulse 33 / Pulse 10 fix the duty cycle; Osc Ctrl narrows other pulses.
    let base_pw = match (dec.osc_mode, dec.waveform, dec.osc_category, dec.osc_wave) {
        (SynthOscMode::Analog, VaWaveform::Pulse, 0, 4) => 0.33,
        (SynthOscMode::Analog, VaWaveform::Pulse, 0, 5) => 0.10,
        _ => 0.5,
    };
    synth.pulse_width = (base_pw - ctrl_norm * 0.45 * base_pw / 0.5).max(0.02);
    synth.wavetable_index = dec.wave_id % (NUM_WAVETABLES as u8);
    synth.wavetable_position = ctrl_norm;
    synth.pitch_cents = f32::from(dec.pitch_fine_cents) + 100.0 * f32::from(dec.pitch_coarse_semi);
}

fn apply_filter(
    synth: &mut SynthLayer,
    cutoff_morph: &mut MorphParameter,
    dec: &SynthLayerDecoded,
) {
    synth.filter_on = dec.filter_on;
    synth.filter_model = dec.filter_model;
    synth.cutoff_hz = dec.filter_cutoff_hz;
    synth.resonance = dec.filter_resonance;
    synth.filter_drive = dec.filter_drive;
    synth.filter_env_amount = dec.filter_env_amount;
    synth.kb_track = dec.kb_track.min(3);
    synth.filter_env_dedicated = dec.filter_env_dedicated;
    synth.filter_env_attack_sec = ms_to_sec(dec.filter_env_attack_ms);
    synth.filter_env_decay_sec = ms_to_sec(dec.filter_env_decay_ms);
    synth.filter_env_release_sec = ms_to_sec(dec.filter_env_release_ms);
    synth.filter_env_vel_amount = if dec.filter_velocity { 1.0 } else { 0.0 };

    cutoff_morph.base = cutoff_hz_to_norm(dec.filter_cutoff_hz);
    cutoff_morph.delta_wheel = dec.filter_cutoff_morph.wheel;
    cutoff_morph.delta_aftertouch = dec.filter_cutoff_morph.aftertouch;
    cutoff_morph.delta_pedal = dec.filter_cutoff_morph.ctrl_pedal;
}

fn apply_envelopes(synth: &mut SynthLayer, dec: &SynthLayerDecoded) {
    synth.attack_sec = ms_to_sec(dec.env_attack_ms);
    synth.decay_sec = ms_to_sec(dec.env_decay_ms);
    // Amp is an ADR envelope: a finite decay falls to silence while held, Hold sustains at full.
    synth.sustain_level = if is_hold(dec.env_decay_ms) { 1.0 } else { 0.0 };
    synth.release_sec = ms_to_sec(dec.env_release_ms);
    synth.amp_velocity = f32::from(dec.amp_velocity.min(3)) / AMP_VELOCITY_STEPS;

    synth.osc_env_attack_sec = ms_to_sec(dec.osc_env_attack_ms);
    synth.osc_env_decay_sec = ms_to_sec(dec.osc_env_decay_ms);
    synth.osc_env_release_sec = ms_to_sec(dec.osc_env_release_ms);
    synth.osc_env_amount = dec.osc_env_amount.clamp(-1.0, 1.0);
    synth.osc_env_to_pitch = dec.osc_env_to_pitch;
    synth.osc_env_velocity = dec.osc_env_velocity;
}

fn apply_modulation(synth: &mut SynthLayer, dec: &SynthLayerDecoded) {
    synth.lfo_rate_hz = dec.lfo_rate_hz;
    synth.lfo_amount = dec.lfo_amount;
    synth.lfo_shape = dec.lfo_shape;
    synth.lfo_dest = dec.lfo_dest;
    synth.vibrato_mode = match dec.vibrato_mode {
        1 => VibratoMode::On,
        2 => VibratoMode::Delay,
        3 => VibratoMode::Wheel,
        4 => VibratoMode::Aftertouch,
        5 => VibratoMode::Pedal,
        _ => VibratoMode::Off,
    };
    synth.vibrato_delay_sec = dec.vibrato_delay_sec;
    synth.vibrato_rate_hz = dec.vibrato_rate_hz;
    synth.vibrato_depth_cents = (dec.vibrato_amount / 10.0).clamp(0.0, 1.0) * VIBRATO_MAX_CENTS;
    synth.arp_mode = if dec.arp_enabled {
        ArpMode::Up
    } else {
        ArpMode::Off
    };
    synth.arp_rate_bpm = dec.arp_bpm;
}

fn apply_voice_mode(synth: &mut SynthLayer, dec: &SynthLayerDecoded) {
    synth.unison = dec.unison.min(3);
    // The panel has separate Mono and Legato LEDs; Legato is also monophonic.
    synth.mono = dec.mono || dec.legato;
    synth.legato = dec.legato;
    synth.note_priority = match dec.voice_priority {
        1 => NotePriority::Lo,
        2 => NotePriority::Hi,
        _ => NotePriority::Off,
    };
    let knob = (dec.glide / 10.0).clamp(0.0, 1.0);
    synth.glide_sec = knob * knob * GLIDE_MAX_SEC_PER_OCTAVE;
    synth.octave_offset = dec.octave.clamp(-3, 3);
}

/// Applies every decoded parameter of one Synth layer to the engine voice.
pub fn apply_decoded_layer(
    synth: &mut SynthLayer,
    cutoff_morph: &mut MorphParameter,
    dec: &SynthLayerDecoded,
) {
    apply_oscillator(synth, dec);
    apply_filter(synth, cutoff_morph, dec);
    apply_envelopes(synth, dec);
    apply_modulation(synth, dec);
    apply_voice_mode(synth, dec);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::{LfoDestination, LfoShape};

    fn apply(dec: &SynthLayerDecoded) -> SynthLayer {
        let mut s = SynthLayer::new(48_000.0);
        let mut m = MorphParameter::new(0.0);
        apply_decoded_layer(&mut s, &mut m, dec);
        s
    }

    #[test]
    fn voice_mode_maps_mono_legato_priority_glide_unison_octave() {
        let dec = SynthLayerDecoded {
            legato: true,
            voice_priority: 2,
            glide: 10.0,
            unison: 3,
            octave: -2,
            ..Default::default()
        };
        let s = apply(&dec);
        assert!(s.mono && s.legato);
        assert_eq!(s.note_priority, NotePriority::Hi);
        assert!((s.glide_sec - GLIDE_MAX_SEC_PER_OCTAVE).abs() < 1e-6);
        assert_eq!(s.unison, 3);
        assert_eq!(s.octave_offset, -2);

        let mono_only = apply(&SynthLayerDecoded {
            mono: true,
            voice_priority: 1,
            ..Default::default()
        });
        assert!(mono_only.mono && !mono_only.legato);
        assert_eq!(mono_only.note_priority, NotePriority::Lo);
        assert_eq!(apply(&SynthLayerDecoded::default()).glide_sec, 0.0);
    }

    #[test]
    fn modulation_maps_lfo_vibrato_and_arp() {
        let dec = SynthLayerDecoded {
            lfo_shape: LfoShape::SawDown,
            lfo_dest: LfoDestination::Off,
            vibrato_mode: 2,
            vibrato_amount: 5.0,
            vibrato_delay_sec: 1.2,
            arp_enabled: true,
            ..Default::default()
        };
        let s = apply(&dec);
        assert_eq!(s.lfo_shape, LfoShape::SawDown);
        assert_eq!(s.lfo_dest, LfoDestination::Off);
        assert_eq!(s.vibrato_mode, VibratoMode::Delay);
        assert!((s.vibrato_depth_cents - 50.0).abs() < 1e-4);
        assert_eq!(s.arp_mode, ArpMode::Up);
        assert_eq!(apply(&SynthLayerDecoded::default()).arp_mode, ArpMode::Off);
    }

    #[test]
    fn envelopes_filter_and_tuning_are_forwarded() {
        let dec = SynthLayerDecoded {
            env_decay_ms: 300.0,
            filter_env_dedicated: true,
            filter_env_attack_ms: 24.0,
            filter_env_decay_ms: 1820.0,
            filter_env_release_ms: 100.0,
            filter_velocity: true,
            filter_on: false,
            filter_env_amount: 0.55,
            amp_velocity: 3,
            osc_env_amount: -0.32,
            osc_env_to_pitch: true,
            osc_env_velocity: true,
            pitch_fine_cents: -8,
            pitch_coarse_semi: 1,
            ..Default::default()
        };
        let s = apply(&dec);
        assert!(s.filter_env_dedicated && !s.filter_on);
        assert!((s.filter_env_attack_sec - 0.024).abs() < 1e-6);
        assert!((s.filter_env_decay_sec - 1.82).abs() < 1e-6);
        assert!((s.filter_env_release_sec - 0.1).abs() < 1e-6);
        assert_eq!(s.filter_env_vel_amount, 1.0);
        assert!((s.filter_env_amount - 0.55).abs() < 1e-6);
        assert_eq!(s.amp_velocity, 1.0);
        assert_eq!(s.sustain_level, 0.0);
        assert!((s.osc_env_amount + 0.32).abs() < 1e-6);
        assert!(s.osc_env_to_pitch && s.osc_env_velocity);
        assert!((s.pitch_cents - 92.0).abs() < 1e-4);

        let hold = apply(&SynthLayerDecoded {
            env_decay_ms: 45_000.0,
            amp_velocity: 0,
            ..Default::default()
        });
        assert_eq!(hold.sustain_level, 1.0);
        assert_eq!(hold.amp_velocity, 0.0);
    }

    #[test]
    fn oscillator_forwards_wavetable_fm_and_pulse_width() {
        let wt = apply(&SynthLayerDecoded {
            osc_mode: SynthOscMode::Wavetable,
            wave_id: 40,
            detune: 5.0,
            ..Default::default()
        });
        assert_eq!(wt.wavetable_index, 40);
        assert!((wt.wavetable_position - 0.5).abs() < 1e-6);

        let fm = apply(&SynthLayerDecoded {
            osc_mode: SynthOscMode::Fm,
            wave_id: 11,
            osc_category: 2,
            ..Default::default()
        });
        assert!((fm.fm_ratio - FM_RATIOS[11]).abs() < 1e-6);
        assert!((fm.fm_feedback - 0.25).abs() < 1e-6);
        assert!((fm.fm_sub_amount - 0.35).abs() < 1e-6);

        let p33 = apply(&SynthLayerDecoded {
            waveform: VaWaveform::Pulse,
            osc_category: 0,
            osc_wave: 4,
            ..Default::default()
        });
        assert!((p33.pulse_width - 0.33).abs() < 1e-6);
        let p10 = apply(&SynthLayerDecoded {
            waveform: VaWaveform::Pulse,
            osc_category: 0,
            osc_wave: 5,
            ..Default::default()
        });
        assert!((p10.pulse_width - 0.10).abs() < 1e-6);
    }
}
