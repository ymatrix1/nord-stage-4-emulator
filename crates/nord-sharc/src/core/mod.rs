//! Execution core of the SHARC oracle (milestone M2 of `docs/specs/sharc-core.md`).
//!
//! Runs VISA code straight from the raw parcels of a flat SRAM image: no decoded-instruction
//! objects, no strings and no allocation in [`Core::step`], so it is fast enough to run whole DSP
//! start-up sequences and single routines in tests. Anything not implemented yet stops the run
//! with a [`Fault`] naming the program counter and the raw parcels, never with wrong results.
//!
//! Provenance of the modelled semantics: instruction layouts are the ones verified in
//! `visa.rs`; behaviour follows the SHARC Processor Programming Reference rev 2.4. Float results
//! use the 40-bit extended format with the MODE1 TRUNC / RND32 rounding modes (`float.rs`).
//!
//! Layout: this file holds the state and the small shared helpers; `memory` the data memory and
//! register files, `dag` address generation, `alu` and `multiplier` the compute units, `moves`
//! the data and register transfers, `branch` jumps, calls, loops and delay slots, and `exec` the
//! single-step decoder that dispatches to them.

mod alu;
mod bitfifo;
mod branch;
mod dag;
mod dualmove;
mod exec;
mod float;
mod float_conv;
mod float_misc;
mod float_ops;
mod float_seeds;
mod memory;
mod moves;
mod multifunction;
mod multiplier;
mod shifter;
#[cfg(test)]
mod support;

use crate::visa::Insn;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// Bytes of the flat SRAM image: index = `address & 0x7FFFFF` (loader address), byte `2 * short`.
pub const SRAM_BYTES: usize = 0x40_0000;
/// 32-bit words of the IOP register window (`0x0000_0000..0x0003_FFFF` normal-word addresses).
const IOP_WORDS: usize = 0x4_0000;
/// Most skipped operations whose pc is logged in lenient mode.
pub const SKIP_LOG_LIMIT: usize = 100_000;
/// MODE1 bit 21: processing element Y enabled (SIMD mode).
const MODE1_PEYEN: u32 = 1 << 21;
/// Most writes kept by the data watch.
pub const WATCH_LIMIT: usize = 64;

/// Hasher for the device store: addresses are `u32` keys that are not attacker controlled, so a
/// multiplicative mix is enough and much cheaper than the default SipHash on the poll loops.
#[derive(Default)]
struct AddrHasher(u64);

impl Hasher for AddrHasher {
    fn finish(&self) -> u64 {
        self.0 ^ (self.0 >> 32)
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        }
    }

    fn write_u32(&mut self, x: u32) {
        self.0 = u64::from(x).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
}

/// Sparse word store keyed by address.
type DeviceMap = HashMap<u32, u32, BuildHasherDefault<AddrHasher>>;

/// Why a run stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// The instruction at `pc` is not modelled yet (`parcels` holds its raw words).
    Unimplemented { pc: u32, parcels: [u16; 3] },
    /// A data or instruction access outside the modelled memory.
    BadAddress { pc: u32, address: u32 },
}

/// Condition flags used by `IF cond` (ASTATx subset plus loop counter expiry).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Flags {
    pub az: bool,
    pub an: bool,
    pub av: bool,
    pub ac: bool,
    /// Float invalid-operation flag (AI) and the sign flag of ABS/MANT (AS).
    pub ai: bool,
    pub as_: bool,
    pub tf: bool,
    /// Shifter flags: zero result, overflow and sign.
    pub sz: bool,
    pub sv: bool,
    pub ss: bool,
    /// Shifter bit FIFO of this processing element (`SF` is derived from its write pointer).
    pub fifo: bitfifo::BitFifo,
}

/// Loop stack entry of `DO ... UNTIL LCE`.
#[derive(Debug, Clone, Copy, Default)]
struct LoopFrame {
    start: u32,
    end: u32,
    remaining: u32,
}

/// A delayed branch waiting for its two delay-slot instructions to finish.
#[derive(Debug, Clone, Copy)]
struct Pending {
    /// Steps left including the one that issued the branch (3 = just issued).
    slots: u8,
    target: u32,
    call: bool,
}

