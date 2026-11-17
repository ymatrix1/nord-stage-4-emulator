//! Lowering for the hot-block AOT prototype: a straight-line range of [`DecodedOp`]s becomes a
//! list of [`Item`]s that `aot_emit.rs` prints as Rust source.
//!
//! Two kinds of instruction can sit in a block:
//!
//! - *native* ops (compute fields, immediate loads, `Rn = Rm [, compute]` moves, shift
//!   immediates) touch only registers, flags, multiplier results, float extensions, ASTAT extras
//!   and STKY of ONE processing element, so a run of them is a [`Item::Batch`]: the generated
//!   code runs the whole run on PEx, then on PEy, instead of swapping around every instruction
//!   like [`Core::step`] does. That reordering is exact (`docs/research/sharc-aot-hot-block.md`).
//! - every other straight-line instruction (memory moves, conditional ops, dual moves, ...) is an
//!   [`Item::Interp`]: the generated code sets the pc and calls [`Core::step`], so it runs the
//!   interpreter's own implementation, in program order, between the batches.
//!
//! Anything that changes the control flow (jumps, calls, returns, `DO`, stack ops, `IDLE`) ends a
//! block; it is never lowered.
use super::aot_pe::{pe_supports, pe_supports_shift};
use super::decode_cache::{ComputeOp, DecodedOp, MoveOp};
use super::shifter::shift_imm_params;
use super::Core;
use crate::visa::bits;

/// How the generated block executes its native ops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AotTier {
    /// Register-resident per-element state and native kernels (`aot_pe.rs`).
    Pe,
    /// [`AotTier::Pe`] with host `f64` float kernels where the result is provably exact
    /// (`aot_native.rs`); still bit-exact.
    Guarded,
    /// [`AotTier::Pe`] with host `f32` kernels: NOT bit-exact (extension dropped, denormals
    /// flushed); only for the audio-metric comparison, never a replacement for the exact path.
    Approx,
}

/// One native instruction of a block, acting on a single processing element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LoweredOp {
    /// Unconditional compute field (`T2a`/`T2b`/`T2c`).
    Compute(ComputeOp),
    /// `Rn = immediate` (the SIMD twin write is the other element's pass).
    LoadImm { reg: u8, value: u32 },
    /// `Rdst = Rsrc [, compute]`; the source and its float extension are read before the compute.
    RegMove {
        src: u8,
        dst: u8,
        compute: Option<ComputeOp>,
    },
    /// `Rn = <shifter op> Rx BY <immediate>` with the register-form opcode and the Ry value.
    Shift { op: u64, n: u8, x: u8, ry: u32 },
}

/// A unit of a lowered block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Item {
    /// Native ops run PE-batched (one step each).
    Batch(Vec<LoweredOp>),
    /// One instruction at `pc` run by [`Core::step`].
    Interp { pc: u32, next: u32 },
}

/// How the lowering sees one instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsnClass {
    /// Lowered natively (a NOP counts as native).
    Native,
    /// Straight-line but not native: run by the interpreter inside the block (shape name).
    Interp(String),
    /// Changes control flow or cannot be run out of order: ends the block (shape name).
    Break(String),
}

/// A lowered block and what the runtime guard needs to know about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Lowered {
    pub start: u32,
    /// Start pc of the last instruction.
    pub last: u32,
    /// pc after the last instruction.
    pub exit: u32,
    /// Instructions (= steps).
    pub steps: u32,
    /// Instruction parcels `start..exit`, to detect self-modified code at run time.
    pub code: Vec<u16>,
    pub items: Vec<Item>,
}

/// Batches shorter than this run as interpreter items: loading and storing both elements costs
/// more than the one or two decoded steps it would replace.
pub(super) const MIN_BATCH: usize = 3;

/// The compute op when the register-resident kernels model it (anything else is interpreted).
fn lower_compute(op: ComputeOp) -> Option<ComputeOp> {
    (op != ComputeOp::Invalid && pe_supports(op)).then_some(op)
}

impl Core {
    /// Lists `(pc, next_pc, Debug of the decoded op)` of each instruction starting in
    /// `start..=last` (diagnostic for choosing lowering coverage).
    pub fn aot_describe_range(&self, start: u32, last: u32) -> Vec<(u32, u32, String)> {
        let mut out = Vec::new();
        let mut pc = start;
        while pc <= last {
            let d = self.fetch(pc);
            let next = pc.wrapping_add(d.insn.len as u32);
            let op = DecodedOp::from_insn(pc, &d.insn, d.w);
            out.push((pc, next, format!("{op:?}")));
            pc = next;
        }
        out
    }

