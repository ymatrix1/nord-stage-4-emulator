//! Level-2 Cranelift JIT backend for [`Core`], compiling basic blocks and counter loops
//! directly from [`DecodedOp`] to host machine code.
//!
//! Enabled via the optional `jit` Cargo feature (`Core::run_jit`). Any instruction or dynamic
//! condition not yet lowered to Cranelift IR either calls an `extern "C"` helper on `Core` or
//! hands control back to [`Core::step`], preserving 100% oracle fidelity.
use super::decode_cache::{ComputeOp, DecodedOp, DualMoveOp, MoveOp};
use super::exec::Decoded;
use super::float::{MODE1_RND32, MODE1_TRUNC};
use super::{Core, Fault, Flags};
use crate::visa::{Insn, Kind};
use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{types, AbiParam, InstBuilder, MemFlags, UserFuncName};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};
use std::collections::HashMap;
use std::mem::offset_of;

const MAX_BLOCK_INSNS: usize = 64;

/// Compiled native function for a SHARC basic block or self-looping `DO ... UNTIL LCE` body.
///
/// Signature: `(core: *mut Core, max_iters: u64, remaining_ptr: *mut u32, sram_ptr: *mut u8) -> u64`
/// (executed steps).
type JitBlockFn = unsafe extern "C" fn(*mut Core, u64, *mut u32, *mut u8) -> u64;

struct CompiledBlock {
    func: JitBlockFn,
    end_pc: u32,
    len_insns: u64,
    is_loop: bool,
}

/// Cranelift JIT module and compiled block cache owned lazily by [`Core`].
pub(super) struct JitEngine {
    module: JITModule,
    fn_ctx: FunctionBuilderContext,
    code_gen: u64,
    code_modified: bool,
    blocks: HashMap<(u32, u32, bool, bool, bool), CompiledBlock>,
    fn_float_mul: FuncId,
    fn_fixed_mult: FuncId,
    fn_mult_ov: FuncId,
    fn_fixed_av: FuncId,
    fn_exec_compute: FuncId,
    fn_write_ureg: FuncId,
    fn_reg_move: FuncId,
    fn_dag_mem: FuncId,
    fn_dual_move: FuncId,
    next_id: u64,
}

/// Executes `FloatMul { n, x, y }` directly from JIT-compiled code.
unsafe extern "C" fn jit_float_mul(core: *mut Core, n: u32, x: u32, y: u32) {
    let c = unsafe { &mut *core };
    c.float_mul(n as usize, x as usize, y as usize);
}

/// Executes `FixedMult { op, n, x, y }` directly from JIT-compiled code.
unsafe extern "C" fn jit_fixed_mult(core: *mut Core, op: u32, n: u32, x: u32, y: u32) {
    let c = unsafe { &mut *core };
    c.fixed_mult(u64::from(op), n as usize, x as usize, y as usize);
}

/// Records multiplier overflow (`STKY_MOS`) from JIT-compiled `FixedMult`.
#[cold]
unsafe extern "C" fn jit_note_mult_overflow(core: *mut Core) {
    let c = unsafe { &mut *core };
    c.note_arith_exception(false, 1 << 6);
}

/// Handles `FixedAlu` overflow (`AV`) or `ALUSAT` saturation from JIT-compiled `ADD`/`SUB`.
#[cold]
unsafe extern "C" fn jit_handle_fixed_av(core: *mut Core, n: u32, rx: u32, ry: u32, is_sub: u8) {
    let c = unsafe { &mut *core };
    let op = if is_sub != 0 { 0x02 } else { 0x01 };
    c.fixed_alu(op, n as usize, rx, ry);
}

/// Executes a pre-decoded 23-bit compute word (`raw_c`) on `Core`, returning `1` on fault.
unsafe extern "C" fn jit_exec_compute(core: *mut Core, raw_c: u64) -> u32 {
    let c = unsafe { &mut *core };
    u32::from(!c.compute(raw_c))
}

/// Writes an immediate value to universal register `ureg` in SISD mode.
unsafe extern "C" fn jit_write_ureg(core: *mut Core, ureg: u8, value: u32) {
    let c = unsafe { &mut *core };
    c.write_ureg(ureg, value);
}

/// Executes an unconditional `RegMove` (`dst = src`) in SISD mode.
unsafe extern "C" fn jit_reg_move(core: *mut Core, src: u8, dst: u8) {
    let c = unsafe { &mut *core };
    let cap = c.capture_move(src);
    c.apply_move_if(dst, src, &cap, true, false);
}

/// Executes an unconditional `MoveOp::DagMem` in SISD mode.
/// Returns `0` on ok, `1` on fault, `2` if a write invalidated compiled JIT code.
unsafe extern "C" fn jit_dag_mem(core: *mut Core, pc: u32, w: u64, kind_u8: u8) -> u32 {
    let c = unsafe { &mut *core };
    let kind = match kind_u8 {
        0 => Kind::T3a,
        1 => Kind::T3b,
        _ => Kind::T3c,
    };
    let insn = Insn {
        kind,
        parcels: [0; 3],
        len: if kind == Kind::T3c { 1 } else { 3 },
    };
    let d = Decoded {
        pc,
        insn,
        w,
        cond_ok: true,
        cond_y: false,
    };
    let mv = MoveOp::decode(&insn, w);
    if c.exec_move_op(&d, mv).is_err() {
        return 1;
    }
    if c.jit.as_ref().is_some_and(|j| j.code_modified) {
        return 2;
    }
    0
}

/// Executes a `DualMoveOp` (`T1a` / `T1b`) in SISD mode.
/// Returns `0` on ok, `1` on fault, `2` if a write invalidated compiled JIT code.
unsafe extern "C" fn jit_dual_move(core: *mut Core, pc: u32, w: u64, is_t1a: u8) -> u32 {
    let c = unsafe { &mut *core };
    let kind = if is_t1a != 0 { Kind::T1a } else { Kind::T1b };
    let d = Decoded {
        pc,
        insn: Insn {
            kind,
            parcels: [0; 3],
            len: 3,
        },
        w,
        cond_ok: true,
        cond_y: false,
    };
    let dm = DualMoveOp::decode(kind, w);
    if c.exec_dual_move_op(&d, dm).is_err() {
        return 1;
    }
    if c.jit.as_ref().is_some_and(|j| j.code_modified) {
        return 2;
    }
    0
}

impl JitEngine {
    fn new(code_gen: u64) -> Box<Self> {
        let mut flag_builder = settings::builder();
        let _ = flag_builder.set("opt_level", "speed");
        let _ = flag_builder.set("use_colocated_libcalls", "false");
        let _ = flag_builder.set("is_pic", "false");
        let isa_builder = cranelift_native::builder().expect("host ISA supported by Cranelift");
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .expect("valid Cranelift ISA");

        let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
        builder.symbol("jit_float_mul", jit_float_mul as *const u8);
        builder.symbol("jit_fixed_mult", jit_fixed_mult as *const u8);
        builder.symbol(
            "jit_note_mult_overflow",
            jit_note_mult_overflow as *const u8,
        );
        builder.symbol("jit_handle_fixed_av", jit_handle_fixed_av as *const u8);
        builder.symbol("jit_exec_compute", jit_exec_compute as *const u8);
        builder.symbol("jit_write_ureg", jit_write_ureg as *const u8);
        builder.symbol("jit_reg_move", jit_reg_move as *const u8);
        builder.symbol("jit_dag_mem", jit_dag_mem as *const u8);
        builder.symbol("jit_dual_move", jit_dual_move as *const u8);

        let mut module = JITModule::new(builder);
        let ptr_ty = module.target_config().pointer_type();

        let mut sig_fmul = module.make_signature();
        sig_fmul.params.push(AbiParam::new(ptr_ty));
        sig_fmul.params.push(AbiParam::new(types::I32));
        sig_fmul.params.push(AbiParam::new(types::I32));
        sig_fmul.params.push(AbiParam::new(types::I32));
        let fn_float_mul = module
            .declare_function("jit_float_mul", Linkage::Import, &sig_fmul)
            .expect("declare jit_float_mul");

        let mut sig_fmult = module.make_signature();
        sig_fmult.params.push(AbiParam::new(ptr_ty));
        sig_fmult.params.push(AbiParam::new(types::I32));
        sig_fmult.params.push(AbiParam::new(types::I32));
        sig_fmult.params.push(AbiParam::new(types::I32));
        sig_fmult.params.push(AbiParam::new(types::I32));
        let fn_fixed_mult = module
            .declare_function("jit_fixed_mult", Linkage::Import, &sig_fmult)
            .expect("declare jit_fixed_mult");

        let mut sig_mov = module.make_signature();
        sig_mov.params.push(AbiParam::new(ptr_ty));
        let fn_mult_ov = module
            .declare_function("jit_note_mult_overflow", Linkage::Import, &sig_mov)
            .expect("declare jit_note_mult_overflow");

        let mut sig_av = module.make_signature();
        sig_av.params.push(AbiParam::new(ptr_ty));
        sig_av.params.push(AbiParam::new(types::I32));
        sig_av.params.push(AbiParam::new(types::I32));
        sig_av.params.push(AbiParam::new(types::I32));
        sig_av.params.push(AbiParam::new(types::I8));
        let fn_fixed_av = module
            .declare_function("jit_handle_fixed_av", Linkage::Import, &sig_av)
            .expect("declare jit_handle_fixed_av");

        let mut sig_comp = module.make_signature();
        sig_comp.params.push(AbiParam::new(ptr_ty));
        sig_comp.params.push(AbiParam::new(types::I64));
        sig_comp.returns.push(AbiParam::new(types::I32));
        let fn_exec_compute = module
            .declare_function("jit_exec_compute", Linkage::Import, &sig_comp)
            .expect("declare jit_exec_compute");

        let mut sig_wureg = module.make_signature();
        sig_wureg.params.push(AbiParam::new(ptr_ty));
        sig_wureg.params.push(AbiParam::new(types::I8));
        sig_wureg.params.push(AbiParam::new(types::I32));
        let fn_write_ureg = module
            .declare_function("jit_write_ureg", Linkage::Import, &sig_wureg)
            .expect("declare jit_write_ureg");

        let mut sig_rmove = module.make_signature();
        sig_rmove.params.push(AbiParam::new(ptr_ty));
        sig_rmove.params.push(AbiParam::new(types::I8));
        sig_rmove.params.push(AbiParam::new(types::I8));
        let fn_reg_move = module
            .declare_function("jit_reg_move", Linkage::Import, &sig_rmove)
            .expect("declare jit_reg_move");

        let mut sig_mem = module.make_signature();
        sig_mem.params.push(AbiParam::new(ptr_ty));
        sig_mem.params.push(AbiParam::new(types::I32));
        sig_mem.params.push(AbiParam::new(types::I64));
        sig_mem.params.push(AbiParam::new(types::I8));
        sig_mem.returns.push(AbiParam::new(types::I32));
        let fn_dag_mem = module
            .declare_function("jit_dag_mem", Linkage::Import, &sig_mem)
            .expect("declare jit_dag_mem");
        let fn_dual_move = module
            .declare_function("jit_dual_move", Linkage::Import, &sig_mem)
            .expect("declare jit_dual_move");

        Box::new(Self {
            module,
            fn_ctx: FunctionBuilderContext::new(),
            code_gen,
            code_modified: false,
            blocks: HashMap::new(),
            fn_float_mul,
            fn_fixed_mult,
            fn_mult_ov,
            fn_fixed_av,
            fn_exec_compute,
            fn_write_ureg,
            fn_reg_move,
            fn_dag_mem,
            fn_dual_move,
            next_id: 0,
        })
    }

    pub(super) fn invalidate_range(&mut self, first_short: u32, count: u32) {
        let before = self.blocks.len();
        let write_end = first_short.saturating_add(count);
        self.blocks.retain(|&(start_pc, _, _, _, _), blk| {
            blk.end_pc + 3 <= first_short || start_pc >= write_end
        });
        if self.blocks.len() != before {
            self.code_modified = true;
        }
    }
}

/// Checks whether a [`DecodedOp`] can be compiled into a JIT basic block without touching PC/loops.
fn can_jit_op(op: DecodedOp, simd: bool) -> bool {
    if simd {
        return false;
    }
    match op {
        DecodedOp::Nop => true,
        DecodedOp::ComputeCond { cond: 31, op } | DecodedOp::ComputeBoth { op } => !matches!(
            op,
            ComputeOp::Invalid | ComputeOp::Multifunction { .. } | ComputeOp::DualAddSub { .. }
        ),
        DecodedOp::Move(MoveOp::LoadImm { .. }) => true,
        DecodedOp::Move(MoveOp::RegMove {
            cond: 31,
            compute: None,
            ..
        }) => true,
        DecodedOp::Move(MoveOp::DagMem { cond: 31, .. }) => true,
        DecodedOp::DualMove(_) => true,
        _ => false,
    }
}

/// Reconstructs a 23-bit compute word from [`ComputeOp`] for fallback `jit_exec_compute`.
fn compute_op_to_raw(op: ComputeOp) -> Option<u64> {
    match op {
        ComputeOp::Nop => Some(0),
        ComputeOp::FixedAlu { op, n, x, y } => {
            Some((u64::from(op) << 12) | (u64::from(n) << 8) | (u64::from(x) << 4) | u64::from(y))
        }
        ComputeOp::FloatAlu { op, n, x, y } => {
            Some((u64::from(op) << 12) | (u64::from(n) << 8) | (u64::from(x) << 4) | u64::from(y))
        }
        ComputeOp::FloatMul { n, x, y } => Some(
            (1 << 20) | (0x30 << 12) | (u64::from(n) << 8) | (u64::from(x) << 4) | u64::from(y),
        ),
        ComputeOp::FixedMult { op, n, x, y } => Some(
            (1 << 20)
                | (u64::from(op) << 12)
                | (u64::from(n) << 8)
                | (u64::from(x) << 4)
                | u64::from(y),
        ),
        ComputeOp::Shifter { op, n, x, y } => Some(
            (2 << 20)
                | (u64::from(op) << 12)
                | (u64::from(n) << 8)
                | (u64::from(x) << 4)
                | u64::from(y),
        ),
        ComputeOp::DualAddSub { c } | ComputeOp::Multifunction { c } => Some(c),
        ComputeOp::Invalid => None,
    }
}

