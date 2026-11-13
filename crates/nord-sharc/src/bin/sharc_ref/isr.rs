//! Scenario commands that drive the SY1SY2 and SMP images the way the hardware does: through the
//! interrupt controller and the command FIFO the ARM fills.
//!
//! ```text
//! irq   VECTOR [MAX_STEPS]               raise interrupt VECTOR (0..41 or a name) and run until the
//!                                        core is back in the idle loop (the real IVT, MODE1 swap, RTI)
//! msg   [op=N] ADDR VALUE                append one `[opcode:4 | DM address:28], value` pair to the
//!                                        command FIFO ring and move the DMA write index DM(0x2400)
//! fifo  ADDR                             command FIFO base of this image (SY1SY2 0x93046, SMP 0x92196)
//! dev   ADDR VALUE                       preset a peripheral status register that the firmware polls
//! ```
//!
//! Provenance (`docs/research/sy-smp-audio.md` section 2): the FIFO pair format and the read
//! pointer reset to `0x93046` are VERIFIED from the SY1SY2 listing `0x1C0638..0x1C064B`; the write
//! index `DM(0x2400)` stands in for the external-port DMA that the ARM drives (ASSUMED: the
//! channel is not modelled, the harness writes the index itself).
use crate::exec::Runner;
use crate::script::{parse_addr, parse_number, parse_value, Cmd};
use crate::signal::Opts;
use nord_sharc::core::vector_by_name;

/// First word of the SY1SY2 command FIFO (`I0` after the FIFO reset interrupt); the SMP image
/// uses `0x92196` (`fifo 0x92196`).
pub const FIFO_BASE: u32 = 0x93046;
/// Interrupt vector (IVT word 52) whose handler `0x1C03FD` re-arms the FIFO read pointer
/// (`I0 = 0x93046`); one batch of messages is one reset plus one audio ISR.
pub const FIFO_RESET_VECTOR: u8 = 13;
/// DMA write index register of the FIFO (an IOP address, read by the ISR as `DM(0x2400)`).
pub const FIFO_INDEX: u32 = 0x2400;

/// Parses `irq` and `msg`; `None` when `cmd` is not one of them.
pub fn parse(cmd: &str, pos: &[String], opts: &Opts) -> Option<Result<Cmd, String>> {
    let bad = |m: &str| Err(format!("{cmd}: {m}"));
    match cmd {
        "irq" => Some(match pos.first().and_then(|v| vector_by_name(v)) {
            None => bad("needs a vector number or name"),
            Some(vector) => Ok(Cmd::Irq {
                vector,
                max_steps: pos
                    .get(1)
                    .and_then(|s| parse_number(s))
                    .unwrap_or(2_000_000),
            }),
        }),
        "msg" => Some(match (pos.first(), pos.get(1)) {
            (Some(a), Some(v)) => {
                let op = opts.get("op").map_or(Some(0), |o| parse_number(o));
                match (op.filter(|o| *o < 16), parse_addr(a), parse_value(v)) {
                    (Some(op), Ok(addr), Ok(value)) if addr < 1 << 28 => Ok(Cmd::Msg {
                        op: op as u32,
                        addr,
                        value,
                    }),
                    _ => bad("needs op 0..15, a 28-bit address and a value"),
                }
            }
            _ => bad("needs ADDR VALUE"),
        }),
        "fifo" => Some(match pos.first().map(|a| parse_addr(a)) {
            Some(Ok(base)) => Ok(Cmd::Fifo(base)),
            _ => bad("needs a base address"),
        }),
        "dev" => Some(match (pos.first(), pos.get(1)) {
            (Some(a), Some(v)) => match (parse_addr(a), parse_value(v)) {
                (Ok(addr), Ok(value)) => Ok(Cmd::Dev(addr, value)),
                _ => bad("needs a numeric ADDR VALUE"),
            },
            _ => bad("needs ADDR VALUE"),
        }),
        _ => None,
    }
}

