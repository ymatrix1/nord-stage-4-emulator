//! ARM-style message injection for `sharc-org`: `--msg`, `--msg-script`, `--msg-base`, `--rx-irq`.
//!
//! The ARM sends two 32-bit words per message (word 1 = channel base + field offset, word 2 =
//! value, `docs/research/arm-dsp-protocol.md` section 3). On the DSP side the words land in data
//! memory through the SPI/DMA receive path and the P4I interrupt
//! (`docs/research/dsp-rx-protocol.md`). The oracle has no SPI or DMA engine, so a message is
//! modelled by its effect: word 2 is stored at `base + word 1` (ASSUMED mapping, default base
//! [`STRIP_BASE`] = the channel strips), and `--rx-irq` raises the Core1 receive interrupt with a
//! SEC source id so that the P4I service routine (not the harness) starts the pass.
use crate::args::{parse_number, parse_value};
use nord_sharc::core::{vector_by_name, Core};

/// First channel strip (`DM(0xE0000)`, 62 words per strip, stage pointers in words 0 and 1).
/// VERIFIED: `docs/research/dsp-rx-protocol.md` section 3 (a stage pointer written here selects
/// the module that the pass runs).
pub const STRIP_BASE: u32 = 0xE_0000;

/// One message delivered right before pass `pass` (0-based).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Msg {
    pub pass: u32,
    pub word1: u32,
    pub word2: u32,
}

/// `[PASS:]WORD1=WORD2` (pass defaults to 0; WORD2 may be a float such as `f0.5`).
pub fn parse_msg(s: &str) -> Result<Msg, String> {
    let bad = || format!("bad message {s:?} (want [PASS:]WORD1=WORD2)");
    let (pass, rest) = match s.split_once(':') {
        Some((p, r)) => (parse_number(p).ok_or_else(bad)?, r),
        None => (0, s),
    };
    let (w1, w2) = rest.split_once('=').ok_or_else(bad)?;
    Ok(Msg {
        pass,
        word1: parse_number(w1.trim()).ok_or_else(bad)?,
        word2: parse_value(w2.trim()).ok_or_else(bad)?,
    })
}

/// Script text: one `PASS WORD1 WORD2` triple per line, `#` starts a comment.
pub fn parse_script(text: &str) -> Result<Vec<Msg>, String> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        let bad = || format!("script line {}: want PASS WORD1 WORD2", n + 1);
        if f.len() != 3 {
            return Err(bad());
        }
        out.push(Msg {
            pass: parse_number(f[0]).ok_or_else(bad)?,
            word1: parse_number(f[1]).ok_or_else(bad)?,
            word2: parse_value(f[2]).ok_or_else(bad)?,
        });
    }
    Ok(out)
}

/// `VECTOR:STATUS` (for example `P4I:0x41`): interrupt raised to start each pass and the SEC
/// source id its service routine reads.
pub fn parse_rx_irq(s: &str) -> Result<(u8, u32), String> {
    let bad = || format!("bad --rx-irq {s:?} (want VECTOR:STATUS, e.g. P4I:1)");
    let (v, st) = s.split_once(':').ok_or_else(bad)?;
    Ok((
        vector_by_name(v).ok_or_else(bad)?,
        parse_number(st).filter(|x| *x != 0).ok_or_else(bad)?,
    ))
}

/// Stores the message in data memory and returns the address written.
pub fn deliver(core: &mut Core, base: u32, m: &Msg) -> Result<u32, String> {
    let addr = base.wrapping_add(m.word1);
    core.dm_write(addr, m.word2)
        .map_err(|a| format!("message address 0x{a:x} not writable"))?;
    Ok(addr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_scripts_and_irqs_parse() {
        let m = parse_msg("2:0x39=f0.5").unwrap();
        assert_eq!((m.pass, m.word1, m.word2), (2, 0x39, 0x3F00_0000));
        assert_eq!(parse_msg("0=0x8012082f").unwrap().pass, 0);
        assert!(parse_msg("0x39").is_err());
        assert!(parse_msg("x:1=2").is_err());
        let s = parse_script("# c\n1 0 0x8012082f  # stage\n\n2 62 7\n").unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(
            s[1],
            Msg {
                pass: 2,
                word1: 62,
                word2: 7
            }
        );
        assert!(parse_script("1 2").is_err());
        assert_eq!(parse_rx_irq("P4I:1"), Ok((15, 1)));
        assert!(parse_rx_irq("P4I:0").is_err());
        assert!(parse_rx_irq("nope:1").is_err());
    }

    #[test]
    fn deliver_stores_word2_at_base_plus_word1() {
        let data = vec![0u8; 0x24_0000 + 32];
        let mut core = Core::new(&data);
        let m = Msg {
            pass: 0,
            word1: 3,
            word2: 9,
        };
        assert_eq!(deliver(&mut core, STRIP_BASE, &m), Ok(0xE_0003));
        assert_eq!(core.dm_read(0xE_0003), Ok(9));
    }
}