/// Emits Cranelift IR for an inline SISD `ADD` (`is_sub = false`) or `SUB` (`is_sub = true`),
/// falling back to `jit_handle_fixed_av` only on signed overflow (`AV != 0`).
#[allow(clippy::too_many_arguments)]
fn emit_fixed_add_sub(
    b: &mut FunctionBuilder,
    core_ptr: cranelift_codegen::ir::Value,
    n: u8,
    x: u8,
    y: u8,
    is_sub: bool,
    fn_av_ref: cranelift_codegen::ir::FuncRef,
) {
    let mem = MemFlags::trusted();
    let off_r = offset_of!(Core, r) as i32;
    let off_fx_lo = (offset_of!(Core, fx) + offset_of!(super::float_ops::ExtFile, lo)) as i32;
    let off_flags = offset_of!(Core, flags) as i32;
    let off_astat0 =
        (offset_of!(Core, sys) + offset_of!(super::sysregs::SysState, astat_extra)) as i32;

    let rx = b
        .ins()
        .load(types::I32, mem, core_ptr, off_r + 4 * i32::from(x));
    let ry = b
        .ins()
        .load(types::I32, mem, core_ptr, off_r + 4 * i32::from(y));
    let rx64 = b.ins().uextend(types::I64, rx);
    let sum64 = if is_sub {
        let nry = b.ins().bnot(ry);
        let nry64 = b.ins().uextend(types::I64, nry);
        let t = b.ins().iadd(rx64, nry64);
        b.ins().iadd_imm(t, 1)
    } else {
        let ry64 = b.ins().uextend(types::I64, ry);
        b.ins().iadd(rx64, ry64)
    };
    let res = b.ins().ireduce(types::I32, sum64);

    // Store r[n] and clear fx.lo[n].
    b.ins().store(mem, res, core_ptr, off_r + 4 * i32::from(n));
    let zero8 = b.ins().iconst(types::I8, 0);
    b.ins()
        .store(mem, zero8, core_ptr, off_fx_lo + i32::from(n));

    // Flags: az, an, ac, ai=0, as_=0, af=0.
    let az = b.ins().icmp_imm(IntCC::Equal, res, 0);
    let an = b.ins().icmp_imm(IntCC::SignedLessThan, res, 0);
    let hi = b.ins().ushr_imm(sum64, 32);
    let ac = b.ins().icmp_imm(IntCC::NotEqual, hi, 0);

    b.ins()
        .store(mem, az, core_ptr, off_flags + offset_of!(Flags, az) as i32);
    b.ins()
        .store(mem, an, core_ptr, off_flags + offset_of!(Flags, an) as i32);
    b.ins()
        .store(mem, ac, core_ptr, off_flags + offset_of!(Flags, ac) as i32);
    b.ins().store(
        mem,
        zero8,
        core_ptr,
        off_flags + offset_of!(Flags, ai) as i32,
    );
    b.ins().store(
        mem,
        zero8,
        core_ptr,
        off_flags + offset_of!(Flags, as_) as i32,
    );

    // Clear AF (ASTAT bit 10) in sys.astat_extra[0].
    let astat0 = b.ins().load(types::I32, mem, core_ptr, off_astat0);
    let astat_cleared = b.ins().band_imm(astat0, !(1i64 << 10));
    b.ins().store(mem, astat_cleared, core_ptr, off_astat0);

    // Signed overflow AV: ((rx ^ res) & (ry ^ res)) < 0 for ADD, ((rx ^ ry) & (rx ^ res)) < 0 for SUB.
    let av_word = if is_sub {
        let a = b.ins().bxor(rx, ry);
        let c = b.ins().bxor(rx, res);
        b.ins().band(a, c)
    } else {
        let a = b.ins().bxor(rx, res);
        let c = b.ins().bxor(ry, res);
        b.ins().band(a, c)
    };
    let av = b.ins().icmp_imm(IntCC::SignedLessThan, av_word, 0);
    b.ins()
        .store(mem, av, core_ptr, off_flags + offset_of!(Flags, av) as i32);

    let ov_blk = b.create_block();
    let cont_blk = b.create_block();
    b.ins().brif(av, ov_blk, &[], cont_blk, &[]);

    b.switch_to_block(ov_blk);
    b.seal_block(ov_blk);
    let n_val = b.ins().iconst(types::I32, i64::from(n));
    let sub_val = b.ins().iconst(types::I8, i64::from(is_sub));
    b.ins().call(fn_av_ref, &[core_ptr, n_val, rx, ry, sub_val]);
    b.ins().jump(cont_blk, &[]);

    b.switch_to_block(cont_blk);
    b.seal_block(cont_blk);
}

/// Emits Cranelift IR for `PASS`, `AND`, `OR`, `XOR` (no carry/overflow).
fn emit_fixed_logic_or_pass(
    b: &mut FunctionBuilder,
    core_ptr: cranelift_codegen::ir::Value,
    op: u8,
    n: u8,
    x: u8,
    y: u8,
) {
    let mem = MemFlags::trusted();
    let off_r = offset_of!(Core, r) as i32;
    let off_fx_lo = (offset_of!(Core, fx) + offset_of!(super::float_ops::ExtFile, lo)) as i32;
    let off_flags = offset_of!(Core, flags) as i32;
    let off_astat0 =
        (offset_of!(Core, sys) + offset_of!(super::sysregs::SysState, astat_extra)) as i32;

    let rx = b
        .ins()
        .load(types::I32, mem, core_ptr, off_r + 4 * i32::from(x));
    let res = if op == 0x21 {
        rx
    } else {
        let ry = b
            .ins()
            .load(types::I32, mem, core_ptr, off_r + 4 * i32::from(y));
        match op {
            0x40 => b.ins().band(rx, ry),
            0x41 => b.ins().bor(rx, ry),
            _ => b.ins().bxor(rx, ry),
        }
    };
    b.ins().store(mem, res, core_ptr, off_r + 4 * i32::from(n));
    let zero8 = b.ins().iconst(types::I8, 0);
    b.ins()
        .store(mem, zero8, core_ptr, off_fx_lo + i32::from(n));

    let az = b.ins().icmp_imm(IntCC::Equal, res, 0);
    let an = b.ins().icmp_imm(IntCC::SignedLessThan, res, 0);
    b.ins()
        .store(mem, az, core_ptr, off_flags + offset_of!(Flags, az) as i32);
    b.ins()
        .store(mem, an, core_ptr, off_flags + offset_of!(Flags, an) as i32);
    b.ins().store(
        mem,
        zero8,
        core_ptr,
        off_flags + offset_of!(Flags, av) as i32,
    );
    b.ins().store(
        mem,
        zero8,
        core_ptr,
        off_flags + offset_of!(Flags, ac) as i32,
    );
    b.ins().store(
        mem,
        zero8,
        core_ptr,
        off_flags + offset_of!(Flags, ai) as i32,
    );
    b.ins().store(
        mem,
        zero8,
        core_ptr,
        off_flags + offset_of!(Flags, as_) as i32,
    );

    let astat0 = b.ins().load(types::I32, mem, core_ptr, off_astat0);
    let astat_cleared = b.ins().band_imm(astat0, !(1i64 << 10));
    b.ins().store(mem, astat_cleared, core_ptr, off_astat0);
}

#[derive(Clone, Copy)]
enum FastFloatKind {
    Mul,
    Add,
    Sub,
}

/// Emits native IEEE-754 `f32` arithmetic in Cranelift IR when `Core::fast_float` is enabled.
fn emit_fast_float_op(
    b: &mut FunctionBuilder,
    core_ptr: cranelift_codegen::ir::Value,
    kind: FastFloatKind,
    n: u8,
    x: u8,
    y: u8,
) {
    let mem = MemFlags::trusted();
    let off_r = offset_of!(Core, r) as i32;
    let off_fx_lo = (offset_of!(Core, fx) + offset_of!(super::float_ops::ExtFile, lo)) as i32;
    let off_flags = offset_of!(Core, flags) as i32;
    let off_astat0 =
        (offset_of!(Core, sys) + offset_of!(super::sysregs::SysState, astat_extra)) as i32;

    let fx = b
        .ins()
        .load(types::F32, mem, core_ptr, off_r + 4 * i32::from(x));
    let fy = b
        .ins()
        .load(types::F32, mem, core_ptr, off_r + 4 * i32::from(y));
    let res = match kind {
        FastFloatKind::Mul => b.ins().fmul(fx, fy),
        FastFloatKind::Add => b.ins().fadd(fx, fy),
        FastFloatKind::Sub => b.ins().fsub(fx, fy),
    };
    b.ins().store(mem, res, core_ptr, off_r + 4 * i32::from(n));
    let zero8 = b.ins().iconst(types::I8, 0);
    b.ins()
        .store(mem, zero8, core_ptr, off_fx_lo + i32::from(n));

    let zero_f = b.ins().f32const(0.0);
    let az = b.ins().fcmp(FloatCC::Equal, res, zero_f);
    let res_i = b.ins().bitcast(types::I32, MemFlags::new(), res);
    let an = b.ins().icmp_imm(IntCC::SignedLessThan, res_i, 0);
    b.ins()
        .store(mem, az, core_ptr, off_flags + offset_of!(Flags, az) as i32);
    b.ins()
        .store(mem, an, core_ptr, off_flags + offset_of!(Flags, an) as i32);
    b.ins().store(
        mem,
        zero8,
        core_ptr,
        off_flags + offset_of!(Flags, av) as i32,
    );
    b.ins().store(
        mem,
        zero8,
        core_ptr,
        off_flags + offset_of!(Flags, ai) as i32,
    );
    b.ins().store(
        mem,
        zero8,
        core_ptr,
        off_flags + offset_of!(Flags, as_) as i32,
    );

    if !matches!(kind, FastFloatKind::Mul) {
        let astat0 = b.ins().load(types::I32, mem, core_ptr, off_astat0);
        let astat_set = b.ins().bor_imm(astat0, 1i64 << 10);
        b.ins().store(mem, astat_set, core_ptr, off_astat0);
    }
}

/// Exact guarded native `f64` fast path for `FloatMul`, `FADD`, and `FSUB` when `fast_float` is
/// disabled. Executes inline only when `(mode1 & (MODE1_TRUNC | MODE1_RND32)) == 0`, both inputs
/// are normal `f32` values with zero 8-bit extension (`fx.lo == 0 || fx.tag != r`), exponent
/// difference `<= 28` for `Add`/`Sub`, and the `f64` result is a normal 40-bit value with zero
/// 21-bit rounding tail; otherwise falls back to `jit_float_mul` / `jit_exec_compute` for 1:1
/// hardware fidelity.
#[allow(clippy::too_many_arguments)]
fn emit_exact_float_op(
    b: &mut FunctionBuilder,
    core_ptr: cranelift_codegen::ir::Value,
    kind: FastFloatKind,
    n: u8,
    x: u8,
    y: u8,
    raw_c: u64,
    fn_fmul_ref: cranelift_codegen::ir::FuncRef,
    fn_comp_ref: cranelift_codegen::ir::FuncRef,
) {
    let mem = MemFlags::trusted();
    let off_r = offset_of!(Core, r) as i32;
    let off_fx_tag = (offset_of!(Core, fx) + offset_of!(super::float_ops::ExtFile, tag)) as i32;
    let off_fx_lo = (offset_of!(Core, fx) + offset_of!(super::float_ops::ExtFile, lo)) as i32;
    let off_flags = offset_of!(Core, flags) as i32;
    let off_astat0 =
        (offset_of!(Core, sys) + offset_of!(super::sysregs::SysState, astat_extra)) as i32;
    let off_mode1 = offset_of!(Core, mode1) as i32;

    let mode1 = b.ins().load(types::I32, mem, core_ptr, off_mode1);
    let mode_mask = i64::from(MODE1_TRUNC | MODE1_RND32);
    let mode_masked = b.ins().band_imm(mode1, mode_mask);
    let mode_ok = b.ins().icmp_imm(IntCC::Equal, mode_masked, 0);

    let rx = b
        .ins()
        .load(types::I32, mem, core_ptr, off_r + 4 * i32::from(x));
    let ry = b
        .ins()
        .load(types::I32, mem, core_ptr, off_r + 4 * i32::from(y));
    let tag_x = b
        .ins()
        .load(types::I32, mem, core_ptr, off_fx_tag + 4 * i32::from(x));
    let tag_y = b
        .ins()
        .load(types::I32, mem, core_ptr, off_fx_tag + 4 * i32::from(y));
    let lo_x = b
        .ins()
        .load(types::I8, mem, core_ptr, off_fx_lo + i32::from(x));
    let lo_y = b
        .ins()
        .load(types::I8, mem, core_ptr, off_fx_lo + i32::from(y));

    let tag_x_eq = b.ins().icmp(IntCC::Equal, tag_x, rx);
    let lo_x_nz = b.ins().icmp_imm(IntCC::NotEqual, lo_x, 0);
    let has_ext_x = b.ins().band(tag_x_eq, lo_x_nz);
    let tag_y_eq = b.ins().icmp(IntCC::Equal, tag_y, ry);
    let lo_y_nz = b.ins().icmp_imm(IntCC::NotEqual, lo_y, 0);
    let has_ext_y = b.ins().band(tag_y_eq, lo_y_nz);
    let no_ext = b.ins().bor(has_ext_x, has_ext_y);
    let ext_ok = b.ins().icmp_imm(IntCC::Equal, no_ext, 0);

    let rx_sh = b.ins().ushr_imm(rx, 23);
    let exp_x = b.ins().band_imm(rx_sh, 0xFF);
    let ry_sh = b.ins().ushr_imm(ry, 23);
    let exp_y = b.ins().band_imm(ry_sh, 0xFF);
    let exp_xm1 = b.ins().iadd_imm(exp_x, -1);
    let norm_x = b.ins().icmp_imm(IntCC::UnsignedLessThan, exp_xm1, 254);
    let exp_ym1 = b.ins().iadd_imm(exp_y, -1);
    let norm_y = b.ins().icmp_imm(IntCC::UnsignedLessThan, exp_ym1, 254);
    let norm_xy = b.ins().band(norm_x, norm_y);
    let mut pre_ok = b.ins().band(mode_ok, ext_ok);
    pre_ok = b.ins().band(pre_ok, norm_xy);
    if !matches!(kind, FastFloatKind::Mul) {
        let diff = b.ins().isub(exp_x, exp_y);
        let diff_off = b.ins().iadd_imm(diff, 28);
        let diff_ok = b.ins().icmp_imm(IntCC::UnsignedLessThan, diff_off, 57);
        pre_ok = b.ins().band(pre_ok, diff_ok);
    }

    let check_blk = b.create_block();
    let slow_blk = b.create_block();
    let done_blk = b.create_block();
    b.ins().brif(pre_ok, check_blk, &[], slow_blk, &[]);

    b.switch_to_block(check_blk);
    b.seal_block(check_blk);
    let fx32 = b.ins().bitcast(types::F32, MemFlags::new(), rx);
    let fy32 = b.ins().bitcast(types::F32, MemFlags::new(), ry);
    let fx64 = b.ins().fpromote(types::F64, fx32);
    let fy64 = b.ins().fpromote(types::F64, fy32);
    let res64 = match kind {
        FastFloatKind::Mul => b.ins().fmul(fx64, fy64),
        FastFloatKind::Add => b.ins().fadd(fx64, fy64),
        FastFloatKind::Sub => b.ins().fsub(fx64, fy64),
    };
    let res_bits = b.ins().bitcast(types::I64, MemFlags::new(), res64);
    let tail = b.ins().band_imm(res_bits, 0x1F_FFFF);
    let tail_zero = b.ins().icmp_imm(IntCC::Equal, tail, 0);
    let exp64_sh = b.ins().ushr_imm(res_bits, 52);
    let exp64 = b.ins().band_imm(exp64_sh, 0x7FF);
    let exp32_64 = b.ins().iadd_imm(exp64, -896);
    let exp32_m1 = b.ins().iadd_imm(exp32_64, -1);
    let res_norm = b.ins().icmp_imm(IntCC::UnsignedLessThan, exp32_m1, 254);
    let post_ok = b.ins().band(tail_zero, res_norm);

    let fast_blk = b.create_block();
    b.ins().brif(post_ok, fast_blk, &[], slow_blk, &[]);

    b.switch_to_block(fast_blk);
    b.seal_block(fast_blk);
    let res_hi32 = b.ins().ushr_imm(res_bits, 32);
    let sign32 = b.ins().band_imm(res_hi32, 0x8000_0000u32 as i64);
    let exp32_sh = b.ins().ishl_imm(exp32_64, 23);
    let mant23_sh = b.ins().ushr_imm(res_bits, 29);
    let mant23 = b.ins().band_imm(mant23_sh, 0x7F_FFFF);
    let em = b.ins().bor(exp32_sh, mant23);
    let r_full64 = b.ins().bor(sign32, em);
    let r_res = b.ins().ireduce(types::I32, r_full64);
    let lo_sh = b.ins().ushr_imm(res_bits, 21);
    let lo_res = b.ins().ireduce(types::I8, lo_sh);

    b.ins()
        .store(mem, r_res, core_ptr, off_r + 4 * i32::from(n));
    b.ins()
        .store(mem, r_res, core_ptr, off_fx_tag + 4 * i32::from(n));
    b.ins()
        .store(mem, lo_res, core_ptr, off_fx_lo + i32::from(n));

    let an = b.ins().icmp_imm(IntCC::SignedLessThan, r_res, 0);
    match kind {
        FastFloatKind::Mul => {
            let mn32 = b.ins().uextend(types::I32, an);
            let mn_sh = b.ins().ishl_imm(mn32, 6);
            let extra = b.ins().load(types::I32, mem, core_ptr, off_astat0);
            let cleared = b.ins().band_imm(extra, !(0xFi64 << 6));
            let updated = b.ins().bor(cleared, mn_sh);
            b.ins().store(mem, updated, core_ptr, off_astat0);
        }
        FastFloatKind::Add | FastFloatKind::Sub => {
            let zero8 = b.ins().iconst(types::I8, 0);
            b.ins().store(
                mem,
                zero8,
                core_ptr,
                off_flags + offset_of!(Flags, az) as i32,
            );
            b.ins()
                .store(mem, an, core_ptr, off_flags + offset_of!(Flags, an) as i32);
            b.ins().store(
                mem,
                zero8,
                core_ptr,
                off_flags + offset_of!(Flags, av) as i32,
            );
            b.ins().store(
                mem,
                zero8,
                core_ptr,
                off_flags + offset_of!(Flags, ac) as i32,
            );
            b.ins().store(
                mem,
                zero8,
                core_ptr,
                off_flags + offset_of!(Flags, ai) as i32,
            );
            b.ins().store(
                mem,
                zero8,
                core_ptr,
                off_flags + offset_of!(Flags, as_) as i32,
            );
            let extra = b.ins().load(types::I32, mem, core_ptr, off_astat0);
            let set_af = b.ins().bor_imm(extra, 1i64 << 10);
            b.ins().store(mem, set_af, core_ptr, off_astat0);
        }
    }
    b.ins().jump(done_blk, &[]);

    b.switch_to_block(slow_blk);
    b.seal_block(slow_blk);
    match kind {
        FastFloatKind::Mul => {
            let n_v = b.ins().iconst(types::I32, i64::from(n));
            let x_v = b.ins().iconst(types::I32, i64::from(x));
            let y_v = b.ins().iconst(types::I32, i64::from(y));
            b.ins().call(fn_fmul_ref, &[core_ptr, n_v, x_v, y_v]);
        }
        FastFloatKind::Add | FastFloatKind::Sub => {
            let c_v = b.ins().iconst(types::I64, raw_c as i64);
            b.ins().call(fn_comp_ref, &[core_ptr, c_v]);
        }
    }
    b.ins().jump(done_blk, &[]);

    b.switch_to_block(done_blk);
    b.seal_block(done_blk);
}