impl Runner {
    /// Raises `vector` and steps until the handler has returned to the idle loop.
    pub fn run_irq(&mut self, vector: u8, max_steps: u64) -> Result<(), String> {
        let start = self.core.steps;
        // vectors 0..16 are IMASK bits; the firmware unmasks the FIFO-reset vector itself only
        // once the ARM DMA runs, so the harness unmasks it for the call (restored afterwards)
        let saved_imask = self.core.imask;
        if vector <= 16 {
            self.core.imask |= 1 << vector;
        }
        if !self.core.raise_interrupt(vector) {
            return Err(format!(
                "interrupt {vector} is invalid or already in service"
            ));
        }
        if vector == FIFO_RESET_VECTOR {
            self.fifo_wr = 0; // the next `msg` starts a new batch at the ring base
        }
        let mut taken = false;
        let mut recent: std::collections::VecDeque<u32> = std::collections::VecDeque::new();
        loop {
            if recent.back() != Some(&self.core.pc) {
                if recent.len() == 400 {
                    recent.pop_front();
                }
                recent.push_back(self.core.pc);
            }
            if self.core.steps - start >= max_steps {
                let tail: Vec<String> = recent
                    .iter()
                    .rev()
                    .take(24)
                    .rev()
                    .map(|p| format!("{p:x}"))
                    .collect();
                return Err(format!(
                    "interrupt {vector} not finished within {max_steps} steps (pc 0x{:x}, last pcs {})",
                    self.core.pc,
                    tail.join(" ")
                ));
            }
            self.core
                .step()
                .map_err(|f| format!("fault in irq {vector}: {f:?}"))?;
            taken |= self.core.servicing().is_some();
            if taken && self.core.servicing().is_none() {
                self.core.imask = saved_imask;
                self.render.run_steps.push(self.core.steps - start);
                return Ok(());
            }
        }
    }

    /// Appends a FIFO command pair and moves the write index.
    pub fn push_msg(&mut self, op: u32, addr: u32, value: u32) -> Result<(), String> {
        let at = self.fifo_wr.max(self.fifo_base);
        let word0 = op << 28 | addr;
        self.core
            .dm_write(at, word0)
            .and_then(|()| self.core.dm_write(at + 1, value))
            .and_then(|()| self.core.dm_write(FIFO_INDEX, at + 2))
            .map_err(|a| format!("bad FIFO address 0x{a:x}"))?;
        self.fifo_wr = at + 2;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(words: &[&str]) -> Vec<String> {
        words.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_irq_and_msg() {
        let none = Opts::new();
        assert_eq!(
            parse("irq", &p(&["17", "99"]), &none),
            Some(Ok(Cmd::Irq {
                vector: 17,
                max_steps: 99
            }))
        );
        assert!(matches!(
            parse("irq", &p(&["P6I"]), &none),
            Some(Ok(Cmd::Irq { vector: 17, .. }))
        ));
        assert!(parse("irq", &p(&["99"]), &none).unwrap().is_err());
        let mut op = Opts::new();
        op.insert("op".into(), "3".into());
        assert_eq!(
            parse("msg", &p(&["0x92414", "0x1c07cd"]), &op),
            Some(Ok(Cmd::Msg {
                op: 3,
                addr: 0x92414,
                value: 0x1c07cd
            }))
        );
        assert!(parse("msg", &p(&["0x10000000", "1"]), &none)
            .unwrap()
            .is_err());
        assert!(parse("msg", &p(&["1"]), &none).unwrap().is_err());
        assert_eq!(
            parse("dev", &p(&["0x5201", "0x40"]), &none),
            Some(Ok(Cmd::Dev(0x5201, 0x40)))
        );
        assert!(parse("dev", &p(&["0x5201"]), &none).unwrap().is_err());
        assert_eq!(
            parse("fifo", &p(&["0x92196"]), &none),
            Some(Ok(Cmd::Fifo(0x92196)))
        );
        assert!(parse("fifo", &p(&[]), &none).unwrap().is_err());
        assert!(parse("poke", &p(&["1", "2"]), &none).is_none());
    }

    #[test]
    fn msg_fills_the_fifo_and_moves_the_index() {
        let mut r = Runner::new(&[0u8; 16], false);
        r.push_msg(0, 0x92414, 0x1c07cd).unwrap();
        r.push_msg(2, 0x1234, 7).unwrap();
        assert_eq!(r.core.dm_read(FIFO_BASE).unwrap(), 0x92414);
        assert_eq!(r.core.dm_read(FIFO_BASE + 1).unwrap(), 0x1c07cd);
        assert_eq!(r.core.dm_read(FIFO_BASE + 2).unwrap(), 0x2000_1234);
        assert_eq!(r.core.dm_read(FIFO_INDEX).unwrap(), FIFO_BASE + 4);
    }

    #[test]
    fn irq_on_an_empty_image_reports_the_stuck_handler() {
        let mut r = Runner::new(&[0u8; 16], false);
        assert!(r.run_irq(99, 10).is_err());
    }
}
