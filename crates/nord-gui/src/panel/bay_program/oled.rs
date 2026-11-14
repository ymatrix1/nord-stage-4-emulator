//! The Program OLED: text of every screen (pure, testable) and its painter.

use super::catalog::display_name;
use super::model::{ProgView, Screen};
use super::morph::source_label;
pub use super::oled_grid::Line;
use super::oled_home::{location_title, name_line, section_rows};
#[cfg(test)]
use super::oled_icons::{ICON_ORGAN, ICON_PIANO, ICON_SYNTH};
use super::store::ProgLoc;
use crate::panel::ui_state::ProgramUi;
use nord_core::morph::LayerId;
use nord_core::piano::label::PianoSize;

/// What the display shows besides the Program area's own state.
#[derive(Debug, Clone, Default)]
pub struct Summary {
    pub organ: String,
    pub piano: String,
    pub synth: String,
    /// Size of the loaded piano file (`S`, `M`, `L`, `XL`), when its header names one.
    pub piano_size: Option<PianoSize>,
    /// Drawbar positions `0..=8` of the Organ preset while the model is the B3 (the only model
    /// with drawbars); `None` for Vox, Farfisa and the pipe models.
    pub organ_bars: Option<[u8; 9]>,
    /// Layers on in the active scene, in `LayerId::ALL` order.
    pub layers_on: [bool; 7],
}

/// The text of one display refresh.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OledText {
    pub big: String,
    pub sub: String,
    pub rows: Vec<String>,
    pub soft: [&'static str; 4],
    /// Title-bar screens: `Some(counter)` paints `big` in an inverted bar with the (possibly
    /// empty) page counter right-aligned; `sub` is then the label and `rows` the body.
    pub bar: Option<String>,
    /// Title-bar screens: the first body row is a lone value drawn in the 13 px font (menu pages
    /// with one item); `false` keeps every body row in the 8 px font.
    pub body_big: bool,
    /// Index of the lit soft button.
    pub soft_on: Option<usize>,
    /// Parameter popup `(label, value)` in the lower half of the home screen.
    pub popup: Option<(String, String)>,
    /// Title-bar screens with their own row layout (the Store screens): when not empty these
    /// lines replace the label and the body rows.
    pub lines: Vec<Line>,
    /// Program-list screens: index within `rows` of the lit (selected) row; the rows then sit at
    /// y = 13, 23, 33, 43 and carry no focus marker.
    pub sel: Option<usize>,
}

impl OledText {
    /// All text joined for the hover tooltip.
    pub fn plain(&self) -> String {
        let title = super::oled_home::strip_marks(&self.big);
        let mut s = match &self.bar {
            Some(counter) => format!("{title} {counter}\n{}\n", self.sub),
            None => format!("{title}\n{}\n", self.sub),
        };
        for r in &self.rows {
            s.push_str(&super::oled_home::strip_marks(
                r.trim_start_matches(super::menu_screen::DIM),
            ));
            s.push('\n');
        }
        for l in &self.lines {
            s.push_str(&l.text);
            s.push('\n');
        }
        if let Some((label, value)) = &self.popup {
            s.push_str(&format!("{label}\n{value}\n"));
        }
        if self.soft.iter().any(|l| !l.is_empty()) {
            s.push_str(&format!("[{}]", self.soft.join("] [")));
        }
        s
    }
}

/// Short layer name.
pub fn layer_label(l: LayerId) -> &'static str {
    [
        "Org A", "Org B", "Pno A", "Pno B", "Syn A", "Syn B", "Syn C",
    ][l as usize]
}

/// Name of the program at `loc`: stored user program, else the backup entry, else empty.
pub fn name_at(ui: &ProgramUi, loc: ProgLoc) -> String {
    if let Some(p) = ui.library.get(loc) {
        return p.name.clone();
    }
    ui.catalog
        .programs
        .get(usize::from(loc.index()))
        .map(|e| display_name(e))
        .unwrap_or_default()
}

fn home(ui: &ProgramUi, sum: &Summary) -> OledText {
    let label = location_title(ui);
    let sources = format!("{} / {} / {}", sum.organ, sum.piano, sum.synth);
    let mut t = match ui.view {
        ProgView::Name => OledText {
            big: label,
            sub: if ui.preset_name {
                sources
            } else {
                name_line(ui)
            },
            ..Default::default()
        },
        ProgView::Layers => OledText {
            big: format!("{label} {}", name_line(ui)).trim_end().to_string(),
            sub: String::new(),
            rows: section_rows(sum),
            ..Default::default()
        },
        ProgView::Full => {
            let on: Vec<&str> = LayerId::ALL
                .iter()
                .filter(|&&l| sum.layers_on[l as usize])
                .map(|&l| layer_label(l))
                .collect();
            OledText {
                big: format!("{label} {}", name_line(ui)).trim_end().to_string(),
                sub: format!("Scene {}", if ui.scene2 { "II" } else { "I" }),
                rows: vec![
                    format!("On: {}", on.join(" ")),
                    format!("Org {} Pno {} Syn {}", sum.organ, sum.piano, sum.synth),
                    format!(
                        "Split {} Tr {:+} {:.0}bpm",
                        if ui.split.on { "on" } else { "off" },
                        ui.transp.effective(),
                        ui.clock.bpm
                    ),
                ],
                ..Default::default()
            }
        }
        ProgView::Page => {
            let rows = (0..8u8)
                .map(|s| {
                    let l = ProgLoc {
                        slot: s,
                        ..ui.browse
                    };
                    format!(
                        "{}{} {}",
                        if l == ui.loc { ">" } else { " " },
                        s + 1,
                        name_at(ui, l)
                    )
                })
                .collect();
            OledText {
                big: format!("Page {}", ui.browse.label()),
                sub: String::new(),
                rows,
                ..Default::default()
            }
        }
    };
    if let Some((m, _)) = &ui.message {
        t.rows.extend(m.lines().map(str::to_string));
    }
    t
}

