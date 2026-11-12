//! Runs a simulator probe (`docs/research/sharc-seq/*_fixture.txt`, made by `dxe2fixture.py`)
//! on a [`Core`] so a test can compare the output array with the measured one.
//!
//! The fixture holds the probe's machine code (short-word parcels), the entry and exit address
//! (the instruction where the C wrapper's `leaf_exit` starts), the output array and an optional
//! fill word for it. The code is exactly what ran on the CCES ADSP-21489 simulator, so any
//! instruction the model does not know shows up here as a fault.
use super::Core;

/// A parsed fixture.
pub(super) struct Fixture {
    pub entry: u32,
    pub exit: u32,
    pub out: u32,
    pub words: usize,
    pub fill: u32,
    /// Parcels as the core stores them (little-endian halves).
    code: Vec<(u32, Vec<u8>)>,
    dm: Vec<(u32, u32)>,
}

impl Fixture {
    pub(super) fn parse(text: &str) -> Fixture {
        let hex = |s: &str| u32::from_str_radix(s, 16).unwrap_or(0);
        let mut f = Fixture {
            entry: 0,
            exit: 0,
            out: 0,
            words: 0,
            fill: 0,
            code: Vec::new(),
            dm: Vec::new(),
        };
        for line in text.lines().filter(|l| !l.starts_with('#')) {
            let t: Vec<&str> = line.split_whitespace().collect();
            match t.as_slice() {
                ["entry", a] => f.entry = hex(a),
                ["exit", a] => f.exit = hex(a),
                ["fill", w] => f.fill = hex(w),
                ["out", a, n] => (f.out, f.words) = (hex(a), n.parse().unwrap_or(0)),
                ["code", a, bytes] => {
                    let b = (0..bytes.len() / 4)
                        .flat_map(|i| {
                            let parcel =
                                u16::from_str_radix(&bytes[4 * i..4 * i + 4], 16).unwrap_or(0);
                            parcel.to_le_bytes()
                        })
                        .collect();
                    f.code.push((hex(a), b));
                }
                ["dm", a, w] => f.dm.push((hex(a), hex(w))),
                _ => {}
            }
        }
        f
    }

    /// Loads the fixture into `c`, runs from `entry` to `exit` (at most `max_steps`
    /// instructions) and returns the output array.
    pub(super) fn run(&self, c: &mut Core, max_steps: usize) -> Result<Vec<u32>, String> {
        for (a, bytes) in &self.code {
            let o = *a as usize * 2;
            c.sram[o..o + bytes.len()].copy_from_slice(bytes);
        }
        for (a, w) in &self.dm {
            c.dm_write(*a, *w).map_err(|e| format!("dm write {e:x}"))?;
        }
        for i in 0..self.words as u32 {
            c.dm_write(self.out + i, self.fill)
                .map_err(|e| format!("out write {e:x}"))?;
        }
        c.pc = self.entry;
        c.r[4] = self.out;
        let mut steps = 0;
        while c.pc != self.exit {
            c.step()
                .map_err(|e| format!("pc {:x} after {steps} steps: {e:?}", c.pc))?;
            steps += 1;
            if steps > max_steps {
                return Err(format!("no exit after {max_steps} steps, pc {:x}", c.pc));
            }
        }
        (0..self.words as u32)
            .map(|i| {
                c.dm_read(self.out + i)
                    .map_err(|e| format!("out read {e:x}"))
            })
            .collect()
    }
}

/// The measured output array of a probe (one hex word per line).
pub(super) fn measured(text: &str) -> Vec<u32> {
    text.lines()
        .filter_map(|l| u32::from_str_radix(l.trim().trim_start_matches("0x"), 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_runs_a_tiny_fixture() {
        // `r0 = 0xD0E5; dm(0xB2000) = r0;` is not hand-encodable here, so run an empty program:
        // entry == exit returns the (filled) array.
        let f = Fixture::parse("entry 124300\nexit 124300\nout b2000 2\nfill 5\n");
        let mut c = Core::new(&[]);
        assert_eq!(f.run(&mut c, 10), Ok(vec![5, 5]));
        assert_eq!(measured("0x00000001\n0xFFFFFFFF\n"), vec![1, u32::MAX]);
    }
}