/// Lowers non-rounding `ComputeOp::FixedMult` (`cls` 1..=3, `!round`) to Cranelift IR with
/// 1:1 80-bit `mr[reg]` arithmetic, `MN`/`MV`/`MU` flags, and a cold `jit_note_mult_overflow`
/// call when `MV` is set.
#[allow(clippy::too_many_arguments)]
fn emit_fixed_mult(
    b: &mut FunctionBuilder,
    core_ptr: cranelift_codegen::ir::Value,
    op: u8,
    n: u8,
    x: u8,
    y: u8,
    fn_mov_ref: cranelift_codegen::ir::FuncRef,
    ssa: Option<&LoopSsaVars>,
) {
    const _: () = assert!(std::mem::size_of::<i128>() == 16);
    const _: () = assert!(cfg!(target_endian = "little"));

    let cls = op >> 6;
    let (sy, sx) = ((op >> 5) & 1 == 1, (op >> 4) & 1 == 1);
    let (frac, sel) = ((op >> 3) & 1 == 1, (op >> 1) & 3);
    let unsigned = !sx && !sy;
    let reg = i32::from(sel == 1 || sel == 3);

    let mem = MemFlags::trusted();
    let off_r = offset_of!(Core, r) as i32;
    let off_mr = offset_of!(Core, mr) as i32 + 16 * reg;
    let off_fx_lo = (offset_of!(Core, fx) + offset_of!(super::float_ops::ExtFile, lo)) as i32;
    let off_astat0 =
        (offset_of!(Core, sys) + offset_of!(super::sysregs::SysState, astat_extra)) as i32;

    let rx = b
        .ins()
        .load(types::I32, mem, core_ptr, off_r + 4 * i32::from(x));
    let ry = b
        .ins()
        .load(types::I32, mem, core_ptr, off_r + 4 * i32::from(y));
    let rx64 = if sx {
        b.ins().sextend(types::I64, rx)
    } else {
        b.ins().uextend(types::I64, rx)
    };
    let ry64 = if sy {
        b.ins().sextend(types::I64, ry)
    } else {
        b.ins().uextend(types::I64, ry)
    };
    let raw_p = b.ins().imul(rx64, ry64);
    let (p_lo, p_hi) = if frac && sx && sy {
        (b.ins().ishl_imm(raw_p, 1), b.ins().sshr_imm(raw_p, 63))
    } else if sx || sy {
        (raw_p, b.ins().sshr_imm(raw_p, 63))
    } else {
        (raw_p, b.ins().iconst(types::I64, 0))
    };

    let (v_lo, v_hi_raw) = match cls {
        1 => (p_lo, p_hi),
        2 => {
            let (mr_lo, mr_hi) = if let Some((vlo, vhi)) = ssa.and_then(|s| s.mr[reg as usize]) {
                (b.use_var(vlo), b.use_var(vhi))
            } else {
                (
                    b.ins().load(types::I64, mem, core_ptr, off_mr),
                    b.ins().load(types::I64, mem, core_ptr, off_mr + 8),
                )
            };
            let sum_lo = b.ins().iadd(mr_lo, p_lo);
            let carry = b.ins().icmp(IntCC::UnsignedLessThan, sum_lo, mr_lo);
            let carry64 = b.ins().uextend(types::I64, carry);
            let sum_hi = b.ins().iadd(mr_hi, p_hi);
            (sum_lo, b.ins().iadd(sum_hi, carry64))
        }
        _ => {
            let (mr_lo, mr_hi) = if let Some((vlo, vhi)) = ssa.and_then(|s| s.mr[reg as usize]) {
                (b.use_var(vlo), b.use_var(vhi))
            } else {
                (
                    b.ins().load(types::I64, mem, core_ptr, off_mr),
                    b.ins().load(types::I64, mem, core_ptr, off_mr + 8),
                )
            };
            let borrow = b.ins().icmp(IntCC::UnsignedLessThan, mr_lo, p_lo);
            let borrow64 = b.ins().uextend(types::I64, borrow);
            let diff_lo = b.ins().isub(mr_lo, p_lo);
            let diff_hi = b.ins().isub(mr_hi, p_hi);
            (diff_lo, b.ins().isub(diff_hi, borrow64))
        }
    };

    // Wrap to 80 bits (sign-extend bit 79 into bits 80..127).
    let v_hi16 = b.ins().ireduce(types::I16, v_hi_raw);
    let v_hi = b.ins().sextend(types::I64, v_hi16);

    let mn = b.ins().icmp_imm(IntCC::SignedLessThan, v_hi, 0);
    let mv = match (frac, unsigned) {
        (true, false) => {
            let sign_lo = b.ins().sshr_imm(v_lo, 63);
            b.ins().icmp(IntCC::NotEqual, v_hi, sign_lo)
        }
        (true, true) => b.ins().icmp_imm(IntCC::NotEqual, v_hi, 0),
        (false, false) => {
            let lo32 = b.ins().ireduce(types::I32, v_lo);
            let sext64 = b.ins().sextend(types::I64, lo32);
            let sign_lo = b.ins().sshr_imm(sext64, 63);
            let b1 = b.ins().icmp(IntCC::NotEqual, v_lo, sext64);
            let b2 = b.ins().icmp(IntCC::NotEqual, v_hi, sign_lo);
            b.ins().bor(b1, b2)
        }
        (false, true) => {
            let hi32 = b.ins().ushr_imm(v_lo, 32);
            let b1 = b.ins().icmp_imm(IntCC::NotEqual, v_hi, 0);
            let b2 = b.ins().icmp_imm(IntCC::NotEqual, hi32, 0);
            b.ins().bor(b1, b2)
        }
    };
    let mu = if frac {
        let hi32_lo = b.ins().sshr_imm(v_lo, 32);
        let is_zero_hi = b.ins().icmp_imm(IntCC::Equal, v_hi, 0);
        let is_neg1_hi = b.ins().icmp_imm(IntCC::Equal, v_hi, -1);
        let hi_sign = b.ins().bor(is_zero_hi, is_neg1_hi);
        let mid_ok = b.ins().icmp(IntCC::Equal, hi32_lo, v_hi);
        let lo32 = b.ins().ireduce(types::I32, v_lo);
        let lo_nz = b.ins().icmp_imm(IntCC::NotEqual, lo32, 0);
        let c1 = b.ins().band(hi_sign, mid_ok);
        b.ins().band(c1, lo_nz)
    } else {
        b.ins().iconst(types::I8, 0)
    };

    let mn32 = b.ins().uextend(types::I32, mn);
    let mv32 = b.ins().uextend(types::I32, mv);
    let mu32 = b.ins().uextend(types::I32, mu);
    let mv_sh = b.ins().ishl_imm(mv32, 1);
    let mu_sh = b.ins().ishl_imm(mu32, 2);
    let n1 = b.ins().bor(mn32, mv_sh);
    let nibble = b.ins().bor(n1, mu_sh);
    let nibble_sh = b.ins().ishl_imm(nibble, 6);

    let extra = b.ins().load(types::I32, mem, core_ptr, off_astat0);
    let masked = b.ins().band_imm(extra, !(0xFi64 << 6));
    let new_extra = b.ins().bor(masked, nibble_sh);
    b.ins().store(mem, new_extra, core_ptr, off_astat0);

    let ov_blk = b.create_block();
    let cont_blk = b.create_block();
    b.ins().brif(mv, ov_blk, &[], cont_blk, &[]);
    b.switch_to_block(ov_blk);
    b.seal_block(ov_blk);
    b.ins().call(fn_mov_ref, &[core_ptr]);
    b.ins().jump(cont_blk, &[]);
    b.switch_to_block(cont_blk);
    b.seal_block(cont_blk);

    if sel < 2 {
        let shifted = if frac {
            b.ins().ushr_imm(v_lo, 32)
        } else {
            v_lo
        };
        let res32 = b.ins().ireduce(types::I32, shifted);
        b.ins()
            .store(mem, res32, core_ptr, off_r + 4 * i32::from(n));
        let zero8 = b.ins().iconst(types::I8, 0);
        b.ins()
            .store(mem, zero8, core_ptr, off_fx_lo + i32::from(n));
    } else if let Some((vlo, vhi)) = ssa.and_then(|s| s.mr[reg as usize]) {
        b.def_var(vlo, v_lo);
        b.def_var(vhi, v_hi);
    } else {
        b.ins().store(mem, v_lo, core_ptr, off_mr);
        b.ins().store(mem, v_hi, core_ptr, off_mr + 8);
    }
}

/// Lowers a [`ComputeOp`] for both standalone compute instructions and `DualMove` parallel compute.
#[allow(clippy::too_many_arguments)]
fn emit_jit_compute_op(
    b: &mut FunctionBuilder,
    core_ptr: cranelift_codegen::ir::Value,
    cop: ComputeOp,
    fast_float: bool,
    fn_fmul_ref: cranelift_codegen::ir::FuncRef,
    fn_fmult_ref: cranelift_codegen::ir::FuncRef,
    fn_mov_ref: cranelift_codegen::ir::FuncRef,
    fn_av_ref: cranelift_codegen::ir::FuncRef,
    fn_comp_ref: cranelift_codegen::ir::FuncRef,
    ssa: Option<&LoopSsaVars>,
) {
    match cop {
        ComputeOp::Nop => {}
        ComputeOp::FixedAlu {
            op: fop @ (0x01 | 0x02),
            n,
            x,
            y,
        } => {
            emit_fixed_add_sub(b, core_ptr, n, x, y, fop == 0x02, fn_av_ref);
        }
        ComputeOp::FixedAlu {
            op: fop @ (0x21 | 0x40 | 0x41 | 0x42),
            n,
            x,
            y,
        } => {
            emit_fixed_logic_or_pass(b, core_ptr, fop, n, x, y);
        }
        ComputeOp::FloatMul { n, x, y } if fast_float => {
            emit_fast_float_op(b, core_ptr, FastFloatKind::Mul, n, x, y);
        }
        ComputeOp::FloatMul { n, x, y } => {
            emit_exact_float_op(
                b,
                core_ptr,
                FastFloatKind::Mul,
                n,
                x,
                y,
                0,
                fn_fmul_ref,
                fn_comp_ref,
            );
        }
        ComputeOp::FixedMult { op: mop, n, x, y }
            if (mop >> 6) != 0 && !((mop & 1 == 1) && ((mop >> 3) & 1 == 1)) =>
        {
            emit_fixed_mult(b, core_ptr, mop, n, x, y, fn_mov_ref, ssa);
        }
        ComputeOp::FixedMult { op: mop, n, x, y } => {
            let op_v = b.ins().iconst(types::I32, i64::from(mop));
            let n_v = b.ins().iconst(types::I32, i64::from(n));
            let x_v = b.ins().iconst(types::I32, i64::from(x));
            let y_v = b.ins().iconst(types::I32, i64::from(y));
            b.ins().call(fn_fmult_ref, &[core_ptr, op_v, n_v, x_v, y_v]);
        }
        ComputeOp::FloatAlu {
            op: fop @ (0x81 | 0x82),
            n,
            x,
            y,
        } => {
            let fk = if fop == 0x81 {
                FastFloatKind::Add
            } else {
                FastFloatKind::Sub
            };
            if fast_float {
                emit_fast_float_op(b, core_ptr, fk, n, x, y);
            } else if let Some(raw_c) = compute_op_to_raw(cop) {
                emit_exact_float_op(b, core_ptr, fk, n, x, y, raw_c, fn_fmul_ref, fn_comp_ref);
            }
        }
        other => {
            if let Some(raw_c) = compute_op_to_raw(other) {
                let c_v = b.ins().iconst(types::I64, raw_c as i64);
                b.ins().call(fn_comp_ref, &[core_ptr, c_v]);
            }
        }
    }
}

/// Computes `Core::dag_next(i, m, l, b)` branchlessly in Cranelift IR.
fn emit_dag_next_ir(
    fb: &mut FunctionBuilder,
    i_val: cranelift_codegen::ir::Value,
    m_val: cranelift_codegen::ir::Value,
    l_val: cranelift_codegen::ir::Value,
    b_val: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    let next = fb.ins().iadd(i_val, m_val);
    let l_zero = fb.ins().icmp_imm(IntCC::Equal, l_val, 0);
    let m_nonneg = fb.ins().icmp_imm(IntCC::SignedGreaterThanOrEqual, m_val, 0);
    let b_plus_l = fb.ins().iadd(b_val, l_val);
    let wrap_pos = fb
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, next, b_plus_l);
    let next_minus_l = fb.ins().isub(next, l_val);
    let pos_res = fb.ins().select(wrap_pos, next_minus_l, next);
    let wrap_neg = fb.ins().icmp(IntCC::UnsignedLessThan, next, b_val);
    let next_plus_l = fb.ins().iadd(next, l_val);
    let neg_res = fb.ins().select(wrap_neg, next_plus_l, next);
    let circ_res = fb.ins().select(m_nonneg, pos_res, neg_res);
    fb.ins().select(l_zero, next, circ_res)
}

/// Loop-carried `i[ii]` and `mr[reg]` registers promoted to Cranelift SSA [`Variable`]s
/// across eligible `DO ... UNTIL LCE` loops.
struct LoopSsaVars {
    i: [Option<Variable>; 16],
    mr: [Option<(Variable, Variable)>; 2],
}

impl LoopSsaVars {
    fn flush(&self, b: &mut FunctionBuilder, core_ptr: cranelift_codegen::ir::Value) {
        let mem = MemFlags::trusted();
        let off_i = offset_of!(Core, i) as i32;
        let off_mr = offset_of!(Core, mr) as i32;
        for (idx, var) in self.i.iter().enumerate() {
            if let Some(v) = *var {
                let val = b.use_var(v);
                b.ins().store(mem, val, core_ptr, off_i + 4 * idx as i32);
            }
        }
        for (reg, pair) in self.mr.iter().enumerate() {
            if let Some((vlo, vhi)) = *pair {
                let lo = b.use_var(vlo);
                let hi = b.use_var(vhi);
                b.ins().store(mem, lo, core_ptr, off_mr + 16 * reg as i32);
                b.ins()
                    .store(mem, hi, core_ptr, off_mr + 16 * reg as i32 + 8);
            }
        }
    }

    fn reload(&self, b: &mut FunctionBuilder, core_ptr: cranelift_codegen::ir::Value) {
        let mem = MemFlags::trusted();
        let off_i = offset_of!(Core, i) as i32;
        let off_mr = offset_of!(Core, mr) as i32;
        for (idx, var) in self.i.iter().enumerate() {
            if let Some(v) = *var {
                let val = b
                    .ins()
                    .load(types::I32, mem, core_ptr, off_i + 4 * idx as i32);
                b.def_var(v, val);
            }
        }
        for (reg, pair) in self.mr.iter().enumerate() {
            if let Some((vlo, vhi)) = *pair {
                let lo = b
                    .ins()
                    .load(types::I64, mem, core_ptr, off_mr + 16 * reg as i32);
                let hi = b
                    .ins()
                    .load(types::I64, mem, core_ptr, off_mr + 16 * reg as i32 + 8);
                b.def_var(vlo, lo);
                b.def_var(vhi, hi);
            }
        }
    }
}

