//! Per-wave P table of the strip chain: one EMPIRICAL [`PBlock`] per oscillator stage.
//!
//! The firmware stages are exact for any P block, but the block a loaded program uses is not
//! reachable (`docs/research/synth-wave-models.md` s8 to s10), so the level, the brightness
//! (cutoff factor) and, for SHAPE, the second oscillator are fitted per stage to hardware clips:
//! `docs/research/synth-validation.md` (fit clips, hold-out clips, residuals), driver
//! `tools/synth_validate.py`. Spec: `docs/specs/synth-engine.md` section 4.2.
//!
//! Selection: [`PWave::from_category_wave`] maps the decoded ANALOG `(category, wave)` pair to a
//! row, [`PTable::block`] returns the block, and `SynthLayer::p_table` switches the table on (the
//! block then also scales the stage oscillators and the legacy waveforms that stand in for stages
//! without a Rust model).

use super::p_block::PBlock;
use crate::synth_types::FilterModel;

/// Number of rows of the table.
pub const PWAVE_COUNT: usize = 9;

/// One row of the table: the oscillator stage family a decoded ANALOG wave plays through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PWave {
    /// PURE wave 2, saw (`0x1215FB`).
    PureSaw = 0,
    /// PURE waves 3 to 5, square / pulse (`0x121B99`).
    PurePulse,
    /// SHAPE wave 0 (`0x1212D3`).
    ShapeTriangle,
    /// SHAPE wave 1 (`0x12169D`).
    ShapeSaw,
    /// SHAPE wave 2 (`0x121AB5`).
    ShapeSquare,
    /// SHAPE SINE, waves 0 to 5.
    ShapeSine,
    /// MULTI, all waves (`0x121C54`).
    Multi,
    /// SUPER wave 0 (`0x121D6D`).
    Super,
    /// Everything else (sine, triangle, noise, SUB OSC, SYNC, MISC): idle block.
    Other,
}

impl PWave {
    const NAMES: [&'static str; PWAVE_COUNT] = [
        "saw",
        "pulse",
        "shape",
        "shapesaw",
        "shapesq",
        "shapesine",
        "multi",
        "super",
        "other",
    ];

    /// Row of an ANALOG `(category, wave)` pair.
    pub fn from_category_wave(category: u8, wave: u8) -> Self {
        match (category, wave) {
            (0, 2) => Self::PureSaw,
            (0, 3..=5) => Self::PurePulse,
            (3, 0) => Self::ShapeTriangle,
            (3, 1) => Self::ShapeSaw,
            (3, 2) => Self::ShapeSquare,
            // only the waves with a stage model: the others play a legacy waveform whose level
            // the row was not fitted for
            (4, 0 | 1 | 2 | 5) => Self::ShapeSine,
            (5, _) => Self::Multi,
            (6, 0) => Self::Super,
            _ => Self::Other,
        }
    }

    /// CLI name (`saw`, `pulse`, `shape`, ...).
    pub fn name(self) -> &'static str {
        Self::NAMES[self as usize]
    }

    /// Row by CLI name.
    pub fn from_name(n: &str) -> Option<Self> {
        let i = Self::NAMES.iter().position(|x| x.eq_ignore_ascii_case(n))?;
        Some(Self::ALL[i])
    }

    /// Every row in table order.
    pub const ALL: [PWave; PWAVE_COUNT] = [
        Self::PureSaw,
        Self::PurePulse,
        Self::ShapeTriangle,
        Self::ShapeSaw,
        Self::ShapeSquare,
        Self::ShapeSine,
        Self::Multi,
        Self::Super,
        Self::Other,
    ];
}

/// One [`PBlock`] per [`PWave`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PTable {
    blocks: [PBlock; PWAVE_COUNT],
}

impl PTable {
    /// The EMPIRICAL table, `docs/research/synth-validation.md` section 4.
    pub const FITTED: PTable = PTable { blocks: BLOCKS };

    /// Block of a row.
    #[inline]
    pub fn block(&self, w: PWave) -> PBlock {
        self.blocks[w as usize]
    }

    /// The table with one row replaced.
    pub fn with(mut self, w: PWave, b: PBlock) -> PTable {
        self.blocks[w as usize] = b;
        self
    }
}

/// Table selection of `--synth-p table[:wave=g/w2/r2/shape/cutoff;...]`: the built-in table plus
/// optional rows in thousandths (fit sweeps). `Eq` so it can live in `FirmwareOptions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PTableSpec {
    rows: [Option<[i32; 5]>; PWAVE_COUNT],
}

