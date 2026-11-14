//! Display label of a piano model: model name and size letter, from the `.npno` header name.
//!
//! The header name (32 bytes at file offset `0x48`, parsed as `NpnoInstrument::name`) is
//! `<model>#<detail padded with spaces> <size>`, for example `Pearl Upright#GRT 120   Lrg` or
//! `White Grand#Stw B     XL`. The text before `#` is the model name the hardware prints; the
//! last word is the size of the Nord Piano Library file (manual p.24 "About Piano size": Small
//! Sml, Medium Med, Large Lrg, Extra Large XL). Electric, Clav and Misc models without a size
//! have no last word from that set.
//!
//! VERIFIED: 30 of the 38 `.npno` of `Nord Stage 4 Factory Restore RevK v1.66.ns4b` carry a size
//! token in the header name and it equals the token in the entry file name for all 38 (the other
//! 8 have neither). Check: `python tools/npno_types.py "<backup>.ns4b"` prints
//! `30 with a size token; header vs file name size mismatches: 0` (`docs/research/piano-types.md`).
//! ASSUMED: the single-letter form on the display (`S`, `M`, `L`, `XL`) comes from the course
//! footage (`Pearl Upright    L`, `Silver Grand M`, `White Grand XL`); no frame shows `S`.

/// Size of a Nord Piano Library file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PianoSize {
    Small,
    Medium,
    Large,
    ExtraLarge,
}

impl PianoSize {
    /// Header / file name token.
    pub fn token(self) -> &'static str {
        match self {
            Self::Small => "Sml",
            Self::Medium => "Med",
            Self::Large => "Lrg",
            Self::ExtraLarge => "XL",
        }
    }

    /// Letter(s) the display prints after the model name.
    pub fn letter(self) -> &'static str {
        match self {
            Self::Small => "S",
            Self::Medium => "M",
            Self::Large => "L",
            Self::ExtraLarge => "XL",
        }
    }

    /// Size of a header token (`Sml`, `Med`, `Lrg`, `XL`).
    pub fn from_token(t: &str) -> Option<Self> {
        [Self::Small, Self::Medium, Self::Large, Self::ExtraLarge]
            .into_iter()
            .find(|s| s.token() == t)
    }
}

/// Model name and size of a loaded piano.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PianoLabel {
    pub model: String,
    pub size: Option<PianoSize>,
}

impl PianoLabel {
    /// Splits a header name; a name without `#` (synthetic or SFZ sounds) is the model itself.
    pub fn parse(raw: &str) -> Self {
        let raw = raw.trim_end_matches(['\0', ' ']);
        let size = raw
            .split_whitespace()
            .last()
            .and_then(PianoSize::from_token);
        let model = match raw.split_once('#') {
            Some((m, _)) => m,
            None => match size {
                Some(s) => raw.strip_suffix(s.token()).unwrap_or(raw),
                None => raw,
            },
        };
        Self {
            model: model.trim().to_string(),
            size,
        }
    }

    /// Text for a display row: `Pearl Upright  L`, or just the model without a size.
    pub fn row_text(&self) -> String {
        match self.size {
            Some(s) => format!("{}  {}", self.model, s.letter()),
            None => self.model.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_names_split_into_model_and_size() {
        let l = PianoLabel::parse("Pearl Upright#GRT 120   Lrg");
        assert_eq!(l.model, "Pearl Upright");
        assert_eq!(l.size, Some(PianoSize::Large));
        assert_eq!(l.row_text(), "Pearl Upright  L");
        let x = PianoLabel::parse("White Grand#Stw B     XL");
        assert_eq!(
            (x.model.as_str(), x.size),
            ("White Grand", Some(PianoSize::ExtraLarge))
        );
        assert_eq!(x.row_text(), "White Grand  XL");
        let m = PianoLabel::parse("Silver Grand#SK7       Med");
        assert_eq!(m.size.map(PianoSize::letter), Some("M"));
        let s = PianoLabel::parse("Bright Grand#YaS4      Sml");
        assert_eq!(s.size.map(PianoSize::letter), Some("S"));
    }

    #[test]
    fn models_without_a_size_keep_their_name_only() {
        let l = PianoLabel::parse("Marimba#Classic   ");
        assert_eq!((l.model.as_str(), l.size), ("Marimba", None));
        assert_eq!(l.row_text(), "Marimba");
        let c = PianoLabel::parse("Wurlitzer 1#200A      \0\0");
        assert_eq!((c.model.as_str(), c.size), ("Wurlitzer 1", None));
    }

    #[test]
    fn names_without_a_separator_are_the_model() {
        assert_eq!(PianoLabel::parse("White Grand").model, "White Grand");
        assert_eq!(PianoLabel::parse("White Grand").size, None);
        let l = PianoLabel::parse("Custom Piano Lrg");
        assert_eq!(
            (l.model.as_str(), l.size),
            ("Custom Piano", Some(PianoSize::Large))
        );
        assert_eq!(PianoLabel::parse("").model, "");
    }

    #[test]
    fn tokens_round_trip() {
        for t in ["Sml", "Med", "Lrg", "XL"] {
            assert_eq!(PianoSize::from_token(t).map(PianoSize::token), Some(t));
        }
        assert_eq!(PianoSize::from_token("Big"), None);
    }
}
