//! Description types of the Synth OLED contents (see `oled_view.rs` for how they are built).

/// Shape drawn for the waveform page.
#[derive(Debug, Clone, PartialEq)]
pub enum Graphic {
    /// Waveform name and OSC CTRL line, with a mini waveform icon.
    Wave {
        name: String,
        ctrl: String,
        icon: Icon,
    },
    /// Attack / decay / release outline.
    Env { a: f32, d: f32, r: f32 },
    /// Filter response sketch: type index, cutoff `0..=1`, resonance `0..=1`.
    Filter { ty: u8, cutoff: f32, res: f32 },
    /// One LFO cycle.
    Lfo(u8),
    /// Gate pattern grid: gate length (eighths) of each of the first `len` steps, optional cursor.
    Pattern {
        steps: [u8; 16],
        len: u8,
        cursor: Option<u8>,
    },
    /// Arpeggio direction arrows.
    Direction(u8),
    /// Vibrato sine, rate and amount.
    Vibrato { rate: f32, amount: f32 },
    /// Large single value.
    Big(String),
    /// Three list rows (previous, current, next); the middle one is selected.
    List([String; 3]),
    /// Plain text lines.
    Lines(Vec<String>),
}

/// Waveform icon.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Icon {
    Sine,
    Triangle,
    Saw,
    Pulse(f32),
    Multi,
    Noise,
    Fm,
    Table,
    Sample,
}

/// One soft label under the display.
#[derive(Debug, Clone, PartialEq)]
pub struct Soft {
    pub value: String,
    pub label: &'static str,
    /// Dimmed: the dial has no engine effect (tooltip lives on the dial).
    pub dim: bool,
}

/// Complete description of the OLED contents.
#[derive(Debug, Clone, PartialEq)]
pub struct OledView {
    pub title: &'static str,
    /// Focused layer letter.
    pub layer: char,
    pub graphic: Graphic,
    pub soft: [Option<Soft>; 3],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn views_compare_by_value() {
        let a = Graphic::Big("x".into());
        assert_eq!(a, Graphic::Big("x".into()));
        assert_ne!(Icon::Pulse(0.5), Icon::Pulse(0.1));
        let s = Soft {
            value: "1".into(),
            label: "A",
            dim: false,
        };
        assert_eq!(s.clone(), s);
    }
}
