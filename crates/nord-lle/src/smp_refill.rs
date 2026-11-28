//! SMP page refill (bean `ns4-pnoc`, `docs/research/smp-page-refill.md`) inside the engine.
//!
//! The ARM never sends a flash address to the SMP chip: it writes a stroke descriptor into the
//! voice record (`rec[-11]` directory index of the file, `rec[-10]` page number of the stroke,
//! `rec[-9]` frame count). [`SmpRefill`] watches the SMP messages, recognises the descriptor by
//! its frame count, checks the page law and returns the directory index to place; the engine then
//! calls [`nord_sharc::runner::live_flash::place`] on the SMP core, which the external-port chain
//! model of the core serves from the user's `.npno` (port of `tools/arm_smp_flash.py`).

use nord_sharc::core::periph::smpflash::SmpFlash;
use nord_sharc::core::Core;
use std::collections::BTreeMap;

/// VERIFIED (`nsmp4-and-npno-cnsp-codec.md` s3.2): file byte = `0x2C` + u32 at stroke `+8`.
pub const CNSP_BASE: u32 = 0x2C;
/// Stroke table offset, record size, channel count and stroke count offsets in a `.npno`
/// (`tools/arm_smp_flash.py`; VERIFIED there on 4 files, ASSUMED to hold for every `.npno`).
pub const STROKE_TABLE: usize = 0x756;
pub const STROKE_SIZE: usize = 0x76;
pub const CHANNELS_AT: usize = 0x64A;
pub const NSTROKES_AT: usize = 0x64C;
/// VERIFIED (`nsmp4-and-npno-cnsp-codec.md` s3.2): page bytes per channel.
pub const PAGE_BYTES: u32 = 1022;
/// VERIFIED (run, `smp-page-refill.md` s2, Astoria Grand strokes 513 and 502): the page grid of
/// `rec[-10]` starts 148 bytes before the file.
pub const GRID_ORIGIN: i64 = -148;

/// One stroke record of a `.npno`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stroke {
    pub index: usize,
    /// File byte of the stroke, CNSP base included.
    pub offset: u32,
    pub frames: u32,
    pub channels: u32,
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}
fn be16(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from(u16::from_be_bytes(
        b.get(at..at + 2)?.try_into().ok()?,
    )))
}

/// The stroke records of a `.npno` (the header tables are enough).
pub fn parse_strokes(npno: &[u8]) -> Result<Vec<Stroke>, String> {
    let short = || "npno header too short".to_string();
    let channels = be16(npno, CHANNELS_AT).ok_or_else(short)?;
    let count = be16(npno, NSTROKES_AT).ok_or_else(short)? as usize;
    (0..count)
        .map(|k| {
            let at = STROKE_TABLE + k * STROKE_SIZE;
            Ok(Stroke {
                index: k,
                offset: be32(npno, at + 8)
                    .ok_or_else(short)?
                    .checked_add(CNSP_BASE)
                    .ok_or("stroke offset overflows the file range")?,
                frames: be32(npno, at + 0x0E).ok_or_else(short)?,
                channels,
            })
        })
        .collect()
}

/// `rec[-10]` of a stroke; `None` when its offset is not on the page grid.
pub fn stroke_page(s: &Stroke) -> Option<u32> {
    let unit = i64::from(PAGE_BYTES * s.channels.max(1));
    let v = i64::from(s.offset) - GRID_ORIGIN;
    (v % unit == 0).then(|| (v / unit) as u32)
}

/// Descriptor watcher of one bound `.npno`.
#[derive(Clone, Debug)]
pub struct SmpRefill {
    strokes: Vec<Stroke>,
    by_frames: BTreeMap<u32, Vec<usize>>,
    last: BTreeMap<u32, u32>,
    /// Directory index of the file placed last.
    pub rec11: Option<u32>,
    /// Descriptors seen, of which obeying the page law, of which placed.
    pub descriptors: u64,
    pub law_ok: u64,
    pub placed: u64,
}

impl SmpRefill {
    pub fn new(npno: &[u8]) -> Result<Self, String> {
        let strokes = parse_strokes(npno)?;
        let mut by_frames: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        for s in &strokes {
            by_frames.entry(s.frames).or_default().push(s.index);
        }
        Ok(Self {
            strokes,
            by_frames,
            last: BTreeMap::new(),
            rec11: None,
            descriptors: 0,
            law_ok: 0,
            placed: 0,
        })
    }

    /// Strokes of the bound file.
    pub fn strokes(&self) -> &[Stroke] {
        &self.strokes
    }

