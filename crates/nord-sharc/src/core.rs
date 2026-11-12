//! Execution core of the SHARC oracle (milestone M2 of `docs/specs/sharc-core.md`).
//!
//! Runs VISA code straight from the raw parcels of a flat SRAM image: no decoded-instruction
//! objects, no strings and no allocation in [`Core::step`], so it is fast enough to run whole DSP
//! start-up sequences and single routines in tests. Anything not implemented yet stops the run
//! with a [`Fault`] naming the program counter and the raw parcels, never with wrong results.
//!
//! Provenance of the modelled semantics: instruction layouts are the ones verified in
//! `visa.rs`; behaviour follows the SHARC Processor Programming Reference rev 2.4. Float results
//! use IEEE single precision with round to nearest (ASSUMED: the hardware default 40-bit extended
//! precision is not modelled yet, see the spec's risk list).

use crate::visa::{bits, classify, ureg_load, word48, Insn, Kind};
use std::collections::HashMap;

/// Bytes of the flat SRAM image: index = `address & 0x7FFFFF` (loader address), byte `2 * short`.
pub const SRAM_BYTES: usize = 0x40_0000;
/// 32-bit words of the IOP register window (`0x0000_0000..0x0003_FFFF` normal-word addresses).
const IOP_WORDS: usize = 0x4_0000;

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
    pub tf: bool,
}

/// Loop stack entry of `DO ... UNTIL LCE`.
#[derive(Debug, Clone, Copy, Default)]
struct LoopFrame {
    start: u32,
    end: u32,
    remaining: u32,
}

/// Architectural state of one SHARC core (processing element X; PEy is not modelled yet).
pub struct Core {
    /// Program counter, a short-word address.
    pub pc: u32,
    pub r: [u32; 16],
    pub i: [u32; 16],
    pub m: [u32; 16],
    pub l: [u32; 16],
    pub b: [u32; 16],
    pub mode1: u32,
    pub mode2: u32,
    pub ustat: [u32; 4],
    pub imask: u32,
    pub mmask: u32,
    pub flags: Flags,
    /// Number of instructions executed.
    pub steps: u64,
    sram: Box<[u8]>,
    iop: Box<[u32]>,
    /// Sparse store for addresses outside the SRAM and IOP windows (peripherals, external bus).
    device: HashMap<u32, u32>,
    /// Values returned by device reads regardless of earlier writes (scripted status registers).
    preset: HashMap<u32, u32>,
    /// Reads and writes that went to the device store (diagnostic counters).
    pub device_reads: u64,
    pub device_writes: u64,
    call_stack: Vec<u32>,
    loops: Vec<LoopFrame>,
}

