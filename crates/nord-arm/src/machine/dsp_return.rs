//! DSP to ARM return channel: the three SPI read routines answered natively
//! (`tools/arm_dsp_return.py`).
//!
//! Layers, first hit wins: SY idle voice mask, the DM shadow of every word the ARM itself sent,
//! then 0. The live-chip and initial-image layers of the Python module are NOT ported: `nord-lle`
//! (`ns4-lle5`) installs this source as is, and a live read needs a synchronous call into a DSP
//! node that its tick order (ARM first, DSP after) cannot give (`docs/specs/lle-engine.md`).

use super::boot::COMM_TABLE;
use super::grouper::Message;
use super::Machine;
use std::collections::HashMap;

/// ASSUMED: entry points of the three SPI read routines, ported from `tools/arm_dsp_return.py`;
/// confirm by disassembling `bank0_CODE.bin` at these addresses (`tools/arm_xref.py`).
pub const SPI_READ4: u32 = 0xC001_19A4;
pub const SPI_READ2: u32 = 0xC001_17E8;
pub const SPI_READN: u32 = 0xC006_6E64;
/// ASSUMED: comm index of the SY chip (`PAD`/`COMM_TABLE` order of `tools/arm_emu.py`).
const SY_CHIP: u32 = 3;
/// ASSUMED: the decoded `SPI_READN` sites read 1 to 4 words; a larger count falls back.
const READN_MAX: u32 = 4;
/// ASSUMED: SY idle voice mask DM words (`tools/arm_dsp_return.py`, from the oracle run); confirm
/// by reading DM `0x92196` from a SY oracle run with all voices idle.
const IDLE_MASK: u32 = 0x92196;
/// ASSUMED: SY release mask DM words, answered 0 (same source and check as `IDLE_MASK`).
const RELEASE_MASK: u32 = 0x9219A;
/// ASSUMED: SY voice count (`tools/arm_dsp_return.py`).
const VOICES: u32 = 56;
/// ASSUMED: DM address of the SY voice-state word of voice 0 (`tools/arm_dsp_return.py`).
const STATE_BASE: u32 = 0x92414;
/// ASSUMED: DM stride between voice-state words, `0x38` (`tools/arm_dsp_return.py`).
const STATE_STRIDE: u32 = 0x38;
/// ASSUMED: voice-state words meaning idle (`tools/arm_dsp_return.py`).
const IDLE_STATES: [u32; 2] = [0x1C07B0, 0x1C0794];
/// ASSUMED: voice-state words meaning busy (`tools/arm_dsp_return.py`).
const BUSY_STATES: [u32; 2] = [0x1C07CD, 0x1C084C];

/// Return source state.
#[derive(Clone, Debug)]
pub struct DspReturn {
    pub shadow: HashMap<(u32, u32, u32), u32>,
    pub sy_idle: u64,
    pub reads: u64,
    seen: usize,
}

impl Default for DspReturn {
    fn default() -> Self {
        Self {
            shadow: HashMap::new(),
            sy_idle: (1u64 << VOICES) - 1,
            reads: 0,
            seen: 0,
        }
    }
}

/// `(core, DM word)` of a DSP address as the ARM sends it (PNO/ORG alias, bit 23 = core 2).
pub fn resolve(chip: u32, address: u32) -> (u32, u32) {
    if (0x2800_0000..0x2C00_0000).contains(&address) && (chip == 1 || chip == 2) {
        return (
            if address & 0x80_0000 != 0 { 2 } else { 1 },
            (address & 0x7F_FFFF) >> 2,
        );
    }
    (0, address)
}

impl DspReturn {
    pub fn observe(&mut self, m: &Message) {
        let chip = m.comm as u32;
        if m.path == 2 {
            let (core, dm) = resolve(chip, m.word1);
            self.shadow.insert((chip, core, dm), m.word2);
        } else {
            self.shadow.insert((chip, 0, m.word1), m.word2);
            let off = m.word1.wrapping_sub(STATE_BASE);
            if chip == SY_CHIP && off.is_multiple_of(STATE_STRIDE) && off / STATE_STRIDE < VOICES {
                let bit = 1u64 << (off / STATE_STRIDE);
                if IDLE_STATES.contains(&m.word2) {
                    self.sy_idle |= bit;
                } else if BUSY_STATES.contains(&m.word2) {
                    self.sy_idle &= !bit;
                }
            }
        }
    }

    /// Observe the messages logged since the last call (`log` is the bus message log).
    pub fn sync(&mut self, log: &[Message]) {
        for m in &log[self.seen.min(log.len())..] {
            self.observe(m);
        }
        self.seen = log.len();
    }

    /// Called when the machine drains its port log.
    pub fn reset_seen(&mut self) {
        self.seen = 0;
    }

    fn mask_word(&self, dm: u32) -> Option<u32> {
        if (IDLE_MASK..IDLE_MASK + 2).contains(&dm) {
            Some((self.sy_idle >> (32 * (dm - IDLE_MASK))) as u32)
        } else if (RELEASE_MASK..RELEASE_MASK + 2).contains(&dm) {
            Some(0)
        } else {
            None
        }
    }

