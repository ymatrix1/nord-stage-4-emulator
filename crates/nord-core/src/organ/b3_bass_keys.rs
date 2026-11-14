//! Key-down weights of the B3 Bass processor `0x122296` (model 5), decoded for every key.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.9. Research: `docs/research/organ-farfisa-bass.md`
//! s10. The ARM key-down of `Edt_Org_kModel = 5` (`tools/organ_bass_keys.py`, the real firmware code
//! in the emulator, drawbars `888888888`, velocity 100) writes the bass weight of key `n` into
//! window 2 of the processor, `DM 0x95241 + (n - 24)`, which multiplies the generator ring note `n`
//! (slot `0xB32F4 + n - 24`); the drawbar item writes window 1, which is inaudible in the oracle
//! (zeroing it leaves the chord output bit-identical, `tools/organ_bass_proc.py chain --zero 1`).
//! Only the weight table lives here: the bass output also depends on per-key lattice words the
//! key-down writes (`DM 0x954E6..0x95529`), so the stage is **not wired** (section 10 of the research).

/// First key with a window 2 weight.
#[allow(dead_code)] // not wired yet (research s10)
pub(super) const FIRST_KEY: usize = 24;

/// IEEE bits of the weight of keys `24..=91` (index `n - 24`).
///
/// VERIFIED: first value written to `DM 0x95241 + (n - 24)` by the ARM key-down of every key
/// (`docs/research/data/bass-keydown.csv`, test `table_matches_the_derived_csv`). Keys 92..=108 write
/// no word inside the 68-word window (key 103 folds onto index 67, not in this table).
#[allow(dead_code)] // not wired yet (research s10)
pub(super) const KEY_WEIGHT_BITS: [u32; 68] = [
    0x3c14d400, 0x3c14d400, 0x3c14d400, 0x3c14d400, 0x3c14d400, 0x3c14d400, 0x3c14d400, 0x3c14d400,
    0x3c14d400, 0x3c14d400, 0x3c14d400, 0x3c14d400, 0x3c14d400, 0x3c101d80, 0x3c101d80, 0x3c0ce400,
    0x3c0ce400, 0x3c17ce00, 0x3c14d400, 0x3c28eb80, 0x3c25b200, 0x3c28eb80, 0x3c2be580, 0x3c2be580,
    0x3c355280, 0x3c7c8300, 0x3c7c8300, 0x3c82f800, 0x3c847500, 0x3c855340, 0x3c815b40, 0x3c8aa880,
    0x3c83b680, 0x3c815b40, 0x3c855340, 0x3c809cc0, 0x3c798900, 0x3c701c00, 0x3c71d880, 0x3c63b500,
    0x3c7e3f80, 0x3c701c00, 0x3c878ec0, 0x3c6e9f00, 0x3c798900, 0x3c71d880, 0x3c71d880, 0x3c74d280,
    0x3c764f80, 0x3c780c00, 0x3c71d880, 0x3c355280, 0x3c28eb80, 0x3c496a00, 0x3c36cf80, 0x3c3a0900,
    0x3c384c80, 0x3c33d580, 0x3c355280, 0x3c384c80, 0x3c3e8000, 0x3c321900, 0x3c3e8000, 0x3c309c00,
    0x3c309c00, 0x3c2da200, 0x3c433680, 0x3c5d8180,
];

/// Weight of key `key` (0.0 outside `24..=91`).
#[allow(dead_code)] // not wired yet (research s10)
pub(super) fn bass_key_weight(key: i32) -> f32 {
    usize::try_from(key - FIRST_KEY as i32)
        .ok()
        .and_then(|i| KEY_WEIGHT_BITS.get(i))
        .map_or(0.0, |&b| f32::from_bits(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_matches_the_derived_csv() {
        let csv = include_str!("../../../../docs/research/data/bass-keydown.csv");
        let mut seen = [false; 68];
        for line in csv.lines().skip(1) {
            let f: Vec<&str> = line.split(',').collect();
            let (key, window, index) = (
                f[0].parse::<usize>().unwrap(),
                f[1].parse::<usize>().unwrap(),
                f[2].parse::<usize>().unwrap(),
            );
            if window == 2 && index + FIRST_KEY == key && key < 92 && !seen[index] {
                seen[index] = true;
                let bits = u32::from_str_radix(&f[3][2..], 16).unwrap();
                assert_eq!(KEY_WEIGHT_BITS[index], bits, "key {key}");
            }
        }
        assert!(seen.iter().all(|&s| s));
    }

    #[test]
    fn chord_keys_match_the_oracle_script_words() {
        // keys 60, 64, 67 of the replayed chord: 0x3c798900, 0x3c7e3f80, 0x3c6e9f00
        for (key, bits) in [(60, 0x3c79_8900u32), (64, 0x3c7e_3f80), (67, 0x3c6e_9f00)] {
            assert_eq!(bass_key_weight(key).to_bits(), bits);
        }
        assert!((bass_key_weight(60) - 0.015_23).abs() < 1e-4);
    }

    #[test]
    fn weight_is_zero_outside_the_window_and_positive_inside() {
        assert_eq!(bass_key_weight(23), 0.0);
        assert_eq!(bass_key_weight(92), 0.0);
        assert!((24..92).all(|k| bass_key_weight(k) > 0.005 && bass_key_weight(k) < 0.03));
    }
}
