//! Level-1 pre-decoded instruction representation and direct-mapped PC cache.
//!
//! Decodes each fetched instruction once into a structured [`DecodedOp`] (foundation for M1 and
//! future Level-2 JIT / Level-3 AOT backends) and caches it per `(pc, code_gen, isa)` so hot DSP
//! loops skip `parcel()` reads, `classify()`, `word48()`, and compute-field bit slicing.
use super::alu::short_op;
use super::{sign_extend, Core};
use crate::visa::{bits, ureg_load, Insn, Kind};

/// Number of slots in the direct-mapped decode cache (power of two).
pub(super) const DECODE_CACHE_SIZE: usize = 4096;
const DECODE_CACHE_MASK: usize = DECODE_CACHE_SIZE - 1;

/// Pre-decoded 23-bit compute operation (`T2a`, `T2b`, and pre-resolved `T2c`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ComputeOp {
    Nop,
    FixedAlu { op: u8, n: u8, x: u8, y: u8 },
    FloatMul { n: u8, x: u8, y: u8 },
    FloatAlu { op: u8, n: u8, x: u8, y: u8 },
    DualAddSub { c: u64 },
    FixedMult { op: u8, n: u8, x: u8, y: u8 },
    Shifter { op: u8, n: u8, x: u8, y: u8 },
    Multifunction { c: u64 },
    Invalid,
}

impl ComputeOp {
    /// Decodes a 23-bit compute field once into a structured [`ComputeOp`].
    #[inline]
    pub(super) fn decode(c: u64) -> Self {
        if c == 0 {
            return Self::Nop;
        }
        if bits(c, 22, 22) == 1 {
            return Self::Multifunction { c };
        }
        let cu = bits(c, 22, 20);
        let op = bits(c, 19, 12) as u8;
        let n = bits(c, 11, 8) as u8;
        let x = bits(c, 7, 4) as u8;
        let y = bits(c, 3, 0) as u8;
        match cu {
            0 if op < 0x80 => {
                if matches!(op >> 4, 0x7) {
                    // 0x70..=0x7F is dual add/sub in fixed-point
                    Self::DualAddSub { c }
                } else {
                    Self::FixedAlu { op, n, x, y }
                }
            }
            0 => {
                if matches!(op >> 4, 0xF) {
                    Self::DualAddSub { c }
                } else {
                    Self::FloatAlu { op, n, x, y }
                }
            }
            1 if op == 0x30 => Self::FloatMul { n, x, y },
            1 => Self::FixedMult { op, n, x, y },
            2 => Self::Shifter { op, n, x, y },
            _ => Self::Invalid,
        }
    }
}

/// Pre-decoded dual data move (`T1a` / `T1b`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DualMoveOp {
    pub dmi: u8,
    pub dmm: u8,
    pub dm_reg: u8,
    pub dm_write: bool,
    pub pmi: u8,
    pub pmm: u8,
    pub pm_reg: u8,
    pub pm_write: bool,
    pub compute: Option<ComputeOp>,
    pub compute_raw: u64,
}

impl DualMoveOp {
    #[inline]
    pub(super) fn decode(kind: Kind, w: u64) -> Self {
        let compute_raw = if kind == Kind::T1a { bits(w, 22, 0) } else { 0 };
        Self {
            dmi: bits(w, 43, 41) as u8,
            dmm: bits(w, 40, 38) as u8,
            dm_reg: bits(w, 36, 33) as u8,
            dm_write: bits(w, 44, 44) == 1,
            pmi: bits(w, 32, 30) as u8 + 8,
            pmm: bits(w, 29, 27) as u8 + 8,
            pm_reg: bits(w, 26, 23) as u8,
            pm_write: bits(w, 37, 37) == 1,
            compute: (kind == Kind::T1a).then(|| ComputeOp::decode(compute_raw)),
            compute_raw,
        }
    }
}

