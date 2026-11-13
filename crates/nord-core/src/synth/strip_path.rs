//! Firmware strip chain of a synth voice (opt-in `SynthLayer::chain = FirmwareStrip`).
//!
//! Per voice, the order of the Core2 strip: firmware saw (`SawModel::FirmwareSy`) -> ADR record
//! chain (`fw_adr_chain.rs`, coefficients from `fw_env_rates.rs`) -> four-pole filter stage
//! (`fw_filter.rs`, cutoff through the strip word 3 law of `fw_strip_cutoff.rs`) -> gain
//! de-zipper (`gain_dezip.rs`). Panel values reach the stages through `strip_map.rs` (ASSUMED
//! mappings); the stages themselves are VERIFIED against the SHARC oracle in their own modules.
//!
//! Spec: `docs/specs/synth-engine.md` section 4.2. No heap use: the records are plain values and
//! the parameter cache is recomputed only when a panel value changes.

use super::strip_map::{
    attack_index, decay_index, strip_word_for_cutoff, DECAY_TAU_FRACTION, STRIP_PASS_FRAMES,
};
use super::*;

/// Which per-voice signal chain a synth layer runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SynthChain {
    /// Classic engine (default): analytic envelopes, `filter_engine` filter, saw per `saw_model`.
    #[default]
    Legacy,
    /// Firmware strip: firmware saw, ADR record chain, strip-law cutoff, firmware filter, gain
    /// de-zipper. Forces `SawModel::FirmwareSy` and `FilterEngine::Firmware` while selected.
    FirmwareStrip,
}

/// Records of one ADR envelope (`[a, b, thr]` of the three phases).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StripEnvRecords {
    pub attack: FwAdrRecord,
    pub decay: FwAdrRecord,
    pub release: FwAdrRecord,
}

impl StripEnvRecords {
    /// Records for panel times (seconds); `sustain` is the decay record threshold (`0` falls to
    /// silence, `1` is Hold).
    ///
    /// ASSUMED: attack time = time to full scale of the linear ramp; decay and release time
    /// constants = [`DECAY_TAU_FRACTION`] x the displayed time; the 3 ms floor of decay and
    /// release is the classic OS v1.66 floor.
    pub fn from_panel(attack_sec: f32, decay_sec: f32, release_sec: f32, sustain: f32) -> Self {
        let hold = decay_sec >= 44.0;
        let atk = attack_coeff(attack_index(attack_sec) as f32);
        let dec_idx = if hold {
            RATE_INDEX_MAX
        } else {
            decay_index(decay_sec.max(0.003) * DECAY_TAU_FRACTION)
        };
        let rel_idx = decay_index(release_sec.max(0.003) * DECAY_TAU_FRACTION);
        Self {
            attack: FwAdrRecord {
                a: 1.0,
                b: atk,
                thr: 0.0,
            },
            decay: FwAdrRecord {
                a: decay_release_coeff(dec_idx as f32),
                b: 0.0,
                thr: if hold { 1.0 } else { sustain.clamp(0.0, 1.0) },
            },
            release: FwAdrRecord {
                a: decay_release_coeff(rel_idx as f32),
                b: 0.0,
                thr: 0.0,
            },
        }
    }
}

/// Everything the render loop reads for the strip chain; rebuilt when a panel value changes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StripParams {
    pub amp: StripEnvRecords,
    pub filter: StripEnvRecords,
    /// Engine frames per envelope pass (`STRIP_PASS_FRAMES * fs / F`).
    pub period_frames: f32,
}

/// Cache key: the panel values `StripParams` is a function of.
pub(super) type StripKey = [f32; 8];

impl SynthLayer {
    fn strip_key(&self) -> StripKey {
        [
            self.attack_sec,
            self.decay_sec,
            self.release_sec,
            self.sustain_level,
            self.filter_env_attack_sec,
            self.filter_env_decay_sec,
            self.filter_env_release_sec,
            self.sample_rate,
        ]
    }

    /// Strip parameters of the current panel values (recomputed only when they changed).
    pub(super) fn strip_params(&mut self) -> StripParams {
        let key = self.strip_key();
        if let Some((k, p)) = self.strip_cache {
            if k == key {
                return p;
            }
        }
        let p = StripParams {
            amp: StripEnvRecords::from_panel(
                self.attack_sec,
                self.decay_sec,
                self.release_sec,
                self.sustain_level,
            ),
            filter: StripEnvRecords::from_panel(
                self.filter_env_attack_sec,
                self.filter_env_decay_sec,
                self.filter_env_release_sec,
                0.0,
            ),
            period_frames: STRIP_PASS_FRAMES * self.sample_rate / SY_FRAME_RATE_HZ,
        };
        self.strip_cache = Some((key, p));
        p
    }

    /// True when the layer runs the firmware strip chain.
    #[inline]
    pub fn strip_chain(&self) -> bool {
        self.chain == SynthChain::FirmwareStrip
    }

    /// Strip word 3 the ARM would write for the layer's cutoff (diagnostics and tests).
    pub fn cutoff_strip_word(&self) -> i32 {
        strip_word_for_cutoff(self.cutoff_hz)
    }
}

