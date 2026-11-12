//! Runs the cases of the firmware-instruction census diff on a [`Core`]
//! (`docs/research/sharc-seq/gen_census_probe.py`, `sim_batch.py`).
//!
//! The fixture holds the probe's machine code and the symbols `_slot`, `_st`, `_res`, `_scr`. A case
//! line is `PARCELS S0 .. Sn` (hex): the instruction's 16-bit parcels poked into the slot and the
//! state table written to `_st`; a `scr` line gives the 512-word scratch table every case starts
//! with. Each case starts on a fresh core; the output line holds the dumped state (`_res`, the words
//! between `_res` and `_scr`) followed by the scratch table, as hex words, or `FAULT message`.
use super::probe_fixture::Fixture;
use super::Core;

const SLOT_PARCELS: usize = 10;
const SCR_WORDS: usize = 512;
const NOP: u16 = 0x0001;
const MAX_STEPS: usize = 20_000;

fn words(tokens: &[&str]) -> Vec<u32> {
    tokens
        .iter()
        .map(|t| u32::from_str_radix(t, 16).unwrap_or(0))
        .collect()
}

fn one_case(f: &Fixture, scr: &[u32], parcels: &str, state: &[u32]) -> Result<String, String> {
    let sym = |n: &str| f.sym(n).ok_or_else(|| format!("fixture lacks sym {n}"));
    let (slot, st, res, scr_a) = (sym("_slot")?, sym("_st")?, sym("_res")?, sym("_scr")?);
    let mut c = Core::new(&[]);
    f.load(&mut c)?;
    let mut ps: Vec<u16> = (0..parcels.len() / 4)
        .map(|i| u16::from_str_radix(&parcels[4 * i..4 * i + 4], 16).unwrap_or(NOP))
        .collect();
    ps.resize(SLOT_PARCELS, NOP);
    for (i, p) in ps.iter().enumerate() {
        let o = (slot as usize + i) * 2;
        c.sram[o..o + 2].copy_from_slice(&p.to_le_bytes());
    }
    for (i, w) in state.iter().enumerate() {
        c.dm_write(st + i as u32, *w).map_err(|e| format!("st write {e:x}"))?;
    }
    for (i, w) in scr.iter().enumerate() {
        c.dm_write(scr_a + i as u32, *w).map_err(|e| format!("scr write {e:x}"))?;
    }
    c.pc = f.entry;
    let mut steps = 0;
    while c.pc != f.exit {
        c.step()
            .map_err(|e| format!("pc {:x} after {steps} steps: {e:?}", c.pc))?;
        steps += 1;
        if steps > MAX_STEPS {
            return Err(format!("no exit, pc {:x}", c.pc));
        }
    }
    let mut out = Vec::new();
    for a in (res..scr_a).chain(scr_a..scr_a + SCR_WORDS as u32) {
        let w = c.dm_read(a).map_err(|e| format!("read {e:x}"))?;
        out.push(format!("{w:08X}"));
    }
    Ok(out.join(" "))
}

/// Runs every case of `cases` and returns one output line per case.
pub fn run_cases(fixture: &str, cases: &str) -> Result<String, String> {
    let f = Fixture::parse(fixture);
    let mut scr = vec![0u32; SCR_WORDS];
    let mut out = String::new();
    for line in cases.lines() {
        let t: Vec<&str> = line.split_whitespace().collect();
        match t.as_slice() {
            [] => {}
            ["scr", rest @ ..] => scr = words(rest),
            [parcels, rest @ ..] => {
                let line = one_case(&f, &scr, parcels, &words(rest))
                    .unwrap_or_else(|e| format!("FAULT {e}"));
                out.push_str(&line);
                out.push('\n');
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_symbol_is_reported_per_case() {
        let out = run_cases("entry 0\nexit 0\n", "scr 1\n0001 0 0\n").unwrap();
        assert!(out.starts_with("FAULT fixture lacks sym _slot"), "{out}");
    }

    #[test]
    fn words_parse_hex_and_default_to_zero() {
        assert_eq!(words(&["1F", "zz"]), vec![0x1F, 0]);
    }
}