/// Pre-decoded control-flow instruction (`T8a`, `T9a`, `T9b`, `T11a`, `T11c`, `T12a`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FlowOp {
    JumpDirect {
        cond: u8,
        target: u32,
        call: bool,
        delayed: bool,
    },
    JumpIndirect {
        cond: u8,
        reg_i: u8,
        reg_m: u8,
        rel_target: Option<u32>,
        call: bool,
        delayed: bool,
        else_form: bool,
        compute: Option<ComputeOp>,
    },
    Return {
        cond: u8,
        rti: bool,
        delayed: bool,
        else_form: bool,
        compute: Option<ComputeOp>,
    },
    DoUntilCounter {
        end: u32,
        count_ureg: Option<u8>,
        imm_count: u32,
    },
}

impl FlowOp {
    #[inline]
    pub(super) fn decode(kind: Kind, pc: u32, w: u64) -> Self {
        let cond = bits(w, 37, 33) as u8;
        let relative = bits(w, 40, 40) == 1;
        match kind {
            Kind::T8a => {
                let raw = bits(w, 23, 0) as u32;
                let target = if relative {
                    pc.wrapping_add(sign_extend(raw, 24))
                } else {
                    raw
                };
                Self::JumpDirect {
                    cond,
                    target,
                    call: bits(w, 39, 39) == 1,
                    delayed: bits(w, 26, 26) == 1,
                }
            }
            Kind::T9a | Kind::T9b => {
                let is_9a = kind == Kind::T9a;
                let rel_target =
                    relative.then(|| pc.wrapping_add(sign_extend(bits(w, 32, 27) as u32, 6)));
                let compute_raw = if is_9a { bits(w, 22, 0) } else { 0 };
                Self::JumpIndirect {
                    cond,
                    reg_i: 8 + bits(w, 32, 30) as u8,
                    reg_m: 8 + bits(w, 29, 27) as u8,
                    rel_target,
                    call: bits(w, 39, 39) == 1,
                    delayed: bits(w, 26, 26) == 1,
                    else_form: is_9a && bits(w, 25, 25) == 1,
                    compute: (is_9a && compute_raw != 0).then(|| ComputeOp::decode(compute_raw)),
                }
            }
            Kind::T11a | Kind::T11c => {
                let wide = kind == Kind::T11a;
                let compute_raw = if wide { bits(w, 22, 0) } else { 0 };
                Self::Return {
                    cond,
                    rti: bits(w, 40, 40) == 1,
                    delayed: if wide {
                        bits(w, 26, 26) == 1
                    } else {
                        bits(w, 38, 38) == 1
                    },
                    else_form: wide && bits(w, 25, 25) == 1,
                    compute: (wide && compute_raw != 0).then(|| ComputeOp::decode(compute_raw)),
                }
            }
            Kind::T12a => {
                let raw = bits(w, 23, 0) as u32;
                let end = pc.wrapping_add(sign_extend(raw & 0x7F_FFFF, 23));
                let from_reg = bits(w, 40, 40) == 1;
                Self::DoUntilCounter {
                    end,
                    count_ureg: from_reg.then(|| bits(w, 38, 32) as u8),
                    imm_count: bits(w, 39, 24) as u32,
                }
            }
            _ => unreachable!("FlowOp::decode called with non-flow Kind"),
        }
    }
}

/// Pre-decoded data/register transfer (`T3`..`T7`, `T14`..`T18`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MoveOp {
    LoadImm {
        ureg: u8,
        value: u32,
    },
    DirectMem {
        addr: u32,
        ureg: u8,
        write: bool,
    },
    RegSwap {
        r: u8,
        s: u8,
        cond: u8,
        compute: Option<ComputeOp>,
    },
    RegMove {
        src: u8,
        dst: u8,
        cond: u8,
        compute: Option<ComputeOp>,
    },
    DagMem {
        ii: u8,
        mi: u8,
        ureg: u8,
        write: bool,
        post_modify: bool,
        cond: u8,
        compute: Option<ComputeOp>,
    },
    /// Remaining move forms (`T4a`/`T4b`, `T7a`/`T7b`, `T15a`/`T15b`, `T16a`/`T16b`, `T18a`, invalid `T17`).
    Other,
}