fn is_inline_compute_for_ssa(cop: ComputeOp) -> bool {
    match cop {
        ComputeOp::Nop => true,
        ComputeOp::FixedAlu {
            op: 0x01 | 0x02 | 0x21 | 0x40 | 0x41 | 0x42,
            ..
        } => true,
        ComputeOp::FloatMul { .. }
        | ComputeOp::FloatAlu {
            op: 0x81 | 0x82, ..
        } => true,
        ComputeOp::FixedMult { op: mop, .. } => {
            (mop >> 6) != 0 && !((mop & 1 == 1) && ((mop >> 3) & 1 == 1))
        }
        _ => false,
    }
}

fn can_promote_loop_ssa(insns: &[(u32, u32, Kind, u64, DecodedOp)], sram_fast: bool) -> bool {
    if !sram_fast {
        return false;
    }
    insns.iter().all(|&(_, _, kind, w, op)| match op {
        DecodedOp::Nop => true,
        DecodedOp::ComputeCond { cond: 31, op: cop } | DecodedOp::ComputeBoth { op: cop } => {
            is_inline_compute_for_ssa(cop)
        }
        DecodedOp::Move(MoveOp::LoadImm { ureg, .. }) => ureg < 16,
        DecodedOp::Move(MoveOp::RegMove { src, dst, .. }) => src < 16 && dst < 16,
        DecodedOp::Move(MoveOp::DagMem {
            ii,
            ureg,
            cond: 31,
            compute: None,
            ..
        }) => ureg < 16 && !matches!(ii, 0 | 1 | 8 | 9) && !Core::lw_flag(kind, w),
        DecodedOp::DualMove(dm) => {
            !dm.dm_write
                && !dm.pm_write
                && dm.dm_reg < 16
                && dm.pm_reg < 16
                && !matches!(dm.dmi, 0 | 1 | 8 | 9)
                && !matches!(dm.pmi, 0 | 1 | 8 | 9)
                && (dm.compute_raw >> 22) & 1 == 0
                && dm.compute.is_none_or(is_inline_compute_for_ssa)
        }
        _ => false,
    })
}

fn preload_loop_ssa(
    b: &mut FunctionBuilder,
    core_ptr: cranelift_codegen::ir::Value,
    insns: &[(u32, u32, Kind, u64, DecodedOp)],
) -> LoopSsaVars {
    let mut need_i = [false; 16];
    let mut need_mr = [false; 2];
    let mut mark_cop = |cop: ComputeOp| {
        if let ComputeOp::FixedMult { op: mop, .. } = cop {
            let sel = (mop >> 1) & 3;
            need_mr[usize::from(sel == 1 || sel == 3)] = true;
        }
    };
    for &(_, _, _, _, op) in insns {
        match op {
            DecodedOp::ComputeCond { cond: 31, op: cop } | DecodedOp::ComputeBoth { op: cop } => {
                mark_cop(cop);
            }
            DecodedOp::Move(MoveOp::DagMem { ii, .. }) => need_i[usize::from(ii)] = true,
            DecodedOp::DualMove(dm) => {
                need_i[usize::from(dm.dmi)] = true;
                need_i[usize::from(dm.pmi)] = true;
                if let Some(cop) = dm.compute {
                    mark_cop(cop);
                }
            }
            _ => {}
        }
    }
    let mut vars = LoopSsaVars {
        i: [None; 16],
        mr: [None; 2],
    };
    for (idx, needed) in need_i.iter().enumerate() {
        if *needed {
            let v = Variable::from_u32(idx as u32);
            b.declare_var(v, types::I32);
            vars.i[idx] = Some(v);
        }
    }
    for (reg, needed) in need_mr.iter().enumerate() {
        if *needed {
            let vlo = Variable::from_u32((16 + 2 * reg) as u32);
            let vhi = Variable::from_u32((17 + 2 * reg) as u32);
            b.declare_var(vlo, types::I64);
            b.declare_var(vhi, types::I64);
            vars.mr[reg] = Some((vlo, vhi));
        }
    }
    vars.reload(b, core_ptr);
    vars
}

/// Loop-invariant DAG configuration values (`MODE1.CBUFEN` and `M`/`L`/`B` registers)
/// loaded once in the loop preheader when the loop body cannot mutate them.
struct HoistedDagCfg {
    has_cbuf: cranelift_codegen::ir::Value,
    m: [Option<cranelift_codegen::ir::Value>; 16],
    l: [Option<cranelift_codegen::ir::Value>; 16],
    b: [Option<cranelift_codegen::ir::Value>; 16],
}

fn is_dag_cfg_invariant(insns: &[(u32, u32, Kind, u64, DecodedOp)]) -> bool {
    insns.iter().all(|&(_, _, _, _, op)| match op {
        DecodedOp::Nop => true,
        DecodedOp::ComputeCond { cond: 31, .. } | DecodedOp::ComputeBoth { .. } => true,
        DecodedOp::Move(MoveOp::LoadImm { ureg, .. }) => ureg < 16,
        DecodedOp::Move(MoveOp::RegMove { dst, .. }) => dst < 16,
        DecodedOp::Move(MoveOp::DagMem { ureg, .. }) => ureg < 16,
        DecodedOp::DualMove(dm) => dm.dm_reg < 16 && dm.pm_reg < 16,
        _ => false,
    })
}

fn preload_dag_cfg(
    b: &mut FunctionBuilder,
    core_ptr: cranelift_codegen::ir::Value,
    insns: &[(u32, u32, Kind, u64, DecodedOp)],
) -> HoistedDagCfg {
    let mem = MemFlags::trusted();
    let off_m = offset_of!(Core, m) as i32;
    let off_l = offset_of!(Core, l) as i32;
    let off_b = offset_of!(Core, b) as i32;
    let mode1 = b
        .ins()
        .load(types::I32, mem, core_ptr, offset_of!(Core, mode1) as i32);
    let cbufen = b.ins().band_imm(mode1, i64::from(super::dag::MODE1_CBUFEN));
    let has_cbuf = b.ins().icmp_imm(IntCC::NotEqual, cbufen, 0);
    let mut cfg = HoistedDagCfg {
        has_cbuf,
        m: [None; 16],
        l: [None; 16],
        b: [None; 16],
    };
    let mut load_one = |ii: u8, mi: u8| {
        let midx = usize::from(mi);
        if cfg.m[midx].is_none() {
            cfg.m[midx] = Some(
                b.ins()
                    .load(types::I32, mem, core_ptr, off_m + 4 * i32::from(mi)),
            );
        }
        let iidx = usize::from(ii);
        if cfg.l[iidx].is_none() {
            cfg.l[iidx] = Some(
                b.ins()
                    .load(types::I32, mem, core_ptr, off_l + 4 * i32::from(ii)),
            );
        }
        if cfg.b[iidx].is_none() {
            cfg.b[iidx] = Some(
                b.ins()
                    .load(types::I32, mem, core_ptr, off_b + 4 * i32::from(ii)),
            );
        }
    };
    for &(_, _, _, _, op) in insns {
        match op {
            DecodedOp::Move(MoveOp::DagMem { ii, mi, .. }) => load_one(ii, mi),
            DecodedOp::DualMove(dm) => {
                load_one(dm.dmi, dm.dmm);
                load_one(dm.pmi, dm.pmm);
            }
            _ => {}
        }
    }
    cfg
}

/// Emits an inline `SRAM_WORDS` load or store for `MoveOp::DagMem` (`ureg < 16`, `ii` not subject
/// to bitrev/broadcast), handling linear and circular post-modify in Cranelift IR and falling
/// back to `jit_dag_mem` when `addr` is outside `0x9_0000..0x10_0000` or a store overlaps
/// `[sram_code_nw_lo, sram_code_nw_hi]`.
#[allow(clippy::too_many_arguments)]
fn emit_inline_sram_dag_transfer(
    b: &mut FunctionBuilder,
    core_ptr: cranelift_codegen::ir::Value,
    sram_ptr: cranelift_codegen::ir::Value,
    ii: u8,
    mi: u8,
    ureg: u8,
    write: bool,
    post_modify: bool,
    hoisted: Option<&HoistedDagCfg>,
    ssa: Option<&LoopSsaVars>,
    insn_pc: u32,
    insn_w: u64,
    arg_kind: i64,
    fn_dagmem_ref: cranelift_codegen::ir::FuncRef,
    on_fault_blk: cranelift_codegen::ir::Block,
) {
    let mem = MemFlags::trusted();
    let off_i = offset_of!(Core, i) as i32;
    let off_m = offset_of!(Core, m) as i32;
    let off_l = offset_of!(Core, l) as i32;
    let off_b = offset_of!(Core, b) as i32;
    let off_r = offset_of!(Core, r) as i32;
    let off_fx_lo = (offset_of!(Core, fx) + offset_of!(super::float_ops::ExtFile, lo)) as i32;

    let i_val = if let Some(v) = ssa.and_then(|s| s.i[usize::from(ii)]) {
        b.use_var(v)
    } else {
        b.ins()
            .load(types::I32, mem, core_ptr, off_i + 4 * i32::from(ii))
    };
    let m_val = hoisted
        .and_then(|h| h.m[usize::from(mi)])
        .unwrap_or_else(|| {
            b.ins()
                .load(types::I32, mem, core_ptr, off_m + 4 * i32::from(mi))
        });

    let raw_next = b.ins().iadd(i_val, m_val);
    let addr = if post_modify { i_val } else { raw_next };

    let rel_addr = b.ins().iadd_imm(addr, -(0x9_0000i64));
    let in_sram = b
        .ins()
        .icmp_imm(IntCC::UnsignedLessThan, rel_addr, 0x7_0000);
    let mut can_fast = in_sram;
    let l_val = if post_modify {
        let l = hoisted
            .and_then(|h| h.l[usize::from(ii)])
            .unwrap_or_else(|| {
                b.ins()
                    .load(types::I32, mem, core_ptr, off_l + 4 * i32::from(ii))
            });
        if matches!(ii, 7 | 15) {
            let l_zero = b.ins().icmp_imm(IntCC::Equal, l, 0);
            can_fast = b.ins().band(can_fast, l_zero);
        }
        Some(l)
    } else {
        None
    };
    if write {
        let nw_lo = b.ins().load(
            types::I32,
            mem,
            core_ptr,
            offset_of!(Core, sram_code_nw_lo) as i32,
        );
        let nw_hi = b.ins().load(
            types::I32,
            mem,
            core_ptr,
            offset_of!(Core, sram_code_nw_hi) as i32,
        );
        let below = b.ins().icmp(IntCC::UnsignedLessThan, addr, nw_lo);
        let above = b.ins().icmp(IntCC::UnsignedGreaterThan, addr, nw_hi);
        let outside_code = b.ins().bor(below, above);
        can_fast = b.ins().band(can_fast, outside_code);
    }

    let fast_blk = b.create_block();
    let slow_blk = b.create_block();
    let done_blk = b.create_block();
    b.ins().brif(can_fast, fast_blk, &[], slow_blk, &[]);

    b.switch_to_block(fast_blk);
    b.seal_block(fast_blk);
    let addr64 = b.ins().uextend(types::I64, addr);
    let byte_off = b.ins().ishl_imm(addr64, 2);
    let ptr = b.ins().iadd(sram_ptr, byte_off);
    if write {
        let val = b
            .ins()
            .load(types::I32, mem, core_ptr, off_r + 4 * i32::from(ureg));
        b.ins().store(mem, val, ptr, 0);
    } else {
        let loaded = b.ins().load(types::I32, mem, ptr, 0);
        b.ins()
            .store(mem, loaded, core_ptr, off_r + 4 * i32::from(ureg));
        let zero8 = b.ins().iconst(types::I8, 0);
        b.ins()
            .store(mem, zero8, core_ptr, off_fx_lo + i32::from(ureg));
    }
    if let Some(l_val) = l_val {
        let b_val = hoisted
            .and_then(|h| h.b[usize::from(ii)])
            .unwrap_or_else(|| {
                b.ins()
                    .load(types::I32, mem, core_ptr, off_b + 4 * i32::from(ii))
            });
        let has_cbuf = if let Some(h) = hoisted {
            h.has_cbuf
        } else {
            let mode1 = b
                .ins()
                .load(types::I32, mem, core_ptr, offset_of!(Core, mode1) as i32);
            let cbufen = b.ins().band_imm(mode1, i64::from(super::dag::MODE1_CBUFEN));
            b.ins().icmp_imm(IntCC::NotEqual, cbufen, 0)
        };
        let zero32 = b.ins().iconst(types::I32, 0);
        let eff_l = b.ins().select(has_cbuf, l_val, zero32);
        let next_i = emit_dag_next_ir(b, i_val, m_val, eff_l, b_val);
        if let Some(v) = ssa.and_then(|s| s.i[usize::from(ii)]) {
            b.def_var(v, next_i);
        } else {
            b.ins()
                .store(mem, next_i, core_ptr, off_i + 4 * i32::from(ii));
        }
    }
    b.ins().jump(done_blk, &[]);

    b.switch_to_block(slow_blk);
    b.seal_block(slow_blk);
    if let Some(s) = ssa {
        s.flush(b, core_ptr);
    }
    let pc_v = b.ins().iconst(types::I32, i64::from(insn_pc));
    let w_v = b.ins().iconst(types::I64, insn_w as i64);
    let k_v = b.ins().iconst(types::I8, arg_kind);
    let call = b.ins().call(fn_dagmem_ref, &[core_ptr, pc_v, w_v, k_v]);
    let status = b.inst_results(call)[0];
    if let Some(s) = ssa {
        s.reload(b, core_ptr);
    }
    let ok = b.ins().icmp_imm(IntCC::Equal, status, 0);
    b.ins().brif(ok, done_blk, &[], on_fault_blk, &[status]);

    b.switch_to_block(done_blk);
    b.seal_block(done_blk);
}

