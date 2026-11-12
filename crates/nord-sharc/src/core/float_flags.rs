//! Checks of the float ALU/multiplier special values, flags and residuals against the
//! ADSP-21489 simulator measurement.
//!
//! VERIFIED: `docs/research/sharc-flags/flags_measured.csv` is the output of
//! `docs/research/sharc-flags/flags_probe_m*.asm` on the CCES 2.12.1 simulator for MODE1 TRUNC,
//! RND32 and ALUSAT each set alone and all clear: every pair of 16 special values through add,
//! subtract, multiply, average, MIN, MAX, COPYSIGN, CLIP and COMP, the unary operations on the
//! special values, 256 random operand pairs with the 40-bit residual, and S-register swaps.
//! ASTAT is compared on AZ, AV, AN, AS and AI (`MODELLED`); the multiplier flags, AC and the
//! sticky register are measured but not modelled.
use super::float::{self, Mode, Out};
use super::float_conv as conv;
use super::float_misc as misc;

const CSV: &str = include_str!("../../../../docs/research/sharc-flags/flags_measured.csv");
/// AZ, AV, AN, AS, AI in ASTAT.
const MODELLED: u64 = 0x37;

fn hex(s: &str) -> Option<u64> {
    u64::from_str_radix(s, 16).ok()
}

fn bits(o: &Out) -> u64 {
    u64::from(o.az)
        | u64::from(o.av) << 1
        | u64::from(o.an) << 2
        | u64::from(o.as_) << 4
        | u64::from(o.ai) << 5
}

/// The model's result and flags for `op` on the 40-bit operands `a`, `b`.
fn model(op: &str, a: u64, b: u64, n: i32, mode: Mode) -> Out {
    match op {
        "add" => float::add(a, b, false, false, mode),
        "sub" => float::add(a, b, true, false, mode),
        "avg" => float::add(a, b, false, true, mode),
        "mul" => float::mul(a, b, mode),
        "copysign" => misc::select_op(a, b, 0, mode),
        "min" => misc::select_op(a, b, 1, mode),
        "max" => misc::select_op(a, b, 2, mode),
        "clip" => misc::select_op(a, b, 3, mode),
        "comp" => misc::compare(a, b, mode),
        "pass" => misc::sign_op(a, 0, mode),
        "neg" => misc::sign_op(a, 1, mode),
        "abs" => misc::sign_op(a, 2, mode),
        "mant" => misc::mant(a, mode),
        "logb" => misc::logb(a, mode),
        "rnd" => conv::rnd(a, mode),
        _ => conv::scalb(a, n, mode),
    }
}

#[test]
fn model_matches_every_measured_flag_row() {
    let (mut errs, mut rows) = (Vec::<String>::new(), 0);
    for line in CSV.lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        if f[0] == "swap" {
            continue;
        }
        rows += 1;
        let (m1, op) = (hex(f[1]).unwrap_or(0) as u32, f[2]);
        let mode = Mode::from_mode1(m1);
        let (a, b) = (hex(f[3]).unwrap_or(0), hex(f[4]).unwrap_or(0));
        let n = if op == "scalb" { b as u32 as i32 } else { 0 };
        let (sa, sb) = (a << 8, if op == "scalb" { 0 } else { b << 8 });
        let o = model(op, sa, sb, n, mode);
        let mult =
            u64::from(o.mn) | u64::from(o.mv) << 1 | u64::from(o.mu) << 2 | u64::from(o.mi) << 3;
        let int_result = op == "mant" || op == "logb";
        let got_v = if op == "comp" {
            0
        } else if int_result {
            o.v & 0xFFFF_FFFF
        } else {
            o.v >> 8
        };
        let want_v = hex(f[5]).unwrap_or(0);
        let astat = hex(f[7]).unwrap_or(0);
        // the multiplier leaves the ALU flags alone and the ALU leaves MN..MI alone
        let (got_s, want_s) = if op == "mul" {
            (mult, astat >> 6 & 0xF)
        } else {
            (bits(&o), astat & MODELLED)
        };
        // sticky bits: AUS AVS AIS for the ALU, MVS MUS MIS for the multiplier
        let (got_t, want_t) = if op == "mul" {
            (
                u64::from(o.mv) << 7 | u64::from(o.mu) << 8 | u64::from(o.mi) << 9,
                hex(f[8]).unwrap_or(0) & 0x380,
            )
        } else {
            (
                u64::from(o.au) | u64::from(o.av) << 1 | u64::from(o.ai) << 5,
                hex(f[8]).unwrap_or(0) & 0x23,
            )
        };
        let mut bad = got_v != want_v || got_s != want_s || got_t != want_t;
        let mut got_r = None;
        if f[0] == "rnd" {
            let r = conv::rnd(o.v, mode).v;
            let resid = float::add(o.v, r, true, false, mode).v >> 8;
            got_r = Some(resid);
            bad |= resid != hex(f[6]).unwrap_or(0);
        }
        if bad {
            errs.push(format!(
                "{} {} mode {m1:05x} a {a:08x} b {b:08x}: got {got_v:08x}/{got_s:02x}/{got_t:x}/{got_r:x?} want {want_v:08x}/{want_s:02x}/{want_t:x}/{}",
                f[0], op, f[6]
            ));
        }
    }
    let _ = std::fs::write(
        "C:/tmp/flag_errs.txt",
        errs.join(
            "
",
        ),
    );
    let mut by = std::collections::BTreeMap::<String, usize>::new();
    for e in &errs {
        *by.entry(e.split(' ').nth(1).unwrap_or("").to_string())
            .or_default() += 1;
    }
    let shown: Vec<String> = by
        .iter()
        .map(|(k, v)| format!("{k}:{v}"))
        .chain(errs.iter().take(0).cloned())
        .collect();
    assert!(
        errs.is_empty(),
        "{} of {rows} rows differ:\n{}",
        errs.len(),
        shown
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}