    fn word(&self, chip: u32, core: u32, dm: u32) -> u32 {
        if chip == SY_CHIP {
            if let Some(w) = self.mask_word(dm) {
                return w;
            }
        }
        self.shadow.get(&(chip, core, dm)).copied().unwrap_or(0)
    }

    pub fn knows(&self, chip: u32, address: u32) -> bool {
        let (core, dm) = resolve(chip, address);
        (chip == SY_CHIP && self.mask_word(dm).is_some())
            || self.shadow.contains_key(&(chip, core, dm))
    }

    /// Fill `out` with the words at `address` (no allocation).
    pub fn read(&mut self, chip: u32, address: u32, out: &mut [u32]) {
        self.reads += 1;
        let (core, dm) = resolve(chip, address);
        for (k, w) in out.iter_mut().enumerate() {
            *w = self.word(chip, core, dm.wrapping_add(k as u32));
        }
    }
}

impl Machine {
    /// Replace the three SPI read routines by answers from [`DspReturn`].
    pub fn install_dsp_return(&mut self) {
        self.dsp_return = Some(DspReturn::default());
        for pc in [SPI_READ4, SPI_READ2, SPI_READN] {
            self.add_hook(pc, super::Hook::DspRead);
        }
    }

    /// Run the read routine at `pc` in place of the firmware code.
    pub(crate) fn dsp_read(&mut self, pc: u32) {
        let mut src = self.dsp_return.take().expect("installed");
        src.sync(&self.bus.messages);
        let r = self.cpu.r;
        let (chip, address, count, dst) = if pc == SPI_READN {
            let chip = (0..5u32)
                .find(|c| self.bus.mem.read32(COMM_TABLE + 4 + 4 * c) == r[0])
                .unwrap_or(u32::MAX);
            let dst = self.bus.mem.read32(r[13]);
            (chip, r[2], r[3], dst)
        } else {
            (r[1], r[2], if pc == SPI_READ4 { 4 } else { 2 }, r[3])
        };
        let ok = pc != SPI_READN
            || (chip < 5 && count > 0 && count <= READN_MAX && src.knows(chip, address));
        if ok {
            let mut words = [0u32; 4];
            let words = &mut words[..count.min(4) as usize];
            src.read(chip, address, words);
            for (k, w) in words.iter().enumerate() {
                self.bus.mem.write32(dst.wrapping_add(4 * k as u32), *w);
            }
        } else if pc == SPI_READN && dst != 0 {
            self.bus.mem.write32(dst, 0);
        }
        self.dsp_return = Some(src);
        self.cpu.r[0] = 0;
        self.return_to_lr();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_maps_the_alias_to_core_and_word() {
        assert_eq!(resolve(1, 0x2800_0010), (1, 4));
        assert_eq!(resolve(2, 0x2880_0010), (2, 4));
        assert_eq!(resolve(3, 0x92196), (0, 0x92196));
    }

    fn msg(path: u8, comm: u8, word1: u32, word2: u32) -> Message {
        Message {
            path,
            comm,
            word1,
            word2,
            pc: 0,
        }
    }

    #[test]
    fn shadow_and_sy_mask_follow_the_arm_messages() {
        let mut r = DspReturn::default();
        // PNO (comm 1) path 2 write, then a SY (comm 3) path 1 state message of voice 1
        let log = [
            msg(2, 1, 0x2800_0010, 0xAA55),
            msg(1, 3, STATE_BASE + STATE_STRIDE, 0x1C07CD),
        ];
        r.sync(&log);
        let mut w = [0u32; 1];
        r.read(1, 0x2800_0010, &mut w);
        assert_eq!(w, [0xAA55]);
        assert!(r.knows(1, 0x2800_0010) && !r.knows(1, 0x2800_0014));
        let mut mask = [0u32; 2];
        r.read(SY_CHIP, IDLE_MASK, &mut mask);
        assert_eq!(mask, [0xFFFF_FFFD, 0x00FF_FFFF]);
        r.sync(&log); // same log again: nothing is observed twice
        assert_eq!(r.shadow.len(), 2);
    }

    #[test]
    fn read4_stub_returns_to_the_caller_and_counts() {
        let mut m = Machine::new();
        m.install_dsp_return();
        m.cpu.r = [0; 16];
        m.cpu.r[1] = 1;
        m.cpu.r[2] = 0x2800_0000;
        m.cpu.r[3] = 0x2000;
        m.cpu.r[14] = 0xC000_0100;
        m.cpu.r[15] = SPI_READ4;
        m.run(1, 0xC000_0100, None);
        assert_eq!(m.cpu.r[15], 0xC000_0100);
        assert_eq!(m.bus.mem.read32(0x200C), 0);
        assert_eq!(m.dsp_return.as_ref().unwrap().reads, 1);
    }
}