    /// Classifies the instruction at `pc`: its length in parcels, the native op when it has one
    /// (`None` for a NOP, which still counts as a step) and the class.
    pub(super) fn lower_insn(&self, pc: u32) -> (u32, Option<LoweredOp>, InsnClass) {
        let d = self.fetch(pc);
        let len = d.insn.len as u32;
        if self.isa_at(pc) {
            return (len, None, InsnClass::Break("IsaMode".to_string()));
        }
        let op = DecodedOp::from_insn(pc, &d.insn, d.w);
        let native = match op {
            DecodedOp::Nop => return (len, None, InsnClass::Native),
            DecodedOp::ComputeCond { cond: 31, op } | DecodedOp::ComputeBoth { op } => {
                lower_compute(op).map(LoweredOp::Compute)
            }
            DecodedOp::Move(MoveOp::LoadImm { ureg, value }) if ureg < 16 => {
                Some(LoweredOp::LoadImm { reg: ureg, value })
            }
            DecodedOp::Move(MoveOp::RegMove {
                src,
                dst,
                cond: 31,
                compute,
            }) if src < 16 && dst < 16 => match compute.map(lower_compute) {
                Some(None) => None,
                c => Some(LoweredOp::RegMove {
                    src,
                    dst,
                    compute: c.flatten(),
                }),
            },
            DecodedOp::ShiftImm if bits(d.w, 37, 33) == 31 => shift_imm_params(d.w)
                .filter(|p| pe_supports_shift(p.0))
                .map(|(op, n, x, ry)| LoweredOp::Shift {
                    op,
                    n: n as u8,
                    x: x as u8,
                    ry,
                }),
            _ => None,
        };
        if let Some(n) = native {
            return (len, Some(n), InsnClass::Native);
        }
        let shape = shape_of(&op);
        let class = match op {
            DecodedOp::ComputeCond { .. }
            | DecodedOp::ComputeBoth { .. }
            | DecodedOp::ModifyImm
            | DecodedOp::ShiftImm
            | DecodedOp::ShiftMove
            | DecodedOp::DualMove(_)
            | DecodedOp::Move(_) => InsnClass::Interp(shape),
            _ => InsnClass::Break(shape),
        };
        (len, None, class)
    }

    /// Class of the instruction at `pc` and its length, for the ranking of `aot_rank.rs`.
    pub fn aot_insn_class(&self, pc: u32) -> (u32, InsnClass) {
        let (len, _, class) = self.lower_insn(pc);
        (len, class)
    }

    /// Lowers the straight-line range whose instructions start in `start..=last`, grouping
    /// native ops into batches of at least `min_batch`.
    pub(super) fn lower_hybrid(
        &self,
        start: u32,
        last: u32,
        min_batch: usize,
    ) -> Result<Lowered, String> {
        let (mut pc, mut code, mut items) = (start, Vec::new(), Vec::new());
        let mut pending: Vec<(u32, u32, LoweredOp)> = Vec::new();
        let mut last_start = start;
        let flush = |pending: &mut Vec<(u32, u32, LoweredOp)>, items: &mut Vec<Item>| {
            if pending.len() >= min_batch {
                items.push(Item::Batch(pending.drain(..).map(|p| p.2).collect()));
            } else {
                items.extend(
                    pending
                        .drain(..)
                        .map(|(pc, next, _)| Item::Interp { pc, next }),
                );
            }
        };
        while pc <= last {
            let (len, native, class) = self.lower_insn(pc);
            match class {
                InsnClass::Native => {
                    // a NOP still counts as a step; keep one slot so `steps` == instructions
                    pending.push((
                        pc,
                        pc.wrapping_add(len),
                        native.unwrap_or(LoweredOp::Compute(ComputeOp::Nop)),
                    ));
                }
                InsnClass::Interp(_) => {
                    flush(&mut pending, &mut items);
                    items.push(Item::Interp {
                        pc,
                        next: pc.wrapping_add(len),
                    });
                }
                InsnClass::Break(shape) => {
                    return Err(format!("unsupported op at 0x{pc:x}: {shape}"));
                }
            }
            for k in 0..len {
                code.push(self.parcel(pc.wrapping_add(k)));
            }
            last_start = pc;
            pc = pc.wrapping_add(len);
        }
        flush(&mut pending, &mut items);
        if last_start != last {
            return Err(format!(
                "range end 0x{last:x} is not an instruction start (last start 0x{last_start:x})"
            ));
        }
        let steps = items
            .iter()
            .map(|i| match i {
                Item::Batch(ops) => ops.len() as u32,
                Item::Interp { .. } => 1,
            })
            .sum();
        Ok(Lowered {
            start,
            last,
            exit: pc,
            steps,
            code,
            items,
        })
    }