impl PTableSpec {
    /// The built-in table, no overrides.
    pub const BUILTIN: PTableSpec = PTableSpec {
        rows: [None; PWAVE_COUNT],
    };

    /// Parses the part after `table` (empty or `:wave=g/w2/r2/shape/cutoff;...`).
    pub fn parse(rest: &str) -> Result<PTableSpec, String> {
        let bad = |what: &str| format!("invalid --synth-p table '{rest}': {what}");
        let mut spec = Self::BUILTIN;
        let body = rest.strip_prefix(':').unwrap_or(rest);
        for item in body.split(';').filter(|s| !s.trim().is_empty()) {
            let (name, vals) = item
                .split_once('=')
                .ok_or_else(|| bad("expected wave=..."))?;
            let w = PWave::from_name(name.trim()).ok_or_else(|| bad("unknown wave"))?;
            let mut m = [0i32; 5];
            let mut it = vals.split('/');
            for slot in &mut m {
                let x: f32 = it
                    .next()
                    .ok_or_else(|| bad("need 5 values"))?
                    .trim()
                    .parse()
                    .map_err(|_| bad("not a number"))?;
                *slot = (x * 1000.0).round() as i32;
            }
            if it.next().is_some() {
                return Err(bad("more than 5 values"));
            }
            spec.rows[w as usize] = Some(m);
        }
        Ok(spec)
    }

    /// The table this selection stands for.
    pub fn table(&self) -> PTable {
        let mut t = PTable::FITTED;
        for w in PWave::ALL {
            if let Some(m) = self.rows[w as usize] {
                t = t.with(w, PBlock::from_milli(m));
            }
        }
        t
    }
}

/// One row without a second oscillator (`weight2` 0); `shape` scales the Osc Ctrl knob on the
/// SHAPE stage (1.0 = the word equals the knob) and is unused elsewhere.
const fn row(gain: f32, shape: f32, cutoff_scale: f32) -> PBlock {
    PBlock {
        gain,
        weight2: 0.0,
        ratio2: 1.0,
        shape,
        cutoff_scale,
    }
}

/// Pure Saw row. EMPIRICAL: gain 0.4 (RMS delta +0.4 dB on `filter_bank`, A:75, band pass,
/// velocity 100; `phat_3osc` +1.0 dB hold-out) and cutoff factor 0.85 from the laser-zap saw
/// (LP24, best 0.8 to 0.9). The earlier disagreement with `filter_bank` (best 1.25 as a saw
/// factor) is carried by the band pass type factor of [`type_cutoff_scale`] instead.
/// `docs/research/synth-validation.md` sections 4 and 7.
const SAW: PBlock = row(0.4, 0.5, 0.85);
/// Pure Square row. EMPIRICAL: gain = saw gain x 0.68 (the two notes of the pitch-blip clip
/// differ by 3.4 dB, so the pulse sits 3.4 dB under the saw; one pair, velocity unknown); the
/// cutoff factor is the saw's (ASSUMED, no filtered pulse clip).
const PULSE: PBlock = row(0.27, 0.5, 0.85);
/// SHAPE wave 0 row. EMPIRICAL: gain 0.6 and cutoff factor 1.75 from the Smooth Lead clip (A:87,
/// fit; 1.0 gives presence -10.1 dB, 1.35 -3.7, 1.75 -2.0), no second oscillator, the shape word
/// equal to the Osc Ctrl knob (`shape` 1.0 scales it); hold-out Crunch Synth (A:66, score 8.3
/// legacy to 1.2).
const SHAPE: PBlock = row(0.6, 1.0, 1.75);
/// MULTI row. EMPIRICAL: gain 0.26 and cutoff factor 1.4 from `sci_fi_80s` (A:44, fit,
/// score 2.0 to 0.24); hold-out `mega_line` (A:48).
const MULTI: PBlock = row(0.26, 0.5, 1.4);
/// SUPER wave 0 row. EMPIRICAL: gain 2.0 (RMS +0.2 dB on `pad_silk`, A:86). Cutoff factor 1.07
/// times the LP12 type factor 1.4 of [`type_cutoff_scale`] = 1.5, the value of the earlier
/// compromise: `pad_silk` alone wants 0.5 (presence +8.7 dB too bright at 1.0) and `mega_line`
/// (A:48, LP12 1200 Hz) wants 2.0. Section 7 of the validation research shows the conflict is
/// the filter envelope of `pad_silk` (its score is 1.5 at 2 envelope octaves per full amount),
/// not a cutoff factor of the SUPER stage, so the SUPER row keeps the compromise.
const SUPER: PBlock = row(2.0, 0.5, 1.07);