    /// Feed one SMP message; returns the directory index to place when a new descriptor that
    /// obeys the page law appeared.
    pub fn observe(&mut self, word: u32, value: u32) -> Option<u32> {
        self.last.insert(word, value);
        let cands = self.by_frames.get(&value)?;
        let rec11 = *self.last.get(&word.wrapping_sub(2))?;
        let rec10 = *self.last.get(&word.wrapping_sub(1))?;
        self.descriptors += 1;
        let ok = cands
            .iter()
            .any(|&i| stroke_page(&self.strokes[i]) == Some(rec10));
        if !ok {
            return None;
        }
        self.law_ok += 1;
        if self.rec11 == Some(rec11) {
            return None;
        }
        self.rec11 = Some(rec11);
        self.placed += 1;
        Some(rec11)
    }
}

/// Installs `npno` as the flash image behind `core`'s external banks (what `L PATH` does).
pub fn bind_flash(core: &mut Core, npno: Vec<u8>) {
    if core.epchain().is_none() {
        core.enable_epchain();
    }
    if let Some(e) = core.epchain() {
        e.flash = Some(SmpFlash::new(npno));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A header-only `.npno`: 2 channels, strokes `(offset field, frames)`.
    pub(crate) fn fake_npno(strokes: &[(u32, u32)], channels: u16) -> Vec<u8> {
        let mut b = vec![0u8; STROKE_TABLE + strokes.len() * STROKE_SIZE];
        b[CHANNELS_AT..CHANNELS_AT + 2].copy_from_slice(&channels.to_be_bytes());
        b[NSTROKES_AT..NSTROKES_AT + 2].copy_from_slice(&(strokes.len() as u16).to_be_bytes());
        for (k, (off, frames)) in strokes.iter().enumerate() {
            let at = STROKE_TABLE + k * STROKE_SIZE;
            b[at + 8..at + 12].copy_from_slice(&off.to_be_bytes());
            b[at + 0x0E..at + 0x12].copy_from_slice(&frames.to_be_bytes());
        }
        b
    }

    /// File offset field that puts a stereo stroke on page `v`: 2044 v - 148 - 0x2C.
    fn off_for_page(v: u32) -> u32 {
        2044 * v - 148 - CNSP_BASE
    }

    #[test]
    fn strokes_parse_and_pages_follow_the_grid_law() {
        let npno = fake_npno(&[(off_for_page(3), 100), (5, 200)], 2);
        let s = parse_strokes(&npno).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].frames, s[0].channels), (100, 2));
        assert_eq!(stroke_page(&s[0]), Some(3));
        assert_eq!(stroke_page(&s[1]), None, "off the grid");
        assert!(parse_strokes(&[0u8; 8]).is_err());
    }

    #[test]
    fn a_descriptor_obeying_the_law_places_the_file_once() {
        let npno = fake_npno(&[(off_for_page(7), 4321)], 2);
        let mut r = SmpRefill::new(&npno).unwrap();
        assert_eq!(r.observe(0x9_3000, 5), None, "rec[-11]");
        assert_eq!(r.observe(0x9_3001, 7), None, "rec[-10]");
        assert_eq!(r.observe(0x9_3002, 4321), Some(5), "rec[-9] = frame count");
        assert_eq!(r.observe(0x9_3002, 4321), None, "same file again");
        assert_eq!((r.descriptors, r.law_ok, r.placed), (2, 2, 1));
        // a wrong page number is counted but not placed
        let mut r = SmpRefill::new(&npno).unwrap();
        r.observe(0x9_3000, 5);
        r.observe(0x9_3001, 8);
        assert_eq!(r.observe(0x9_3002, 4321), None);
        assert_eq!((r.descriptors, r.law_ok, r.placed), (1, 0, 0));
    }

    #[test]
    fn a_stroke_offset_at_the_top_of_the_range_is_an_error_not_a_panic() {
        let npno = fake_npno(&[(u32::MAX, 1)], 2);
        assert!(parse_strokes(&npno).unwrap_err().contains("overflows"));
        let npno = fake_npno(&[(u32::MAX - CNSP_BASE, 1)], 2);
        assert!(
            parse_strokes(&npno).is_ok(),
            "the largest valid offset still parses"
        );
    }

    #[test]
    fn bind_flash_enables_the_chain_model() {
        let mut c = Core::new(&vec![0u8; 0x24_0000]);
        bind_flash(&mut c, vec![0u8; 4096]);
        assert!(c.epchain().unwrap().flash.is_some());
    }
}