/// Layers a Layer Init of each section would reset, as the display prints them above the soft
/// buttons: Organ always `AB`, Piano and Synth the focused layer.
fn layer_init_letters(focus: LayerId) -> String {
    let piano = if focus == LayerId::PianoB { 'B' } else { 'A' };
    let synth = match focus {
        LayerId::SynthB => 'B',
        LayerId::SynthC => 'C',
        _ => 'A',
    };
    format!("Organ AB  Piano {piano}  Synth {synth}")
}

/// Compose the text for the current screen.
pub fn compose(ui: &ProgramUi, sum: &Summary) -> OledText {
    let t = |big: &str, sub: String, rows: Vec<String>| OledText {
        big: big.into(),
        sub,
        rows,
        soft: ui.screen.soft_labels(),
        ..Default::default()
    };
    match ui.screen {
        Screen::Home => home(ui, sum),
        Screen::List => super::oled_lists::list(ui),
        Screen::Menu(m) => super::menu_screen::compose(ui, m),
        Screen::StoreTo | Screen::StoreAs => super::oled_store::compose(ui),
        Screen::Split => super::oled_lists::split(ui),
        // VERIFIED (ARM frame, research note s16.7, key id 16): a popup in the lower half of the
        // home screen, label `Master Clock Tempo` (row y = 45, four beat boxes at the right, the
        // first lit) and `120 bpm` (y = 55). ASSUMED: the Keyboard Sync state is appended to the
        // value (the real popup has no room for the KBS soft button, which stays functional).
        Screen::Clock => OledText {
            popup: Some((
                "Master Clock Tempo".into(),
                format!("{:.0} bpm\tKBS {}", ui.clock.bpm, ui.clock.kbs.label()),
            )),
            ..home(ui, sum)
        },
        // VERIFIED: Transpose is a popup in the lower half of the home screen, label and value
        // (`Transpose` / `+1 semi`, ARM frames, research note s12). ASSUMED: `Off` for the off state.
        Screen::Transpose => OledText {
            popup: Some((
                "Transpose".into(),
                if ui.transp.on {
                    format!("{:+} semi", ui.transp.semitones)
                } else {
                    "Off".into()
                },
            )),
            ..home(ui, sum)
        },
        Screen::Preset { kind, single } => super::oled_lists::preset(ui, kind, single),
        // VERIFIED (ARM frame, research note s16.7, key id 21 with the modifier bit set): bar `UNDO`
        // without a counter, `Nothing to Undo` (8 px, ink rows 16 to 23), and one outlined `Ok`
        // button in the fourth soft slot. ASSUMED: with history the label reads `Undo to before` and
        // the row `the last edit?` (the pool pair `Undo to before` / `%s?`), `Redo` sits in the
        // third slot (the pool order is `Ok`, `Redo`); the Redo slot is only drawn when a redo exists.
        Screen::Undo => {
            let (sub, rows) = if ui.undo_blocked {
                ("Nothing to Undo", Vec::new())
            } else {
                ("Undo to before", vec!["the last edit?".to_string()])
            };
            let mut soft = ui.screen.soft_labels();
            if !ui.redo_available {
                soft[2] = "";
            }
            OledText {
                bar: Some(String::new()),
                big: "UNDO".into(),
                sub: sub.into(),
                rows,
                soft,
                ..Default::default()
            }
        }
        // VERIFIED: title, prompt and the per-section layer letters (`AB`, `A`, `A`) above the
        // All / Organ / Piano / Synth soft buttons (soft-piano video `NVMtX0epXog` 1:16,
        // piano-pad video `hR31DQrxfT4` 1:02). PAGE picking the layer is emulator-only.
        Screen::LayerInit => OledText {
            bar: Some(String::new()),
            ..t(
                "LAYER INIT",
                "Press to initialize".into(),
                vec![layer_init_letters(ui.focus), "PAGE picks the layer".into()],
            )
        },
        Screen::Copy => t(
            "MON/COPY",
            format!("Layer: {}", layer_label(ui.focus)),
            vec!["PAGE picks the layer".into()],
        ),
        Screen::Paste => t(
            "PASTE",
            format!("Layer: {}", layer_label(ui.focus)),
            vec![format!(
                "Clip: {}",
                ui.copied.map_or("empty".into(), |c| format!("{c:?}"))
            )],
        ),
        Screen::Morph(s) => {
            let rows = (0..crate::audio::MorphTarget::COUNT)
                .filter(|&i| ui.morph.depth_at(s, i) != 0.0)
                .map(|i| {
                    format!(
                        "{} {:+.2}",
                        crate::audio::MorphTarget::from_index(i).label(),
                        ui.morph.depth_at(s, i)
                    )
                })
                .collect();
            t(
                &format!("MORPH {}", source_label(s)),
                format!(
                    "{} {:+.2}",
                    crate::audio::MorphTarget::from_index(ui.morph.target).label(),
                    ui.morph.depth(s)
                ),
                rows,
            )
        }
        Screen::NumPad => t(
            "NUM PAD",
            match ui.num_first {
                Some(p) => format!("{}:{}-", (b'A' + ui.browse.bank) as char, p),
                None => "Press page 1-8".into(),
            },
            Vec::new(),
        ),
    }
}

pub use super::oled_paint::paint;

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn layer_labels_follow_the_hardware_short_names() {
        assert_eq!(layer_label(LayerId::OrganA), "Org A");
        assert_eq!(layer_label(LayerId::SynthC), "Syn C");
    }
}

#[cfg(test)]
#[path = "oled_screens_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "oled_tests.rs"]
mod more_tests;
