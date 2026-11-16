//! Level-2 Cranelift JIT backend for [`Core`], compiling basic blocks and counter loops
//! directly from [`DecodedOp`] to host machine code.
//!
//! Enabled via the optional `jit` Cargo feature (`Core::run_jit`). Any instruction or dynamic
//! condition not yet lowered to Cranelift IR either calls an `extern "C"` helper on `Core` or
//! hands control back to [`Core::step`], preserving 100% oracle fidelity.
use super::decode_cache::{ComputeOp, DecodedOp, DualMoveOp, MoveOp};
use super::exec::Decoded;
use super::{Core, Fault, Flags};
use crate::visa::{Insn, Kind};
use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{types, AbiParam, InstBuilder, MemFlags, UserFuncName};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};
use std::collections::HashMap;
use std::mem::offset_of;

const MAX_BLOCK_INSNS: usize = 64;

/// Compiled native function for a SHARC basic block or self-looping `DO ... UNTIL LCE` body.
///
/// Signature: `(core: *mut Core, max_iters: u64, remaining_ptr: *mut u32) -> u64` (executed steps,
/// or `u64::MAX` on fault).
type JitBlockFn = unsafe extern "C" fn(*mut Core, u64, *mut u32) -> u64;

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
    blocks: HashMap<(u32, u32, bool, bool), CompiledBlock>,
    fn_float_mul: FuncId,
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
        self.blocks.retain(|&(start_pc, _, _, _), blk| {
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

impl Core {
    /// Compiles (or looks up) a JIT basic block starting at `start_pc`, stopping at `loop_end`
    /// if inside a counter loop. Returns `None` if the first instruction is not JIT-eligible.
    fn get_or_compile_jit_block(&mut self, start_pc: u32, loop_end: u32) -> Option<JitBlockFn> {
        let simd = self.simd();
        let fast_float = self.fast_float;
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
        let key = (start_pc, loop_end, simd, fast_float);

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

            let fn_fmul_ref = jit.module.declare_func_in_func(jit.fn_float_mul, b.func);
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
                        match op {
                            ComputeOp::Nop => {}
                            ComputeOp::FixedAlu {
                                op: fop @ (0x01 | 0x02),
                                n,
                                x,
                                y,
                            } => {
                                emit_fixed_add_sub(
                                    &mut b,
                                    core_ptr,
                                    n,
                                    x,
                                    y,
                                    fop == 0x02,
                                    fn_av_ref,
                                );
                            }
                            ComputeOp::FixedAlu {
                                op: fop @ (0x21 | 0x40 | 0x41 | 0x42),
                                n,
                                x,
                                y,
                            } => {
                                emit_fixed_logic_or_pass(&mut b, core_ptr, fop, n, x, y);
                            }
                            ComputeOp::FloatMul { n, x, y } if fast_float => {
                                emit_fast_float_op(&mut b, core_ptr, FastFloatKind::Mul, n, x, y);
                            }
                            ComputeOp::FloatMul { n, x, y } => {
                                let n_v = b.ins().iconst(types::I32, i64::from(n));
                                let x_v = b.ins().iconst(types::I32, i64::from(x));
                                let y_v = b.ins().iconst(types::I32, i64::from(y));
                                b.ins().call(fn_fmul_ref, &[core_ptr, n_v, x_v, y_v]);
                            }
                            ComputeOp::FloatAlu {
                                op: fop @ (0x81 | 0x82),
                                n,
                                x,
                                y,
                            } if fast_float => {
                                let fk = if fop == 0x81 {
                                    FastFloatKind::Add
                                } else {
                                    FastFloatKind::Sub
                                };
                                emit_fast_float_op(&mut b, core_ptr, fk, n, x, y);
                            }
                            other => {
                                if let Some(raw_c) = compute_op_to_raw(other) {
                                    let c_v = b.ins().iconst(types::I64, raw_c as i64);
                                    b.ins().call(fn_comp_ref, &[core_ptr, c_v]);
                                }
                            }
                        }
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

            if let Some(func) = self.get_or_compile_jit_block(start_pc, loop_end) {
                let (len_insns, is_loop) = {
                    let blk =
                        &self.jit.as_ref().unwrap().blocks[&(start_pc, loop_end, simd, fast_float)];
                    (blk.len_insns, blk.is_loop)
                };
                let max_iters_by_steps = safe_steps / len_insns;
                if is_loop && start_pc == loop_start && loop_rem > 1 && max_iters_by_steps > 0 {
                    let max_iters = max_iters_by_steps.min(u64::from(loop_rem - 1));
                    let rem_ptr = std::ptr::addr_of_mut!(self.loops.last_mut().unwrap().remaining);
                    let executed = unsafe { func(self as *mut Core, max_iters, rem_ptr) };
                    self.steps += executed;
                    continue;
                } else if !is_loop && max_iters_by_steps >= 1 {
                    let executed = unsafe { func(self as *mut Core, 1, std::ptr::null_mut()) };
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
}
