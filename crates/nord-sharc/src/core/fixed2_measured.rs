//! Second set of checks against the ADSP-21489 simulator: shifter OR forms (with R0 preset),
//! `FIX`/`TRUNC` underflow and sticky bits, the background accumulator MRB with the
//! integer-rounding multiplier forms, and the MODE1 SRCU swap.
//!
//! VERIFIED: `docs/research/sharc-fixed/fixed2_measured.csv`, produced by `gen_fixed_probe2.py`
//! (builds `or`, `fix0`/`fix1`, `mrb`, `srcu`) on the CCES 2.12.1 simulator; every row is
//! executed as the assembled compute word like in `fixed_measured.rs`.
use super::fixed_measured::{hex, mr_value, ASTAT_MASK, STKY_MASK};
use super::Core;

const CSV: &str = include_str!("../../../../docs/research/sharc-fixed/fixed2_measured.csv");
const SRCU: u32 = 1 << 2;
const MODE1: u8 = 0x72;

fn row(f: &[&str]) -> Option<([u64; 9], [u64; 9])> {
    let mut c = Core::new(&[]);
    c.mode1 = hex(f[1])? as u32;
    let word = hex(f[4])?;
    let (name, x) = (f[2], hex(f[5])?);
    c.r[1] = x as u32;
    c.r[2] = hex(f[6]).unwrap_or(0) as u32;
    let p = [
        hex(f[7]).unwrap_or(0),
        hex(f[8]).unwrap_or(0),
        hex(f[9]).unwrap_or(0),
    ];
    let q = [
        hex(f[10]).unwrap_or(0),
        hex(f[11]).unwrap_or(0),
        hex(f[12]).unwrap_or(0),
    ];
    let presets = |c: &mut Core| {
        c.mr[0] = mr_value(p);
        c.mr[1] = mr_value(q);
    };
    if name.starts_with("or") {
        c.r[0] = p[0] as u32;
    } else if f[0] == "srcu" && name == "srcu_a" {
        presets(&mut c);
        c.write_ureg(MODE1, c.mode1 | SRCU);
    } else if f[0] == "srcu" {
        c.write_ureg(MODE1, c.mode1 | SRCU);
        presets(&mut c);
    } else if f[0] == "mrb" {
        presets(&mut c);
    }
    if !(if f[3] == "1" {
        c.short_compute(word)
    } else {
        c.compute(word)
    }) {
        return None;
    }
    let part = |v: i128| {
        [
            (v as u64) & 0xFFFF_FFFF,
            (v >> 32) as u64 & 0xFFFF_FFFF,
            (v >> 64) as i16 as i32 as u32 as u64,
        ]
    };
    let (m, b) = (part(c.mr[0]), part(c.mr[1]));
    let got = [
        u64::from(c.r[0]),
        m[0],
        m[1],
        m[2],
        b[0],
        b[1],
        b[2],
        u64::from(c.astat(0)) & ASTAT_MASK,
        u64::from(c.sys.stky[0]) & STKY_MASK,
    ];
    let mut want = [0u64; 9];
    for (i, w) in want.iter_mut().enumerate() {
        *w = hex(f[13 + i]).unwrap_or(0);
    }
    (want[7], want[8]) = (want[7] & ASTAT_MASK, want[8] & STKY_MASK);
    Some((got, want))
}

#[test]
fn model_matches_every_measured_row_of_the_second_probe_set() {
    let (mut errs, mut rows) = (Vec::<String>::new(), 0);
    for line in CSV.lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        rows += 1;
        let uses_mr = f[0] == "mrb" || f[0] == "srcu";
        match row(&f) {
            Some((mut got, mut want)) => {
                if !uses_mr {
                    for i in 1..7 {
                        (got[i], want[i]) = (0, 0);
                    }
                }
                if got != want {
                    errs.push(format!(
                        "{} {} x {} y {} q {}: got {got:x?} want {want:x?}",
                        f[0],
                        f[2],
                        f[5],
                        f[6],
                        f[10..13].join(":")
                    ));
                }
            }
            None => errs.push(format!("{} {}: not executed", f[0], f[2])),
        }
    }
    let mut by = std::collections::BTreeMap::<String, usize>::new();
    for e in &errs {
        *by.entry(e.split(' ').take(2).collect::<Vec<_>>().join(" "))
            .or_default() += 1;
    }
    assert!(
        errs.is_empty(),
        "{} of {rows} rows differ: {}",
        errs.len(),
        by.iter()
            .map(|(k, v)| format!("{k}:{v}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
}