    /// Native-only lowering (one batch): the `sharc-aot --hot` form of the single-block studies.
    #[cfg(test)]
    pub(super) fn lower_block(&self, start: u32, last: u32) -> Result<Lowered, String> {
        let l = self.lower_hybrid(start, last, 1)?;
        if l.items.iter().any(|i| matches!(i, Item::Interp { .. })) {
            return Err("range holds ops that are not native".to_string());
        }
        Ok(l)
    }
}

/// Short name of an op that is not native: its variant, plus the inner variant for `Flow` and
/// `Move`.
fn shape_of(op: &DecodedOp) -> String {
    let text = format!("{op:?}");
    let mut parts = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty());
    match (parts.next(), parts.next()) {
        (Some(a @ ("Flow" | "Move")), Some(b)) => format!("{a}::{b}"),
        (Some(a), _) => a.to_string(),
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    use super::super::support::core_with;
    use super::*;

    /// R14 = 0x3f800000; F4 = F14 * F14 (FloatMul n=4); NOP
    fn program() -> Vec<u16> {
        vec![
            0x0f0e, 0x3f80, 0x0000, // R14 = 0x3f800000 (48-bit load)
            0x013e, 0x0013, 0x04ee, // F4 = F14 * F14
            0x0001, // NOP
        ]
    }

    fn batch(l: &Lowered) -> &Vec<LoweredOp> {
        match &l.items[0] {
            Item::Batch(ops) => ops,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn lowers_loadimm_float_mul_and_nop_with_code_snapshot() {
        let c = core_with(&program());
        let b = c.lower_block(c.pc, c.pc + 6).unwrap();
        assert_eq!(
            (b.start, b.last, b.exit, b.steps),
            (c.pc, c.pc + 6, c.pc + 7, 3)
        );
        assert_eq!(b.code.len(), 7);
        let ops = batch(&b);
        assert_eq!(
            ops[0],
            LoweredOp::LoadImm {
                reg: 14,
                value: 0x3f80_0000
            }
        );
        assert!(matches!(
            ops[1],
            LoweredOp::Compute(ComputeOp::FloatMul { n: 4, x: 14, y: 14 })
        ));
        assert_eq!(ops[2], LoweredOp::Compute(ComputeOp::Nop));
    }

    #[test]
    fn jumps_break_a_block_and_a_misaligned_end_is_rejected() {
        // JUMP is control flow: it ends a block, it is never lowered
        let c = core_with(&[0x073e, 0x00ff, 0xfffa]);
        assert!(matches!(c.lower_insn(c.pc).2, InsnClass::Break(_)));
        assert!(c.lower_hybrid(c.pc, c.pc, 1).is_err());
        let c = core_with(&program());
        assert!(c.lower_hybrid(c.pc, c.pc + 1, 1).is_err(), "inside an insn");
    }

    #[test]
    fn memory_moves_become_interpreter_items_between_batches() {
        // R14 = imm; R14 = imm; R14 = imm; R0 = DM(I2, M0); R14 = imm x3
        let imm = [0x0f0eu16, 0x3f80, 0x0000];
        let mut p = Vec::new();
        p.extend_from_slice(&imm);
        p.extend_from_slice(&imm);
        p.extend_from_slice(&imm);
        p.push(0x9410); // R0 = DM(I2, M0)
        p.extend_from_slice(&imm);
        p.extend_from_slice(&imm);
        p.extend_from_slice(&imm);
        let c = core_with(&p);
        let last = c.pc + 9 + 1 + 6;
        let (_, _, class) = c.lower_insn(c.pc + 9);
        assert!(matches!(class, InsnClass::Interp(_)), "{class:?}");
        let l = c.lower_hybrid(c.pc, last, MIN_BATCH).unwrap();
        assert_eq!(l.steps, 7);
        assert!(
            matches!(&l.items[..], [Item::Batch(a), Item::Interp { .. }, Item::Batch(b)]
            if a.len() == 3 && b.len() == 3)
        );
        // short batches run as interpreter items
        let short = c.lower_hybrid(c.pc, last, 4).unwrap();
        assert!(short.items.iter().all(|i| matches!(i, Item::Interp { .. })));
        assert!(
            c.lower_block(c.pc, last).is_err(),
            "native-only form refuses it"
        );
    }

    #[test]
    fn describe_range_lists_each_instruction() {
        let c = core_with(&program());
        let rows = c.aot_describe_range(c.pc, c.pc + 6);
        assert_eq!(rows.len(), 3);
        assert!(rows[0].2.contains("LoadImm"), "{rows:?}");
        assert_eq!(rows[2].1, c.pc + 7);
    }

    #[test]
    fn shape_names_keep_the_flow_and_move_variants() {
        assert_eq!(shape_of(&DecodedOp::Nop), "Nop");
        assert_eq!(shape_of(&DecodedOp::Move(MoveOp::Other)), "Move::Other");
    }
}