/// Cutoff factor per filter type, applied with the table on top of the row factor (the strip
/// cutoff word of the displayed value is multiplied by it).
///
/// EMPIRICAL (`docs/research/synth-validation.md` section 7, grid on the 15 clips; the Hz law of
/// the stages themselves, `R = tan(pi fc / F)`, is VERIFIED against the oracle, the displayed
/// cutoff to word map is not):
///
/// LP12 1.4 (grid 0.7 to 1.7 on `sweep_12db`, `mega_line`, `sine_pluck`, `lead_crunch`,
/// `pad_silk`), BP 1.4 (`filter_bank` presence -2.6 to -0.7 dB), LP24 / Ladder24 / HP 1.0 (LP24 1.3
/// is clearly worse, no clip for the others).
pub fn type_cutoff_scale(model: FilterModel) -> f32 {
    match model {
        FilterModel::Lp12 | FilterModel::BandPass => 1.4,
        FilterModel::Lp24 | FilterModel::Ladder24 | FilterModel::HighPass => 1.0,
    }
}

/// Rows in [`PWave`] order. SHAPE saw reuses the saw row and SHAPE SQUARE / SHAPE SINE / other
/// waves stay at the idle block: ASSUMED, no fit clip with a hold-out.
const BLOCKS: [PBlock; PWAVE_COUNT] = [
    SAW,
    PULSE,
    SHAPE,
    SAW,
    PBlock::IDLE,
    PBlock::IDLE,
    MULTI,
    SUPER,
    PBlock::IDLE,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_wave_pairs_map_to_their_stage_rows() {
        assert_eq!(PWave::from_category_wave(0, 2), PWave::PureSaw);
        assert_eq!(PWave::from_category_wave(0, 4), PWave::PurePulse);
        assert_eq!(PWave::from_category_wave(3, 0), PWave::ShapeTriangle);
        assert_eq!(PWave::from_category_wave(3, 1), PWave::ShapeSaw);
        assert_eq!(PWave::from_category_wave(3, 2), PWave::ShapeSquare);
        assert_eq!(PWave::from_category_wave(4, 5), PWave::ShapeSine);
        assert_eq!(
            PWave::from_category_wave(4, 3),
            PWave::Other,
            "no stage model"
        );
        assert_eq!(PWave::from_category_wave(5, 3), PWave::Multi);
        assert_eq!(PWave::from_category_wave(6, 0), PWave::Super);
        assert_eq!(
            PWave::from_category_wave(6, 1),
            PWave::Other,
            "legacy super saw"
        );
        assert_eq!(PWave::from_category_wave(0, 6), PWave::Other);
        assert_eq!(PWave::from_category_wave(7, 0), PWave::Other);
    }

    #[test]
    fn type_factors_and_super_row_keep_the_lp12_super_cutoff_at_the_old_compromise() {
        assert_eq!(type_cutoff_scale(FilterModel::Lp12), 1.4);
        assert_eq!(type_cutoff_scale(FilterModel::BandPass), 1.4);
        assert_eq!(type_cutoff_scale(FilterModel::Lp24), 1.0);
        let sup = PTable::FITTED.block(PWave::Super).cutoff_scale;
        let lp12 = sup * type_cutoff_scale(FilterModel::Lp12);
        assert!((lp12 - 1.5).abs() < 0.01, "SUPER x LP12 = {lp12}");
        assert_eq!(PTable::FITTED.block(PWave::PureSaw).cutoff_scale, 0.85);
        assert_eq!(PTable::FITTED.block(PWave::PurePulse).cutoff_scale, 0.85);
    }

    #[test]
    fn names_round_trip_and_cover_every_row() {
        for w in PWave::ALL {
            assert_eq!(PWave::from_name(w.name()), Some(w));
        }
        assert_eq!(PWave::from_name("SAW"), Some(PWave::PureSaw));
        assert_eq!(PWave::from_name("nope"), None);
        assert_eq!(PWave::ALL.len(), PWAVE_COUNT);
    }

    #[test]
    fn spec_overrides_one_row_and_keeps_the_rest() {
        let s = PTableSpec::parse(":saw=0.5/0/1/0.5/1.2;super=0.7/0/1/0.5/0.9").unwrap();
        let t = s.table();
        assert_eq!(t.block(PWave::PureSaw).gain, 0.5);
        assert_eq!(t.block(PWave::PureSaw).cutoff_scale, 1.2);
        assert_eq!(t.block(PWave::Super).gain, 0.7);
        assert_eq!(
            t.block(PWave::Multi),
            PTable::FITTED.block(PWave::Multi),
            "untouched rows keep the built-in values"
        );
        assert_eq!(PTableSpec::parse("").unwrap().table(), PTable::FITTED);
    }

    #[test]
    fn spec_rejects_malformed_rows() {
        for bad in [
            ":saw",
            ":nope=1/1/1/1/1",
            ":saw=1/2/3/4",
            ":saw=1/2/3/4/5/6",
            ":saw=a/b/c/d/e",
        ] {
            assert!(PTableSpec::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn built_in_rows_are_inside_the_physical_ranges() {
        for w in PWave::ALL {
            let p = PTable::FITTED.block(w);
            assert!(p.gain > 0.0 && p.gain < 8.0, "{w:?} gain");
            assert!((0.0..=4.0).contains(&p.weight2), "{w:?} weight2");
            assert!((0.05..=8.0).contains(&p.ratio2), "{w:?} ratio2");
            assert!((-1.0..=1.0).contains(&p.shape), "{w:?} shape");
            assert!((0.25..=4.0).contains(&p.cutoff_scale), "{w:?} cutoff");
        }
    }

    fn rms(table: Option<PTable>, wave: PWave, stage: Option<super::super::FwStage>) -> f32 {
        use super::super::{SynthChain, SynthLayer, VaWaveform};
        let mut s = SynthLayer::new(48_000.0);
        s.chain = SynthChain::FirmwareStrip;
        s.filter_on = false;
        s.waveform = VaWaveform::Saw;
        s.p_table = table;
        s.p_wave = wave;
        s.strip_stage = stage;
        s.strip_stages = stage.is_some();
        s.note_on(60, 127);
        let mut acc = 0.0f64;
        for i in 0..24_000 {
            let [l, _] = s.render_frame();
            if i >= 12_000 {
                acc += f64::from(l * l);
            }
        }
        (acc / 12_000.0).sqrt() as f32
    }

    #[test]
    fn table_gain_scales_the_saw_and_the_stage_oscillators() {
        let half = |w| {
            PTable::FITTED.with(
                w,
                PBlock {
                    gain: 0.5,
                    ..PBlock::IDLE
                },
            )
        };
        let saw0 = rms(Some(PTable::FITTED), PWave::PureSaw, None);
        let saw1 = rms(Some(half(PWave::PureSaw)), PWave::PureSaw, None);
        let g_saw = PTable::FITTED.block(PWave::PureSaw).gain;
        assert!(saw0 > 1e-4, "{saw0}");
        assert!((saw1 / saw0 - 0.5 / g_saw).abs() < 2e-3, "{saw1} vs {saw0}");
        let st = Some(super::super::FwStage::Super);
        let sup0 = rms(Some(PTable::FITTED), PWave::Super, st);
        let sup1 = rms(Some(half(PWave::Super)), PWave::Super, st);
        let g_sup = PTable::FITTED.block(PWave::Super).gain;
        assert!(sup0 > 1e-4, "{sup0}");
        assert!((sup1 / sup0 - 0.5 / g_sup).abs() < 2e-3, "{sup1} vs {sup0}");
    }

    #[test]
    fn rows_of_other_waves_do_not_touch_a_layer() {
        let loud = PTable::FITTED.with(
            PWave::Multi,
            PBlock {
                gain: 4.0,
                ..PBlock::IDLE
            },
        );
        let a = rms(Some(PTable::FITTED), PWave::PureSaw, None);
        let b = rms(Some(loud), PWave::PureSaw, None);
        assert_eq!(a, b);
        let off = rms(None, PWave::Other, None);
        let idle = rms(Some(PTable::FITTED), PWave::Other, None);
        assert!(
            (off / idle - 1.0).abs() < 1e-3,
            "idle row equals no table: {off} vs {idle}"
        );
        assert!(
            (a / off - 0.4).abs() < 0.02,
            "the saw row scales by its gain: {a} vs {off}"
        );
    }
}
