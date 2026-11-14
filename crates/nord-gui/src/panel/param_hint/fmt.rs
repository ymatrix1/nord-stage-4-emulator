//! Value formatters of the parameter hint line: a normalised knob position `0..=1` in, text out.
//!
//! Units follow the firmware's own value strings (`%.1f dB`, `%d bpm`, `0.08 Hz` rates, `kHz`
//! cutoffs; `docs/research/oled-strings-vs-firmware.md` section 2).

/// How a knob position turns into the value text of its hint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fmt {
    /// Log sweep `lo..=hi` Hz: `0.08 Hz`, `440 Hz`, `1.5 kHz`.
    LogHz(f32, f32),
    /// Linear gain `lo..=hi` dB: `-3.2 dB` (firmware `%.1f dB`).
    Db(f32, f32),
    /// Linear percentage `0..=max`: `42 %`.
    Percent(f32),
    /// Panel scale `0..=10` with one decimal: `7.1`.
    Ten,
    /// Panel scale `-10..=10` with one decimal (bipolar amounts): `-3.5`.
    TenBipolar,
    /// Time `max * n^2` seconds: `103 ms`, `1.5 s`.
    TimeSquared(f32),
    /// Tempo `lo..=hi` bpm: `120 bpm` (firmware `%d bpm`).
    Bpm(f32, f32),
    /// Whole steps `lo..=hi` with a unit word: `3 oct`.
    Steps(u8, u8, &'static str),
    /// The value text comes from the control's owner (`touch_value`); a bare touch shows the label.
    Caller,
}

/// `103 ms`, `1.5 s`, `11 s`.
pub fn time(s: f32) -> String {
    if s < 1.0 {
        format!("{:.0} ms", s * 1000.0)
    } else if s < 10.0 {
        format!("{s:.1} s")
    } else {
        format!("{s:.0} s")
    }
}

/// `0.08 Hz`, `2.2 Hz`, `13 Hz`, `1.5 kHz`.
///
/// EMPIRICAL: precision read off the hardware overlay `LFO Rate nn Hz` in the LFO course clip
/// (0.15, 0.95, 1.5, 2.2, 10, 13, 305 Hz; `docs/research/dsoundman-synth-lfo.md` section 2): two
/// decimals below 1 Hz, one below 10 Hz, none above. The `kHz` switch at 1000 Hz follows the
/// firmware's own cutoff strings (`Hz` / `kHz` runs, `docs/research/oled-strings-vs-firmware.md`
/// section 2).
pub fn hz(f: f32) -> String {
    if f < 1.0 {
        format!("{f:.2} Hz")
    } else if f < 10.0 {
        format!("{f:.1} Hz")
    } else if f < 1000.0 {
        format!("{f:.0} Hz")
    } else {
        format!("{:.1} kHz", f / 1000.0)
    }
}

/// Log interpolation `lo..=hi` at `n`.
pub fn log_lerp(lo: f32, hi: f32, n: f32) -> f32 {
    lo * (hi / lo).powf(n.clamp(0.0, 1.0))
}

impl Fmt {
    /// Value text at knob position `n`, or `None` for [`Fmt::Caller`].
    pub fn render(self, n: f32) -> Option<String> {
        let n = n.clamp(0.0, 1.0);
        Some(match self {
            Self::LogHz(lo, hi) => hz(log_lerp(lo, hi, n)),
            Self::Db(lo, hi) => format!("{:.1} dB", lo + (hi - lo) * n),
            Self::Percent(max) => format!("{:.0} %", max * n),
            Self::Ten => format!("{:.1}", n * 10.0),
            Self::TenBipolar => format!("{:.1}", n * 20.0 - 10.0),
            Self::TimeSquared(max) => time(max * n * n),
            Self::Bpm(lo, hi) => format!("{:.0} bpm", lo + (hi - lo) * n),
            Self::Steps(lo, hi, unit) => {
                let v = (f32::from(lo) + f32::from(hi - lo) * n).round() as u8;
                format!("{v} {unit}")
            }
            Self::Caller => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hz_switches_precision_and_unit() {
        assert_eq!(hz(0.08), "0.08 Hz");
        assert_eq!(hz(2.24), "2.2 Hz");
        assert_eq!(hz(12.54), "13 Hz");
        assert_eq!(hz(305.0), "305 Hz");
        assert_eq!(hz(1500.0), "1.5 kHz");
    }

    #[test]
    fn time_reads_ms_and_seconds() {
        assert_eq!(time(0.103), "103 ms");
        assert_eq!(time(1.5), "1.5 s");
        assert_eq!(time(11.0), "11 s");
    }

    #[test]
    fn log_sweep_hits_both_ends_and_the_geometric_middle() {
        let f = Fmt::LogHz(0.05, 40.0);
        assert_eq!(f.render(0.0).as_deref(), Some("0.05 Hz"));
        assert_eq!(f.render(1.0).as_deref(), Some("40 Hz"));
        let mid = log_lerp(0.05, 40.0, 0.5);
        assert!((mid - (0.05_f32 * 40.0).sqrt()).abs() < 1e-4);
    }

    #[test]
    fn linear_formats_scale_and_clamp() {
        assert_eq!(Fmt::Db(-16.0, 16.0).render(0.5).as_deref(), Some("0.0 dB"));
        assert_eq!(Fmt::Db(-16.0, 16.0).render(2.0).as_deref(), Some("16.0 dB"));
        assert_eq!(Fmt::Percent(95.0).render(1.0).as_deref(), Some("95 %"));
        assert_eq!(Fmt::Ten.render(0.71).as_deref(), Some("7.1"));
        assert_eq!(Fmt::TenBipolar.render(0.0).as_deref(), Some("-10.0"));
        assert_eq!(
            Fmt::Bpm(20.0, 300.0).render(0.5).as_deref(),
            Some("160 bpm")
        );
        assert_eq!(
            Fmt::Steps(1, 4, "oct").render(1.0).as_deref(),
            Some("4 oct")
        );
        assert_eq!(Fmt::TimeSquared(4.0).render(0.5).as_deref(), Some("1.0 s"));
        assert_eq!(Fmt::Caller.render(0.5), None);
    }
}
