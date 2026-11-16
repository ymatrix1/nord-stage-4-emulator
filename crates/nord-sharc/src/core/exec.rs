//! Single-step decoder of [`Core`]: fetches the parcels, decodes the instruction type and
//! dispatches to the compute, move and flow handlers.
use super::decode_cache::{CachedInsn, ComputeOp, DecodeCache, DecodedOp};
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
    /// The same condition for PEy (only meaningful in SIMD mode, false otherwise).
    pub cond_y: bool,
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
    pub(super) fn fetch(&self, pc: u32) -> Decoded {
        if self.isa_at(pc) {
            return self.fetch_isa(pc);
        }
        let p0 = self.parcel(pc);
        let p1 = self.parcel(pc.wrapping_add(1));
        let (mut kind, mut len) = classify(p0, Some(p1));
        let mut parcels = [p0, 0, 0];
        if len > 1 {
            parcels[1] = p1;
        }
        if len > 2 {
            parcels[2] = self.parcel(pc.wrapping_add(2));
        }
        if let Some(isa) = self.isa_window_parcels(pc) {
            // interrupt vector slot: 48-bit ISA words, least significant parcel first
            parcels = isa;
            (kind, len) = (classify(isa[0], Some(isa[1])).0, 3);
        }
        let w = word48(&parcels[..len]);
        Decoded {
            pc,
            insn: Insn { kind, parcels, len },
            w,
            cond_ok: self.cond(bits(w, 37, 33)),
            cond_y: self.simd() && self.cond_for(true, bits(w, 37, 33)),
        }
    }

    /// Runs `op` on PEx when `x_ok` and, in SIMD mode, on PEy when `y_ok`. An operation that is
    /// not modelled faults (or is skipped in lenient mode).
    pub(super) fn on_each_pe(
        &mut self,
        d: &Decoded,
        x_ok: bool,
        y_ok: bool,
        op: impl Fn(&mut Core) -> bool,
    ) -> Result<(), Fault> {
        if x_ok && !op(self) {
            self.unmodelled(d.pc, &d.insn)?;
        }
        if y_ok && self.simd() {
            self.swap_pe();
            let ok = op(self);
            self.swap_pe();
            if !ok {
                self.unmodelled(d.pc, &d.insn)?;
            }
        }
        Ok(())
    }

    /// Runs the compute field `c` of a conditional instruction on each enabled element, each
    /// with its own condition.
    pub(super) fn run_compute(&mut self, d: &Decoded, c: u64) -> Result<(), Fault> {
        self.on_each_pe(d, d.cond_ok, d.cond_y, |core| core.compute(c))
    }

    /// Runs a pre-decoded compute operation on each enabled element.
    #[inline]
    fn run_compute_op(
        &mut self,
        c: &CachedInsn,
        cond_ok: bool,
        cond_y: bool,
        cop: ComputeOp,
    ) -> Result<(), Fault> {
        if cond_ok && !self.exec_compute_op(cop) {
            self.unmodelled(c.pc, &c.insn)?;
        }
        if cond_y && self.simd() {
            self.swap_pe();
            let ok = self.exec_compute_op(cop);
            self.swap_pe();
            if !ok {
                self.unmodelled(c.pc, &c.insn)?;
            }
        }
        Ok(())
    }

    #[cold]
    #[inline(never)]
    fn log_trace(&mut self) {
        if let Some((lo, hi, limit)) = self.trace {
            if self.pc >= lo && self.pc <= hi && self.trace_log.len() < limit {
                let hex = |v: &[u32; 16]| {
                    v.iter()
                        .map(|x| format!("{x:x}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                self.trace_log.push(format!(
                    "pc {:x} R {} S {} I {} M {}",
                    self.pc,
                    hex(&self.r),
                    hex(&self.s),
                    hex(&self.i),
                    hex(&self.m)
                ));
            }
        }
    }

    /// Fetches and decodes the instruction at `pc` into a [`CachedInsn`] on cache miss.
    #[inline(never)]
    pub(super) fn fetch_and_cache(&mut self, pc: u32, isa: bool) -> CachedInsn {
        let d = self.fetch(pc);
        let next_pc = d.pc.wrapping_add(if isa { 1 } else { d.insn.len as u32 });
        let op = DecodedOp::from_insn(pc, &d.insn, d.w);
        let cond = bits(d.w, 37, 33) as u8;
        let entry = CachedInsn {
            pc,
            gen: self.code_gen,
            next_pc,
            isa,
            cond,
            insn: d.insn,
            w: d.w,
            op,
        };
        // Normal-word SRAM begins at 0x9_0000 (`memory.rs::SRAM_WORDS`, 4 bytes/word); in VISA
        // mode `pc` counts 16-bit short words (2 per normal word), so SRAM starts at 0x12_0000.
        const VISA_SRAM_SHORT_BASE: u32 = 0x9_0000 * 2;
        if !isa && pc >= VISA_SRAM_SHORT_BASE {
            let nw_lo = pc / 2;
            let nw_hi = (pc + (d.insn.len as u32).saturating_sub(1)) / 2;
            self.sram_code_nw_lo = self.sram_code_nw_lo.min(nw_lo);
            self.sram_code_nw_hi = self.sram_code_nw_hi.max(nw_hi);
        }
        let cache = self.decode_cache.get_or_insert_with(DecodeCache::new);
        cache.insert(entry);
        entry
    }

    /// Executes one instruction. Returns `Ok(())` or the reason it could not.
    #[inline]
    pub fn step(&mut self) -> Result<(), Fault> {
        if self.steps >= self.irq.due && self.sequencer_event() {
            return Ok(()); // the core slept in IDLE for this step
        }
        if self.trace.is_some() {
            self.log_trace();
        }
        let pc = self.pc;
        let isa = self.isa_at(pc);
        let c = if let Some(hit) = self
            .decode_cache
            .as_deref()
            .and_then(|cache| cache.get(pc, self.code_gen, isa))
            .copied()
        {
            hit
        } else {
            self.fetch_and_cache(pc, isa)
        };
        let mut next = c.next_pc;
        let simd = self.simd();
        let make_decoded = |core: &Core, c: &CachedInsn| -> Decoded {
            Decoded {
                pc: c.pc,
                insn: c.insn,
                w: c.w,
                cond_ok: core.cond(u64::from(c.cond)),
                cond_y: simd && core.cond_for(true, u64::from(c.cond)),
            }
        };
        match c.op {
            DecodedOp::Nop => {}
            DecodedOp::Idle => self.enter_idle(),
            DecodedOp::ComputeCond { cond, op } => {
                let cond_ok = cond == 31 || self.cond(u64::from(cond));
                let cond_y = simd && (cond == 31 || self.cond_for(true, u64::from(cond)));
                self.run_compute_op(&c, cond_ok, cond_y, op)?;
            }
            DecodedOp::ComputeBoth { op } => {
                self.run_compute_op(&c, true, simd, op)?;
            }
            DecodedOp::ModifyImm => {
                let d = make_decoded(self, &c);
                self.exec_modify_imm(&d);
            }
            DecodedOp::StackOp => {
                let d = make_decoded(self, &c);
                self.exec_stack_op(&d);
            }
            DecodedOp::DoUntil => {
                let d = make_decoded(self, &c);
                self.exec_do_until(&d, next)?;
            }
            DecodedOp::Type25 => {
                let d = make_decoded(self, &c);
                next = self.exec_type25(&d, next)?;
            }
            DecodedOp::ShiftImm => {
                let d = make_decoded(self, &c);
                self.exec_shift_imm(&d)?;
            }
            DecodedOp::ShiftMove => {
                let d = make_decoded(self, &c);
                self.exec_shift_move(&d)?;
            }
            DecodedOp::DualMove(dm) => {
                let d = make_decoded(self, &c);
                self.exec_dual_move_op(&d, dm)?;
            }
            DecodedOp::Flow(fl) => {
                let d = make_decoded(self, &c);
                next = self.exec_flow_op(&d, fl, next)?;
            }
            DecodedOp::Move(mv) => {
                let d = make_decoded(self, &c);
                self.exec_move_op(&d, mv)?;
            }
            DecodedOp::Unimplemented => return Err(self.fault(c.pc, &c.insn)),
        }
        let pc = c.pc;
        let kind = c.insn.kind;
        self.pc = if self.loops.is_empty() && self.pending.is_none() {
            next
        } else {
            self.finish_step(pc, kind, next)
        };
        self.steps += 1;
        if self.cb_wrap.get() != 0 {
            self.latch_cb_overflow();
        }
        if self.irq.window_len != 0 {
            self.close_window_if_left();
        }
        Ok(())
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
    fn simd_compute_runs_on_both_elements_with_their_own_flags() {
        // R3 = R1 + R2 (type 2a, unconditional TRUE = cond 31): parcels 013e 0000 1312
        let program = [0x013eu16, 0x0000, 0x1312];
        let mut c = core_with(&program);
        c.mode1 = 1 << 21;
        (c.r[1], c.r[2], c.s[1], c.s[2]) = (4, 5, 10, 20);
        assert_eq!(c.step(), Ok(()));
        assert_eq!((c.r[3], c.s[3]), (9, 30));
        assert_eq!((c.flags.az, c.flags_y.az), (false, false));
        // without SIMD only PEx runs
        let mut x_only = core_with(&program);
        (x_only.r[1], x_only.r[2], x_only.s[1], x_only.s[2]) = (4, 5, 10, 20);
        assert_eq!(x_only.step(), Ok(()));
        assert_eq!((x_only.r[3], x_only.s[3]), (9, 0));
    }

    #[test]
    fn simd_conditions_are_evaluated_per_element() {
        // IF EQ R3 = R1 + R2 where only PEy has AZ set
        let program = [0x0100u16, 0x0000, 0x1312];
        let mut c = core_with(&program);
        c.mode1 = 1 << 21;
        (c.r[1], c.r[2], c.s[1], c.s[2]) = (4, 5, 10, 20);
        c.flags_y.az = true;
        assert_eq!(c.step(), Ok(()));
        assert_eq!((c.r[3], c.s[3]), (0, 30), "only PEy meets the condition");
    }

    #[test]
    fn faulting_step_leaves_state_untouched() {
        let mut c = core_with(&[0xd70a, 0x3c23, 0xd70a]);
        let before = (c.pc, c.steps);
        assert!(c.step().is_err());
        assert_eq!((c.pc, c.steps), before);
    }

    #[test]
    fn simd_multiplier_flags_belong_to_each_element() {
        // R3 = R1 * R2 (signed integer): PEx positive, PEy negative product -> MN (ASTAT bit 6) only in ASTATy
        let mut c = core_with(&[0x0000u16]);
        c.mode1 = 1 << 21;
        c.set_astat(0, 0);
        (c.r[1], c.r[2], c.s[1], c.s[2]) = (0x4000_0000, 0x4000_0000, 0xC000_0000, 0x4000_0000);
        c.on_each_pe(&c.fetch(0), true, true, |core| {
            core.compute((1 << 20) | (0x70 << 12) | (3 << 8) | (1 << 4) | 2)
        })
        .unwrap();
        assert_eq!((c.astat(0) >> 6) & 1, 0);
        assert_eq!((c.astat(1) >> 6) & 1, 1);
    }

    #[test]
    fn trace_logs_registers_before_steps_inside_the_window() {
        let mut c = core_with(&[0x0001, 0x0001, 0x0001]);
        c.r[3] = 0xABCD;
        let start = c.pc;
        c.trace = Some((start, start, 10));
        for _ in 0..3 {
            c.step().unwrap();
        }
        assert_eq!(c.trace_log.len(), 1, "only the step at the window pc");
        let want = format!("pc {start:x} R 0 0 0 abcd");
        assert!(c.trace_log[0].starts_with(&want), "{}", c.trace_log[0]);
        c.trace = Some((0, u32::MAX, 1));
        c.trace_log.clear();
        c.pc = start;
        c.step().unwrap();
        c.step().unwrap();
        assert_eq!(c.trace_log.len(), 1, "the limit stops the log");
    }

    #[test]
    #[ignore = "micro-benchmark: run with cargo test --release -p nord-sharc --lib bench_step_throughput -- --ignored --nocapture"]
    fn bench_step_throughput() {
        // Loop of 4 instructions: fixed add, float mul, nop, nop, repeated 2_000_000 times (8M steps).
        let mut program = vec![0x0d00u16, 0x0000, 3 + 3 + 3 + 1];
        program.extend_from_slice(&[0x013e, 0x0000, 0x1312]); // R3 = R1 + R2
        program.extend_from_slice(&[0x013e, 0x0013, 0x0312]); // F3 = F1 * F2
        program.extend_from_slice(&[0x0001]); // NOP
        let mut c = core_with(&program);
        c.r[0] = 2_000_000;
        c.r[1] = 0x3f80_0000;
        c.r[2] = 0x4000_0000;
        let t0 = std::time::Instant::now();
        assert_eq!(c.run(1 + 2_000_000 * 3), None);
        let dt = t0.elapsed();
        let msteps = c.steps as f64 / 1e6 / dt.as_secs_f64();
        eprintln!(
            "bench_step_throughput: {} steps in {:?} ({:.2} Msteps/s)",
            c.steps, dt, msteps
        );
    }
}