/// Architectural state of one SHARC core (processing element X; PEy is not modelled yet).
pub struct Core {
    /// Program counter, a short-word address.
    pub pc: u32,
    pub r: [u32; 16],
    /// PEy data registers (S0-S15), reached only through the `Dreg <-> CDreg` swap for now.
    pub s: [u32; 16],
    pub i: [u32; 16],
    pub m: [u32; 16],
    pub l: [u32; 16],
    pub b: [u32; 16],
    pub mode1: u32,
    pub mode2: u32,
    pub ustat: [u32; 4],
    /// Multiplier result registers MRF (index 0) and MRB (index 1), 80 bits sign-extended.
    pub mr: [i128; 2],
    /// When set, compute operations that are not modelled are skipped and counted instead of faulting.
    pub lenient: bool,
    /// Number of skipped unmodelled compute operations and the pc of the first one.
    pub skipped: u64,
    pub first_skipped: Option<u32>,
    /// Program counters of skipped operations (first [`SKIP_LOG_LIMIT`]), for histograms.
    pub skipped_pcs: Vec<u32>,
    /// Data address whose writes are logged in `watch_hits` (`u32::MAX` = none).
    pub watch: u32,
    /// Writes to the watched address as `(step, pc, value)`, at most [`WATCH_LIMIT`].
    pub watch_hits: Vec<(u64, u32, u32)>,
    pub imask: u32,
    pub mmask: u32,
    pub flags: Flags,
    /// PEy condition flags and multiplier result registers (used in SIMD mode).
    pub flags_y: Flags,
    pub mr_y: [i128; 2],
    /// Low 8 bits of the 40-bit float registers of PEx and PEy (see `float_ops`).
    fx: float_ops::ExtFile,
    fx_y: float_ops::ExtFile,
    /// Number of instructions executed.
    pub steps: u64,
    sram: Box<[u8]>,
    iop: Box<[u32]>,
    /// Sparse store for addresses outside the SRAM and IOP windows (peripherals, external bus).
    device: DeviceMap,
    /// Values returned by device reads regardless of earlier writes (scripted status registers).
    preset: DeviceMap,
    /// Reads and writes that went to the device store (diagnostic counters).
    pub device_reads: u64,
    pub device_writes: u64,
    call_stack: Vec<u32>,
    loops: Vec<LoopFrame>,
    pending: Option<Pending>,
}

#[inline]
fn sign_extend(value: u32, width: u32) -> u32 {
    let shift = 32 - width;
    (((value << shift) as i32) >> shift) as u32
}

impl Core {
    /// A core with `sram` as its internal memory (padded or cut to [`SRAM_BYTES`]).
    pub fn new(sram: &[u8]) -> Self {
        let mut mem = vec![0u8; SRAM_BYTES].into_boxed_slice();
        let n = sram.len().min(SRAM_BYTES);
        mem[..n].copy_from_slice(&sram[..n]);
        Core {
            pc: 0,
            r: [0; 16],
            s: [0; 16],
            i: [0; 16],
            m: [0; 16],
            l: [0; 16],
            b: [0; 16],
            mode1: 0,
            mode2: 0,
            ustat: [0; 4],
            mr: [0; 2],
            lenient: false,
            skipped: 0,
            first_skipped: None,
            skipped_pcs: Vec::new(),
            watch: u32::MAX,
            watch_hits: Vec::new(),
            imask: 0,
            mmask: 0,
            flags: Flags::default(),
            flags_y: Flags::default(),
            mr_y: [0; 2],
            fx: float_ops::ExtFile::default(),
            fx_y: float_ops::ExtFile::default(),
            steps: 0,
            sram: mem,
            iop: vec![0u32; IOP_WORDS].into_boxed_slice(),
            device: DeviceMap::default(),
            preset: DeviceMap::default(),
            device_reads: 0,
            device_writes: 0,
            call_stack: Vec::with_capacity(64),
            loops: Vec::with_capacity(8),
            pending: None,
        }
    }

    /// Instruction parcel at a short-word address (for tools that decode skipped operations).
    pub fn parcel_at(&self, short: u32) -> u16 {
        self.parcel(short)
    }

    #[inline]
    fn parcel(&self, short: u32) -> u16 {
        let o = (short as usize) * 2;
        u16::from_le_bytes([
            self.sram[o & (SRAM_BYTES - 1)],
            self.sram[(o + 1) & (SRAM_BYTES - 1)],
        ])
    }

    /// Whether SIMD mode (PEy enabled) is on.
    #[inline]
    fn simd(&self) -> bool {
        self.mode1 & MODE1_PEYEN != 0
    }

    /// Exchanges the PEx and PEy views (data registers, flags, multiplier results), so the code
    /// written for PEx can run unchanged on PEy and the views are swapped back afterwards.
    fn swap_pe(&mut self) {
        std::mem::swap(&mut self.r, &mut self.s);
        std::mem::swap(&mut self.flags, &mut self.flags_y);
        std::mem::swap(&mut self.mr, &mut self.mr_y);
        std::mem::swap(&mut self.fx, &mut self.fx_y);
    }

