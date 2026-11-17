//! Helpers the generated AOT modules call (`aot_emit.rs`) and the guard signature an interpreter
//! item inside a block must leave unchanged.
use super::decode_cache::ComputeOp;
use super::{Core, Fault};

/// MODE1 bits the rest of a block depends on: SIMD (both elements) and IRPTEN (arithmetic
/// exception interrupts).
const SIG_MODE1: u32 = 1 << 21 | 1 << 12;

/// State an interpreter item inside a block must leave unchanged for the rest of the block to
/// stay exact: the guard of [`Core::aot_ready`] re-checked piecewise (SIMD mode, enabled
/// arithmetic-exception interrupts, the interrupt controller's next event `due` (any IRPTL or
/// mask write that could raise one resets it), code, loop and call stacks, delayed branch).
/// Other MODE1 bits (rounding, saturation) are read again by every native batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AotSig {
    /// MODE1 bits SIMD (21) and IRPTEN (12).
    mode: u32,
    /// Interrupt mask and the LIRPTL mask bits, which decide `arith_irq_enabled`.
    imask: u32,
    lmask: u32,
    due: u64,
    window_len: u32,
    idle: bool,
    code_gen: u64,
    loops: usize,
    calls: usize,
    pending: bool,
}

// Called only by the generated block module and the test oracle (`aot_state.rs`), so a build
// without `NORD_SHARC_AOT_HOT` sees them as unused.
#[allow(dead_code)]
impl Core {
    /// Exchanges the PEx/PEy views.
    #[inline(always)]
    pub(super) fn aot_swap_pe(&mut self) {
        self.swap_pe();
    }

    /// Runs one lowered compute field on the current element.
    #[inline(always)]
    pub(super) fn aot_op(&mut self, op: ComputeOp) {
        let ok = self.exec_compute_op(op);
        debug_assert!(ok, "AOT block holds an unmodelled compute op: {op:?}");
    }

    /// Reads the source of `Rdst = Rsrc`: value, float-extension tag and low byte.
    #[inline(always)]
    pub(super) fn aot_move_src(&self, src: u8) -> (u32, u32, u8) {
        let n = usize::from(src);
        (self.r[n], self.fx.tag[n], self.fx.lo[n])
    }

    /// Writes a captured register move; a move between data registers keeps the extension.
    #[inline(always)]
    pub(super) fn aot_move_dst(&mut self, dst: u8, m: (u32, u32, u8)) {
        let n = usize::from(dst);
        self.write_ureg(dst, m.0);
        (self.fx.tag[n], self.fx.lo[n]) = (m.1, m.2);
    }

    /// `Rn = <shifter op> Rx BY imm` with the register-form opcode.
    #[inline(always)]
    pub(super) fn aot_shift(&mut self, op: u64, n: u8, x: u8, ry: u32) {
        let ok = self.shifter_op(op, usize::from(n), self.r[usize::from(x)], ry);
        debug_assert!(ok, "AOT block holds an unmodelled shift op {op:#x}");
    }

    /// The guarded state of the running block (see [`AotSig`]).
    #[inline(always)]
    pub(super) fn aot_sig(&self) -> AotSig {
        AotSig {
            mode: self.mode1 & SIG_MODE1,
            imask: self.imask,
            lmask: self.irq.lirptl >> 10 & 0x3FF,
            due: self.irq.due,
            window_len: self.irq.window_len,
            idle: self.irq.idle,
            code_gen: self.code_gen,
            loops: self.loops.len(),
            calls: self.call_stack.len(),
            pending: self.pending.is_some(),
        }
    }

    /// Runs the interpreter item at `pc` (one [`Core::step`], kept out of line so a block of
    /// thousands of items stays small) and returns true when the block must stop after it (see
    /// [`Core::aot_bail`]).
    #[inline(never)]
    pub(super) fn aot_item(&mut self, pc: u32, sig: &AotSig, next: u32) -> Result<bool, Fault> {
        self.pc = pc;
        self.step()?;
        Ok(self.aot_bail(sig, next))
    }

    /// After an interpreter item: true when the rest of the block must not run natively (the pc
    /// is not the next instruction, or guarded state changed). The core is then exactly where the
    /// interpreter would be.
    #[inline(always)]
    pub(super) fn aot_bail(&self, sig: &AotSig, next: u32) -> bool {
        self.pc != next || self.aot_sig() != *sig
    }
}

#[cfg(test)]
mod tests {
    use super::super::support::core_with;
    use super::*;

    fn simd_core() -> Core {
        let mut c = core_with(&[0x0001u16, 0x0001, 0x0001]);
        c.mode1 |= 1 << 21;
        c.irq.due = u64::MAX;
        c
    }

    #[test]
    fn bail_sees_pc_mode_and_code_changes() {
        let mut c = simd_core();
        let sig = c.aot_sig();
        let next = c.pc;
        assert!(!c.aot_bail(&sig, next));
        assert!(c.aot_bail(&sig, next + 1), "pc moved");
        c.mode1 ^= 1 << 21;
        assert!(c.aot_bail(&sig, next), "SIMD mode changed");
        c.mode1 ^= 1 << 21;
        c.mode1 ^= 1 << 13; // ALUSAT is re-read by each batch: no bail
        assert!(!c.aot_bail(&sig, next));
        c.mode1 ^= 1 << 13;
        let due = c.irq.due;
        c.irq.due = 5;
        assert!(
            c.aot_bail(&sig, next),
            "interrupt controller event time moved"
        );
        c.irq.due = due;
        c.code_gen += 1;
        assert!(c.aot_bail(&sig, next), "code changed");
    }

    #[test]
    fn item_runs_one_step_and_reports_no_bail_for_a_nop() {
        let mut c = simd_core();
        let sig = c.aot_sig();
        let (pc, steps) = (c.pc, c.steps);
        assert_eq!(c.aot_item(pc, &sig, pc + 1), Ok(false));
        assert_eq!((c.pc, c.steps), (pc + 1, steps + 1));
        assert_eq!(
            c.aot_item(pc + 1, &sig, pc + 5),
            Ok(true),
            "wrong next pc bails"
        );
    }
}
