//! Firmware envelope record chain (opt-in, not wired into the voice render).
//!
//! The ORG core 2 pass runs one control step per pass on a chain of 4-word records
//! `[a, b, thr, link]` (`pc 0x1274EB..0x127AFF`, `docs/research/synth-adr-and-strip-law.md`):
//!
//! ```text
//! y' = y a + b
//! if y' < thr { y' = thr; if thr < 0 { y' = 0, restart } }
//! if y' > L   { y' = L; record = link }        (L = 1.0)
//! ```
//!
//! The ADR strip words become the attack `b` (with `a = 1`), the decay `a` (`b = 0`, `thr` = the
//! sustain level, 0.5 by default) and the release `a` (`b = 0`, `thr = 0`) through the rate tables
//! of `fw_env_rates.rs`. The arithmetic is the 40-bit register model (32-bit significand after each
//! operation, truncating store), no allocation.
//!
//! VERIFIED: the step reproduces the oracle words of the attack, decay-to-sustain, release and
//! negative-threshold scenarios bit for bit (tests `golden_*`, from `tools/synth_adr_oracle.py`).
//! EMPIRICAL: the start records selected by a controller trigger change. ASSUMED: one step per
//! pass and the pass length in samples.

/// Level limit `L` (`DM(F - 22)`).
pub const FW_ADR_LIMIT: f32 = 1.0;

/// One chain record.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FwAdrRecord {
    /// Multiplier of the level.
    pub a: f32,
    /// Addend per pass.
    pub b: f32,
    /// Lower threshold (the sustain level of a decay record).
    pub thr: f32,
}

/// What a step did besides updating the level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FwAdrAction {
    /// Stay on the record.
    None,
    /// The level passed the limit: continue with the record's `link`.
    Link,
    /// A negative threshold fired: the fast fade is done, restart on the attack record.
    Restart,
}

/// Rounds a finite `f64` to a 32-bit significand (round to nearest even), flushing denormals.
fn round32(x: f64) -> f64 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    let bits = x.to_bits();
    let drop = 52 - 31; // keep 31 fraction bits
    let half = 1u64 << (drop - 1);
    let rem = bits & ((1u64 << drop) - 1);
    let mut kept = bits & !((1u64 << drop) - 1);
    if rem > half || (rem == half && (kept >> drop) & 1 == 1) {
        kept += 1u64 << drop;
    }
    let r = f64::from_bits(kept);
    if r.abs() < f64::from_bits(0x3810_0000_0000_0000) {
        0.0 // below 2^-126: flushed
    } else {
        r
    }
}

/// Truncates a register value to the float32 a store writes (toward zero).
fn store_f32(x: f64) -> f32 {
    if x == 0.0 {
        return 0.0;
    }
    let b = x.to_bits() & !((1u64 << 29) - 1);
    let t = f64::from_bits(b);
    if t.abs() < f32::MIN_POSITIVE as f64 {
        0.0
    } else {
        t as f32
    }
}

/// One control pass on the level `y` with the current record; returns the stored level and what
/// happened.
pub fn fw_adr_step(y: f32, rec: &FwAdrRecord) -> (f32, FwAdrAction) {
    let mut out = round32(round32(f64::from(y) * f64::from(rec.a)) + f64::from(rec.b));
    let thr = f64::from(rec.thr);
    let mut action = FwAdrAction::None;
    if out < thr {
        out = thr;
        if thr < 0.0 {
            out = 0.0;
            action = FwAdrAction::Restart;
        }
    }
    if out > f64::from(FW_ADR_LIMIT) {
        out = f64::from(FW_ADR_LIMIT);
        action = FwAdrAction::Link;
    }
    (store_f32(out), action)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ATTACK: FwAdrRecord = FwAdrRecord {
        a: 1.0,
        b: 0.1,
        thr: 0.0,
    };
    const DECAY: FwAdrRecord = FwAdrRecord {
        a: 0.8,
        b: 0.0,
        thr: 0.5,
    };
    const RELEASE: FwAdrRecord = FwAdrRecord {
        a: 0.9,
        b: 0.0,
        thr: 0.0,
    };
    const FAST: FwAdrRecord = FwAdrRecord {
        a: 0.5,
        b: -0.03,
        thr: -0.02,
    };

    // Oracle words of passes 1..15 (`tools/tests/test_synth_adr_model.py`).
    const A_Y: [u32; 15] = [
        0x3DCCCCCD, 0x3E4CCCCD, 0x3E999999, 0x3ECCCCCC, 0x3EFFFFFF, 0x3F199999, 0x3F333332,
        0x3F4CCCCB, 0x3F666664, 0x3F7FFFFD, 0x3F800000, 0x3F4CCCCD, 0x3F23D70A, 0x3F03126E,
        0x3F000000,
    ];
    const B_Y: [u32; 8] = [
        0x3F2147AC, 0x3F1126E7, 0x3F02A302, 0x3EEB2569, 0x3ED3A1AA, 0x3EBE77E5, 0x3EAB6BE7,
        0x3E9A4782,
    ];
    const D_Y: [u32; 4] = [0x3E6147AD, 0x3DA3D709, 0x3C23D706, 0x0000_0000];

    fn run(start_rec: [&FwAdrRecord; 2], n_attack: usize) -> Vec<u32> {
        // attack for `n_attack` passes then the second record, with Link handled by the caller
        let mut y = 0.0f32;
        let mut out = Vec::new();
        let mut rec = start_rec[0];
        for p in 0..n_attack + 20 {
            if p == n_attack {
                rec = start_rec[1];
            }
            let (ny, act) = fw_adr_step(y, rec);
            y = ny;
            out.push(y.to_bits());
            if act == FwAdrAction::Link {
                rec = &DECAY;
            }
        }
        out
    }

    #[test]
    fn golden_attack_decay_sustain() {
        let out = run([&ATTACK, &ATTACK], 100);
        assert_eq!(&out[..15], &A_Y[..]);
        // sustain: the level stays at the decay threshold
        assert_eq!(f32::from_bits(out[40]), 0.5);
    }

    #[test]
    fn golden_release_from_attack_pass_7() {
        let out = run([&ATTACK, &RELEASE], 7);
        assert_eq!(&out[7..15], &B_Y[..]);
    }

    #[test]
    fn golden_negative_threshold_restarts() {
        let mut y = f32::from_bits(0x3EFF_FFFF);
        let mut got = Vec::new();
        for _ in 0..4 {
            let (ny, act) = fw_adr_step(y, &FAST);
            y = ny;
            got.push(y.to_bits());
            if act == FwAdrAction::Restart {
                break;
            }
        }
        assert_eq!(got, D_Y.to_vec());
        assert_eq!(fw_adr_step(0.0, &FAST).1, FwAdrAction::Restart);
    }

    #[test]
    fn clamp_and_sustain_floor() {
        let (y, act) = fw_adr_step(0.95, &ATTACK);
        assert_eq!((y, act), (1.0, FwAdrAction::Link));
        let (y, act) = fw_adr_step(0.55, &DECAY);
        assert_eq!((y, act), (0.5, FwAdrAction::None));
    }

    #[test]
    fn round32_keeps_a_32_bit_significand() {
        // 1 + 2^-31 survives, 1 + 2^-33 rounds away
        assert_eq!(round32(1.0 + 2f64.powi(-31)), 1.0 + 2f64.powi(-31));
        assert_eq!(round32(1.0 + 2f64.powi(-33)), 1.0);
        assert_eq!(round32(0.0), 0.0);
        assert_eq!(store_f32(1.0 + 2f64.powi(-30)), 1.0);
    }
}