/// Phase of one ADR chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum EnvPhase {
    #[default]
    Attack,
    Decay,
    Release,
}

/// State of one envelope chain: the level and the active record.
#[derive(Debug, Clone, Copy, Default)]
struct EnvChain {
    y: f32,
    phase: EnvPhase,
}

impl EnvChain {
    #[inline]
    fn step(&mut self, recs: &StripEnvRecords) {
        let rec = match self.phase {
            EnvPhase::Attack => &recs.attack,
            EnvPhase::Decay => &recs.decay,
            EnvPhase::Release => &recs.release,
        };
        let (y, action) = fw_adr_step(self.y, rec);
        self.y = y;
        if action == FwAdrAction::Link && self.phase == EnvPhase::Attack {
            self.phase = EnvPhase::Decay;
        }
    }
}

/// Per-voice strip state (`Copy`, no heap): amp and filter chains plus the pass clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct StripVoice {
    amp: EnvChain,
    filter: EnvChain,
    clock: f32,
    released: bool,
}

impl StripVoice {
    /// Advances one engine frame; steps both chains once per pass. Returns `(amp, filter)` levels.
    ///
    /// Gate off moves both chains to their release record once (the controller `T` change of the
    /// firmware loop, `synth-adr-and-strip-law.md` section 2).
    #[inline]
    pub fn advance(&mut self, p: &StripParams, gate: bool) -> (f32, f32) {
        if !gate && !self.released {
            self.released = true;
            self.amp.phase = EnvPhase::Release;
            self.filter.phase = EnvPhase::Release;
        }
        self.clock += 1.0;
        while self.clock >= p.period_frames {
            self.clock -= p.period_frames;
            self.amp.step(&p.amp);
            self.filter.step(&p.filter);
        }
        (self.amp.y, self.filter.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip_layer() -> SynthLayer {
        let mut s = SynthLayer::new(48_000.0);
        s.chain = SynthChain::FirmwareStrip;
        s.attack_sec = 0.005;
        s.decay_sec = 0.3;
        s.sustain_level = 0.0;
        s.release_sec = 0.2;
        s.filter_env_dedicated = false;
        s
    }

    #[test]
    fn default_chain_is_legacy() {
        assert_eq!(SynthLayer::new(48_000.0).chain, SynthChain::Legacy);
        assert!(!SynthLayer::new(48_000.0).strip_chain());
    }

    #[test]
    fn records_use_the_table_coefficients() {
        let r = StripEnvRecords::from_panel(0.01, 1.0, 0.5, 0.0);
        assert_eq!(r.attack.a, 1.0);
        assert!(r.attack.b > 1e-5 && r.attack.b < 0.1);
        assert!(r.decay.a < 1.0 && r.decay.a > 0.99);
        assert!(r.release.a > r.decay.a - 0.1 && r.release.thr == 0.0);
        let hold = StripEnvRecords::from_panel(0.01, 45.0, 0.5, 0.0);
        assert_eq!(hold.decay.thr, 1.0);
        assert_eq!(hold.decay.a, decay_release_coeff(64.0));
    }

    #[test]
    fn params_are_cached_until_a_value_changes() {
        let mut s = strip_layer();
        let a = s.strip_params();
        assert_eq!(a, s.strip_params());
        assert!((a.period_frames - 2.0 * 48_000.0 / SY_FRAME_RATE_HZ).abs() < 1e-6);
        s.attack_sec = 1.0;
        assert_ne!(a.amp.attack, s.strip_params().amp.attack);
    }

    #[test]
    fn envelope_equals_the_record_step_iteration() {
        let mut s = strip_layer();
        let p = s.strip_params();
        let mut v = StripVoice::default();
        let (mut y, mut phase_decay) = (0.0f32, false);
        let mut clock = 0.0f32;
        for n in 0..4_000 {
            let (amp, _) = v.advance(&p, true);
            clock += 1.0;
            while clock >= p.period_frames {
                clock -= p.period_frames;
                let rec = if phase_decay {
                    &p.amp.decay
                } else {
                    &p.amp.attack
                };
                let (ny, act) = fw_adr_step(y, rec);
                y = ny;
                if act == FwAdrAction::Link {
                    phase_decay = true;
                }
            }
            assert_eq!(amp, y, "frame {n}");
        }
        assert!(phase_decay, "attack finished");
    }

    #[test]
    fn gate_off_releases_to_silence() {
        let mut s = strip_layer();
        let p = s.strip_params();
        let mut v = StripVoice::default();
        for _ in 0..2_000 {
            v.advance(&p, true);
        }
        let held = v.advance(&p, true).0;
        let mut last = held;
        for _ in 0..48_000 {
            last = v.advance(&p, false).0;
        }
        assert!(last < held * 0.05, "released from {held} to {last}");
    }

    #[test]
    fn hold_keeps_full_level() {
        let mut s = strip_layer();
        s.decay_sec = 45.0;
        s.sustain_level = 1.0;
        let p = s.strip_params();
        let mut v = StripVoice::default();
        let mut y = 0.0;
        for _ in 0..96_000 {
            y = v.advance(&p, true).0;
        }
        assert!(y > 0.999, "hold level {y}");
    }
}
