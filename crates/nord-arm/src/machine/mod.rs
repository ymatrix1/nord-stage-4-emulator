//! The ARM0 machine: CPU, system bus, code hooks (stubs), run loop and calls.
//!
//! Replaces `tools/arm_emu.py` + `tools/arm_steady.py` (`ArmEmu`, `run_until`, `call_preserving`,
//! `inject_irq`) with zero Python callbacks: every hook is a native enum arm.

pub mod boot;
pub mod consumer;
pub mod dsp_return;
mod fastrun;
pub mod grouper;
mod hooks;
pub mod i2c;
pub mod idle;
pub mod layout;
pub mod metrics;
pub mod micro;
pub mod paced;
pub mod panel;
mod probe;
pub mod producer;
mod runstats;
pub mod sb;
pub mod sysbus;
pub mod system;
pub mod uart;

pub use fastrun::CodeCache;
pub use hooks::Hook;
pub use runstats::RunStats;
use std::collections::HashMap;
pub use sysbus::SysBus;

use crate::cpu::{Cpu, Fault};
use crate::mem::HOOK_X;

/// Return address of a top-level call; never executed (`tools/arm_emu.py` SENTINEL).
pub const SENTINEL: u32 = 0x7010_0000;
/// Stack top of the harness (`tools/arm_emu.py` STACK_TOP).
pub const STACK_TOP: u32 = 0x700F_0000;
/// Size of the zero-filled null-call guard at address 0 (`ArmEmu.guard_null_calls`).
pub const LOW_MEMORY: u32 = 0x1_0000;

/// Hook runs in a row without a CPU step before `run` gives up (a stub returning to a hooked pc).
pub const HOOK_SPIN: u32 = 64;
/// Entries kept in `null_calls`; later ones only increment `null_call_count`.
pub const NULL_CALL_CAP: usize = 4096;

/// Why [`Machine::run`] returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// Instruction budget used up.
    Budget,
    /// `pc` reached the `until` address.
    Until,
    /// `pc` reached the `stop_pc` address (about to execute it).
    StopPc,
    Fault(Fault),
    /// A hook returned to a hooked address [`HOOK_SPIN`] times without executing an instruction.
    HookLoop(u32),
}

/// CPU + bus + hooks.
#[derive(Clone)]
pub struct Machine {
    pub cpu: Cpu,
    pub bus: SysBus,
    pub(crate) hooks: HashMap<u32, Hook>,
    /// Calls fetched from page 0 (null guard): `(address, lr)`.
    pub null_calls: Vec<(u32, u32)>,
    /// Every null call, also those past `NULL_CALL_CAP`.
    pub null_call_count: u64,
    /// `WFI` instructions executed (both run paths: the rest of the budget is skipped).
    pub wfi: u64,
    /// Interrupts injected while the CPSR I bit was set (`inject_irq` ignores the mask, as the
    /// Python harness does).
    pub irq_while_masked: u64,
    pub null_guard: bool,
    /// Address that stops `run_idle_skip` before executing it (boot to the main loop).
    pub stop_pc: Option<u32>,
    pub dsp_return: Option<dsp_return::DspReturn>,
    pub heap_top: u32,
    /// Decoded ARM code (see `fastrun.rs`).
    pub code: CodeCache,
    /// Run ARM-state code from the predecode cache (default on); `false` keeps the plain
    /// fetch-decode-execute path for comparison runs.
    pub predecode: bool,
    /// Work counters of the run loops (executed instructions are derived from `cpu.insns`).
    pub stats: RunStats,
    /// Firmware addresses of the loaded OS version (`load_firmware` picks it from the image).
    pub layout: layout::Layout,
    /// Whether `load_firmware` recognised the image length (`false`: the default layout is a guess).
    pub layout_known: bool,
}

impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine {
    pub fn new() -> Self {
        let mut bus = SysBus::new();
        bus.install_devices();
        Self {
            cpu: Cpu::new(),
            bus,
            hooks: HashMap::new(),
            null_calls: Vec::new(),
            null_call_count: 0,
            wfi: 0,
            irq_while_masked: 0,
            null_guard: false,
            stop_pc: None,
            dsp_return: None,
            heap_top: 0,
            code: CodeCache::default(),
            predecode: true,
            stats: RunStats::default(),
            layout: layout::Layout::V166,
            layout_known: false,
        }
    }

    pub fn reg(&self, n: usize) -> u32 {
        self.cpu.r[n]
    }

    pub fn set_reg(&mut self, n: usize, v: u32) {
        self.cpu.r[n] = v;
    }

    pub fn read32(&mut self, addr: u32) -> u32 {
        self.bus.mem.read32(addr)
    }

    pub fn write32(&mut self, addr: u32, v: u32) {
        self.bus.mem.write32(addr, v);
    }

    /// Register `hook` at `addr` (runs before the instruction there).
    pub fn add_hook(&mut self, addr: u32, hook: Hook) {
        self.hooks.insert(addr, hook);
        self.bus.mem.add_flag(addr, addr, HOOK_X);
    }

    /// Turn on the null-call guard: map and zero page 0..`LOW_MEMORY`, fetches there return 0.
    pub fn guard_null_calls(&mut self) {
        self.bus.mem.map(0, LOW_MEMORY);
        self.bus.mem.add_flag(0, LOW_MEMORY - 1, HOOK_X);
        self.null_guard = true;
    }

    /// Zero the null-guard page (done between run chunks, `tools/arm_steady.py::run_until`).
    pub fn zero_low_memory(&mut self) {
        self.bus.mem.zero(0, LOW_MEMORY as usize);
    }

    /// Run from the current `pc`. Stops at `until`, at `stop_pc` (before executing it), after
    /// `max` instructions or on a fault. Returns `(instructions executed, reason)`.
    pub fn run(&mut self, max: u64, until: u32, stop_pc: Option<u32>) -> (u64, Stop) {
        let start = self.cpu.insns;
        let limit = start + max;
        self.drop_stale_code();
        let mut page = u32::MAX;
        let mut hooked = false;
        let stop = stop_pc.unwrap_or(u32::MAX);
        let mut spin = 0u32;
        let reason = loop {
            if self.cpu.insns >= limit {
                break Stop::Budget;
            }
            let pc = self.cpu.r[15];
            if pc == until {
                break Stop::Until;
            }
            if pc == stop {
                break Stop::StopPc;
            }
            if pc >> 12 != page {
                page = pc >> 12;
                hooked = self.bus.mem.flags(pc) & HOOK_X != 0;
            }
            if hooked && self.run_hook(pc) {
                self.stats.hook_runs += 1;
                page = u32::MAX;
                spin += 1;
                if spin >= HOOK_SPIN {
                    break Stop::HookLoop(self.cpu.r[15]);
                }
                continue;
            }
            spin = 0;
            if self.predecode && !hooked && !self.cpu.thumb && pc & 3 == 0 {
                if let Some(f) = self.run_cached(limit, until, stop) {
                    break Stop::Fault(f);
                }
                continue;
            }
            self.bus.pc = pc;
            self.stats.plain_steps += 1;
            if let Err(f) = self.cpu.step(&mut self.bus) {
                break Stop::Fault(f);
            }
        };
        (self.cpu.insns - start, reason)
    }

    /// [`Machine::run`] with the one `WFI` semantic of both run paths: the instruction completes
    /// and the rest of the budget is skipped (virtual time jumps to the end; the tick interrupt is
    /// the only wake-up the harness models).
    pub fn run_wfi(&mut self, max: u64, until: u32, stop_pc: Option<u32>) -> (u64, Stop) {
        let start = self.cpu.insns;
        let end = start + max;
        let (_, stop) = self.run(max, until, stop_pc);
        if stop == Stop::Fault(Fault::Wfi) {
            self.wfi += 1;
            self.cpu.insns = end;
            return (max, Stop::Budget);
        }
        (self.cpu.insns - start, stop)
    }

    /// Hash of registers, CPSR and every non-zero memory page (equivalence checks).
    pub fn state_hash(&self) -> u64 {
        let mut h = self.bus.mem.state_hash();
        for r in self.cpu.r.iter().chain([&self.cpu.cpsr(), &self.cpu.spsr]) {
            h = (h ^ *r as u64).wrapping_mul(0x100_0000_01B3);
        }
        h
    }

    /// Call `addr(args..)` with the AAPCS (r0..r3) and run until it returns to [`SENTINEL`].
    pub fn call(&mut self, addr: u32, args: &[u32], max: u64) -> (u32, Stop) {
        for (i, a) in args.iter().take(4).enumerate() {
            self.cpu.r[i] = *a;
        }
        self.cpu.r[13] = STACK_TOP - 0x1000;
        self.cpu.r[14] = SENTINEL;
        self.cpu.thumb = addr & 1 != 0;
        self.cpu.r[15] = addr & !1;
        let (_, stop) = self.run(max, SENTINEL, None);
        (self.cpu.r[0], stop)
    }

    /// Call `addr(args..)` in the middle of a running program and restore every register and the
    /// CPSR afterwards (`arm_steady.call_preserving`): the call uses the stack below the current sp.
    pub fn call_preserving(&mut self, addr: u32, args: &[u32], max: u64) -> (u32, Stop) {
        let saved = self.cpu.clone();
        self.cpu.r[13] = saved.r[13].wrapping_sub(0x800) & !7;
        self.cpu.r[14] = SENTINEL;
        for (i, a) in args.iter().take(4).enumerate() {
            self.cpu.r[i] = *a;
        }
        self.cpu.thumb = addr & 1 != 0;
        self.cpu.r[15] = addr & !1;
        let (_, stop) = self.run(max, SENTINEL, None);
        let ret = self.cpu.r[0];
        let insns = self.cpu.insns;
        self.cpu = saved;
        self.cpu.insns = insns;
        (ret, stop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine_with(code: &[u32]) -> Machine {
        let mut m = Machine::new();
        for (i, w) in code.iter().enumerate() {
            m.bus.mem.write32(0xC000_0000 + 4 * i as u32, *w);
        }
        m.cpu.r[15] = 0xC000_0000;
        m
    }

    #[test]
    fn run_stops_on_budget_until_and_stop_pc() {
        // mov r0,#1 ; mov r1,#2 ; mov r2,#3 ; b .
        let code = [0xE3A0_0001, 0xE3A0_1002, 0xE3A0_2003, 0xEAFF_FFFE];
        let mut m = machine_with(&code);
        assert_eq!(m.run(2, 0, None), (2, Stop::Budget));
        assert_eq!(m.run(10, 0xC000_000C, None), (1, Stop::Until));
        let mut m = machine_with(&code);
        assert_eq!(m.run(10, 0, Some(0xC000_0008)), (2, Stop::StopPc));
        assert_eq!(m.reg(1), 2);
    }

    #[test]
    fn call_returns_r0_and_preserving_restores_registers() {
        // add r0,r0,r1 ; bx lr
        let code = [0xE080_0001, 0xE12F_FF1E];
        let mut m = machine_with(&code);
        assert_eq!(m.call(0xC000_0000, &[40, 2], 100), (42, Stop::Until));
        m.cpu.r[5] = 77;
        m.cpu.r[13] = 0x7000_8000;
        m.cpu.r[15] = 0xC000_1000;
        let (ret, _) = m.call_preserving(0xC000_0000, &[1, 1], 100);
        assert_eq!(ret, 2);
        assert_eq!(
            (m.reg(5), m.reg(13), m.reg(15)),
            (77, 0x7000_8000, 0xC000_1000)
        );
    }

    #[test]
    fn device_store_records_the_instruction_pc_with_and_without_the_cache() {
        // pure instructions first, so a per-block pc would differ from the store's own pc
        let code = [
            0xE3A0_0012, // mov r0,#0x12
            0xE3A0_2001, // mov r2,#1
            0xE282_2001, // add r2,r2,#1
            0xE588_0030, // str r0,[r8,#0x30]   (first word of comm 3, path 1)
            0xE282_2001, // add r2,r2,#1
            0xE282_2001, // add r2,r2,#1
            0xE5C8_0031, // strb r0,[r8,#0x31]  (second word completes the message)
            0xEAFF_FFFE, // b .
        ];
        let run = |predecode: bool| {
            let mut m = machine_with(&code);
            m.predecode = predecode;
            m.cpu.r[8] = sysbus::PORT1_BASE;
            m.run(7, 0, None);
            m.bus.messages.clone()
        };
        let (plain, cached) = (run(false), run(true));
        assert_eq!(plain.len(), 1);
        assert_eq!(plain, cached);
        assert_eq!(cached[0].pc, 0xC000_0018, "pc of the storing instruction");
    }

    #[test]
    fn undefined_instruction_faults_with_pc() {
        let mut m = machine_with(&[0xE7F0_00F0]);
        let (_, stop) = m.run(5, 0, None);
        assert!(matches!(
            stop,
            Stop::Fault(Fault::Undefined {
                pc: 0xC000_0000,
                ..
            })
        ));
    }
}
