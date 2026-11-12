//! Checks of the float rounding model against the ADSP-21489 simulator measurement.
//!
//! VERIFIED: `docs/research/sharc-round/round_measured.csv` is the output of
//! `docs/research/sharc-round/round_probe.asm` run on the CCES 2.12.1 simulator for the 8
//! combinations of MODE1 TRUNC, RND32 and ALUSAT: `FIX`/`TRUNC` (with scale) with ASTAT, the
//! 40-bit extension of add, multiply and `FLOAT` (observed by subtracting a reference), and how
//! the extension survives moves, `PASS`, memory and `RND`. Procedure: `docs/research/sharc-float.md`.
use super::float::{self, Mode, MODE1_ALUSAT, MODE1_RND32, MODE1_TRUNC};
use super::float_conv as conv;
use super::float_misc as misc;

const CSV: &str = include_str!("../../../../docs/research/sharc-round/round_measured.csv");
const ONE: u64 = 0x3F80_0000 << 8;

fn hex(s: &str) -> u64 {
    u64::from_str_radix(s, 16).unwrap_or(0)
}

fn sub(a: u64, b: u64, mode: Mode) -> u32 {
    (float::add(a, b, true, false, mode).v >> 8) as u32
}

fn astat(o: &float::Out) -> u64 {
    u64::from(o.az) | u64::from(o.av) << 1 | u64::from(o.an) << 2 | u64::from(o.ai) << 5
}

fn same<const N: usize>(errs: &mut Vec<String>, got: [u64; N], want: &[u64], tag: &str) {
    if got[..] != want[..N] {
        errs.push(format!("{tag}: got {got:x?} want {:x?}", &want[..N]));
    }
}

fn check_row(errs: &mut Vec<String>, c: &[u64], kind: &str) {
    let mode = Mode::from_mode1(c[0] as u32);
    let base = Mode::from_mode1(c[0] as u32 & !(MODE1_TRUNC | MODE1_RND32 | MODE1_ALUSAT));
    let (a, b) = (c[1] << 8, c[2] << 8);
    let tag = format!("{kind} mode {:05x} a {:08x} b {:08x}", c[0], c[1], c[2]);
    match kind {
        "fix" => {
            let scale = c[2] as u32 as i32;
            let f = conv::to_int(a, scale, false, mode);
            let t = conv::to_int(a, scale, true, mode);
            let got = [
                u64::from(f.v as u32),
                astat(&f),
                u64::from(t.v as u32),
                astat(&t),
            ];
            same(errs, got, &c[3..], &tag);
        }
        "add" => {
            let f0 = float::add(a, b, false, false, mode).v;
            let one = ONE;
            let dropped = (f0 >> 8) << 8;
            let got = [
                f0 >> 8,
                u64::from(sub(f0, one, mode)),
                u64::from(sub(f0, one, mode)), // register move keeps the extension
                u64::from(sub(misc::sign_op(f0, 0, mode).v, one, mode)),
                u64::from(sub(dropped, one, mode)), // through memory: upper 32 bits only
                u64::from(sub(conv::rnd(f0, mode).v, one, mode)),
                u64::from(sub(float::add(a, b, false, false, base).v, one, mode)),
            ];
            same(errs, got, &c[3..], &tag);
        }
        "mul" => {
            let p = float::mul(a, b, mode).v;
            same(errs, [p >> 8, u64::from(sub(p, ONE, mode))], &c[3..], &tag);
        }
        _ => {
            let p = conv::from_int(c[1] as u32 as i32, 0, mode).v;
            let two30 = 0x4E80_0000u64 << 8;
            same(
                errs,
                [p >> 8, u64::from(sub(p, two30, mode))],
                &c[3..],
                &tag,
            );
        }
    }
}

#[test]
fn model_matches_every_measured_round_row() {
    let (mut n, mut errs) = (0, Vec::new());
    for line in CSV.lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        let c: Vec<u64> = f[1..].iter().map(|s| hex(s)).collect();
        check_row(&mut errs, &c, f[0]);
        n += 1;
    }
    assert!(
        errs.is_empty(),
        "{} mismatches:
{}",
        errs.len(),
        errs.join(
            "
"
        )
    );
    assert_eq!(n, 912);
}