impl MoveOp {
    #[inline]
    pub(super) fn decode(insn: &Insn, w: u64) -> Self {
        let kind = insn.kind;
        let with_compute = matches!(kind, Kind::T3a | Kind::T4a | Kind::T5a | Kind::T7a);
        let compute = with_compute.then(|| ComputeOp::decode(bits(w, 22, 0)));
        let cond = bits(w, 37, 33) as u8;
        match kind {
            Kind::T17a | Kind::T17b => match ureg_load(insn) {
                Some((ureg, imm)) => Self::LoadImm {
                    ureg,
                    value: if kind == Kind::T17b {
                        sign_extend(imm, 16)
                    } else {
                        imm
                    },
                },
                None => Self::Other,
            },
            Kind::T14a => Self::DirectMem {
                addr: bits(w, 31, 0) as u32,
                ureg: bits(w, 38, 32) as u8,
                write: bits(w, 40, 40) == 1,
            },
            Kind::T5a | Kind::T5b => {
                if bits(w, 43, 43) == 1 {
                    Self::RegSwap {
                        r: bits(w, 26, 23) as u8,
                        s: bits(w, 41, 38) as u8,
                        cond,
                        compute,
                    }
                } else {
                    Self::RegMove {
                        src: ((bits(w, 42, 38) << 2) | bits(w, 32, 31)) as u8,
                        dst: bits(w, 29, 23) as u8,
                        cond,
                        compute,
                    }
                }
            }
            Kind::T3a | Kind::T3b => {
                let bank = Core::dag_bank(bits(w, 32, 32)) as u8;
                Self::DagMem {
                    ii: bits(w, 43, 41) as u8 + bank,
                    mi: bits(w, 40, 38) as u8 + bank,
                    ureg: bits(w, 29, 23) as u8,
                    write: bits(w, 31, 31) == 1,
                    post_modify: bits(w, 44, 44) == 1,
                    cond,
                    compute,
                }
            }
            Kind::T3c => Self::DagMem {
                ii: bits(w, 43, 41) as u8,
                mi: bits(w, 40, 38) as u8,
                ureg: bits(w, 35, 32) as u8,
                write: bits(w, 37, 37) == 1,
                post_modify: true,
                cond: 31,
                compute: None,
            },
            _ => Self::Other,
        }
    }
}

/// Structured decoded instruction for [`Core::step`] and future JIT/AOT lowering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DecodedOp {
    Nop,
    Idle,
    /// Conditional compute (`T2a`): `cond` is `0..=31` (`31` = unconditional `TRUE`).
    ComputeCond {
        cond: u8,
        op: ComputeOp,
    },
    /// Unconditional compute (`T2b` / `T2c`): runs on both PEs when SIMD is enabled.
    ComputeBoth {
        op: ComputeOp,
    },
    ModifyImm,
    StackOp,
    DoUntil,
    Type25,
    ShiftImm,
    ShiftMove,
    DualMove(DualMoveOp),
    Flow(FlowOp),
    Move(MoveOp),
    Unimplemented,
}

impl DecodedOp {
    /// Decodes a classified instruction and its 48-bit word into a [`DecodedOp`].
    #[inline]
    pub(super) fn from_insn(pc: u32, insn: &Insn, w: u64) -> Self {
        match insn.kind {
            Kind::T21c if insn.parcels[0] == 0x0001 => Self::Nop,
            Kind::T21c if insn.parcels[0] | 0x10 == 0x0091 => Self::Idle,
            Kind::T21 if w & !(1 << 36) == 0x0080_0000_0000 => Self::Idle,
            Kind::T21 if w == 0 => Self::Nop,
            Kind::T19a => Self::ModifyImm,
            Kind::T20a => Self::StackOp,
            Kind::T13a => Self::DoUntil,
            Kind::T25 => Self::Type25,
            Kind::T2a => Self::ComputeCond {
                cond: bits(w, 37, 33) as u8,
                op: ComputeOp::decode(bits(w, 22, 0)),
            },
            Kind::T2b => Self::ComputeBoth {
                op: ComputeOp::decode(bits(w, 38, 16)),
            },
            Kind::T2c => {
                let c = bits(w, 43, 32);
                let (n, x) = (bits(c, 7, 4), bits(c, 3, 0));
                let op = match short_op(bits(c, 11, 8)) {
                    Some(s) => {
                        let (xs, ys) = if s.binary { (n, x) } else { (x, 0) };
                        ComputeOp::decode(
                            (s.unit << 20) | (s.opcode << 12) | (n << 8) | (xs << 4) | ys,
                        )
                    }
                    None => ComputeOp::Invalid,
                };
                Self::ComputeBoth { op }
            }
            Kind::T6n => Self::ShiftImm,
            Kind::T6a => Self::ShiftMove,
            Kind::T1a | Kind::T1b => Self::DualMove(DualMoveOp::decode(insn.kind, w)),
            Kind::T8a | Kind::T9a | Kind::T9b | Kind::T11a | Kind::T11c | Kind::T12a => {
                Self::Flow(FlowOp::decode(insn.kind, pc, w))
            }
            Kind::T17a
            | Kind::T17b
            | Kind::T14a
            | Kind::T15a
            | Kind::T15b
            | Kind::T16a
            | Kind::T16b
            | Kind::T18a
            | Kind::T5a
            | Kind::T5b
            | Kind::T3a
            | Kind::T3b
            | Kind::T3c
            | Kind::T4a
            | Kind::T4b
            | Kind::T7a
            | Kind::T7b => Self::Move(MoveOp::decode(insn, w)),
            _ => Self::Unimplemented,
        }
    }
}