#[inline]
fn f32_of(x: u32) -> f32 {
    f32::from_bits(x)
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
            i: [0; 16],
            m: [0; 16],
            l: [0; 16],
            b: [0; 16],
            mode1: 0,
            mode2: 0,
            ustat: [0; 4],
            imask: 0,
            mmask: 0,
            flags: Flags::default(),
            steps: 0,
            sram: mem,
            iop: vec![0u32; IOP_WORDS].into_boxed_slice(),
            device: HashMap::new(),
            preset: HashMap::new(),
            device_reads: 0,
            device_writes: 0,
            call_stack: Vec::with_capacity(64),
            loops: Vec::with_capacity(8),
        }
    }

    #[inline]
    fn parcel(&self, short: u32) -> u16 {
        let o = (short as usize) * 2;
        u16::from_le_bytes([
            self.sram[o & (SRAM_BYTES - 1)],
            self.sram[(o + 1) & (SRAM_BYTES - 1)],
        ])
    }

    /// Reads a 32-bit data word at a normal-word address.
    ///
    /// Internal SRAM and the IOP window are modelled; every other address reads from a sparse
    /// device store (ASSUMED: peripherals and the external bus behave like plain memory until
    /// they are modelled, `device_reads` counts these accesses).
    pub fn dm_read(&mut self, addr: u32) -> Result<u32, u32> {
        if (addr as usize) < IOP_WORDS {
            return Ok(self.iop[addr as usize]);
        }
        let o = (addr as usize).wrapping_mul(4);
        if (0x9_0000..0x10_0000).contains(&addr) && o + 4 <= SRAM_BYTES {
            return Ok(u32::from_le_bytes([
                self.sram[o],
                self.sram[o + 1],
                self.sram[o + 2],
                self.sram[o + 3],
            ]));
        }
        self.device_reads += 1;
        if let Some(v) = self.preset.get(&addr) {
            return Ok(*v);
        }
        Ok(self.device.get(&addr).copied().unwrap_or(0))
    }

    /// Writes a 32-bit data word at a normal-word address (see [`Core::dm_read`]).
    pub fn dm_write(&mut self, addr: u32, value: u32) -> Result<(), u32> {
        if (addr as usize) < IOP_WORDS {
            self.iop[addr as usize] = value;
            return Ok(());
        }
        let o = (addr as usize).wrapping_mul(4);
        if (0x9_0000..0x10_0000).contains(&addr) && o + 4 <= SRAM_BYTES {
            self.sram[o..o + 4].copy_from_slice(&value.to_le_bytes());
            return Ok(());
        }
        self.device_writes += 1;
        self.device.insert(addr, value);
        Ok(())
    }

    /// Scripts a device read: later reads of `addr` return `value` whatever was written there.
    pub fn preset_device(&mut self, addr: u32, value: u32) {
        self.preset.insert(addr, value);
    }

    /// Value of a 7-bit universal register code (Table 10-2, PEx view).
    pub fn read_ureg(&self, code: u8) -> u32 {
        let n = (code & 15) as usize;
        match (code >> 4) & 7 {
            0 => self.r[n],
            1 => self.i[n],
            2 => self.m[n],
            3 => self.l[n],
            4 => self.b[n],
            6 => match n {
                3 => self.pc,
                7 => self.loops.last().map_or(0, |f| f.remaining),
                8 => self.loops.last().map_or(0, |f| f.remaining),
                _ => 0,
            },
            7 => match n {
                0 | 1 => self.ustat[n],
                2 => self.mode1,
                3 => self.mmask,
                4 => self.mode2,
                11 => self.imask,
                14 | 15 => self.ustat[n - 12],
                _ => 0,
            },
            _ => 0,
        }
    }

    /// Writes a universal register (system registers not listed are accepted and ignored).
    pub fn write_ureg(&mut self, code: u8, v: u32) {
        let n = (code & 15) as usize;
        match (code >> 4) & 7 {
            0 => self.r[n] = v,
            1 => self.i[n] = v,
            2 => self.m[n] = v,
            3 => self.l[n] = v,
            4 => self.b[n] = v,
            7 => match n {
                0 | 1 => self.ustat[n] = v,
                2 => self.mode1 = v,
                3 => self.mmask = v,
                4 => self.mode2 = v,
                11 => self.imask = v,
                14 | 15 => self.ustat[n - 12] = v,
                _ => {}
            },
            _ => {}
        }
    }

    /// Whether condition code `c` (0..32, `31` = TRUE) holds.
    fn cond(&self, c: u64) -> bool {
        let f = self.flags;
        let lce = self.loops.last().is_some_and(|l| l.remaining > 1);
        let base = match c & 15 {
            0 => f.az,
            1 => f.an && !f.az,
            2 => f.an || f.az,
            3 => f.ac,
            4 => f.av,
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

    /// Next DAG index after modifying `i` by `m` with circular-buffer length `l` and base `b`.
    #[inline]
    fn dag_next(i: u32, m: u32, l: u32, b: u32) -> u32 {
        let next = i.wrapping_add(m);
        if l == 0 {
            return next;
        }
        if (m as i32) >= 0 {
            if next >= b.wrapping_add(l) {
                next.wrapping_sub(l)
            } else {
                next
            }
        } else if next < b {
            next.wrapping_add(l)
        } else {
            next
        }
    }

    fn set_arith_flags(&mut self, result: u32, float: bool) {
        self.flags.az = if float {
            f32_of(result) == 0.0
        } else {
            result == 0
        };
        self.flags.an = if float {
            f32_of(result) < 0.0
        } else {
            (result as i32) < 0
        };
    }

    /// Executes a 23-bit compute field. Returns false when the operation is not modelled.
    fn compute(&mut self, c: u64) -> bool {
        if c == 0 {
            return true;
        }
        if bits(c, 22, 22) == 1 {
            return false;
        }
        let cu = bits(c, 22, 20);
        let op = bits(c, 19, 12);
        let (n, x, y) = (
            bits(c, 11, 8) as usize,
            bits(c, 7, 4) as usize,
            bits(c, 3, 0) as usize,
        );
        let (rx, ry) = (self.r[x], self.r[y]);
        let (fx, fy) = (f32_of(rx), f32_of(ry));
        match (cu, op) {
            (0, 0x01) => {
                let (r, carry) = rx.overflowing_add(ry);
                self.flags.ac = carry;
                self.flags.av = (rx as i32).checked_add(ry as i32).is_none();
                self.r[n] = r;
                self.set_arith_flags(r, false);
            }
            (0, 0x02) => {
                let r = rx.wrapping_sub(ry);
                self.flags.av = (rx as i32).checked_sub(ry as i32).is_none();
                self.r[n] = r;
                self.set_arith_flags(r, false);
            }
            (0, 0x21) => {
                self.r[n] = rx;
                self.set_arith_flags(rx, false);
            }
            (0, 0x22) => {
                let r = rx.wrapping_neg();
                self.r[n] = r;
                self.set_arith_flags(r, false);
            }
            (0, 0x30) => {
                let r = (rx as i32).wrapping_abs() as u32;
                self.r[n] = r;
                self.set_arith_flags(r, false);
            }
            (0, 0x40) => {
                self.r[n] = rx & ry;
                self.set_arith_flags(self.r[n], false);
            }
            (0, 0x41) => {
                self.r[n] = rx | ry;
                self.set_arith_flags(self.r[n], false);
            }
            (0, 0x42) => {
                self.r[n] = rx ^ ry;
                self.set_arith_flags(self.r[n], false);
            }
            (0, 0x43) => {
                self.r[n] = !rx;
                self.set_arith_flags(self.r[n], false);
            }
            (0, 0x05) => {
                let r = rx.wrapping_add(ry).wrapping_add(u32::from(self.flags.ac));
                self.flags.ac =
                    (u64::from(rx) + u64::from(ry) + u64::from(self.flags.ac)) > 0xFFFF_FFFF;
                self.r[n] = r;
                self.set_arith_flags(r, false);
            }
            (0, 0x06) => {
                let r = rx
                    .wrapping_sub(ry)
                    .wrapping_add(u32::from(self.flags.ac))
                    .wrapping_sub(1);
                self.r[n] = r;
                self.set_arith_flags(r, false);
            }
            (0, 0x09) => {
                let r = ((i64::from(rx as i32) + i64::from(ry as i32)) >> 1) as u32;
                self.r[n] = r;
                self.set_arith_flags(r, false);
            }
            (0, 0x0A) => {
                self.flags.az = rx == ry;
                self.flags.an = (rx as i32) < (ry as i32);
            }
            (0, 0x0B) => {
                self.flags.az = rx == ry;
                self.flags.an = rx < ry;
            }
            (0, 0x25) => {
                self.r[n] = rx.wrapping_add(u32::from(self.flags.ac));
                self.set_arith_flags(self.r[n], false);
            }
            (0, 0x26) => {
                self.r[n] = rx.wrapping_add(u32::from(self.flags.ac)).wrapping_sub(1);
                self.set_arith_flags(self.r[n], false);
            }
            (0, 0x29) => {
                self.r[n] = rx.wrapping_add(1);
                self.set_arith_flags(self.r[n], false);
            }
            (0, 0x2A) => {
                self.r[n] = rx.wrapping_sub(1);
                self.set_arith_flags(self.r[n], false);
            }
            (0, 0x61) => {
                self.r[n] = (rx as i32).min(ry as i32) as u32;
                self.set_arith_flags(self.r[n], false);
            }
            (0, 0x62) => {
                self.r[n] = (rx as i32).max(ry as i32) as u32;
                self.set_arith_flags(self.r[n], false);
            }
            (0, 0x63) => {
                let lim = (ry as i32).wrapping_abs();
                self.r[n] = (rx as i32).clamp(lim.wrapping_neg(), lim) as u32;
                self.set_arith_flags(self.r[n], false);
            }
            (0, 0x91) => self.r[n] = self.float_result((fx + fy).abs()),
            (0, 0x92) => self.r[n] = self.float_result((fx - fy).abs()),
            (0, 0x8A) => {
                self.flags.az = fx == fy;
                self.flags.an = fx < fy;
            }
            (0, 0xA5) => self.r[n] = self.float_result(fx),
            (0, 0xBD) => self.r[n] = self.float_result(fx * 2f32.powi(ry as i32)),
            (0, 0xC1) => {
                self.r[n] = if fx == 0.0 || !fx.is_finite() {
                    return false;
                } else {
                    (((rx >> 23) & 0xFF) as i32 - 127) as u32
                };
                self.set_arith_flags(self.r[n], false);
            }
            (0, 0xD9) => {
                self.r[n] = (fx * 2f32.powi(ry as i32)).round_ties_even() as i32 as u32;
                self.set_arith_flags(self.r[n], false);
            }
            (0, 0xCD) => {
                self.r[n] = fx.trunc() as i32 as u32;
                self.set_arith_flags(self.r[n], false);
            }
            (0, 0xDD) => {
                self.r[n] = (fx * 2f32.powi(ry as i32)).trunc() as i32 as u32;
                self.set_arith_flags(self.r[n], false);
            }
            (0, 0xDA) => self.r[n] = self.float_result(rx as i32 as f32 * 2f32.powi(ry as i32)),
            (0, 0x81) => self.r[n] = self.float_result(fx + fy),
            (0, 0x82) => self.r[n] = self.float_result(fx - fy),
            (0, 0xA1) => self.r[n] = self.float_result(fx),
            (0, 0xA2) => self.r[n] = self.float_result(-fx),
            (0, 0xB0) => self.r[n] = self.float_result(fx.abs()),
            (0, 0x89) => self.r[n] = self.float_result((fx + fy) / 2.0),
            (0, 0xE0) => self.r[n] = self.float_result(fx.copysign(fy)),
            (0, 0xE1) => self.r[n] = self.float_result(fx.min(fy)),
            (0, 0xE2) => self.r[n] = self.float_result(fx.max(fy)),
            (0, 0xE3) => self.r[n] = self.float_result(fx.clamp(-fy.abs(), fy.abs())),
            (0, 0xCA) => self.r[n] = self.float_result(rx as i32 as f32),
            (0, 0xC9) => {
                self.r[n] = (fx.round_ties_even() as i32) as u32;
                self.set_arith_flags(self.r[n], false);
            }
            (1, 0x30) => self.r[n] = self.float_result(fx * fy),
            _ => return false,
        }
        true
    }

    #[inline]
    fn float_result(&mut self, v: f32) -> u32 {
        let bits = v.to_bits();
        self.set_arith_flags(bits, true);
        bits
    }

    /// Short (16-bit) compute of Type 2c: OP bits 11..8, Rn 7..4, Rx 3..0.
    fn short_compute(&mut self, c: u64) -> bool {
        let n = bits(c, 7, 4) as usize;
        let x = bits(c, 3, 0) as usize;
        let (fn_, fx) = (f32_of(self.r[n]), f32_of(self.r[x]));
        match bits(c, 11, 8) {
            0 => self.r[n] = self.r[n].wrapping_add(self.r[x]),
            1 => self.r[n] = self.r[n].wrapping_sub(self.r[x]),
            2 => self.r[n] = self.r[x],
            3 => {
                self.flags.az = self.r[n] == self.r[x];
                self.flags.an = (self.r[n] as i32) < (self.r[x] as i32);
            }
            4 => self.r[n] = !self.r[x],
            5 => self.r[n] = self.r[x].wrapping_add(1),
            6 => self.r[n] = self.r[x].wrapping_sub(1),
            11 => {
                self.flags.az = fn_ == fx;
                self.flags.an = fn_ < fx;
            }
            12 => self.r[n] &= self.r[x],
            13 => self.r[n] |= self.r[x],
            14 => self.r[n] ^= self.r[x],

            8 => self.r[n] = self.float_result(fn_ + fx),
            9 => self.r[n] = self.float_result(fn_ - fx),
            10 => self.r[n] = self.float_result(self.r[x] as i32 as f32),
            15 => self.r[n] = self.float_result(fn_ * fx),
            _ => return false,
        }
        true
    }

    fn fault(&self, pc: u32, insn: &Insn) -> Fault {
        Fault::Unimplemented {
            pc,
            parcels: insn.parcels,
        }
    }

    /// Executes one instruction. Returns `Ok(())` or the reason it could not.
    pub fn step(&mut self) -> Result<(), Fault> {
        let pc = self.pc;
        let p0 = self.parcel(pc);
        let p1 = self.parcel(pc.wrapping_add(1));
        let (kind, len) = classify(p0, Some(p1));
        let mut parcels = [0u16; 3];
        parcels[0] = p0;
        if len > 1 {
            parcels[1] = p1;
        }
        if len > 2 {
            parcels[2] = self.parcel(pc.wrapping_add(2));
        }
        let insn = Insn { kind, parcels, len };
        let w = word48(&parcels[..len]);
        let mut next = pc.wrapping_add(len as u32);
        let cond_ok = self.cond(bits(w, 37, 33));
        let bad = |address: u32| Fault::BadAddress { pc, address };

        match kind {
            Kind::T21c if p0 == 0x0001 => {}
            Kind::T21 if w == 0 => {}
            Kind::T17a | Kind::T17b => {
                let (code, imm) = ureg_load(&insn).ok_or_else(|| self.fault(pc, &insn))?;
                let value = if kind == Kind::T17b {
                    sign_extend(imm, 16)
                } else {
                    imm
                };
                self.write_ureg(code, value);
            }
            Kind::T14a | Kind::T15a | Kind::T15b => {
                let (addr, ureg, write) = match kind {
                    Kind::T14a => (
                        bits(w, 31, 0) as u32,
                        bits(w, 38, 32) as u8,
                        bits(w, 40, 40) == 1,
                    ),
                    Kind::T15a => (
                        self.i
                            [(bits(w, 43, 41) + if bits(w, 44, 44) == 1 { 8 } else { 0 }) as usize]
                            .wrapping_add(bits(w, 31, 0) as u32),
                        bits(w, 38, 32) as u8,
                        bits(w, 40, 40) == 1,
                    ),
                    _ => (
                        self.i[bits(w, 43, 41) as usize].wrapping_add(bits(w, 29, 23) as u32),
                        bits(w, 22, 16) as u8,
                        bits(w, 40, 40) == 1,
                    ),
                };
                if write {
                    let v = self.read_ureg(ureg);
                    self.dm_write(addr, v).map_err(bad)?;
                } else {
                    let v = self.dm_read(addr).map_err(bad)?;
                    self.write_ureg(ureg, v);
                }
            }
            Kind::T16a => {
                // DM/PM(Ii, Mm) = <data32>, post-modify with update
                let pm = bits(w, 37, 37) == 1;
                let base = if pm { 8 } else { 0 };
                let ii = (bits(w, 43, 41) + base) as usize;
                let mi = (bits(w, 40, 38) + base) as usize;
                let addr = self.i[ii];
                self.dm_write(addr, bits(w, 31, 0) as u32).map_err(bad)?;
                self.i[ii] = Self::dag_next(addr, self.m[mi], self.l[ii], self.b[ii]);
            }
            Kind::T18a => {
                let sreg = (0x70 | bits(w, 35, 32)) as u8;
                let data = bits(w, 31, 0) as u32;
                let cur = self.read_ureg(sreg);
                match bits(w, 39, 37) {
                    0 => self.write_ureg(sreg, cur | data),
                    1 => self.write_ureg(sreg, cur & !data),
                    2 => self.write_ureg(sreg, cur ^ data),
                    4 => self.flags.tf = cur & data == data,
                    5 => self.flags.tf = cur == data,
                    _ => return Err(self.fault(pc, &insn)),
                }
            }
            Kind::T5a | Kind::T5b if bits(w, 43, 43) == 0 => {
                if cond_ok {
                    let src = ((bits(w, 42, 38) << 2) | bits(w, 32, 31)) as u8;
                    let v = self.read_ureg(src);
                    self.write_ureg(bits(w, 30, 24) as u8, v);
                    if kind == Kind::T5a && !self.compute(bits(w, 22, 0)) {
                        return Err(self.fault(pc, &insn));
                    }
                }
            }
            Kind::T3a | Kind::T3b => {
                if cond_ok {
                    let pm = bits(w, 32, 32) == 1;
                    let base = if pm { 8 } else { 0 };
                    let ii = (bits(w, 43, 41) + base) as usize;
                    let mi = (bits(w, 40, 38) + base) as usize;
                    let (i, m, l, b) = (self.i[ii], self.m[mi], self.l[ii], self.b[ii]);
                    let post = bits(w, 44, 44) == 1;
                    let (addr, update) = if post {
                        (i, Some(Self::dag_next(i, m, l, b)))
                    } else {
                        (Self::dag_next(i, m, l, b), None)
                    };
                    let ureg = bits(w, 29, 23) as u8;
                    if kind == Kind::T3a && !self.compute(bits(w, 22, 0)) {
                        return Err(self.fault(pc, &insn));
                    }
                    if bits(w, 31, 31) == 1 {
                        let v = self.read_ureg(ureg);
                        self.dm_write(addr, v).map_err(bad)?;
                    } else {
                        let v = self.dm_read(addr).map_err(bad)?;
                        self.write_ureg(ureg, v);
                    }
                    if let Some(u) = update {
                        self.i[ii] = u;
                    }
                }
            }
            Kind::T4a | Kind::T4b => {
                if cond_ok {
                    let pm = bits(w, 40, 40) == 1;
                    let ii = (bits(w, 43, 41) + if pm { 8 } else { 0 }) as usize;
                    let addr = self.i[ii].wrapping_add(bits(w, 32, 27) as u32);
                    let reg = bits(w, 26, 23) as usize;
                    if kind == Kind::T4a && !self.compute(bits(w, 22, 0)) {
                        return Err(self.fault(pc, &insn));
                    }
                    if bits(w, 39, 39) == 1 {
                        self.dm_write(addr, self.r[reg]).map_err(bad)?;
                    } else {
                        self.r[reg] = self.dm_read(addr).map_err(bad)?;
                    }
                }
            }
            Kind::T2a | Kind::T2b | Kind::T2x => {
                let c = if kind == Kind::T2b {
                    bits(w, 38, 16)
                } else {
                    bits(w, 22, 0)
                };
                if (kind == Kind::T2b || cond_ok) && !self.compute(c) {
                    return Err(self.fault(pc, &insn));
                }
            }
            Kind::T2c => {
                if !self.short_compute(bits(w, 43, 32)) {
                    return Err(self.fault(pc, &insn));
                }
            }
            Kind::T8a => {
                let raw = bits(w, 23, 0) as u32;
                let delayed = bits(w, 26, 26) == 1;
                if delayed {
                    return Err(self.fault(pc, &insn));
                }
                if cond_ok {
                    let target = if bits(w, 40, 40) == 1 {
                        pc.wrapping_add(sign_extend(raw, 24))
                    } else {
                        raw
                    };
                    if bits(w, 39, 39) == 1 {
                        self.call_stack.push(next);
                    }
                    next = target;
                }
            }
            Kind::T11a | Kind::T11c => {
                if cond_ok {
                    match self.call_stack.pop() {
                        Some(ret) => next = ret,
                        None => return Err(self.fault(pc, &insn)),
                    }
                }
            }
            Kind::T12a => {
                let raw = bits(w, 23, 0) as u32;
                let end = if raw & 0x80_0000 != 0 {
                    pc.wrapping_add(sign_extend(raw & 0x7F_FFFF, 23))
                } else {
                    raw
                };
                let count = bits(w, 39, 24) as u32;
                if count == 0 {
                    return Err(self.fault(pc, &insn));
                }
                self.loops.push(LoopFrame {
                    start: next,
                    end,
                    remaining: count,
                });
            }
            _ => return Err(self.fault(pc, &insn)),
        }

        // end-of-loop handling: after the last instruction of the innermost loop body
        if let Some(frame) = self.loops.last_mut() {
            if pc == frame.end && kind != Kind::T12a {
                if frame.remaining > 1 {
                    frame.remaining -= 1;
                    next = frame.start;
                } else {
                    self.loops.pop();
                }
            }
        }
        self.pc = next;
        self.steps += 1;
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
    use super::*;

    /// Writes parcels (little-endian, most significant parcel first) at a short-word address.
    fn load(core: &mut Core, short: u32, parcels: &[u16]) {
        for (k, p) in parcels.iter().enumerate() {
            let o = (short as usize + k) * 2;
            core.sram[o..o + 2].copy_from_slice(&p.to_le_bytes());
        }
    }

    fn core_with(program: &[u16]) -> Core {
        let mut c = Core::new(&[]);
        load(&mut c, 0x12_0000, program);
        c.pc = 0x12_0000;
        c
    }

    #[test]
    fn immediates_registers_and_bit_ops() {
        // I0 = 0xc0430; M1 = 1; BIT SET MODE1 0x4f8; BIT CLR MODE1 0x8; NOP
        let mut c = core_with(&[
            0x0F10, 0x000C, 0x0430, 0x0FA1, 0x0001, 0x1402, 0x0000, 0x04F8, 0x1422, 0x0000, 0x0008,
            0x0001,
        ]);
        assert_eq!(c.run(5), None);
        assert_eq!(c.i[0], 0xC_0430);
        assert_eq!(c.m[1], 1);
        assert_eq!(c.mode1, 0x4F0);
        assert_eq!(c.steps, 5);
    }

    #[test]
    fn do_loop_zeroes_memory_like_the_firmware_start_up() {
        // R0 = 0; I0 = 0xc0430; DO (+3) UNTIL LCE, LCNTR = 4; DM(I0, M1) = R0; then NOP
        let mut c = core_with(&[
            0x0F80, 0x0000, 0x0F10, 0x000C, 0x0430, 0x0C00, 0x0480, 0x0003, 0x507E, 0x8000, 0x0000,
            0x0001,
        ]);
        c.m[1] = 1;
        for k in 0..6 {
            c.dm_write(0xC_0430 + k, 0xDEAD_BEEF).unwrap();
        }
        assert_eq!(c.run(2 + 1 + 4 + 1), None);
        for k in 0..4 {
            assert_eq!(c.dm_read(0xC_0430 + k).unwrap(), 0, "word {k}");
        }
        assert_eq!(c.dm_read(0xC_0434).unwrap(), 0xDEAD_BEEF);
        assert_eq!(c.i[0], 0xC_0434);
        assert!(c.loops.is_empty());
    }

    #[test]
    fn call_and_return() {
        // CALL 0x120008; NOP ... at 0x120008: R1 = 7 (17b); RTS
        let mut c = core_with(&[0x06BE, 0x0012, 0x0008, 0x0001]);
        load(&mut c, 0x12_0008, &[0x0F81, 0x0007, 0x0A3E, 0x0000, 0x0000]);
        assert_eq!(c.run(3), None);
        assert_eq!(c.r[1], 7);
        assert_eq!(c.pc, 0x12_0003);
    }

    #[test]
    fn float_compute_and_flags() {
        let mut c = core_with(&[0x0001]);
        c.r[1] = 1.5f32.to_bits();
        c.r[2] = 2.0f32.to_bits();
        // F3 = F1 * F2 via the Type 2b compute 0x130 + regs; use direct compute() calls
        assert!(c.compute((1 << 20) | (0x30 << 12) | (3 << 8) | (1 << 4) | 2));
        assert_eq!(f32::from_bits(c.r[3]), 3.0);
        assert!(c.compute((0x81 << 12) | (4 << 8) | (1 << 4) | 2));
        assert_eq!(f32::from_bits(c.r[4]), 3.5);
        assert!(c.compute((0x82 << 12) | (5 << 8) | (2 << 4) | 2));
        assert!(c.flags.az && !c.flags.an);
        // fixed-point and compare operations
        c.r[1] = 7;
        c.r[2] = (-3i32) as u32;
        assert!(c.compute((0x62 << 12) | (6 << 8) | (1 << 4) | 2));
        assert_eq!(c.r[6], 7);
        assert!(c.compute((0x61 << 12) | (6 << 8) | (1 << 4) | 2));
        assert_eq!(c.r[6] as i32, -3);
        assert!(c.compute((0x0A << 12) | (1 << 4) | 2));
        assert!(!c.flags.az && !c.flags.an);
        assert!(c.compute((0x63 << 12) | (6 << 8) | (1 << 4) | 2));
        assert_eq!(c.r[6], 3, "CLIP 7 by 3 saturates at the magnitude of Ry");
        assert!(c.compute((0xBD << 12) | (7 << 8) | (3 << 4) | 1)); // SCALB F3 by R1 (7)
        assert_eq!(f32::from_bits(c.r[7]), 3.0 * 128.0);
        assert!(!c.compute(0x40_0000), "multifunction is not modelled yet");
        assert!(
            !c.compute((0xC4 << 12) | (1 << 8)),
            "RECIPS seed table is unknown"
        );
    }

    #[test]
    fn dag_addressing_modes_and_circular_buffers() {
        assert_eq!(Core::dag_next(10, 1, 0, 0), 11);
        assert_eq!(Core::dag_next(10, 0xFFFF_FFFF, 0, 0), 9);
        // circular buffer base 100 length 4: 103 + 1 wraps to 100, 100 - 1 wraps to 103
        assert_eq!(Core::dag_next(103, 1, 4, 100), 100);
        assert_eq!(Core::dag_next(100, 0xFFFF_FFFF, 4, 100), 103);
    }

    #[test]
    fn unimplemented_instruction_is_reported_not_guessed() {
        let mut c = core_with(&[0x8000, 0x0000, 0x0000]);
        assert!(matches!(
            c.step(),
            Err(Fault::Unimplemented { pc: 0x12_0000, .. })
        ));
        assert_eq!(
            c.pc, 0x12_0000,
            "a faulting instruction does not advance the pc"
        );
        // access to unmapped memory
        assert_eq!(c.dm_read(0x70_0000), Ok(0));
        c.dm_write(0x3108_D004, 7).unwrap();
        assert_eq!(c.dm_read(0x3108_D004), Ok(7));
        assert_eq!((c.device_reads, c.device_writes), (2, 1));
        c.preset_device(0x3108_D008, 5);
        c.dm_write(0x3108_D008, 9).unwrap();
        assert_eq!(c.dm_read(0x3108_D008), Ok(5));
    }

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
}
