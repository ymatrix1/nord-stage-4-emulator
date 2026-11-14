//! The Leslie port against `sharc-fx` oracle output (`tools/fx_leslie_golden.py`).

use super::fw_ext40::E40;
use super::fw_leslie::{FwLeslieE40, SLOW_SPEED_WORDS};
use super::fw_leslie_ring::RingAdvance;
use super::tests_fw_leslie_golden_data as g;

/// FNV-1a 64 over the little-endian bytes of the words (as `tools/fx_leslie_words.fnv`).
fn fnv(words: &[u32]) -> u64 {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for w in words {
        for b in w.to_le_bytes() {
            h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01B3);
        }
    }
    h
}

/// The integer LCG noise of `tools/fx_leslie_run.py::lcg_words`, exact in `f32`.
fn lcg(calls: usize) -> Vec<[f32; 4]> {
    let mut s: u32 = 12345;
    (0..calls)
        .map(|_| {
            let mut row = [0.0f32; 4];
            for v in &mut row {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                *v = ((s >> 8) as i32 - 8_388_608) as f32 / 33_554_432.0;
            }
            row
        })
        .collect()
}

fn run(mult: u32, ring: RingAdvance) -> Vec<u32> {
    let mut l = FwLeslieE40::with_ring(ring);
    l.set_speed_words(SLOW_SPEED_WORDS.map(|w| w * mult));
    lcg(g::CALLS)
        .into_iter()
        .flat_map(|row| l.process_block(row).map(f32::to_bits))
        .collect()
}

#[test]
fn slow_speed_words_match_the_oracle_for_2000_calls() {
    let got = run(g::SLOW_MULT, RingAdvance::FROZEN);
    assert_eq!(got[..g::SLOW_FIRST.len()], g::SLOW_FIRST);
    assert_eq!(fnv(&got), g::SLOW_FNV);
}

#[test]
fn speed_words_times_twelve_match_the_oracle_through_several_rotor_wraps() {
    let got = run(g::X12_MULT, RingAdvance::FROZEN);
    assert_eq!(got[..g::X12_FIRST.len()], g::X12_FIRST);
    assert_eq!(fnv(&got), g::X12_FNV);
}

#[test]
fn the_pass_routine_ring_matches_the_oracle_for_2000_calls() {
    let got = run(g::RESET8_MULT, RingAdvance::PASS_RESET8);
    assert_eq!(got[..g::RESET8_FIRST.len()], g::RESET8_FIRST);
    assert_eq!(fnv(&got), g::RESET8_FNV);
}

#[test]
fn a_plain_four_word_advance_matches_the_oracle_for_2000_calls() {
    let got = run(g::ADV_MULT, RingAdvance::ADVANCE);
    assert_eq!(got[..g::ADV_FIRST.len()], g::ADV_FIRST);
    assert_eq!(fnv(&got), g::ADV_FNV);
}

#[test]
fn the_default_processor_uses_the_plain_advance() {
    let (mut a, mut b) = (
        FwLeslieE40::new(),
        FwLeslieE40::with_ring(RingAdvance::ADVANCE),
    );
    for row in lcg(50) {
        assert_eq!(a.process_block(row), b.process_block(row));
    }
}

#[test]
fn a_call_runs_1886_instructions_and_stays_inside_the_windows() {
    use super::fw_leslie_callee::STATIC_STEPS_CALLEE;
    use super::fw_leslie_core::Core;
    use super::fw_leslie_main_a::MAIN_STEPS;
    let mut c = Core::<f32>::new();
    c.call([0.1, -0.2, 0.3, 0.05]);
    assert_eq!(c.ticks(), 1886, "1885 traced steps plus the final rts");
    assert!(c.ticks() >= MAIN_STEPS + STATIC_STEPS_CALLEE);
    assert_eq!(c.stray(), 0, "no store outside the committed windows");
}

#[test]
fn the_frame_interface_delays_by_one_frame() {
    let mut a = FwLeslieE40::new();
    let mut b = FwLeslieE40::new();
    let rows = lcg(4);
    let blocks: Vec<[f32; 4]> = rows.iter().map(|r| a.process_block(*r)).collect();
    let mut frames = Vec::new();
    for r in &rows {
        frames.push(b.process(r[0], r[1]));
        frames.push(b.process(r[2], r[3]));
    }
    assert_eq!(frames[0], [0.0, 0.0]);
    assert_eq!(frames[1], [blocks[0][0], blocks[0][1]]);
    assert_eq!(frames[2], [blocks[0][2], blocks[0][3]]);
    assert_eq!(frames[3], [blocks[1][0], blocks[1][1]]);
}

/// The ARM fast speed words (`tools/arm_leslie_speeds.py`: horn 7.0 Hz, rotor 6.4 Hz, follow word
/// 0.5748 x horn): the rotor phase `DM(0xB2F23)` gains the rotor word per call (VERIFIED by the
/// listing `0x1256B8..0x1256D5` and by the x12 golden above, which covers the same word range).
#[test]
fn the_arm_fast_speed_words_advance_the_rotor_phase_by_the_word_per_call() {
    use super::fw_leslie_core::Core;
    let word = |hz: f64| ((hz * 8_388_607.0 / 11_025.0) as u32) << 8;
    let fast = [word(6.4), word(7.0), word(0.5748 * 7.0)];
    assert_eq!(fast[0], 0x130500);
    assert_eq!(fast[1], 0x14CE00);
    let mut c = Core::<E40>::new();
    c.set_speed_words(fast);
    let p0 = c.rd(0xB2F23);
    for row in lcg(100) {
        c.call(row);
    }
    assert_eq!(c.rd(0xB2F23), p0.wrapping_add(100 * fast[0]));
}