/// One cached instruction entry in [`DecodeCache`].
#[derive(Clone, Copy)]
pub(super) struct CachedInsn {
    pub pc: u32,
    pub gen: u64,
    pub next_pc: u32,
    pub isa: bool,
    pub cond: u8,
    pub insn: Insn,
    pub w: u64,
    pub op: DecodedOp,
}

impl Default for CachedInsn {
    fn default() -> Self {
        Self {
            pc: 0,
            gen: 0,
            next_pc: 0,
            isa: false,
            cond: 31,
            insn: Insn {
                kind: Kind::Unknown,
                parcels: [0; 3],
                len: 0,
            },
            w: 0,
            op: DecodedOp::Unimplemented,
        }
    }
}

/// Direct-mapped instruction decode cache keyed by `pc & DECODE_CACHE_MASK`.
pub(super) struct DecodeCache {
    slots: Box<[CachedInsn; DECODE_CACHE_SIZE]>,
}

impl DecodeCache {
    pub(super) fn new() -> Box<Self> {
        Box::new(Self {
            slots: vec![CachedInsn::default(); DECODE_CACHE_SIZE]
                .into_boxed_slice()
                .try_into()
                .unwrap_or_else(|_| unreachable!()),
        })
    }

    #[inline]
    pub(super) fn get(&self, pc: u32, gen: u64, isa: bool) -> Option<&CachedInsn> {
        let slot = &self.slots[(pc as usize) & DECODE_CACHE_MASK];
        if slot.gen == gen && slot.pc == pc && slot.isa == isa {
            Some(slot)
        } else {
            None
        }
    }

    #[inline]
    pub(super) fn insert(&mut self, entry: CachedInsn) {
        self.slots[(entry.pc as usize) & DECODE_CACHE_MASK] = entry;
    }

    /// Invalidates slots whose instruction could overlap short-word range `first_short..first_short + count`.
    #[inline]
    pub(super) fn invalidate_sram_range(&mut self, first_short: u32, count: u32) {
        let start = first_short.saturating_sub(2);
        let end = first_short.saturating_add(count);
        for s in start..end {
            self.slots[(s as usize) & DECODE_CACHE_MASK].gen = 0;
        }
    }
}

impl Core {
    /// Invalidates cached instructions overlapping `first_short..first_short + count`.
    #[inline]
    pub(super) fn invalidate_sram_cache(&mut self, first_short: u32, count: u32) {
        if let Some(cache) = self.decode_cache.as_deref_mut() {
            cache.invalidate_sram_range(first_short, count);
        }
        #[cfg(feature = "jit")]
        if let Some(jit) = self.jit.as_deref_mut() {
            jit.invalidate_range(first_short, count);
        }
    }

    /// Invalidates the entire decode cache in O(1) by advancing the code generation.
    #[inline]
    pub(super) fn invalidate_code_cache(&mut self) {
        self.code_gen = self.code_gen.wrapping_add(1).max(1);
    }
}

#[cfg(test)]
mod tests {
    use super::super::support::core_with;
    use super::*;

