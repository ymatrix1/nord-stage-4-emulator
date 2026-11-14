//! Fingerprints of the committed Leslie data windows (`tools/fx_leslie_words.py` prints them).

use super::fw_leslie_data_a as a;
use super::fw_leslie_data_b as b;
use super::fw_leslie_data_c as c;

fn fnv(words: &[u32]) -> u64 {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for w in words {
        for byte in w.to_le_bytes() {
            h = (h ^ u64::from(byte)).wrapping_mul(0x0100_0000_01B3);
        }
    }
    h
}

/// FNV-1a 64 of windows 0..16 in order (`fx_leslie_words.py` output, VERIFIED on OS v1.66).
#[rustfmt::skip]
const FNV: [u64; 16] = [
    0xb9b23f3a46fd0825, 0xe8dcf635e9892f75, 0xb4b9f844bf11bfcb, 0x0243cfa845185aa5,
    0x4c19ae5279a9501e, 0xb5a2df74dd56f524, 0x0149abc4f5a0ea24, 0x70611a2ad446b571,
    0xe32492ecf8ef6164, 0xd974ed96b9cd4f2b, 0x88201fb960ff6465, 0x88201fb960ff6465,
    0xcfd85f1700474095, 0xe8dd3076bfc57b65, 0x88201fb960ff6465, 0x0abbf48dcaa9f3ce,
];

#[test]
fn the_data_windows_match_their_fingerprints() {
    let all: Vec<&(u32, &[u32])> = a::WINDOWS
        .iter()
        .chain(b::WINDOWS.iter())
        .chain(c::WINDOWS.iter())
        .collect();
    assert_eq!(all.len(), FNV.len());
    for (k, (w, want)) in all.iter().zip(FNV).enumerate() {
        assert_eq!(fnv(w.1), want, "window {k} at {:#x}", w.0);
    }
    let words: usize = all.iter().map(|w| w.1.len()).sum();
    assert_eq!(words, 3810);
}

#[test]
fn the_entry_files_and_slow_speed_words_are_present() {
    assert_eq!(a::SLOW_SPEED, [0x22b00, 0x26000, 0x15d00]);
    assert_eq!(a::ENTRY_I.len(), 16);
    assert!(a::ENTRY_M.iter().any(|&m| m != 0));
}