/// Emits an inline `SRAM_WORDS` dual load for `DualMoveOp` (`!dm_write && !pm_write`,
/// no multifunction compute), handling linear and circular DAG post-modify in Cranelift IR
/// and falling back to `jit_dual_move` if either address is outside `SRAM_WORDS`.
#[allow(clippy::too_many_arguments)]
fn emit_inline_dual_sram_load(
    b: &mut FunctionBuilder,
    core_ptr: cranelift_codegen::ir::Value,
    sram_ptr: cranelift_codegen::ir::Value,
    dm: DualMoveOp,
    fast_float: bool,
    hoisted: Option<&HoistedDagCfg>,
    ssa: Option<&LoopSsaVars>,
    insn_pc: u32,
    insn_w: u64,
    arg_kind: i64,
    fn_fmul_ref: cranelift_codegen::ir::FuncRef,
    fn_fmult_ref: cranelift_codegen::ir::FuncRef,
    fn_mov_ref: cranelift_codegen::ir::FuncRef,
    fn_av_ref: cranelift_codegen::ir::FuncRef,
    fn_comp_ref: cranelift_codegen::ir::FuncRef,
    fn_dual_ref: cranelift_codegen::ir::FuncRef,
    on_fault_blk: cranelift_codegen::ir::Block,
) {
    let mem = MemFlags::trusted();
    let off_i = offset_of!(Core, i) as i32;
    let off_m = offset_of!(Core, m) as i32;
    let off_l = offset_of!(Core, l) as i32;
    let off_b = offset_of!(Core, b) as i32;
    let off_r = offset_of!(Core, r) as i32;
    let off_fx_lo = (offset_of!(Core, fx) + offset_of!(super::float_ops::ExtFile, lo)) as i32;

    let dmi = i32::from(dm.dmi);
    let dmm = i32::from(dm.dmm);
    let pmi = i32::from(dm.pmi);
    let pmm = i32::from(dm.pmm);

    let dm_i = if let Some(v) = ssa.and_then(|s| s.i[dmi as usize]) {
        b.use_var(v)
    } else {
        b.ins().load(types::I32, mem, core_ptr, off_i + 4 * dmi)
    };
    let dm_m = hoisted
        .and_then(|h| h.m[dmi as usize])
        .unwrap_or_else(|| b.ins().load(types::I32, mem, core_ptr, off_m + 4 * dmm));
    let dm_l = hoisted
        .and_then(|h| h.l[dmi as usize])
        .unwrap_or_else(|| b.ins().load(types::I32, mem, core_ptr, off_l + 4 * dmi));
    let dm_b = hoisted
        .and_then(|h| h.b[dmi as usize])
        .unwrap_or_else(|| b.ins().load(types::I32, mem, core_ptr, off_b + 4 * dmi));
    let pm_i = if let Some(v) = ssa.and_then(|s| s.i[pmi as usize]) {
        b.use_var(v)
    } else {
        b.ins().load(types::I32, mem, core_ptr, off_i + 4 * pmi)
    };
    let pm_m = hoisted
        .and_then(|h| h.m[pmi as usize])
        .unwrap_or_else(|| b.ins().load(types::I32, mem, core_ptr, off_m + 4 * pmm));
    let pm_l = hoisted
        .and_then(|h| h.l[pmi as usize])
        .unwrap_or_else(|| b.ins().load(types::I32, mem, core_ptr, off_l + 4 * pmi));
    let pm_b = hoisted
        .and_then(|h| h.b[pmi as usize])
        .unwrap_or_else(|| b.ins().load(types::I32, mem, core_ptr, off_b + 4 * pmi));

    let dm_rel = b.ins().iadd_imm(dm_i, -(0x9_0000i64));
    let pm_rel = b.ins().iadd_imm(pm_i, -(0x9_0000i64));
    let dm_ok = b.ins().icmp_imm(IntCC::UnsignedLessThan, dm_rel, 0x7_0000);
    let pm_ok = b.ins().icmp_imm(IntCC::UnsignedLessThan, pm_rel, 0x7_0000);
    let mut can_fast = b.ins().band(dm_ok, pm_ok);
    if matches!(dm.dmi, 7 | 15) {
        let z = b.ins().icmp_imm(IntCC::Equal, dm_l, 0);
        can_fast = b.ins().band(can_fast, z);
    }
    if matches!(dm.pmi, 7 | 15) {
        let z = b.ins().icmp_imm(IntCC::Equal, pm_l, 0);
        can_fast = b.ins().band(can_fast, z);
    }

    let fast_blk = b.create_block();
    let slow_blk = b.create_block();
    let done_blk = b.create_block();
    b.ins().brif(can_fast, fast_blk, &[], slow_blk, &[]);

    b.switch_to_block(fast_blk);
    b.seal_block(fast_blk);
    let dm_addr64 = b.ins().uextend(types::I64, dm_i);
    let pm_addr64 = b.ins().uextend(types::I64, pm_i);
    let dm_off = b.ins().ishl_imm(dm_addr64, 2);
    let pm_off = b.ins().ishl_imm(pm_addr64, 2);
    let dm_ptr = b.ins().iadd(sram_ptr, dm_off);
    let pm_ptr = b.ins().iadd(sram_ptr, pm_off);
    let dm_val = b.ins().load(types::I32, mem, dm_ptr, 0);
    let pm_val = b.ins().load(types::I32, mem, pm_ptr, 0);

    // Execute optional parallel compute BEFORE writing the loaded registers.
    if let Some(cop) = dm.compute {
        emit_jit_compute_op(
            b,
            core_ptr,
            cop,
            fast_float,
            fn_fmul_ref,
            fn_fmult_ref,
            fn_mov_ref,
            fn_av_ref,
            fn_comp_ref,
            ssa,
        );
    }

    b.ins()
        .store(mem, dm_val, core_ptr, off_r + 4 * i32::from(dm.dm_reg));
    b.ins()
        .store(mem, pm_val, core_ptr, off_r + 4 * i32::from(dm.pm_reg));
    let zero8 = b.ins().iconst(types::I8, 0);
    b.ins()
        .store(mem, zero8, core_ptr, off_fx_lo + i32::from(dm.dm_reg));
    b.ins()
        .store(mem, zero8, core_ptr, off_fx_lo + i32::from(dm.pm_reg));

    let has_cbuf = if let Some(h) = hoisted {
        h.has_cbuf
    } else {
        let mode1 = b
            .ins()
            .load(types::I32, mem, core_ptr, offset_of!(Core, mode1) as i32);
        let cbufen = b.ins().band_imm(mode1, i64::from(super::dag::MODE1_CBUFEN));
        b.ins().icmp_imm(IntCC::NotEqual, cbufen, 0)
    };
    let zero32 = b.ins().iconst(types::I32, 0);
    let eff_dm_l = b.ins().select(has_cbuf, dm_l, zero32);
    let eff_pm_l = b.ins().select(has_cbuf, pm_l, zero32);
    let dm_next = emit_dag_next_ir(b, dm_i, dm_m, eff_dm_l, dm_b);
    let pm_next = emit_dag_next_ir(b, pm_i, pm_m, eff_pm_l, pm_b);
    if let Some(v) = ssa.and_then(|s| s.i[dmi as usize]) {
        b.def_var(v, dm_next);
    } else {
        b.ins().store(mem, dm_next, core_ptr, off_i + 4 * dmi);
    }
    if let Some(v) = ssa.and_then(|s| s.i[pmi as usize]) {
        b.def_var(v, pm_next);
    } else {
        b.ins().store(mem, pm_next, core_ptr, off_i + 4 * pmi);
    }
    b.ins().jump(done_blk, &[]);

    b.switch_to_block(slow_blk);
    b.seal_block(slow_blk);
    if let Some(s) = ssa {
        s.flush(b, core_ptr);
    }
    let pc_v = b.ins().iconst(types::I32, i64::from(insn_pc));
    let w_v = b.ins().iconst(types::I64, insn_w as i64);
    let k_v = b.ins().iconst(types::I8, arg_kind);
    let call = b.ins().call(fn_dual_ref, &[core_ptr, pc_v, w_v, k_v]);
    let status = b.inst_results(call)[0];
    if let Some(s) = ssa {
        s.reload(b, core_ptr);
    }
    let ok = b.ins().icmp_imm(IntCC::Equal, status, 0);
    b.ins().brif(ok, done_blk, &[], on_fault_blk, &[status]);

    b.switch_to_block(done_blk);
    b.seal_block(done_blk);
}

impl Core {
    /// Compiles (or looks up) a JIT basic block starting at `start_pc`, stopping at `loop_end`
    /// if inside a counter loop. Returns `None` if the first instruction is not JIT-eligible.
    fn get_or_compile_jit_block(&mut self, start_pc: u32, loop_end: u32) -> Option<JitBlockFn> {
        let simd = self.simd();
        let fast_float = self.fast_float;
        let sram_fast = self.periph.is_none()
            && self.watch == u32::MAX
            && self.watch_len <= 1
            && !self.rwatch_on;
        if self
            .jit
            .as_ref()
            .is_some_and(|j| j.code_gen != self.code_gen)
        {
            self.jit = None;
        }

        // Collect the basic block instructions first using DecodeCache.
        let mut insns = Vec::with_capacity(16);
        let mut pc = start_pc;
        let mut end_pc = start_pc;
        while insns.len() < MAX_BLOCK_INSNS {
            let isa = self.isa_at(pc);
            let cached = if let Some(hit) = self
                .decode_cache
                .as_deref()
                .and_then(|c| c.get(pc, self.code_gen, isa))
                .copied()
            {
                hit
            } else {
                self.fetch_and_cache(pc, isa)
            };
            if !can_jit_op(cached.op, simd) {
                break;
            }
            end_pc = cached.pc;
            let next_pc = cached.next_pc;
            insns.push((
                cached.pc,
                cached.next_pc,
                cached.insn.kind,
                cached.w,
                cached.op,
            ));
            if pc == loop_end {
                break;
            }
            pc = next_pc;
        }
        if insns.is_empty() {
            return None;
        }
        let is_loop = loop_end != u32::MAX && end_pc == loop_end;
        let len_insns = insns.len() as u64;
        let key = (start_pc, loop_end, simd, fast_float, sram_fast);

        let jit = self
            .jit
            .get_or_insert_with(|| JitEngine::new(self.code_gen));
        jit.code_modified = false;
        if let Some(blk) = jit.blocks.get(&key) {
            return Some(blk.func);
        }

        let ptr_ty = jit.module.target_config().pointer_type();
        let mut sig = jit.module.make_signature();
        sig.params.push(AbiParam::new(ptr_ty));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(ptr_ty));
        sig.params.push(AbiParam::new(ptr_ty));
        sig.returns.push(AbiParam::new(types::I64));

        let fn_name = format!("sharc_blk_{start_pc:x}_{}", jit.next_id);
        jit.next_id += 1;
        let func_id = jit
            .module
            .declare_function(&fn_name, Linkage::Export, &sig)
            .expect("declare JIT block");

        let mut ctx = jit.module.make_context();
        ctx.func.signature = sig;
        ctx.func.name = UserFuncName::user(0, func_id.as_u32());

        {
            let mut b = FunctionBuilder::new(&mut ctx.func, &mut jit.fn_ctx);
            let entry = b.create_block();
            b.append_block_params_for_function_params(entry);
            b.switch_to_block(entry);
            b.seal_block(entry);

            let core_ptr = b.block_params(entry)[0];
            let max_iters = b.block_params(entry)[1];
            let rem_ptr = b.block_params(entry)[2];
            let sram_ptr = b.block_params(entry)[3];

            let fn_fmul_ref = jit.module.declare_func_in_func(jit.fn_float_mul, b.func);
            let fn_fmult_ref = jit.module.declare_func_in_func(jit.fn_fixed_mult, b.func);
            let fn_mov_ref = jit.module.declare_func_in_func(jit.fn_mult_ov, b.func);
            let fn_av_ref = jit.module.declare_func_in_func(jit.fn_fixed_av, b.func);
            let fn_comp_ref = jit.module.declare_func_in_func(jit.fn_exec_compute, b.func);
            let fn_wureg_ref = jit.module.declare_func_in_func(jit.fn_write_ureg, b.func);
            let fn_rmove_ref = jit.module.declare_func_in_func(jit.fn_reg_move, b.func);
            let fn_dagmem_ref = jit.module.declare_func_in_func(jit.fn_dag_mem, b.func);
            let fn_dual_ref = jit.module.declare_func_in_func(jit.fn_dual_move, b.func);

            let early_exit = b.create_block();
            b.append_block_param(early_exit, types::I64); // steps_done
            b.append_block_param(early_exit, types::I64); // iters_dec
            b.append_block_param(early_exit, types::I32); // resume_pc

            let hoisted = (is_loop && is_dag_cfg_invariant(&insns))
                .then(|| preload_dag_cfg(&mut b, core_ptr, &insns));
            let ssa = (is_loop && can_promote_loop_ssa(&insns, sram_fast))
                .then(|| preload_loop_ssa(&mut b, core_ptr, &insns));

            let loop_hdr = b.create_block();
            b.append_block_param(loop_hdr, types::I64); // iters_done
            let zero64 = b.ins().iconst(types::I64, 0);
            b.ins().jump(loop_hdr, &[zero64]);

            b.switch_to_block(loop_hdr);
            let iters_done = b.block_params(loop_hdr)[0];

            for (idx, &(insn_pc, insn_next_pc, insn_kind, insn_w, op)) in insns.iter().enumerate() {
                match op {
                    DecodedOp::Nop => {}
                    DecodedOp::ComputeCond { cond: 31, op } | DecodedOp::ComputeBoth { op } => {
                        emit_jit_compute_op(
                            &mut b,
                            core_ptr,
                            op,
                            fast_float,
                            fn_fmul_ref,
                            fn_fmult_ref,
                            fn_mov_ref,
                            fn_av_ref,
                            fn_comp_ref,
                            ssa.as_ref(),
                        );
                    }
                    DecodedOp::Move(MoveOp::LoadImm { ureg, value }) => {
                        let mem = MemFlags::trusted();
                        let val_v = b.ins().iconst(types::I32, i64::from(value));
                        let idx = i32::from(ureg & 15);
                        match ureg >> 4 {
                            0 => {
                                b.ins().store(
                                    mem,
                                    val_v,
                                    core_ptr,
                                    offset_of!(Core, r) as i32 + 4 * idx,
                                );
                                let zero8 = b.ins().iconst(types::I8, 0);
                                let off_fx_lo = (offset_of!(Core, fx)
                                    + offset_of!(super::float_ops::ExtFile, lo))
                                    as i32;
                                b.ins().store(mem, zero8, core_ptr, off_fx_lo + idx);
                            }
                            1 => {
                                b.ins().store(
                                    mem,
                                    val_v,
                                    core_ptr,
                                    offset_of!(Core, i) as i32 + 4 * idx,
                                );
                            }
                            2 => {
                                b.ins().store(
                                    mem,
                                    val_v,
                                    core_ptr,
                                    offset_of!(Core, m) as i32 + 4 * idx,
                                );
                            }
                            3 => {
                                b.ins().store(
                                    mem,
                                    val_v,
                                    core_ptr,
                                    offset_of!(Core, l) as i32 + 4 * idx,
                                );
                            }
                            _ => {
                                let u_v = b.ins().iconst(types::I8, i64::from(ureg));
                                b.ins().call(fn_wureg_ref, &[core_ptr, u_v, val_v]);
                            }
                        }
                    }
                    DecodedOp::Move(MoveOp::RegMove { src, dst, .. }) => {
                        if src < 16 && dst < 16 {
                            let mem = MemFlags::trusted();
                            let off_r = offset_of!(Core, r) as i32;
                            let off_fx_tag = (offset_of!(Core, fx)
                                + offset_of!(super::float_ops::ExtFile, tag))
                                as i32;
                            let off_fx_lo = (offset_of!(Core, fx)
                                + offset_of!(super::float_ops::ExtFile, lo))
                                as i32;
                            let s_idx = i32::from(src);
                            let d_idx = i32::from(dst);
                            let v = b.ins().load(types::I32, mem, core_ptr, off_r + 4 * s_idx);
                            b.ins().store(mem, v, core_ptr, off_r + 4 * d_idx);
                            let tag =
                                b.ins()
                                    .load(types::I32, mem, core_ptr, off_fx_tag + 4 * s_idx);
                            b.ins().store(mem, tag, core_ptr, off_fx_tag + 4 * d_idx);
                            let lo = b.ins().load(types::I8, mem, core_ptr, off_fx_lo + s_idx);
                            b.ins().store(mem, lo, core_ptr, off_fx_lo + d_idx);
                        } else {
                            let s_v = b.ins().iconst(types::I8, i64::from(src));
                            let d_v = b.ins().iconst(types::I8, i64::from(dst));
                            b.ins().call(fn_rmove_ref, &[core_ptr, s_v, d_v]);
                        }
                    }
                    DecodedOp::Move(MoveOp::DagMem {
                        ii,
                        mi,
                        ureg,
                        write,
                        post_modify,
                        cond: 31,
                        compute: None,
                    }) if sram_fast
                        && ureg < 16
                        && !matches!(ii, 0 | 1 | 8 | 9)
                        && !Self::lw_flag(insn_kind, insn_w) =>
                    {
                        let arg_kind = match insn_kind {
                            Kind::T3a => 0,
                            Kind::T3b => 1,
                            _ => 2,
                        };
                        let fail_blk = b.create_block();
                        b.append_block_param(fail_blk, types::I32);
                        emit_inline_sram_dag_transfer(
                            &mut b,
                            core_ptr,
                            sram_ptr,
                            ii,
                            mi,
                            ureg,
                            write,
                            post_modify,
                            hoisted.as_ref(),
                            ssa.as_ref(),
                            insn_pc,
                            insn_w,
                            arg_kind,
                            fn_dagmem_ref,
                            fail_blk,
                        );
                        let cur = b.current_block().unwrap();
                        b.switch_to_block(fail_blk);
                        b.seal_block(fail_blk);
                        let status = b.block_params(fail_blk)[0];
                        let is_fault = b.ins().icmp_imm(IntCC::Equal, status, 1);
                        let base_steps = b.ins().imul_imm(iters_done, len_insns as i64);
                        let steps_fault = b.ins().iadd_imm(base_steps, idx as i64);
                        let steps_mod = b.ins().iadd_imm(base_steps, idx as i64 + 1);
                        let steps_out = b.ins().select(is_fault, steps_fault, steps_mod);

                        let is_last_loop_insn = is_loop && insn_pc == loop_end;
                        let iters_plus1 = b.ins().iadd_imm(iters_done, 1);
                        let iters_mod = if is_last_loop_insn {
                            iters_plus1
                        } else {
                            iters_done
                        };
                        let iters_out = b.ins().select(is_fault, iters_done, iters_mod);

                        let pc_v = b.ins().iconst(types::I32, i64::from(insn_pc));
                        let mod_pc = if is_last_loop_insn {
                            start_pc
                        } else {
                            insn_next_pc
                        };
                        let mod_pc_v = b.ins().iconst(types::I32, i64::from(mod_pc));
                        let resume_pc_v = b.ins().select(is_fault, pc_v, mod_pc_v);
                        b.ins()
                            .jump(early_exit, &[steps_out, iters_out, resume_pc_v]);
                        b.switch_to_block(cur);
                    }
                    DecodedOp::DualMove(dm)
                        if sram_fast
                            && !dm.dm_write
                            && !dm.pm_write
                            && dm.dm_reg < 16
                            && dm.pm_reg < 16
                            && !matches!(dm.dmi, 0 | 1 | 8 | 9)
                            && !matches!(dm.pmi, 0 | 1 | 8 | 9)
                            && (dm.compute_raw >> 22) & 1 == 0 =>
                    {
                        let arg_kind = i64::from(insn_kind == Kind::T1a);
                        let fail_blk = b.create_block();
                        b.append_block_param(fail_blk, types::I32);
                        emit_inline_dual_sram_load(
                            &mut b,
                            core_ptr,
                            sram_ptr,
                            dm,
                            fast_float,
                            hoisted.as_ref(),
                            ssa.as_ref(),
                            insn_pc,
                            insn_w,
                            arg_kind,
                            fn_fmul_ref,
                            fn_fmult_ref,
                            fn_mov_ref,
                            fn_av_ref,
                            fn_comp_ref,
                            fn_dual_ref,
                            fail_blk,
                        );
                        let cur = b.current_block().unwrap();
                        b.switch_to_block(fail_blk);
                        b.seal_block(fail_blk);
                        let status = b.block_params(fail_blk)[0];
                        let is_fault = b.ins().icmp_imm(IntCC::Equal, status, 1);
                        let base_steps = b.ins().imul_imm(iters_done, len_insns as i64);
                        let steps_fault = b.ins().iadd_imm(base_steps, idx as i64);
                        let steps_mod = b.ins().iadd_imm(base_steps, idx as i64 + 1);
                        let steps_out = b.ins().select(is_fault, steps_fault, steps_mod);

                        let is_last_loop_insn = is_loop && insn_pc == loop_end;
                        let iters_plus1 = b.ins().iadd_imm(iters_done, 1);
                        let iters_mod = if is_last_loop_insn {
                            iters_plus1
                        } else {
                            iters_done
                        };
                        let iters_out = b.ins().select(is_fault, iters_done, iters_mod);

                        let pc_v = b.ins().iconst(types::I32, i64::from(insn_pc));
                        let mod_pc = if is_last_loop_insn {
                            start_pc
                        } else {
                            insn_next_pc
                        };
                        let mod_pc_v = b.ins().iconst(types::I32, i64::from(mod_pc));
                        let resume_pc_v = b.ins().select(is_fault, pc_v, mod_pc_v);
                        b.ins()
                            .jump(early_exit, &[steps_out, iters_out, resume_pc_v]);
                        b.switch_to_block(cur);
                    }
                    DecodedOp::Move(MoveOp::DagMem { .. }) | DecodedOp::DualMove(_) => {
                        let is_dual = matches!(op, DecodedOp::DualMove(_));
                        let arg_kind = if is_dual {
                            i64::from(insn_kind == Kind::T1a)
                        } else {
                            match insn_kind {
                                Kind::T3a => 0,
                                Kind::T3b => 1,
                                _ => 2,
                            }
                        };
                        let pc_v = b.ins().iconst(types::I32, i64::from(insn_pc));
                        let w_v = b.ins().iconst(types::I64, insn_w as i64);
                        let k_v = b.ins().iconst(types::I8, arg_kind);
                        let call = if is_dual {
                            b.ins().call(fn_dual_ref, &[core_ptr, pc_v, w_v, k_v])
                        } else {
                            b.ins().call(fn_dagmem_ref, &[core_ptr, pc_v, w_v, k_v])
                        };
                        let status = b.inst_results(call)[0];
                        let ok = b.ins().icmp_imm(IntCC::Equal, status, 0);
                        let ok_blk = b.create_block();
                        let fail_blk = b.create_block();
                        b.ins().brif(ok, ok_blk, &[], fail_blk, &[]);

                        b.switch_to_block(fail_blk);
                        b.seal_block(fail_blk);
                        let is_fault = b.ins().icmp_imm(IntCC::Equal, status, 1);
                        let base_steps = b.ins().imul_imm(iters_done, len_insns as i64);
                        let steps_fault = b.ins().iadd_imm(base_steps, idx as i64);
                        let steps_mod = b.ins().iadd_imm(base_steps, idx as i64 + 1);
                        let steps_out = b.ins().select(is_fault, steps_fault, steps_mod);

                        let is_last_loop_insn = is_loop && insn_pc == loop_end;
                        let iters_plus1 = b.ins().iadd_imm(iters_done, 1);
                        let iters_mod = if is_last_loop_insn {
                            iters_plus1
                        } else {
                            iters_done
                        };
                        let iters_out = b.ins().select(is_fault, iters_done, iters_mod);

                        let mod_pc = if is_last_loop_insn {
                            start_pc
                        } else {
                            insn_next_pc
                        };
                        let mod_pc_v = b.ins().iconst(types::I32, i64::from(mod_pc));
                        let resume_pc_v = b.ins().select(is_fault, pc_v, mod_pc_v);
                        b.ins()
                            .jump(early_exit, &[steps_out, iters_out, resume_pc_v]);

                        b.switch_to_block(ok_blk);
                        b.seal_block(ok_blk);
                    }
                    _ => {}
                }
            }

            let next_iters = b.ins().iadd_imm(iters_done, 1);
            let mem = MemFlags::trusted();
            let off_pc = offset_of!(Core, pc) as i32;
            if is_loop {
                let cont = b.ins().icmp(IntCC::UnsignedLessThan, next_iters, max_iters);
                let exit_blk = b.create_block();
                b.ins().brif(cont, loop_hdr, &[next_iters], exit_blk, &[]);
                b.seal_block(loop_hdr);
                b.switch_to_block(exit_blk);
                b.seal_block(exit_blk);
                if let Some(s) = &ssa {
                    s.flush(&mut b, core_ptr);
                }

                let rem_old = b.ins().load(types::I32, mem, rem_ptr, 0);
                let dec32 = b.ins().ireduce(types::I32, next_iters);
                let rem_new = b.ins().isub(rem_old, dec32);
                b.ins().store(mem, rem_new, rem_ptr, 0);
                let start_pc_v = b.ins().iconst(types::I32, i64::from(start_pc));
                b.ins().store(mem, start_pc_v, core_ptr, off_pc);

                let steps = b.ins().imul_imm(next_iters, len_insns as i64);
                b.ins().return_(&[steps]);
            } else {
                b.seal_block(loop_hdr);
                let next_pc_v = b.ins().iconst(types::I32, i64::from(pc));
                b.ins().store(mem, next_pc_v, core_ptr, off_pc);
                let steps = b.ins().iconst(types::I64, len_insns as i64);
                b.ins().return_(&[steps]);
            }

            b.switch_to_block(early_exit);
            b.seal_block(early_exit);
            if let Some(s) = &ssa {
                s.flush(&mut b, core_ptr);
            }
            let e_steps = b.block_params(early_exit)[0];
            let e_iters = b.block_params(early_exit)[1];
            let e_pc = b.block_params(early_exit)[2];
            if is_loop {
                let rem_old = b.ins().load(types::I32, mem, rem_ptr, 0);
                let dec32 = b.ins().ireduce(types::I32, e_iters);
                let rem_new = b.ins().isub(rem_old, dec32);
                b.ins().store(mem, rem_new, rem_ptr, 0);
            }
            b.ins().store(mem, e_pc, core_ptr, off_pc);
            b.ins().return_(&[e_steps]);
            b.finalize();
        }

