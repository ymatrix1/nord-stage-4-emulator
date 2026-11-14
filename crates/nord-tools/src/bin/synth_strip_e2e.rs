//! Reference values of the firmware strip chain for `tools/synth_strip_e2e.py` (oracle check).
//!
//! Usage: `cargo run --release -p nord-tools --bin synth_strip_e2e > strip_e2e.json`.
//! Prints, for the Smooth Lead panel values (cutoff 370 Hz, resonance 0.516, Triangle -> saw),
//! the strip word, `R`, the table position word, the records of a short envelope, the Rust level
//! after every envelope step (attack, decay to a sustain level, release) and the Rust output of
//! the strip filter over a firmware saw. The script feeds the same words to the SHARC oracle.

use nord_core::synth::{
    firmware_saw, fw_adr_step, fw_strip_cutoff_r, strip_word_for_cutoff, FwAdrAction, FwCoefs,
    FwFilter, FwFilterType, StripEnvRecords, TablePos, SY_FRAME_RATE_HZ,
};
use serde_json::json;

const STEPS: usize = 15;
const RELEASE_AT: usize = 8;
const SAMPLES: usize = 192;

/// Level after each pass of the strip envelope records; `release_at` starts the release record.
fn levels(recs: &StripEnvRecords, release_at: Option<usize>) -> Vec<u32> {
    let (mut y, mut decay) = (0.0f32, false);
    let mut out = Vec::with_capacity(STEPS);
    for step in 1..=STEPS {
        let rec = if release_at.is_some_and(|r| step >= r) {
            &recs.release
        } else if decay {
            &recs.decay
        } else {
            &recs.attack
        };
        let (ny, action) = fw_adr_step(y, rec);
        y = ny;
        if action == FwAdrAction::Link && !decay {
            decay = true;
        }
        out.push(y.to_bits());
    }
    out
}

fn main() {
    // Envelope at the fast end of the tables (attack 0.5 ms = index -64, decay and release 3 ms):
    // the pass installer rewrites the oracle records every 16th pass, so only 15 passes can be
    // compared with records poked from outside. Sustain 0.5 is the firmware default.
    let recs = StripEnvRecords::from_panel(0.0005, 0.003, 0.003, 0.5);
    let levels_a = levels(&recs, None);
    let levels_b = levels(&recs, Some(RELEASE_AT));

    // Filter: cutoff 370 Hz through the strip law, resonance 0.516 on the table position.
    let word = strip_word_for_cutoff(370.0);
    let r = fw_strip_cutoff_r(word);
    let pos = TablePos::from_resonance(0.516);
    let w28 = ((pos.idx as u32) << 24) + (pos.fr * 16_777_216.0).round() as u32;
    let co = FwCoefs::new(FwFilterType::Lp4, r, pos);
    let mut f = FwFilter::default();
    let inc = 220.0 / SY_FRAME_RATE_HZ;
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    let mut phase = 0.0f32;
    for _ in 0..SAMPLES {
        let x = 0.25 * firmware_saw(phase, inc);
        xs.push(x.to_bits());
        ys.push(f.process(x, &co, FwFilterType::Lp4).to_bits());
        phase = (phase + inc) % 1.0;
    }

    let rec =
        |r: &nord_core::synth::FwAdrRecord| json!([r.a.to_bits(), r.b.to_bits(), r.thr.to_bits()]);
    let out = json!({
        "records": {"attack": rec(&recs.attack), "decay": rec(&recs.decay), "release": rec(&recs.release)},
        "release_at": RELEASE_AT,
        "levels_a": levels_a,
        "levels_b": levels_b,
        "cutoff_word": word,
        "r_bits": r.to_bits(),
        "w28": w28,
        "saw": xs,
        "filtered": ys,
    });
    println!("{out}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_gives_one_finite_level_per_step_and_release_changes_the_tail() {
        let recs = StripEnvRecords::from_panel(0.0005, 0.003, 0.003, 0.5);
        let a = levels(&recs, None);
        let b = levels(&recs, Some(RELEASE_AT));
        assert_eq!(a.len(), STEPS);
        assert!(a.iter().all(|&v| f32::from_bits(v).is_finite()));
        assert_eq!(a[..RELEASE_AT - 1], b[..RELEASE_AT - 1]);
        assert_ne!(a, b);
    }
}
