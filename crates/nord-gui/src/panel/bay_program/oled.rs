//! The Program OLED: text of every screen (pure, testable) and its painter.

use super::catalog::display_name;
use super::model::{ListMode, PresetKind, ProgView, Screen};
use super::morph::source_label;
use super::nav::ORGAN_PRESETS;
use super::oled_home::{location_title, name_line, section_rows};
use super::split::{note_name, xfade_label, SplitRow};
use super::store::{ProgLoc, StoreStage, StoreTarget};
use crate::panel::ui_state::ProgramUi;
use nord_core::morph::LayerId;

/// What the display shows besides the Program area's own state.
#[derive(Debug, Clone, Default)]
pub struct Summary {
    pub organ: String,
    pub piano: String,
    pub synth: String,
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
}

impl OledText {
    /// All text joined for the hover tooltip.
    pub fn plain(&self) -> String {
        let mut s = format!(
            "{}\n{}\n",
            super::oled_home::strip_marks(&self.big),
            self.sub
        );
        for r in &self.rows {
            s.push_str(r.trim_start_matches(super::menu_screen::DIM));
            s.push('\n');
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

fn window(items: &[String], cursor: usize, n: usize) -> Vec<String> {
    let start = cursor
        .saturating_sub(n / 2)
        .min(items.len().saturating_sub(n));
    items
        .iter()
        .enumerate()
        .skip(start)
        .take(n)
        .map(|(i, t)| format!("{}{t}", if i == cursor { ">" } else { " " }))
        .collect()
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
        t.rows.push(m.clone());
    }
    t
}

fn list(ui: &ProgramUi) -> OledText {
    let order = ui.catalog.order(ui.list_mode);
    let names: Vec<String> = order
        .iter()
        .map(|&i| display_name(&ui.catalog.programs[i]))
        .collect();
    let mode = match ui.list_mode {
        ListMode::Num => "Num",
        ListMode::Abc => "Abc",
        ListMode::Cat => "Cat",
    };
    OledText {
        big: format!("LIST {mode}"),
        sub: names
            .get(ui.list_cursor)
            .cloned()
            .unwrap_or_else(|| "No backup open".into()),
        rows: window(&names, ui.list_cursor, 4),
        soft: ui.screen.soft_labels(),
    }
}

fn store(ui: &ProgramUi) -> OledText {
    let ed = &ui.store.editor;
    if ui.store.stage == StoreStage::Naming {
        let at = ed.cursor();
        let chars: Vec<char> = ed.display().chars().collect();
        let marked: String = (0..chars.len().max(at + 1))
            .map(|i| {
                let c = chars.get(i).copied().unwrap_or(' ');
                if i == at {
                    format!("[{c}]")
                } else {
                    c.to_string()
                }
            })
            .collect();
        return OledText {
            big: "STORE PROGRAM AS".into(),
            sub: marked,
            rows: vec![format!("Cat: {}", ed.category_label())],
            soft: ui.screen.soft_labels(),
        };
    }
    let (target, existing) = match ui.store.target {
        Some(StoreTarget::Live(s)) => (format!("Live {}", s + 1), String::new()),
        Some(StoreTarget::Bank(l)) => (l.label(), name_at(ui, l)),
        None => (String::new(), String::new()),
    };
    // VERIFIED: hardware lists the name now at the target (`<empty>` when unused) and then
    // `As: <name>` (piano-pad video, 5:40-5:45).
    let existing = if existing.is_empty() {
        super::nav::EMPTY_NAME.to_string()
    } else {
        existing
    };
    OledText {
        big: "STORE PROGRAM TO".into(),
        sub: target,
        rows: vec![existing, format!("As: {}", ui.store.editor.name())],
        ..Default::default()
    }
}

fn split(ui: &ProgramUi) -> OledText {
    let s = &ui.split;
    let pts = |f: &dyn Fn(usize) -> String| {
        ["L", "M", "H"]
            .iter()
            .enumerate()
            .map(|(i, n)| format!("{}{n}:{}", if i == s.point { ">" } else { "" }, f(i)))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let note = pts(&|i| {
        if s.enabled[i] {
            note_name(s.notes[i])
        } else {
            "Off".into()
        }
    });
    let xf = pts(&|i| xfade_label(s.xfade[i]).into());
    let (a, b) = if s.row == SplitRow::Note {
        (">", " ")
    } else {
        (" ", ">")
    };
    OledText {
        big: format!("KEYBOARD SPLIT {}", if s.on { "ON" } else { "OFF" }),
        sub: String::new(),
        rows: vec![format!("{a}Note {note}"), format!("{b}xFade {xf}")],
        soft: ui.screen.soft_labels(),
    }
}

fn preset(ui: &ProgramUi, kind: PresetKind, single: bool) -> OledText {
    let names: Vec<String> = match kind {
        PresetKind::Organ => ORGAN_PRESETS.iter().map(|m| format!("{m:?}")).collect(),
        PresetKind::Piano => ui.catalog.pianos.iter().map(|e| display_name(e)).collect(),
        PresetKind::Synth => ui.catalog.samples.iter().map(|e| display_name(e)).collect(),
    };
    let mut rows = window(&names, ui.preset_cursor, 4);
    if single {
        rows.insert(0, format!("Layer: {}", layer_label(ui.focus)));
    }
    OledText {
        big: kind.title().into(),
        sub: names
            .get(ui.preset_cursor)
            .cloned()
            .unwrap_or_else(|| "No backup open".into()),
        rows,
        soft: ui.screen.soft_labels(),
    }
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
    };
    match ui.screen {
        Screen::Home => home(ui, sum),
        Screen::List => list(ui),
        Screen::Menu(m) => super::menu_screen::compose(ui, m),
        Screen::StoreTo | Screen::StoreAs => store(ui),
        Screen::Split => split(ui),
        Screen::Clock => t(
            "MST CLK",
            format!("{:.0} BPM", ui.clock.bpm),
            vec![
                format!("KBS: {}", ui.clock.kbs.label()),
                "External clock: none".into(),
            ],
        ),
        Screen::Transpose => t(
            "TRANSPOSE",
            format!(
                "{:+} st {}",
                ui.transp.semitones,
                if ui.transp.on { "on" } else { "off" }
            ),
            Vec::new(),
        ),
        Screen::Preset { kind, single } => preset(ui, kind, single),
        Screen::Undo => t(
            "UNDO",
            match (ui.undo_blocked, ui.redo_available) {
                (true, true) => "Nothing to Undo. Redo?",
                (true, false) => "Nothing to Undo",
                (false, true) => "Revert the last edit? (Redo ready)",
                (false, false) => "Revert the last edit?",
            }
            .into(),
            Vec::new(),
        ),
        // VERIFIED: title, prompt and the per-section layer letters (`AB`, `A`, `A`) above the
        // All / Organ / Piano / Synth soft buttons (soft-piano video `NVMtX0epXog` 1:16,
        // piano-pad video `hR31DQrxfT4` 1:02). PAGE picking the layer is emulator-only.
        Screen::LayerInit => t(
            "LAYER INIT",
            "Press to initialize".into(),
            vec![layer_init_letters(ui.focus), "PAGE picks the layer".into()],
        ),
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
mod tests {
    use super::*;

    fn sum() -> Summary {
        Summary {
            organ: "B3".into(),
            piano: "Grand".into(),
            synth: "SuperSaw".into(),
            layers_on: [true, false, true, false, true, false, false],
        }
    }

    #[test]
    fn four_prog_views_and_preset_name() {
        let mut u = ProgramUi::default();
        u.title = "Sweet".into();
        let t = compose(&u, &sum());
        assert_eq!((t.big.as_str(), t.sub.as_str()), ("A:11", "Sweet"));
        u.preset_name = true;
        assert!(compose(&u, &sum()).sub.contains("B3 / Grand / SuperSaw"));
        u.preset_name = false;
        u.view = u.view.next();
        let t = compose(&u, &sum());
        assert_eq!(t.rows, ["Organ B3", "Piano Grand", "Synth SuperSaw"]);
        assert!(t.big.contains("Sweet"));
        u.view = u.view.next();
        let t = compose(&u, &sum());
        assert!(t.rows[0].contains("Org A") && t.rows[0].contains("Syn A"));
        u.view = u.view.next();
        let t = compose(&u, &sum());
        assert_eq!(t.rows.len(), 8);
        assert!(t.rows[0].starts_with('>'), "current program is highlighted");
    }

    #[test]
    fn layer_init_screen_matches_the_hardware_prompt_and_letters() {
        let mut u = ProgramUi::default();
        u.screen = Screen::LayerInit;
        u.focus = LayerId::SynthC;
        let t = compose(&u, &sum());
        assert_eq!(t.big, "LAYER INIT");
        assert_eq!(t.sub, "Press to initialize");
        assert_eq!(t.rows[0], "Organ AB  Piano A  Synth C");
        assert_eq!(t.soft, ["All", "Organ", "Piano", "Synth"]);
        u.focus = LayerId::PianoB;
        assert_eq!(compose(&u, &sum()).rows[0], "Organ AB  Piano B  Synth A");
    }

    #[test]
    fn store_screens_show_target_and_name_cursor() {
        let mut u = ProgramUi::default();
        u.title = "Rusty".into();
        u.store_as();
        let t = compose(&u, &sum());
        assert_eq!(t.big, "STORE PROGRAM AS");
        assert!(t.sub.starts_with("[R]usty"), "{}", t.sub);
        assert_eq!(t.soft, ["Abc", "Cat", "Del", "Ins"]);
        assert_eq!(t.rows[0], "Cat: Undefined");
        u.store_press();
        let t = compose(&u, &sum());
        assert_eq!(t.big, "STORE PROGRAM TO");
        assert_eq!(t.sub, "A:11");
        // Unused target: `<empty>`, then the name that will be stored.
        assert_eq!(t.rows, ["<empty>", "As: Rusty"]);
    }
}

#[cfg(test)]
#[path = "oled_tests.rs"]
mod more_tests;