        jit.module
            .define_function(func_id, &mut ctx)
            .expect("define JIT block");
        jit.module.clear_context(&mut ctx);
        jit.module
            .finalize_definitions()
            .expect("finalize JIT definitions");

        let code_ptr = jit.module.get_finalized_function(func_id);
        let func: JitBlockFn = unsafe { std::mem::transmute(code_ptr) };
        jit.blocks.insert(
            key,
            CompiledBlock {
                func,
                end_pc,
                len_insns,
                is_loop,
            },
        );
        Some(func)
    }

    /// Runs up to `max_steps` instructions using the Level-2 Cranelift JIT for eligible basic
    /// blocks and counter loops, falling back to [`Core::step`] for everything else.
    pub fn run_jit(&mut self, max_steps: u64) -> Option<Fault> {
        let limit = self.steps + max_steps;
        while self.steps < limit {
            let safe_steps = (limit - self.steps).min(self.irq.due.saturating_sub(self.steps));
            if safe_steps == 0
                || self.trace.is_some()
                || self.pending.is_some()
                || self.irq.window_len != 0
                || self.loops.last().is_some_and(|l| l.term.is_some())
            {
                if let Err(f) = self.step() {
                    return Some(f);
                }
                continue;
            }

            let (loop_start, loop_end, loop_rem) = match self.loops.last() {
                Some(l) => (l.start, l.end, l.remaining),
                None => (u32::MAX, u32::MAX, 0),
            };
            let start_pc = self.pc;
            let simd = self.simd();
            let fast_float = self.fast_float;
            let sram_fast = self.periph.is_none()
                && self.watch == u32::MAX
                && self.watch_len <= 1
                && !self.rwatch_on;

            if let Some(func) = self.get_or_compile_jit_block(start_pc, loop_end) {
                let (len_insns, is_loop) = {
                    let blk = &self.jit.as_ref().unwrap().blocks
                        [&(start_pc, loop_end, simd, fast_float, sram_fast)];
                    (blk.len_insns, blk.is_loop)
                };
                let max_iters_by_steps = safe_steps / len_insns;
                let sram_ptr = self.sram.as_mut_ptr();
                if is_loop && start_pc == loop_start && loop_rem > 1 && max_iters_by_steps > 0 {
                    let max_iters = max_iters_by_steps.min(u64::from(loop_rem - 1));
                    let rem_ptr = std::ptr::addr_of_mut!(self.loops.last_mut().unwrap().remaining);
                    let executed = unsafe { func(self as *mut Core, max_iters, rem_ptr, sram_ptr) };
                    self.steps += executed;
                    continue;
                } else if !is_loop && max_iters_by_steps >= 1 {
                    let executed =
                        unsafe { func(self as *mut Core, 1, std::ptr::null_mut(), sram_ptr) };
                    self.steps += executed;
                    continue;
                }
            }

            if let Err(f) = self.step() {
                return Some(f);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::super::support::core_with;

    #[test]
    fn jit_matches_interpreter_on_do_loop_with_fixed_and_float_ops() {
        // Loop body starts at +3, last instruction (NOP) starts at +9.
        let mut program = vec![0x0d00u16, 0x0000, 3 + 3 + 3];
        program.extend_from_slice(&[0x013e, 0x0000, 0x1312]); // R3 = R1 + R2
        program.extend_from_slice(&[0x013e, 0x0013, 0x0312]); // F3 = F1 * F2
        program.extend_from_slice(&[0x0001]); // NOP

        let mut interp = core_with(&program);
        interp.r[0] = 25;
        interp.r[1] = 0x3f80_0000;
        interp.r[2] = 0x4000_0000;

        let mut jit = core_with(&program);
        jit.r[0] = 25;
        jit.r[1] = 0x3f80_0000;
        jit.r[2] = 0x4000_0000;

        assert_eq!(interp.run(1 + 25 * 3), None);
        assert_eq!(jit.run_jit(1 + 25 * 3), None);
        assert_eq!(jit.pc, interp.pc);
        assert_eq!(jit.steps, interp.steps);
        assert_eq!(jit.r, interp.r);
        assert_eq!(jit.flags, interp.flags);
        assert_eq!(jit.astat(0), interp.astat(0));
        assert!(jit.loops.is_empty());
    }

    #[test]
    fn jit_matches_interpreter_on_fixed_overflow_and_alusat() {
        // R3 = R1 + R2 with signed overflow (0x7fff_ffff + 1)
        let program = [0x013eu16, 0x0000, 0x1312];
        for alusat in [false, true] {
            let mut interp = core_with(&program);
            let mut jit = core_with(&program);
            if alusat {
                interp.mode1 |= super::super::float::MODE1_ALUSAT;
                jit.mode1 |= super::super::float::MODE1_ALUSAT;
            }
            (interp.r[1], interp.r[2]) = (0x7FFF_FFFF, 1);
            (jit.r[1], jit.r[2]) = (0x7FFF_FFFF, 1);

            assert_eq!(interp.run(1), None);
            assert_eq!(jit.run_jit(1), None);
            assert_eq!(jit.r[3], interp.r[3]);
            assert_eq!(jit.flags, interp.flags);
            assert_eq!(jit.astat(0), interp.astat(0));
        }
    }

    #[test]
    fn jit_matches_interpreter_on_load_imm_and_add_block() {
        // R1 = 7 (17b); R2 = 5 (17b); R3 = R1 + R2 (2a)
        let program = [0x0F81u16, 0x0007, 0x0F82, 0x0005, 0x013e, 0x0000, 0x1312];
        let mut interp = core_with(&program);
        let mut jit = core_with(&program);
        assert_eq!(interp.run(3), None);
        assert_eq!(jit.run_jit(3), None);
        assert_eq!((jit.r[1], jit.r[2], jit.r[3]), (7, 5, 12));
        assert_eq!(jit.r, interp.r);
        assert_eq!(jit.flags, interp.flags);
        assert_eq!(jit.pc, interp.pc);
    }

    #[test]
    fn jit_matches_interpreter_on_dual_move_dot_product_loop() {
        // DO (+3) UNTIL LCE, LCNTR=R5; MRF=MRF+r0*r4(ssf), r0=dm(i6,m3), r4=pm(i10,m10)
        let program = [0x0d05u16, 0x0000, 0x0003, 0x2cc0, 0x921b, 0xc004];
        let mut interp = core_with(&program);
        let mut jit = core_with(&program);
        for c in [&mut interp, &mut jit] {
            c.r[5] = 8;
            (c.i[6], c.m[3], c.l[6], c.b[6]) = (0x9_0100, 1, 4, 0x9_0100);
            (c.i[10], c.m[10], c.l[10], c.b[10]) = (0x9_0300, 1, 4, 0x9_0300);
            for k in 0..4 {
                c.dm_write(0x9_0100 + k, 0x2000_0000).unwrap();
                c.dm_write(0x9_0300 + k, 0x2000_0000).unwrap();
            }
        }
        assert_eq!(interp.run(1 + 8), None);
        assert_eq!(jit.run_jit(1 + 8), None);
        assert_eq!(jit.mr, interp.mr);
        assert_eq!((jit.r[0], jit.r[4]), (interp.r[0], interp.r[4]));
        assert_eq!((jit.i[6], jit.i[10]), (interp.i[6], interp.i[10]));
        assert_eq!((jit.pc, jit.steps), (interp.pc, interp.steps));
        assert!(jit.loops.is_empty());
    }

    #[test]
    fn jit_inline_load_imm_iml_and_reg_move_preserve_state() {
        // R1 = 7 (17b); I2 = 0x1234 (17b); R3 = R1 (5b: 0x10be 0x0180 0x0000 -> let's use R1=7, I2=0x1234, M3=2, L4=8)
        let program = [
            0x0F81u16, 0x0007, // R1 = 7
            0x0F92, 0x1234, // I2 = 0x1234
            0x0FA3, 0x0002, // M3 = 2
            0x0FB4, 0x0008, // L4 = 8
        ];
        let mut interp = core_with(&program);
        let mut jit = core_with(&program);
        assert_eq!(interp.run(4), None);
        assert_eq!(jit.run_auto(4), None);
        assert_eq!((jit.r[1], jit.i[2], jit.m[3], jit.l[4]), (7, 0x1234, 2, 8));
        assert_eq!(jit.r, interp.r);
        assert_eq!(jit.i, interp.i);
        assert_eq!(jit.m, interp.m);
        assert_eq!(jit.l, interp.l);
    }

    #[test]
    fn jit_fast_float_matches_interpreter_on_normal_f32_ops() {
        let mut program = vec![0x0d00u16, 0x0000, 3 + 3 + 3];
        program.extend_from_slice(&[0x013e, 0x0000, 0x1312]); // R3 = R1 + R2
        program.extend_from_slice(&[0x013e, 0x0013, 0x0312]); // F3 = F1 * F2
        program.extend_from_slice(&[0x0001]); // NOP

        let mut interp = core_with(&program);
        let mut jit = core_with(&program);
        jit.fast_float = true;
        for c in [&mut interp, &mut jit] {
            c.r[0] = 10;
            c.r[1] = 1.5f32.to_bits();
            c.r[2] = 2.5f32.to_bits();
        }
        assert_eq!(interp.run(1 + 10 * 3), None);
        assert_eq!(jit.run_jit(1 + 10 * 3), None);
        assert_eq!(f32::from_bits(jit.r[3]), f32::from_bits(interp.r[3]));
        assert_eq!(jit.flags.az, interp.flags.az);
        assert_eq!(jit.flags.an, interp.flags.an);
    }

    #[test]
    fn jit_inline_sram_load_matches_interpreter_linear_and_circular_fallback() {
        // DO (+4) UNTIL LCE, LCNTR=R5; R0 = DM(I2, M0) (T3c: 0x9410); R1 = R1 + R0 (T2a: 0x013e, 0x0000, 0x1110)
        let program = [0x0d05u16, 0x0000, 0x0004, 0x9410, 0x013e, 0x0000, 0x1110];
        for l2 in [0u32, 4u32] {
            let mut interp = core_with(&program);
            let mut jit = core_with(&program);
            for c in [&mut interp, &mut jit] {
                c.r[5] = 8;
                (c.i[2], c.m[0], c.l[2], c.b[2]) = (0x9_0100, 1, l2, 0x9_0100);
                for k in 0..8 {
                    c.dm_write(0x9_0100 + k, k + 1).unwrap();
                }
            }
            assert_eq!(interp.run(1 + 8 * 2), None);
            assert_eq!(jit.run_jit(1 + 8 * 2), None);
            assert_eq!(
                (jit.r[0], jit.r[1], jit.i[2]),
                (interp.r[0], interp.r[1], interp.i[2])
            );
            assert_eq!(jit.flags, interp.flags);
            assert_eq!((jit.pc, jit.steps), (interp.pc, interp.steps));
            assert!(jit.loops.is_empty());
        }
    }

    #[test]
    fn jit_inline_sram_write_and_dual_load_match_interpreter() {
        // 1) T3c store: DM(I2, M0) = R1 (0x9421: DMI=2, DMM=0, D=1, R1)
        // 2) T1a dual load + FADD: F2 = F0 + F4, r0=dm(i6,m3), r4=pm(i10,m10)
        //    Base 0x2cc0_921b_c004 with compute = 0x08_1204 (F2 = F0 + F4) -> 0x2cc0_9208_1204
        let program = [
            0x0d05u16, 0x0000, 0x0004, // DO (+4) UNTIL LCE, LCNTR=R5
            0x9431, // DM(I2, M0) = R1 (T3c: bit36=1, D=1)
            0x2cc0, 0x9208, 0x1204, // F2=F0+F4, r0=dm(i6,m3), r4=pm(i10,m10)
        ];
        let mut interp = core_with(&program);
        let mut jit = core_with(&program);
        for c in [&mut interp, &mut jit] {
            c.r[5] = 4;
            c.r[1] = 0xCAFE_0001;
            c.r[0] = 1.0f32.to_bits();
            c.r[4] = 2.0f32.to_bits();
            (c.i[2], c.m[0], c.l[2]) = (0x9_0200, 1, 0);
            (c.i[6], c.m[3], c.l[6]) = (0x9_0100, 1, 0);
            (c.i[10], c.m[10], c.l[10]) = (0x9_0300, 1, 0);
            for k in 0..4 {
                c.dm_write(0x9_0100 + k, (1.5f32 + k as f32).to_bits())
                    .unwrap();
                c.dm_write(0x9_0300 + k, (0.5f32 + k as f32).to_bits())
                    .unwrap();
            }
        }
        assert_eq!(interp.run(1 + 4 * 2), None);
        assert_eq!(jit.run_jit(1 + 4 * 2), None);
        assert_eq!(
            (jit.r[0], jit.r[2], jit.r[4]),
            (interp.r[0], interp.r[2], interp.r[4])
        );
        assert_eq!(
            (jit.i[2], jit.i[6], jit.i[10]),
            (interp.i[2], interp.i[6], interp.i[10])
        );
        for k in 0..4 {
            assert_eq!(
                jit.dm_read(0x9_0200 + k).unwrap(),
                interp.dm_read(0x9_0200 + k).unwrap()
            );
        }
        assert!(jit.loops.is_empty());
    }

    #[test]
    #[ignore = "micro-benchmark: run with cargo test --release -p nord-sharc --features jit --lib bench_jit_dagmem_loop -- --ignored --nocapture"]
    fn bench_jit_dagmem_loop() {
        // DO (+4) UNTIL LCE, LCNTR=R5; R0 = DM(I2, M0) (T3c: 0x9410); R1 = R1 + R0
        let program = [0x0d05u16, 0x0000, 0x0004, 0x9410, 0x013e, 0x0000, 0x1110];
        let mut interp = core_with(&program);
        let mut jit = core_with(&program);
        for c in [&mut interp, &mut jit] {
            c.r[5] = 2_000_000;
            (c.i[2], c.m[0], c.l[2], c.b[2]) = (0x9_0100, 0, 0, 0x9_0100);
            c.dm_write(0x9_0100, 3).unwrap();
        }
        let t0 = std::time::Instant::now();
        assert_eq!(interp.run(1 + 2_000_000 * 2), None);
        let dt_interp = t0.elapsed();

        assert_eq!(jit.run_jit(3), None);
        let t1 = std::time::Instant::now();
        assert_eq!(jit.run_jit(2_000_000 * 2 - 2), None);
        let dt_jit = t1.elapsed();
        assert_eq!(jit.r[1], interp.r[1]);
        eprintln!(
            "bench_jit_dagmem_loop: interp {:?} ({:.2} Msteps/s) vs jit {:?} ({:.2} Msteps/s)",
            dt_interp,
            interp.steps as f64 / 1e6 / dt_interp.as_secs_f64(),
            dt_jit,
            (2_000_000 * 2 - 2) as f64 / 1e6 / dt_jit.as_secs_f64()
        );
    }

    #[test]
    #[ignore = "micro-benchmark: run with cargo test --release -p nord-sharc --features jit --lib bench_jit_fast_float_throughput -- --ignored --nocapture"]
    fn bench_jit_fast_float_throughput() {
        let mut program = vec![0x0d00u16, 0x0000, 3 + 3 + 3];
        program.extend_from_slice(&[0x013e, 0x0000, 0x1312]); // R3 = R1 + R2
        program.extend_from_slice(&[0x013e, 0x0013, 0x0312]); // F3 = F1 * F2
        program.extend_from_slice(&[0x0001]); // NOP

        let mut c = core_with(&program);
        c.fast_float = true;
        c.r[0] = 2_000_000;
        c.r[1] = 1.5f32.to_bits();
        c.r[2] = 2.0f32.to_bits();
        assert_eq!(c.run_jit(4), None);
        let t0 = std::time::Instant::now();
        assert_eq!(c.run_jit(2_000_000 * 3 - 3), None);
        let dt = t0.elapsed();
        let msteps = (2_000_000 * 3 - 3) as f64 / 1e6 / dt.as_secs_f64();
        eprintln!(
            "bench_jit_fast_float_throughput: {} steps in {:?} ({:.2} Msteps/s)",
            c.steps, dt, msteps
        );
    }

    #[test]
    #[ignore = "micro-benchmark: run with cargo test --release -p nord-sharc --features jit --lib bench_jit_dsp_loop -- --ignored --nocapture"]
    fn bench_jit_dsp_loop() {
        // Real DSP dot-product loop: DO (+3) UNTIL LCE, LCNTR=R5; MRF=MRF+r0*r4(ssf), r0=dm(i6,m3), r4=pm(i10,m10)
        let program = [0x0d05u16, 0x0000, 0x0003, 0x2cc0, 0x921b, 0xc004];
        let mut interp = core_with(&program);
        let mut jit = core_with(&program);
        for c in [&mut interp, &mut jit] {
            c.r[5] = 2_000_000;
            (c.i[6], c.m[3], c.l[6], c.b[6]) = (0x9_0100, 1, 64, 0x9_0100);
            (c.i[10], c.m[10], c.l[10], c.b[10]) = (0x9_0300, 1, 64, 0x9_0300);
            for k in 0..64 {
                c.dm_write(0x9_0100 + k, 0x2000_0000).unwrap();
                c.dm_write(0x9_0300 + k, 0x2000_0000).unwrap();
            }
        }
        let t0 = std::time::Instant::now();
        assert_eq!(interp.run(1 + 2_000_000), None);
        let dt_interp = t0.elapsed();

        assert_eq!(jit.run_jit(2), None);
        let t1 = std::time::Instant::now();
        assert_eq!(jit.run_jit(2_000_000 - 1), None);
        let dt_jit = t1.elapsed();
        assert_eq!(jit.mr, interp.mr);
        eprintln!(
            "bench_jit_dsp_loop: interp {:?} ({:.2} Msteps/s) vs jit {:?} ({:.2} Msteps/s)",
            dt_interp,
            interp.steps as f64 / 1e6 / dt_interp.as_secs_f64(),
            dt_jit,
            (2_000_000 - 1) as f64 / 1e6 / dt_jit.as_secs_f64()
        );
    }

    #[test]
    #[ignore = "micro-benchmark: run with cargo test --release -p nord-sharc --features jit --lib bench_jit_throughput -- --ignored --nocapture"]
    fn bench_jit_throughput() {
        let mut program = vec![0x0d00u16, 0x0000, 3 + 3 + 3];
        program.extend_from_slice(&[0x013e, 0x0000, 0x1312]); // R3 = R1 + R2
        program.extend_from_slice(&[0x013e, 0x0013, 0x0312]); // F3 = F1 * F2
        program.extend_from_slice(&[0x0001]); // NOP

        let mut c = core_with(&program);
        c.r[0] = 2_000_000;
        c.r[1] = 0x3f80_0000;
        c.r[2] = 0x4000_0000;
        // Warm up JIT compilation of the loop block before timing.
        assert_eq!(c.run_jit(4), None);
        let t0 = std::time::Instant::now();
        assert_eq!(c.run_jit(2_000_000 * 3 - 3), None);
        let dt = t0.elapsed();
        let msteps = (2_000_000 * 3 - 3) as f64 / 1e6 / dt.as_secs_f64();
        eprintln!(
            "bench_jit_throughput: {} steps in {:?} ({:.2} Msteps/s)",
            c.steps, dt, msteps
        );
    }

    #[test]
    fn jit_inline_sram_write_invalidates_other_cached_block() {
        // Block A at 0x12_0000 (nw 0x9_0000): R0 = 1 (0x0F80, 0x0001)
        // Block B at 0x12_0020 (nw 0x9_0010): DM(I2, M0) = R1 (0x9431)
        let mut program = vec![0u16; 0x22];
        program[0] = 0x0F80;
        program[1] = 0x0001;
        program[0x20] = 0x9431;

        let mut interp = core_with(&program);
        let mut jit = core_with(&program);
        for c in [&mut interp, &mut jit] {
            c.i[2] = 0x9_0000;
            c.m[0] = 0;
            c.r[1] = 0x0002_0F80; // R0 = 2
        }

        // Execute Block A first so it is cached in both decode_cache and jit.blocks.
        assert_eq!(interp.run(1), None);
        assert_eq!(jit.run_jit(1), None);
        assert_eq!((interp.r[0], jit.r[0]), (1, 1));

        // Execute Block B, overwriting Block A in SRAM.
        interp.pc = 0x12_0020;
        jit.pc = 0x12_0020;
        assert_eq!(interp.run(1), None);
        assert_eq!(jit.run_jit(1), None);

        // Re-execute Block A: must observe the updated instruction (R0 = 2).
        interp.pc = 0x12_0000;
        jit.pc = 0x12_0000;
        assert_eq!(interp.run(1), None);
        assert_eq!(jit.run_jit(1), None);
        assert_eq!(jit.r[0], 2);
        assert_eq!(jit.r[0], interp.r[0]);
    }

    #[test]
    fn jit_sram_write_with_watch_records_watch_addrs() {
        let program = [0x9431u16]; // DM(I2, M0) = R1
        let mut interp = core_with(&program);
        let mut jit = core_with(&program);
        for c in [&mut interp, &mut jit] {
            c.i[2] = 0x9_0100;
            c.r[1] = 0xDEAD_BEEF;
            c.watch = 0x9_0100;
            c.watch_len = 4;
        }
        assert_eq!(interp.run(1), None);
        assert_eq!(jit.run_jit(1), None);
        assert_eq!(jit.watch_addrs, vec![0x9_0100]);
        assert_eq!(jit.watch_addrs, interp.watch_addrs);
        assert_eq!(jit.watch_hits.len(), interp.watch_hits.len());
    }

    #[test]
    fn jit_circular_dag_pos_and_neg_modify_matches_interp() {
        // Loop 10 times: R0 = DM(I2, M0) (0x9410, post-modify) + R1 = DM(I3, M1) (0x9531? wait: let's use 0x9511)
        // 0x9410: T3c load R0 = DM(I2, M0)
        // 0x9511: T3c load R1 = DM(I3, M1)
        // Also test DualMove with circular L != 0 and FixedMult:
        // DO (+3) UNTIL LCE, LCNTR=R5; MRF=MRF+r0*r4(ssf), r0=dm(i6,m3), r4=pm(i10,m10)
        let prog = [0x0d05u16, 0x0000, 0x0003, 0x2cc0, 0x921b, 0xc004];
        let mut interp = core_with(&prog);
        let mut jit = core_with(&prog);
        for c in [&mut interp, &mut jit] {
            c.mode1 = crate::core::dag::MODE1_CBUFEN;
            c.r[5] = 20;
            // Positive modify (m = +3) on i6 (l = 8), negative modify (m = -3) on i10 (l = 8)
            (c.i[6], c.m[3], c.l[6], c.b[6]) = (0x9_0100, 3, 8, 0x9_0100);
            (c.i[10], c.m[10], c.l[10], c.b[10]) = (0x9_0306, (-3i32) as u32, 8, 0x9_0300);
            for k in 0..8u32 {
                c.dm_write(0x9_0100 + k, (k + 1) * 0x0800_0000).unwrap();
                c.dm_write(0x9_0300 + k, (8 - k) * 0x0400_0000).unwrap();
            }
        }
        assert_eq!(interp.run(21), None);
        assert_eq!(jit.run_jit(21), None);
        assert_eq!(jit.mr, interp.mr);
        assert_eq!(jit.i[6], interp.i[6]);
        assert_eq!(jit.i[10], interp.i[10]);
        assert_eq!(jit.r[0], interp.r[0]);
        assert_eq!(jit.r[4], interp.r[4]);

        // Also test single DagMem post-modify circular wrap (m > 0 and m < 0) via T3c
        // 0x9410 = R0 = DM(I2, M0) (u=0, i=2, m=0, bit36=1 -> 0x8000 | 0x1000 | 0x0400 | 0x0010 | 0x0000)
        let prog2 = [0x0d05u16, 0x0000, 0x0001, 0x9410];
        let mut interp2 = core_with(&prog2);
        let mut jit2 = core_with(&prog2);
        for c in [&mut interp2, &mut jit2] {
            c.mode1 = crate::core::dag::MODE1_CBUFEN;
            c.r[5] = 12;
            (c.i[2], c.m[0], c.l[2], c.b[2]) = (0x9_0204, (-2i32) as u32, 5, 0x9_0200);
            for k in 0..5u32 {
                c.dm_write(0x9_0200 + k, 0x1234_0000 + k).unwrap();
            }
        }
        assert_eq!(interp2.run(13), None);
        assert_eq!(jit2.run_jit(13), None);
        assert_eq!(jit2.r[0], interp2.r[0]);
        assert_eq!(jit2.i[2], interp2.i[2]);
    }

    #[test]
    fn jit_isa_mode_block_and_sram_write_match_interpreter() {
        fn isa_bytes(words: &[[u16; 3]]) -> Vec<u8> {
            let mut out = Vec::with_capacity(words.len() * 6);
            for &[hi, mid, lo] in words {
                out.extend_from_slice(&lo.to_le_bytes());
                out.extend_from_slice(&mid.to_le_bytes());
                out.extend_from_slice(&hi.to_le_bytes());
            }
            out
        }

        // ISA-mode 48-bit block:
        // 0: R1 = 0x1420                     (T17a)
        // 1: R3 = R1 + R2                    (T2a)
        // 2: IF TRUE DM(I2, M0) = R1         (T3a store, post-modify)
        // 3: DO (+2) UNTIL LCE, LCNTR = 3    (T12a)
        // 4:   IF TRUE DM(I2, M0) = R3       (T3a store, post-modify)
        // 5:   R3 = R3 + R2                  (T2a)
        let image = isa_bytes(&[
            [0x0f81, 0x0000, 0x1420],
            [0x013e, 0x0000, 0x1312],
            [0x543e, 0x8080, 0x0000],
            [0x0c00, 0x0300, 0x0002],
            [0x543e, 0x8180, 0x0000],
            [0x013e, 0x0000, 0x1332],
        ]);
        let mut interp = super::Core::new(&[]);
        let mut jit = super::Core::new(&[]);
        for c in [&mut interp, &mut jit] {
            c.load_isa(&image);
            c.pc = super::super::isa::ISA_BASE;
            c.r[2] = 0x10;
            c.i[2] = 0x9_0100;
            c.m[0] = 1;
        }
        // 3 setup insns + 1 DO + 3 * 2 loop body insns = 10 steps
        assert_eq!(interp.run(10), None);
        assert_eq!(jit.run_jit(10), None);
        assert_eq!((jit.pc, jit.steps), (interp.pc, interp.steps));
        assert_eq!((jit.r, jit.i), (interp.r, interp.i));
        assert_eq!(jit.flags, interp.flags);
        assert!(jit.loops.is_empty());
        for addr in 0x9_0100..0x9_0104 {
            assert_eq!(jit.dm_read(addr), interp.dm_read(addr));
        }

        // Also test an ISA-mode block writing into the ISA Block-0 normal-word window (0x9_2000)
        // AND overwriting a cached VISA block at 0x12_0000 (nw 0x9_0000).
        let visa_prog = [0x0F80u16, 0x0001]; // R0 = 1 at 0x12_0000
        let isa_writer = isa_bytes(&[
            [0x543e, 0x8080, 0x0000], // DM(I2, M0) = R1 (writes 0x9_2000)
            [0x543e, 0x8080, 0x0000], // DM(I2, M0) = R1 (writes 0x9_0000 after I2 update)
        ]);
        let mut interp2 = core_with(&visa_prog);
        let mut jit2 = core_with(&visa_prog);
        for c in [&mut interp2, &mut jit2] {
            c.load_isa(&isa_writer);
            c.r[1] = 0x0002_0F80; // R0 = 2
            c.i[2] = 0x9_2000;
            c.m[0] = (-0x2000i32) as u32;
        }
        // Cache VISA block at 0x12_0000 first.
        assert_eq!(interp2.run(1), None);
        assert_eq!(jit2.run_jit(1), None);
        assert_eq!((interp2.r[0], jit2.r[0]), (1, 1));
        // Run ISA-mode block at ISA_BASE writing 0x9_2000 then 0x9_0000.
        interp2.pc = super::super::isa::ISA_BASE;
        jit2.pc = super::super::isa::ISA_BASE;
        assert_eq!(interp2.run(2), None);
        assert_eq!(jit2.run_jit(2), None);
        assert_eq!(jit2.dm_read(0x9_2000), interp2.dm_read(0x9_2000));
        // Re-execute VISA block at 0x12_0000: must observe R0 = 2.
        interp2.pc = 0x12_0000;
        jit2.pc = 0x12_0000;
        assert_eq!(interp2.run(1), None);
        assert_eq!(jit2.run_jit(1), None);
        assert_eq!(jit2.r[0], 2);
        assert_eq!(jit2.r[0], interp2.r[0]);
    }

    #[test]
    fn jit_inline_fixed_mult_matches_interp_including_mv_and_mu() {
        // 1) MRF = MRF + r0 * r4 (ssf) -> 0x013e, 0x001b, 0xc004
        // 2) MRF = MRF - r0 * r4 (ssf) -> 0x013e, 0x001f, 0xc004
        // 3) R3 = r1 * r2 (ssf)        -> 0x013e, 0x0017, 0x8312
        // 4) R3 = r1 * r2 (ssi)        -> 0x013e, 0x0017, 0x0312
        let prog = [
            0x013eu16, 0x001b, 0xc004, 0x013e, 0x001f, 0xc004, 0x013e, 0x0017, 0x8312, 0x013e,
            0x0017, 0x0312,
        ];
        // Case A: normal + underflow (MU) + overflow (MV + STKY_MOS)
        for (r0, r4, r1, r2) in [
            (
                0x2000_0000u32,
                0x2000_0000u32,
                0x4000_0000u32,
                0x4000_0000u32,
            ),
            (0x0000_0001, 0x0000_0001, 0x8000_0000, 0x8000_0000), // MU on r0*r4, MV on 0x8000_0000*0x8000_0000
            (0x8000_0000, 0x8000_0000, 0x0001_0000, 0x0002_0000), // MV on ssi
        ] {
            let mut interp = core_with(&prog);
            let mut jit = core_with(&prog);
            for c in [&mut interp, &mut jit] {
                (c.r[0], c.r[4], c.r[1], c.r[2]) = (r0, r4, r1, r2);
                c.mr[0] = 0x7FFF_FFFF_0000_0000i128;
            }
            assert_eq!(interp.run(4), None);
            assert_eq!(jit.run_jit(4), None);
            assert_eq!(jit.mr, interp.mr);
            assert_eq!(jit.r, interp.r);
            assert_eq!(jit.astat(0), interp.astat(0));
            assert_eq!(jit.sys.stky[0], interp.sys.stky[0]);
        }
    }

    #[test]
    fn jit_exact_float_matches_interp_including_edge_cases() {
        // F2 = F0 * F1; F3 = F0 + F1; F4 = F0 - F1
        let prog = [
            0x013eu16, 0x0013, 0x0201, 0x013e, 0x0008, 0x1301, 0x013e, 0x0008, 0x2401,
        ];
        let cases = [
            (0.5f32.to_bits(), 0.25f32.to_bits(), 0),   // exact normal
            (0.1f32.to_bits(), 0.2f32.to_bits(), 0),    // 40-bit rounding tail -> fallback
            (1e15f32.to_bits(), 1.0f32.to_bits(), 0),   // exp diff > 28 -> fallback
            (0.0f32.to_bits(), (-0.0f32).to_bits(), 0), // zeros
            (1u32, 2.0f32.to_bits(), 0),                // denormal
            (f32::INFINITY.to_bits(), 0.0f32.to_bits(), 0), // Inf * 0 -> MI
            (f32::NAN.to_bits(), 1.0f32.to_bits(), 0),  // NaN
            (1e38f32.to_bits(), 1e38f32.to_bits(), 0),  // overflow -> MV/AV
            (1e-30f32.to_bits(), 1e-30f32.to_bits(), 0), // underflow -> MU/AU
            (
                0.5f32.to_bits(),
                0.25f32.to_bits(),
                super::super::float::MODE1_RND32,
            ),
            (
                0.1f32.to_bits(),
                0.2f32.to_bits(),
                super::super::float::MODE1_TRUNC,
            ),
        ];
        for (x, y, mode1) in cases {
            let mut interp = core_with(&prog);
            let mut jit = core_with(&prog);
            for c in [&mut interp, &mut jit] {
                c.mode1 = mode1;
                (c.r[0], c.r[1]) = (x, y);
            }
            assert_eq!(interp.run(3), None);
            assert_eq!(jit.run_jit(3), None);
            assert_eq!(
                jit.r, interp.r,
                "r mismatch for x=0x{x:08x} y=0x{y:08x} m1=0x{mode1:x}"
            );
            assert_eq!(
                jit.flags, interp.flags,
                "flags mismatch for x=0x{x:08x} y=0x{y:08x} m1=0x{mode1:x}"
            );
            assert_eq!(
                jit.astat(0),
                interp.astat(0),
                "astat mismatch for x=0x{x:08x} y=0x{y:08x} m1=0x{mode1:x}"
            );
            assert_eq!(
                jit.sys.stky[0], interp.sys.stky[0],
                "stky mismatch for x=0x{x:08x} y=0x{y:08x} m1=0x{mode1:x}"
            );
        }
    }

    #[test]
    fn jit_loop_hoist_positive_and_mutating_mlb_or_mode1() {
        use super::{is_dag_cfg_invariant, DecodedOp, MoveOp};
        use crate::visa::Kind;

        // 1) Positive hoist case: R0 = DM(I2, M0); R1 = R1 + R0 (is_dag_cfg_invariant == true)
        assert!(is_dag_cfg_invariant(&[
            (
                0,
                1,
                Kind::T3c,
                0,
                DecodedOp::Move(MoveOp::DagMem {
                    ii: 2,
                    mi: 0,
                    ureg: 0,
                    write: false,
                    post_modify: true,
                    cond: 31,
                    compute: None,
                }),
            ),
            (
                1,
                4,
                Kind::T2a,
                0,
                DecodedOp::ComputeCond {
                    cond: 31,
                    op: super::super::decode_cache::ComputeOp::FixedAlu {
                        op: 0x01,
                        n: 1,
                        x: 1,
                        y: 0,
                    },
                },
            ),
        ]));
        let pos_prog = [0x0d05u16, 0x0000, 0x0004, 0x9410, 0x013e, 0x0000, 0x1110];
        let mut p_interp = core_with(&pos_prog);
        let mut p_jit = core_with(&pos_prog);
        for c in [&mut p_interp, &mut p_jit] {
            c.mode1 = crate::core::dag::MODE1_CBUFEN;
            c.r[5] = 6;
            (c.i[2], c.m[0], c.l[2], c.b[2]) = (0x9_0100, 1, 4, 0x9_0100);
            for k in 0..4u32 {
                c.dm_write(0x9_0100 + k, k + 10).unwrap();
            }
        }
        assert_eq!(p_interp.run(1 + 6 * 2), None);
        assert_eq!(p_jit.run_jit(1 + 6 * 2), None);
        assert_eq!(
            (p_jit.r[0], p_jit.r[1], p_jit.i[2]),
            (p_interp.r[0], p_interp.r[1], p_interp.i[2])
        );

        // 2) Loop mutating M0 and L2 inside body: must NOT hoist M/L
        assert!(!is_dag_cfg_invariant(&[(
            0,
            2,
            Kind::T17b,
            0,
            DecodedOp::Move(MoveOp::LoadImm {
                ureg: 0x20,
                value: 3,
            }),
        )]));
        // DO (+5) UNTIL LCE, LCNTR=R5; R0 = DM(I2, M0); M0 = 3; L2 = 4
        let ml_prog = [
            0x0d05u16, 0x0000, 0x0005, 0x9410, 0x0FA0, 0x0003, 0x0FB2, 0x0004,
        ];
        let mut ml_interp = core_with(&ml_prog);
        let mut ml_jit = core_with(&ml_prog);
        for c in [&mut ml_interp, &mut ml_jit] {
            c.mode1 = crate::core::dag::MODE1_CBUFEN;
            c.r[5] = 5;
            (c.i[2], c.m[0], c.l[2], c.b[2]) = (0x9_0100, 1, 0, 0x9_0100);
            for k in 0..8u32 {
                c.dm_write(0x9_0100 + k, 100 + k).unwrap();
            }
        }
        assert_eq!(ml_interp.run(1 + 5 * 3), None);
        assert_eq!(ml_jit.run_jit(1 + 5 * 3), None);
        assert_eq!(
            (ml_jit.r[0], ml_jit.i[2], ml_jit.m[0], ml_jit.l[2]),
            (
                ml_interp.r[0],
                ml_interp.i[2],
                ml_interp.m[0],
                ml_interp.l[2]
            )
        );

        // 3) Loop toggling MODE1 (clears CBUFEN after first iteration): must NOT hoist MODE1
        assert!(!is_dag_cfg_invariant(&[(
            0,
            2,
            Kind::T17b,
            0,
            DecodedOp::Move(MoveOp::LoadImm {
                ureg: 0x72,
                value: 0,
            }),
        )]));
        // DO (+3) UNTIL LCE, LCNTR=R5; R0 = DM(I2, M0); MODE1 = 0
        let m1_prog = [0x0d05u16, 0x0000, 0x0003, 0x9410, 0x0FF2, 0x0000];
        let mut m1_interp = core_with(&m1_prog);
        let mut m1_jit = core_with(&m1_prog);
        for c in [&mut m1_interp, &mut m1_jit] {
            c.mode1 = crate::core::dag::MODE1_CBUFEN;
            c.r[5] = 4;
            (c.i[2], c.m[0], c.l[2], c.b[2]) = (0x9_0102, 2, 4, 0x9_0100);
            for k in 0..8u32 {
                c.dm_write(0x9_0100 + k, 200 + k).unwrap();
            }
        }
        assert_eq!(m1_interp.run(1 + 4 * 2), None);
        assert_eq!(m1_jit.run_jit(1 + 4 * 2), None);
        assert_eq!(
            (m1_jit.r[0], m1_jit.i[2], m1_jit.mode1),
            (m1_interp.r[0], m1_interp.i[2], m1_interp.mode1)
        );
    }

    #[test]
    fn jit_loop_hoist_rejects_call_jump_and_indirect_dag_writes() {
        use super::super::decode_cache::FlowOp;
        use super::{is_dag_cfg_invariant, DecodedOp, MoveOp};
        use crate::visa::Kind;

        // 1) Invariance analysis rejects CALL, JUMP out, RegMove to UREG (>=16), DagMem to UREG, and DirectMem.
        assert!(!is_dag_cfg_invariant(&[(
            0,
            3,
            Kind::T8a,
            0,
            DecodedOp::Flow(FlowOp::JumpDirect {
                cond: 31,
                target: 0x12_0020,
                call: true,
                delayed: false,
            }),
        )]));
        assert!(!is_dag_cfg_invariant(&[(
            0,
            3,
            Kind::T8a,
            0,
            DecodedOp::Flow(FlowOp::JumpDirect {
                cond: 31,
                target: 0x12_0020,
                call: false,
                delayed: false,
            }),
        )]));
        assert!(!is_dag_cfg_invariant(&[(
            0,
            2,
            Kind::T5b,
            0,
            DecodedOp::Move(MoveOp::RegMove {
                src: 1,
                dst: 0x20, // M0
                cond: 31,
                compute: None,
            }),
        )]));
        assert!(!is_dag_cfg_invariant(&[(
            0,
            3,
            Kind::T3a,
            0,
            DecodedOp::Move(MoveOp::DagMem {
                ii: 2,
                mi: 0,
                ureg: 0x20, // M0 = DM(I2, M0)
                write: false,
                post_modify: true,
                cond: 31,
                compute: None,
            }),
        )]));
        assert!(!is_dag_cfg_invariant(&[(
            0,
            3,
            Kind::T14a,
            0,
            DecodedOp::Move(MoveOp::DirectMem {
                addr: 0x9_0100,
                ureg: 0x32, // L2 = DM(...)
                write: false,
            }),
        )]));

        // 2) End-to-end loop updating M0 indirectly via DM: DO (+3) UNTIL LCE; M0 = DM(I2, M0)
        // T3a M0 = DM(I2, M0): 0x543e, 0x1000, 0x0000
        let dm_m_prog = [0x0d05u16, 0x0000, 0x0003, 0x543e, 0x1000, 0x0000];
        let mut dm_interp = core_with(&dm_m_prog);
        let mut dm_jit = core_with(&dm_m_prog);
        for c in [&mut dm_interp, &mut dm_jit] {
            c.r[5] = 3;
            (c.i[2], c.m[0]) = (0x9_0100, 2);
            c.dm_write(0x9_0100, 3).unwrap();
            c.dm_write(0x9_0102, 4).unwrap();
            c.dm_write(0x9_0105, 5).unwrap();
        }
        assert_eq!(dm_interp.run(4), None);
        assert_eq!(dm_jit.run_jit(4), None);
        assert_eq!((dm_jit.i[2], dm_jit.m[0]), (dm_interp.i[2], dm_interp.m[0]));

        // 3) End-to-end loop with JUMP out: DO (+6) UNTIL LCE; R0 = DM(I2, M0); JUMP (+5) -> M0 = 9
        let jmp_prog = [
            0x0d05u16, 0x0000, 0x0006, // DO (+6) UNTIL LCE
            0x9410, // R0 = DM(I2, M0)
            0x073e, 0x0000, 0x0005, // JUMP (+5) -> index 9
            0x0001, 0x0001, // skipped padding
            0x0FA0, 0x0009, // M0 = 9
        ];
        let mut j_interp = core_with(&jmp_prog);
        let mut j_jit = core_with(&jmp_prog);
        for c in [&mut j_interp, &mut j_jit] {
            c.r[5] = 4;
            (c.i[2], c.m[0]) = (0x9_0100, 1);
            c.dm_write(0x9_0100, 77).unwrap();
        }
        assert_eq!(j_interp.run(4), None);
        assert_eq!(j_jit.run_jit(4), None);
        assert_eq!(
            (j_jit.r[0], j_jit.i[2], j_jit.m[0], j_jit.pc),
            (j_interp.r[0], j_interp.i[2], j_interp.m[0], j_interp.pc)
        );
    }
}
