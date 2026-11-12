//! Single-step decoder of [`Core`]: fetches the parcels, decodes the instruction type and
//! dispatches to the compute, move and flow handlers.
use super::{Core, Fault};
use crate::visa::{bits, classify, word48, Insn, Kind};

/// One fetched instruction with the fields every handler needs.
pub(super) struct Decoded {
    pub pc: u32,
    pub insn: Insn,
    /// The instruction as a 48-bit ISA word (shorter forms are left-aligned).
    pub w: u64,
    /// Whether the `IF cond` field (bits 37..33) holds; unconditional types read it as true.
    pub cond_ok: bool,
}

impl Decoded {
    #[inline]
    pub fn kind(&self) -> Kind {
        self.insn.kind
    }
}

impl Core {
    /// Fetches the instruction at `pc`.
    #[inline]
    fn fetch(&self, pc: u32) -> Decoded {
        let p0 = self.parcel(pc);
        let p1 = self.parcel(pc.wrapping_add(1));
        let (kind, len) = classify(p0, Some(p1));
        let mut parcels = [p0, 0, 0];
        if len > 1 {
            parcels[1] = p1;
        }
        if len > 2 {
            parcels[2] = self.parcel(pc.wrapping_add(2));
        }
        let w = word48(&parcels[..len]);
        Decoded {
            pc,
            insn: Insn { kind, parcels, len },
            w,
            cond_ok: self.cond(bits(w, 37, 33)),
        }
    }

    /// Runs the compute field `c` of `d`; an unmodelled operation faults (or is skipped in
    /// lenient mode).
    #[inline]
    pub(super) fn run_compute(&mut self, d: &Decoded, c: u64) -> Result<(), Fault> {
        if self.compute(c) {
            Ok(())
        } else {
            self.unmodelled(d.pc, &d.insn)
        }
    }

    /// Executes one instruction. Returns `Ok(())` or the reason it could not.
    pub fn step(&mut self) -> Result<(), Fault> {
        let d = self.fetch(self.pc);
        let mut next = d.pc.wrapping_add(d.insn.len as u32);
        match d.kind() {
            Kind::T21c if d.insn.parcels[0] == 0x0001 => {}
            Kind::T21 if d.w == 0 => {}
            Kind::T2a | Kind::T2b | Kind::T2x | Kind::T2c => self.exec_compute(&d)?,
            Kind::T8a | Kind::T9a | Kind::T9b | Kind::T11a | Kind::T11c | Kind::T12a => {
                next = self.exec_flow(&d, next)?;
            }
            Kind::T17a
            | Kind::T17b
            | Kind::T14a
            | Kind::T15a
            | Kind::T15b
            | Kind::T16a
            | Kind::T18a
            | Kind::T5a
            | Kind::T5b
            | Kind::T3a
            | Kind::T3b
            | Kind::T3c
            | Kind::T4a
            | Kind::T4b
            | Kind::T7a
            | Kind::T7b => self.exec_move(&d)?,
            _ => return Err(self.fault(d.pc, &d.insn)),
        }
        self.pc = self.finish_step(&d, next);
        self.steps += 1;
        Ok(())
    }

    /// Types 2a/2b/2x (23-bit compute) and 2c (short compute).
    #[inline]
    fn exec_compute(&mut self, d: &Decoded) -> Result<(), Fault> {
        match d.kind() {
            Kind::T2b => self.run_compute(d, bits(d.w, 38, 16)),
            Kind::T2c => {
                if self.short_compute(bits(d.w, 43, 32)) {
                    Ok(())
                } else {
                    self.unmodelled(d.pc, &d.insn)
                }
            }
            _ if d.cond_ok => self.run_compute(d, bits(d.w, 22, 0)),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::support::core_with;
    use super::*;

    #[test]
    fn nop_forms_advance_by_their_length() {
        // 16-bit NOP (type 21c) then the all-zero 48-bit NOP
        let mut c = core_with(&[0x0001, 0x0000, 0x0000, 0x0000]);
        assert_eq!(c.run(2), None);
        assert_eq!(c.pc, 0x12_0000 + 1 + 3);
    }

    #[test]
    fn conditional_compute_is_skipped_when_the_condition_fails() {
        // IF NE R3 = R1 + R2 (type 2a, cond 16 = NE holds while AZ is clear)
        let program = [0x0120u16, 0x0000, 0x1312];
        let mut c = core_with(&program);
        (c.r[1], c.r[2]) = (4, 5);
        assert_eq!(c.fetch(c.pc).kind(), Kind::T2a);
        assert_eq!(c.step(), Ok(()));
        assert_eq!((c.r[3], c.pc), (9, 0x12_0003));
        let mut skipped = core_with(&program);
        (skipped.r[1], skipped.r[2], skipped.flags.az) = (4, 5, true);
        assert_eq!(skipped.step(), Ok(()));
        assert_eq!(skipped.r[3], 0);
    }

    #[test]
    fn faulting_step_leaves_state_untouched() {
        let mut c = core_with(&[0x8000, 0x0000, 0x0000]);
        let before = (c.pc, c.steps);
        assert!(c.step().is_err());
        assert_eq!((c.pc, c.steps), before);
    }
}