    #[test]
    fn decode_op_extracts_compute_and_short_compute_fields() {
        // T2a: IF TRUE R3 = R1 + R2 -> 0x013e, 0x0000, 0x1312
        let t2a = Insn {
            kind: Kind::T2a,
            parcels: [0x013e, 0x0000, 0x1312],
            len: 3,
        };
        let w = crate::visa::word48(&t2a.parcels);
        assert_eq!(
            DecodedOp::from_insn(0, &t2a, w),
            DecodedOp::ComputeCond {
                cond: 31,
                op: ComputeOp::FixedAlu {
                    op: 0x01,
                    n: 3,
                    x: 1,
                    y: 2,
                },
            }
        );

        // T2c: R1 = R1 - R2 (short op 1, n=1, x=2 -> 0x0112)
        let t2c = Insn {
            kind: Kind::T2c,
            parcels: [0xc112, 0, 0],
            len: 1,
        };
        let w_short = crate::visa::word48(&t2c.parcels[..1]);
        assert_eq!(
            DecodedOp::from_insn(0, &t2c, w_short),
            DecodedOp::ComputeBoth {
                op: ComputeOp::FixedAlu {
                    op: 0x02,
                    n: 1,
                    x: 1,
                    y: 2,
                },
            }
        );
    }

    #[test]
    fn decode_op_extracts_move_dualmove_and_flow_fields() {
        // T17b: R1 = 7 -> 0x0F81, 0x0007
        let t17b = Insn {
            kind: Kind::T17b,
            parcels: [0x0F81, 0x0007, 0],
            len: 2,
        };
        assert_eq!(
            DecodedOp::from_insn(0, &t17b, crate::visa::word48(&t17b.parcels[..2])),
            DecodedOp::Move(MoveOp::LoadImm { ureg: 1, value: 7 })
        );

        // T1a: MRF=MRF+r0*r4(ssf), r0=dm(i6,m3), r4=pm(i10,m10) -> 0x2cc0, 0x921b, 0xc004
        let t1a = Insn {
            kind: Kind::T1a,
            parcels: [0x2cc0, 0x921b, 0xc004],
            len: 3,
        };
        let DecodedOp::DualMove(dm) =
            DecodedOp::from_insn(0, &t1a, crate::visa::word48(&t1a.parcels))
        else {
            panic!("expected DualMove");
        };
        assert_eq!((dm.dmi, dm.dmm, dm.dm_reg, dm.dm_write), (6, 3, 0, false));
        assert_eq!((dm.pmi, dm.pmm, dm.pm_reg, dm.pm_write), (10, 10, 4, false));
        assert!(dm.compute.is_some());

        // T12a: DO (pc, 3) UNTIL LCE, LCNTR = R0 -> 0x0d00, 0x0000, 0x0003
        let t12a = Insn {
            kind: Kind::T12a,
            parcels: [0x0d00, 0x0000, 0x0003],
            len: 3,
        };
        assert_eq!(
            DecodedOp::from_insn(0x10, &t12a, crate::visa::word48(&t12a.parcels)),
            DecodedOp::Flow(FlowOp::DoUntilCounter {
                end: 0x13,
                count_ureg: Some(0),
                imm_count: 0,
            })
        );
    }

    #[test]
    fn decode_cache_hits_and_invalidates_on_sram_write() {
        // Two NOPs at CODE, then overwrite the second NOP with a faultable/different word via dm_write.
        let mut c = core_with(&[0x0001, 0x0001, 0x0001]);
        assert_eq!(c.step(), Ok(()));
        let gen_before = c.code_gen;
        assert!(c
            .decode_cache
            .as_ref()
            .unwrap()
            .get(c.pc - 1, gen_before, false)
            .is_some());

        // Overwrite short word CODE + 1 via normal-word dm_write at CODE / 2.
        c.dm_write(c.pc / 2, 0x0001_0001).unwrap();
        assert!(c
            .decode_cache
            .as_ref()
            .unwrap()
            .get(c.pc - 1, gen_before, false)
            .is_none());
    }

    #[test]
    fn decode_cache_invalidates_on_isa_reload() {
        let mut c = Core::new(&[]);
        let g0 = c.code_gen;
        c.load_isa(&[0u8; 6]);
        assert_ne!(c.code_gen, g0);
    }
}