    /// Whether condition code `c` (0..32, `31` = TRUE) holds for PEx.
    fn cond(&self, c: u64) -> bool {
        self.cond_for(self.flags, c)
    }

    /// Whether condition code `c` holds for the given flag set.
    fn cond_for(&self, f: Flags, c: u64) -> bool {
        let lce = self.loops.last().is_some_and(|l| l.remaining > 1);
        let base = match c & 15 {
            0 => f.az,
            1 => f.an && !f.az,
            2 => f.an || f.az,
            3 => f.ac,
            4 => f.av,
            7 => f.sv,
            8 => f.sz,
            // BM on ADSP-2136x/2146x; on the ADSP-214xx the same code selects SF (table 10-4)
            14 => f.fifo.sf(),
            13 => f.tf,
            15 => !lce,
            _ => false,
        };
        match c {
            0..=15 => base,
            31 => true,
            16..=30 => !base,
            _ => true,
        }
    }

    fn fault(&self, pc: u32, insn: &Insn) -> Fault {
        Fault::Unimplemented {
            pc,
            parcels: insn.parcels,
        }
    }

    /// Skips an unmodelled compute operation in lenient mode, otherwise reports it as a fault.
    fn unmodelled(&mut self, pc: u32, insn: &Insn) -> Result<(), Fault> {
        if !self.lenient {
            return Err(self.fault(pc, insn));
        }
        self.skipped += 1;
        self.first_skipped.get_or_insert(pc);
        if self.skipped_pcs.len() < SKIP_LOG_LIMIT {
            self.skipped_pcs.push(pc);
        }
        Ok(())
    }

    /// Runs until a fault or `max_steps` instructions; returns the fault, if any.
    pub fn run(&mut self, max_steps: u64) -> Option<Fault> {
        let limit = self.steps + max_steps;
        while self.steps < limit {
            if let Err(f) = self.step() {
                return Some(f);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::support::{core_at_zero, core_with};
    use super::*;

    #[test]
    fn sign_extension_and_conditions() {
        assert_eq!(sign_extend(0xFFFF, 16), 0xFFFF_FFFF);
        assert_eq!(sign_extend(0x7FFF, 16), 0x7FFF);
        let mut c = core_with(&[0x0001]);
        assert!(c.cond(31) && !c.cond(0) && c.cond(16));
        c.flags.az = true;
        assert!(c.cond(0) && !c.cond(16) && c.cond(2));
        c.flags.az = false;
        c.flags.an = true;
        assert!(c.cond(1) && c.cond(2) && !c.cond(17));
    }

    #[test]
    fn unimplemented_instruction_is_reported_not_guessed() {
        let mut c = core_with(&[0xd70a, 0x3c23, 0xd70a]);
        assert!(matches!(
            c.step(),
            Err(Fault::Unimplemented { pc: 0x12_0000, .. })
        ));
        assert_eq!(
            c.pc, 0x12_0000,
            "a faulting instruction does not advance the pc"
        );
    }

    #[test]
    fn lenient_mode_skips_unmodelled_compute_and_counts_it() {
        // type 2a word `IF TRUE` with compute 0x40_3000: an MR transfer of MR3, which does not exist
        let parcels = [0x013eu16, 0x0040, 0x3000, 0x0000];
        let mut strict = core_at_zero(&parcels);
        assert!(strict.step().is_err());
        let mut c = core_at_zero(&parcels);
        c.lenient = true;
        assert!(c.step().is_ok());
        assert_eq!((c.skipped, c.first_skipped), (1, Some(0)));
    }

    #[test]
    fn addr_hasher_spreads_neighbouring_addresses() {
        use std::hash::BuildHasher;
        let build = BuildHasherDefault::<AddrHasher>::default();
        let hashes: std::collections::HashSet<u64> = (0..1024u32)
            .map(|k| build.hash_one(0x3108_C000 + 4 * k))
            .collect();
        assert_eq!(hashes.len(), 1024);
        let mut h = AddrHasher::default();
        h.write(&[1, 2, 3]);
        assert_ne!(h.finish(), 0);
    }

    #[test]
    fn run_stops_at_the_step_limit() {
        let mut c = core_with(&[0x0001, 0x0001, 0x0001]);
        assert_eq!(c.run(2), None);
        assert_eq!(c.steps, 2);
    }
}
