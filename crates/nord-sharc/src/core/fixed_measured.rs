//! Checks of the fixed-point ALU, multiplier/MR and shifter against the ADSP-21489 simulator.
//!
//! VERIFIED: `docs/research/sharc-fixed/fixed_measured.csv` holds the output of the probes in
//! `docs/research/sharc-fixed/` (builds `alu0`/`alu1` with ALUSAT clear/set, `mul0..mul2`,
//! `shift`) run on the CCES 2.12.1 simulator. Each row is one instruction: its compute word as
//! assembled by ADI's `easm21k` (read back with `elfdump`), the operands in R1/R2, the
//! pre-state (carry-in, accumulator preset) and the observed R0, MR0..MR2, ASTAT and STKY.
//! The test executes the same word on a [`Core`] and compares.
use super::Core;

const CSV: &str = include_str!("../../../../docs/research/sharc-fixed/fixed_measured.csv");

/// ASTAT bits compared: AZ AV AN AC AS AI, MN MV MU MI, SV SZ SS (AF is ignored) and CACC.
pub(super) const ASTAT_MASK: u64 = 0xFF00_3BFF;
/// STKY bits compared: AUS AVS AOS, AIS MOS MVS MUS MIS.
pub(super) const STKY_MASK: u64 = 0x3E7;

pub(super) fn hex(s: &str) -> Option<u64> {
    u64::from_str_radix(s, 16).ok()
}

pub(super) fn mr_value(p: [u64; 3]) -> i128 {
    let raw = i128::from(p[0] as u16) << 64 | i128::from(p[1]) << 32 | i128::from(p[2]);
    (raw << 48) >> 48
}

/// Runs one CSV row on a fresh core and returns the observed (r0, mr0, mr1, mr2, astat, stky).
fn run(f: &[&str], prev_mr: [u64; 3]) -> Option<[u64; 6]> {
    let mut c = Core::new(&[]);
    c.mode1 = hex(f[1])? as u32;
    let word = hex(f[4])?;
    let (x, y) = (hex(f[5])?, hex(f[6]).unwrap_or(0));
    let p = [
        hex(f[7]).unwrap_or(0),
        hex(f[8]).unwrap_or(0),
        hex(f[9]).unwrap_or(0),
    ];
    let name = f[2];
    c.r[1] = x as u32;
    c.r[2] = y as u32;
    if name.starts_with("or") {
        c.r[0] = p[0] as u32;
    }
    if name.starts_with("addci") || name.starts_with("subci") {
        c.flags.ac = p[0] != 0;
    }
    if name.starts_with("rmul") {
        // the Rn forms leave MRF alone: it still holds the product of the preceding `mul` row
        c.mr[0] = mr_value([prev_mr[2], prev_mr[1], prev_mr[0]]);
    }
    if name.starts_with("mac")
        || name.starts_with("msub")
        || name.starts_with("rmac")
        || name == "mrd"
    {
        c.mr[0] = mr_value([p[0], p[1], p[2]]);
    }
    let done = if f[3] == "1" {
        c.short_compute(word)
    } else {
        c.compute(word)
    };
    if !done {
        return None;
    }
    let mr = c.mr[0];
    Some([
        u64::from(c.r[0]),
        (mr as u64) & 0xFFFF_FFFF,
        ((mr >> 32) as u64) & 0xFFFF_FFFF,
        u64::from((mr >> 64) as i16 as i32 as u32),
        u64::from(c.astat(0)) & ASTAT_MASK,
        u64::from(c.sys.stky[0]) & STKY_MASK,
    ])
}

#[test]
fn model_matches_every_measured_fixed_point_row() {
    let (mut rows, mut errs) = (0usize, Vec::<String>::new());
    let mut prev_mr = [0u64; 3];
    for line in CSV.lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        rows += 1;
        let want: Vec<u64> = f[10..16].iter().map(|s| hex(s).unwrap_or(0)).collect();
        let has_mr = !f[11].is_empty();
        if has_mr {
            prev_mr = [want[1], want[2], want[3]];
        }
        let mut want = [
            want[0],
            want[1],
            want[2],
            want[3],
            want[4] & ASTAT_MASK,
            want[5] & STKY_MASK,
        ];
        let got = match run(&f, prev_mr) {
            Some(g) => g,
            None => {
                errs.push(format!("{} {} {}: not executed", f[0], f[2], f[4]));
                continue;
            }
        };
        let mut got = got;
        if !has_mr {
            (want[1], want[2], want[3]) = (0, 0, 0);
            (got[1], got[2], got[3]) = (0, 0, 0);
        }
        if got != want {
            errs.push(format!(
                "{} {} x {} y {} p {},{},{}: got {got:x?} want {want:x?}",
                f[0], f[2], f[5], f[6], f[7], f[8], f[9]
            ));
        }
    }
    let mut by = std::collections::BTreeMap::<String, usize>::new();
    for e in &errs {
        let mut it = e.split(' ');
        *by.entry(format!(
            "{} {}",
            it.next().unwrap_or(""),
            it.next().unwrap_or("")
        ))
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
