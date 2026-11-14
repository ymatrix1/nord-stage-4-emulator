//! One-entry memo of the firmware filter prologue ([`FwCoefs::from_params`]) per voice.
//!
//! The prologue (`tan`, `sqrt`, two table interpolations) is a pure function of its four inputs,
//! and a voice's cutoff, resonance and sample rate rarely change from one sample to the next, so
//! the voice reuses the last result when the inputs are bit-identical. The output is therefore
//! bit-identical to recomputing it every sample (perf task TD-14).

use super::{FwCoefs, FwFilterType};

/// Inputs of the last computation, as bit patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Key {
    kind: FwFilterType,
    cutoff: u32,
    resonance: u32,
    sample_rate: u32,
}

/// Last [`FwCoefs`] of a voice with the inputs that produced them. `Copy`, no heap.
#[derive(Debug, Clone, Copy, Default)]
pub struct FwCoefMemo {
    last: Option<(Key, FwCoefs)>,
}

impl FwCoefMemo {
    /// [`FwCoefs::from_params`] for these inputs, recomputed only when an input changed.
    #[inline]
    pub fn get(
        &mut self,
        kind: FwFilterType,
        cutoff_hz: f32,
        resonance: f32,
        sample_rate: f32,
    ) -> FwCoefs {
        let key = Key {
            kind,
            cutoff: cutoff_hz.to_bits(),
            resonance: resonance.to_bits(),
            sample_rate: sample_rate.to_bits(),
        };
        if let Some((k, c)) = &self.last {
            if *k == key {
                return *c;
            }
        }
        let c = FwCoefs::from_params(kind, cutoff_hz, resonance, sample_rate);
        self.last = Some((key, c));
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(a: &FwCoefs, b: &FwCoefs) -> bool {
        a.cin.to_bits() == b.cin.to_bits()
            && a.g.to_bits() == b.g.to_bits()
            && a.k.to_bits() == b.k.to_bits()
            && a.h.to_bits() == b.h.to_bits()
    }

    #[test]
    fn memo_equals_a_fresh_computation_bit_for_bit() {
        let mut m = FwCoefMemo::default();
        for (kind, fc, res) in [
            (FwFilterType::Lp4, 1000.0_f32, 0.3_f32),
            (FwFilterType::Lp4, 1000.0, 0.3),
            (FwFilterType::Hp4, 1000.0, 0.3),
            (FwFilterType::Bp4, 440.0, 0.9),
            (FwFilterType::Lp4, 1000.0001, 0.3),
            (FwFilterType::Lp2, 30.0, 0.0),
            (FwFilterType::Lp4, f32::NAN, 0.5),
        ] {
            let got = m.get(kind, fc, res, 48_000.0);
            let want = FwCoefs::from_params(kind, fc, res, 48_000.0);
            assert!(same(&got, &want), "{kind:?} {fc} {res}");
        }
    }

    #[test]
    fn a_changed_sample_rate_is_not_served_from_the_memo() {
        let mut m = FwCoefMemo::default();
        let a = m.get(FwFilterType::Lp4, 2000.0, 0.2, 48_000.0);
        let b = m.get(FwFilterType::Lp4, 2000.0, 0.2, 44_100.0);
        assert!(!same(&a, &b));
        assert!(same(
            &b,
            &FwCoefs::from_params(FwFilterType::Lp4, 2000.0, 0.2, 44_100.0)
        ));
    }
}
